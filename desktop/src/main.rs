// A windowed program on Windows, so opening it shows no console. Commands
// run from a terminal attach to that terminal's console instead.
#![cfg_attr(windows, windows_subsystem = "windows")]
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use copyblip::clipboard::{Clipboard, Content};
use copyblip::config::{self, Config, DEFAULT_PORT};
use copyblip::{discovery, hub::Hub, update};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(version, about = "Shared clipboard across your devices")]
struct Cli {
    /// Where to keep this device's config
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    /// Without a command, opens Carryover: its setup if this computer isn't
    /// ready, or else its window.
    #[command(subcommand)]
    command: Option<Command>,
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
    /// Sync the clipboard until stopped
    Run {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Use a private pasteboard instead of the real clipboard (macOS, for testing)
        #[arg(long)]
        pasteboard: Option<String>,
        /// Close the console window and log to copyblip.log in the config folder (Windows)
        #[arg(long)]
        background: bool,
        /// Show an icon in the menu bar (macOS) or notification area (Windows)
        #[arg(long)]
        tray: bool,
        /// What other devices call this one. Defaults to the host name
        #[arg(long)]
        name: Option<String>,
        /// Open the main window at start, with --tray
        #[arg(long, requires = "tray")]
        window: bool,
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
    /// Open the setup wizard, as Carryover-Setup.exe does once it has put
    /// the program in place
    #[command(hide = true)]
    Setup,
    /// Remove Carryover from this PC, as Settings › Apps › Uninstall does
    #[command(hide = true)]
    Uninstall,
    /// For publishing: create the release signing key if missing, and print
    /// its public half
    #[command(hide = true)]
    ReleaseKey {
        #[arg(long)]
        key: PathBuf,
    },
    /// For publishing: write a signed manifest.json for the programs in a
    /// folder, named copyblip-<platform>[.exe]
    #[command(hide = true)]
    ReleaseManifest {
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        version: String,
        #[arg(long, default_value = "")]
        notes: String,
        folder: PathBuf,
    },
}

fn read_release_key(path: &std::path::Path) -> Result<[u8; 32]> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("can't read the release key at {}", path.display()))?;
    hex::decode(text.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .context("release key is malformed")
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
        // Fails when there's no console, as when opened from Explorer.
        let _ = FreeConsole();
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

/// After an update, the old copy may still be letting go of the port.
fn bind(port: u16) -> Result<TcpListener> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match TcpListener::bind(("0.0.0.0", port)) {
            Ok(listener) => return Ok(listener),
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("can't listen on port {port}"));
            }
        }
    }
}

#[cfg(windows)]
fn device_name() -> String {
    // Running hostname.exe would flash a console window.
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "device".to_owned())
}

#[cfg(not(windows))]
fn device_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "device".to_owned())
}

