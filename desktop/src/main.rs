use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use copyblip::clipboard::{Clipboard, Content};
use copyblip::config::{self, Config, DEFAULT_PORT};
use copyblip::{discovery, hub::Hub, relay};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    /// Join a group, reading an invite link or pairing code from standard input
    Join {
        #[arg(long)]
        force: bool,
    },
    /// Print this group's pairing code
    PairingCode,
    /// Show a QR code for adding a device. Scan it with the iPhone's camera
    Invite,
    /// Manage addresses to connect to besides the local network, such as
    /// Tailscale names. Restart `run` to apply changes
    Peer {
        #[command(subcommand)]
        action: PeerAction,
    },
    /// Show, set or remove the relay for syncing across networks. Restart
    /// `run` to apply changes
    Relay {
        /// Relay URL, such as https://copyblip-relay.example.workers.dev
        url: Option<String>,
        /// Stop using a relay
        #[arg(long, conflicts_with = "url")]
        off: bool,
    },
    /// Sync the clipboard until stopped
    Run {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Also connect to this address, on top of saved peers
        #[arg(long = "peer")]
        peers: Vec<String>,
        /// Use this relay instead of the saved one
        #[arg(long, conflicts_with = "no_relay")]
        relay: Option<String>,
        /// Don't use the saved relay
        #[arg(long)]
        no_relay: bool,
        /// Don't advertise or look for devices on the local network
        #[arg(long)]
        no_discovery: bool,
        /// Use a private pasteboard instead of the real clipboard (macOS, for testing)
        #[arg(long)]
        pasteboard: Option<String>,
        /// Close the console window and log to copyblip.log in the config folder (Windows)
        #[arg(long)]
        background: bool,
        /// Show an icon in the menu bar (macOS) or notification area (Windows)
        #[arg(long)]
        tray: bool,
    },
    /// Read or set a clipboard, for testing
    Clip {
        /// New text. Prints the current text if omitted
        text: Option<String>,
        /// Put this PNG file on the clipboard instead of text
        #[arg(long, conflicts_with = "text")]
        image: Option<PathBuf>,
        /// Save an image on the clipboard to this file
        #[arg(long, conflicts_with_all = ["text", "image"])]
        save_image: Option<PathBuf>,
        #[arg(long)]
        pasteboard: Option<String>,
    },
}

#[derive(Subcommand)]
enum PeerAction {
    /// Add an address, as host or host:port
    Add { address: String },
    /// Remove an address
    Remove { address: String },
    /// List saved addresses
    List,
}

