use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use copyblip::clipboard::Clipboard;
use copyblip::config::{self, Config};
use copyblip::{discovery, hub::Hub};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const DEFAULT_PORT: u16 = 47820;

#[derive(Parser)]
#[command(version, about = "Shared clipboard across your devices")]
struct Cli {
    /// Where to keep this device's config
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start a new group on this device and print its pairing code
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Join a group, reading the pairing code from standard input
    Join {
        #[arg(long)]
        force: bool,
    },
    /// Print this group's pairing code
    PairingCode,
    /// Sync the clipboard until stopped
    Run {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Also connect to this address, for example over a VPN
        #[arg(long = "peer")]
        peers: Vec<SocketAddr>,
        /// Don't advertise or look for devices on the local network
        #[arg(long)]
        no_discovery: bool,
        /// Use a private pasteboard instead of the real clipboard (macOS, for testing)
        #[arg(long)]
        pasteboard: Option<String>,
        /// Close the console window and log to copyblip.log in the config folder (Windows)
        #[arg(long)]
        background: bool,
    },
    /// Read or set a clipboard, for testing
    Clip {
        /// New contents. Prints the current contents if omitted
        text: Option<String>,
        #[arg(long)]
        pasteboard: Option<String>,
    },
}

#[cfg(target_os = "macos")]
fn open_clipboard(pasteboard: Option<String>) -> Result<Arc<dyn Clipboard>> {
    Ok(Arc::new(copyblip::clipboard::macos::MacClipboard::new(pasteboard)))
}

#[cfg(windows)]
fn open_clipboard(pasteboard: Option<String>) -> Result<Arc<dyn Clipboard>> {
    if pasteboard.is_some() {
        bail!("--pasteboard is macOS only");
    }
    Ok(Arc::new(copyblip::clipboard::windows::WindowsClipboard))
}

#[cfg(windows)]
fn detach_console(log: &std::path::Path) -> Result<()> {
    use std::os::windows::io::IntoRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::{FreeConsole, STD_ERROR_HANDLE, SetStdHandle};
    std::fs::create_dir_all(log.parent().context("log path has no folder")?)?;
    let file = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    // Rust looks up the standard error handle on every write, so log lines
    // go to the file from here on. The handle stays open for the process.
    unsafe {
        FreeConsole()?;
        SetStdHandle(STD_ERROR_HANDLE, HANDLE(file.into_raw_handle()))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn detach_console(_log: &std::path::Path) -> Result<()> {
    bail!("--background is for Windows. On macOS, use scripts/install-mac-agent.sh")
}

fn device_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "device".to_owned())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = match cli.config_dir {
        Some(dir) => dir,
        None => config::default_dir()?,
    };
    match cli.command {
        Command::Init { force } => {
            let config = Config::new(&rand::random());
            config.save(&dir, force)?;
            println!("New group created. Run `copyblip join` on your other devices with this code:");
            println!("{}", config.pairing_code());
        }
        Command::Join { force } => {
            eprintln!("Paste the pairing code, then press Enter:");
            let mut code = String::new();
            std::io::stdin().read_line(&mut code)?;
            Config::new(&config::parse_code(&code)?).save(&dir, force)?;
            println!("Joined. Start syncing with `copyblip run`.");
        }
        Command::PairingCode => println!("{}", Config::load(&dir)?.pairing_code()),
        Command::Run {
            port,
            peers,
            no_discovery,
            pasteboard,
            background,
        } => {
            if background {
                detach_console(&dir.join("copyblip.log"))?;
            }
            let config = Config::load(&dir)?;
            let key = config.key()?;
            let clipboard = open_clipboard(pasteboard)?;
            if let Some(note) = clipboard.startup_note() {
                eprintln!("{note}");
            }
            let listener = TcpListener::bind(("0.0.0.0", port))
                .with_context(|| format!("can't listen on port {port}"))?;
            let port = listener.local_addr()?.port();
            let hub = Hub::new(key, config.device.clone(), device_name(), clipboard);
            hub.serve(listener);
            for peer in peers {
                hub.dial(Arc::new(Mutex::new(vec![peer])));
            }
            if !no_discovery {
                discovery::start(hub.clone(), &key, &config.device, port)?;
            }
            hub.watch_clipboard();
            eprintln!("syncing on port {port}");
            loop {
                std::thread::park();
            }
        }
        Command::Clip { text, pasteboard } => {
            let clipboard = open_clipboard(pasteboard)?;
            match text {
                Some(text) => clipboard.write_text(&text)?,
                None => match clipboard.read_text()? {
                    Some(text) => println!("{text}"),
                    None => bail!("no syncable text on the clipboard"),
                },
            }
        }
    }
    Ok(())
}