/// Opened from Explorer, Finder or the Start menu, with no command.
#[cfg(any(target_os = "macos", windows))]
fn launch(dir: PathBuf) -> Result<()> {
    use copyblip::control::{self, Command};
    use copyblip::{install, setup};
    let pair = Config::load(&dir).is_err();
    let needs_install = install::needed();
    if pair || needs_install {
        setup::run(setup::Plan {
            install: needs_install,
            pair,
            dir,
        });
    }
    if control::send(Command::Open) {
        return Ok(());
    }
    // Not running, as after Quit: start it with its window open.
    let program = std::env::current_exe()?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new(program)
            .args(["run", "--background", "--tray", "--window"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .context("couldn't start Carryover")?;
    }
    #[cfg(target_os = "macos")]
    install::spawn_mac(&program, &["--window"])?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn launch(_dir: PathBuf) -> Result<()> {
    bail!("choose a command; see --help")
}

/// Lets output from commands run in a terminal reach it.
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() -> Result<()> {
    #[cfg(windows)]
    if std::env::args_os().len() > 1 {
        attach_console();
    }
    let cli = Cli::parse();
    let dir = match cli.config_dir {
        Some(dir) => dir,
        None => config::default_dir()?,
    };
    let Some(command) = cli.command else {
        return launch(dir);
    };
    match command {
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
            let config = Config::new(&config::parse_invite(&code)?);
            config.save(&dir, force)?;
            println!("Joined. Start syncing with `copyblip run`.");
        }
        Command::PairingCode => println!("{}", Config::load(&dir)?.pairing_code()),
        Command::Invite => {
            let link = Config::load(&dir)?.invite_link();
            println!("{}", qr(&link)?);
            println!("On an iPhone, scan this with the Camera app and tap the Carryover link.");
            println!("On a computer, run `copyblip join` and paste this link:\n{link}");
            println!(
                "Anyone with this code can read what you copy, so only share it with your own devices."
            );
        }
        Command::Run {
            port,
            pasteboard,
            background,
            tray,
            name,
            window,
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
            let listener = bind(port)?;
            let port = listener.local_addr()?.port();
            let hub = Hub::new(
                key,
                config.device.clone(),
                name.unwrap_or_else(device_name),
                clipboard,
            );
            hub.serve(listener);
            discovery::start(hub.clone(), &key, &config.device, port)?;
            hub.watch_clipboard();
            eprintln!("Carryover {} syncing on port {port}", update::VERSION);
            if tray {
                #[cfg(target_os = "macos")]
                let log = PathBuf::from(std::env::var_os("HOME").context("no home folder")?)
                    .join("Library/Logs/CopyBlip.log");
                #[cfg(windows)]
                let log = dir.join("copyblip.log");
                hub.remember_devices(dir.join("devices.json"));
                hub.keep_history(dir.join("history"));
                let app = copyblip::app::App::start(hub, dir, config, log)?;
                copyblip::tray::run(app, window);
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
        #[cfg(any(target_os = "macos", windows))]
        Command::Setup => copyblip::setup::run(copyblip::setup::Plan {
            // Adds the shortcut and the Settings › Apps entry, and replaces
            // any login item from an older install.
            install: true,
            pair: Config::load(&dir).is_err(),
            dir,
        }),
        #[cfg(not(any(target_os = "macos", windows)))]
        Command::Setup => bail!("setup is for macOS and Windows"),
        Command::Uninstall => {
            #[cfg(windows)]
            {
                let sure = copyblip::setup::message(
                    "Uninstall Carryover",
                    "Remove Carryover from this PC?

This PC stays in your group, so                      installing Carryover again picks up where it left off.",
                    true,
                );
                if sure {
                    match copyblip::install::uninstall() {
                        Ok(()) => {
                            copyblip::setup::message(
                                "Uninstall Carryover",
                                "Carryover was removed from this PC.",
                                false,
                            );
                        }
                        Err(error) => copyblip::setup::fail(&format!(
                            "Couldn't uninstall Carryover: {error:#}"
                        )),
                    }
                }
            }
            #[cfg(not(windows))]
            bail!("uninstall is for Windows. On macOS, drag Carryover to the Trash");
        }
        Command::ReleaseKey { key } => {
            if !key.exists() {
                use std::io::Write;
                #[cfg(unix)]
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::create_dir_all(key.parent().context("key path has no folder")?)?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                options.mode(0o600);
                let mut file = options.open(&key)?;
                writeln!(file, "{}", hex::encode(rand::random::<[u8; 32]>()))?;
                eprintln!(
                    "Created {}. Back it up: losing it means no more updates.",
                    key.display()
                );
            }
            println!("{}", update::public_key_for(&read_release_key(&key)?));
        }
        Command::ReleaseManifest {
            key,
            version,
            notes,
            folder,
        } => {
            use sha2::Digest;
            let secret = read_release_key(&key)?;
            let mut files = std::collections::BTreeMap::new();
            for entry in std::fs::read_dir(&folder)? {
                let name = entry?.file_name().to_string_lossy().into_owned();
                let Some(platform) = name
                    .strip_prefix("copyblip-")
                    .map(|rest| rest.trim_end_matches(".exe").to_owned())
                else {
                    continue;
                };
                let bytes = std::fs::read(folder.join(&name))?;
                files.insert(
                    platform,
                    update::Program {
                        size: bytes.len() as u64,
                        sha256: hex::encode(sha2::Sha256::digest(&bytes)),
                        name,
                    },
                );
            }
            if files.is_empty() {
                bail!("no copyblip-<platform> programs in {}", folder.display());
            }
            let manifest = serde_json::to_vec_pretty(&update::Manifest {
                version,
                notes,
                files,
            })?;
            std::fs::write(folder.join("manifest.json"), &manifest)?;
            std::fs::write(
                folder.join("manifest.json.sig"),
                update::sign(&manifest, &secret),
            )?;
            println!("Wrote a signed manifest in {}", folder.display());
        }
    }
    Ok(())
}