#[cfg(target_os = "macos")]
fn open_clipboard(pasteboard: Option<String>) -> Result<Arc<dyn Clipboard>> {
    Ok(Arc::new(copyblip::clipboard::macos::MacClipboard::new(
        pasteboard,
    )))
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
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
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

/// Draws a QR code with half-height blocks, two rows per line. Colors are set
/// explicitly, black on white, so it scans in dark terminal themes too.
fn qr(text: &str) -> Result<String> {
    let code = qrcode::QrCode::new(text)?;
    let width = code.width();
    let colors = code.to_colors();
    // A four-module quiet zone, which scanners need.
    let dark = |x: isize, y: isize| {
        (0..width as isize).contains(&x)
            && (0..width as isize).contains(&y)
            && colors[y as usize * width + x as usize] == qrcode::Color::Dark
    };
    let mut out = String::new();
    for y in (-4..width as isize + 4).step_by(2) {
        out.push_str("\x1b[30;47m");
        for x in -4..width as isize + 4 {
            out.push(match (dark(x, y), dark(x, y + 1)) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        out.push_str("\x1b[0m\n");
    }
    Ok(out)
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
            println!(
                "New group created. Run `copyblip join` on your other devices with this code:"
            );
            println!("{}", config.pairing_code());
        }
        Command::Join { force } => {
            eprintln!("Paste the invite link or pairing code, then press Enter:");
            let mut code = String::new();
            std::io::stdin().read_line(&mut code)?;
            let (key, relay) = config::parse_invite(&code)?;
            let mut config = Config::new(&key);
            config.relay = relay;
            config.save(&dir, force)?;
            println!("Joined. Start syncing with `copyblip run`.");
        }
        Command::PairingCode => println!("{}", Config::load(&dir)?.pairing_code()),
        Command::Invite => {
            let link = Config::load(&dir)?.invite_link();
            println!("{}", qr(&link)?);
            println!("On an iPhone, scan this with the Camera app and tap the CopyBlip link.");
            println!("On a computer, run `copyblip join` and paste this link:\n{link}");
            println!(
                "Anyone with this code can read what you copy, so only share it with your own devices."
            );
        }
        Command::Peer { action } => {
            let mut config = Config::load(&dir)?;
            match action {
                PeerAction::Add { address } => {
                    let address = config::normalize_peer(&address)?;
                    if !config.peers.contains(&address) {
                        config.peers.push(address.clone());
                        config.update(&dir)?;
                    }
                    println!("Saved {address}.");
                }
                PeerAction::Remove { address } => {
                    let address = config::normalize_peer(&address)?;
                    let before = config.peers.len();
                    config.peers.retain(|peer| *peer != address);
                    if config.peers.len() == before {
                        bail!("{address} isn't saved");
                    }
                    config.update(&dir)?;
                    println!("Removed {address}.");
                }
                PeerAction::List => config.peers.iter().for_each(|peer| println!("{peer}")),
            }
        }
        Command::Relay { url, off } => {
            let mut config = Config::load(&dir)?;
            if off {
                config.relay = None;
                config.update(&dir)?;
                println!("Relay turned off.");
            } else if let Some(url) = url {
                config.relay = Some(config::normalize_relay(&url)?);
                config.update(&dir)?;
                println!("Relay saved.");
            } else {
                println!("{}", config.relay.as_deref().unwrap_or("No relay set."));
            }
        }
        Command::Run {
            port,
            peers,
            relay: relay_override,
            no_relay,
            no_discovery,
            pasteboard,
            background,
            tray,
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
            for peer in config.peers.iter().cloned().chain(peers) {
                hub.dial(Arc::new(Mutex::new(vec![config::normalize_peer(&peer)?])));
            }
            if !no_discovery {
                discovery::start(hub.clone(), &key, &config.device, port)?;
            }
            let relay_url = match relay_override {
                Some(url) => Some(config::normalize_relay(&url)?),
                None if no_relay => None,
                None => config.relay.clone(),
            };
            if let Some(url) = relay_url {
                relay::start(hub.clone(), url, &key, &config.device);
            }
            hub.watch_clipboard();
            eprintln!("syncing on port {port}");
            if tray {
                #[cfg(target_os = "macos")]
                let log = PathBuf::from(std::env::var_os("HOME").context("no home folder")?)
                    .join("Library/Logs/CopyBlip.log");
                #[cfg(windows)]
                let log = dir.join("copyblip.log");
                copyblip::tray::run(hub, config.invite_link(), log);
            }
            loop {
                std::thread::park();
            }
        }
        Command::Clip {
            text,
            image,
            save_image,
            pasteboard,
        } => {
            let clipboard = open_clipboard(pasteboard)?;
            if let Some(text) = text {
                clipboard.write(&Content::Text(text))?;
            } else if let Some(path) = image {
                clipboard.write(&Content::Image(std::fs::read(path)?))?;
            } else {
                match (clipboard.read()?, save_image) {
                    (Some(Content::Text(text)), None) => println!("{text}"),
                    (Some(Content::Image(png)), Some(path)) => std::fs::write(path, png)?,
                    (Some(content), _) => bail!("the clipboard holds {}", content.describe()),
                    (None, _) => bail!("nothing syncable on the clipboard"),
                }
            }
        }
    }
    Ok(())
}
