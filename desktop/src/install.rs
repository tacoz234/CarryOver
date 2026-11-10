//! Putting Carryover in place on this computer, and starting it.
//!
//! - **Windows:** the program lives at `%LOCALAPPDATA%\CopyBlip\copyblip.exe`,
//!   where earlier versions put it, so their updates and login items keep
//!   working. Installing also adds a Start menu shortcut and an entry in
//!   Settings › Apps, whose Uninstall runs `copyblip.exe uninstall`.
//! - **macOS:** the app stays wherever it was dragged, normally
//!   Applications. Installing points the login item at it.
use crate::autostart;
use crate::control::{self, Command};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Whether this copy still needs installing before it can run, as when
/// it's a download that was just double-clicked.
pub fn needed() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::linux_install::needed()
    }
    #[cfg(windows)]
    {
        let (Ok(current), Ok(installed)) = (std::env::current_exe(), program()) else {
            return true;
        };
        !same_file(&current, &installed)
    }
    #[cfg(target_os = "macos")]
    {
        // A login item pointing at another copy, such as one set up by
        // scripts/install-mac-agent.sh, should move to this one.
        match (autostart::plist(), std::env::current_exe()) {
            (Ok(plist), Ok(current)) => std::fs::read_to_string(plist)
                .is_ok_and(|text| !text.contains(&*current.to_string_lossy())),
            _ => false,
        }
    }
}

/// Where the installed program lives.
pub fn program() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA").context("no LOCALAPPDATA folder")?;
        Ok(PathBuf::from(base).join("CopyBlip").join("copyblip.exe"))
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_install::program()
    }
    #[cfg(target_os = "macos")]
    {
        Ok(std::env::current_exe()?)
    }
}

#[cfg(windows)]
fn same_file(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_owned())
            .to_string_lossy()
            .to_lowercase()
    };
    normalize(a) == normalize(b)
}

/// Asks a running copy to quit, and waits for it to let go of its port.
pub fn stop_running() {
    if !control::send(Command::Quit) {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while control::is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    // The process can outlive its listener by a moment.
    std::thread::sleep(Duration::from_millis(500));
}

/// Copies this program into place, replacing a running copy, and returns
/// where it went.
pub fn install() -> Result<PathBuf> {
    stop_running();
    let target = program()?;
    #[cfg(target_os = "linux")]
    crate::linux_install::install_from(&std::env::current_exe()?)?;
    #[cfg(windows)]
    {
        let current = std::env::current_exe()?;
        if !same_file(&current, &target) {
            std::fs::create_dir_all(target.parent().context("no install folder")?)?;
            copy_over(&current, &target)?;
            // From a folder with the WebView2 loader beside the program,
            // which it needs there too.
            let loader = current.with_file_name("WebView2Loader.dll");
            if loader.exists() {
                copy_over(&loader, &target.with_file_name("WebView2Loader.dll"))?;
            }
        }
        if let Err(error) = shortcut(&target) {
            eprintln!("couldn't add the Start menu shortcut: {error:#}");
        }
        register(&target)?;
    }
    Ok(target)
}

/// Copies, retrying while an old copy that's still exiting holds the file.
#[cfg(windows)]
fn copy_over(from: &Path, to: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::copy(from, to) {
            Ok(_) => return Ok(()),
            Err(_) if Instant::now() < deadline => {
                // A copy that won't quit, such as one too old to listen.
                kill_others();
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "couldn't copy Carryover to {}. If antivirus removed it, allow it and try again",
                        to.display()
                    )
                });
            }
        }
    }
}

/// Ends other copyblip.exe processes, but not this one.
#[cfg(windows)]
fn kill_others() {
    let filter = format!("PID ne {}", std::process::id());
    let _ = hidden("taskkill.exe")
        .args(["/f", "/im", "copyblip.exe", "/fi", &filter])
        .status();
}

#[cfg(windows)]
fn hidden(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(windows)]
fn shortcut_path() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA").context("no APPDATA folder")?;
    Ok(PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Carryover.lnk"))
}

/// A Start menu shortcut, made through PowerShell since shortcuts are COM
/// objects. Paths go through the environment, so quotes in them are safe.
#[cfg(windows)]
fn shortcut(program: &Path) -> Result<()> {
    let status = hidden("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = (New-Object -ComObject WScript.Shell).CreateShortcut($env:CARRYOVER_LINK); \
             $s.TargetPath = $env:CARRYOVER_PROGRAM; \
             $s.WorkingDirectory = Split-Path $env:CARRYOVER_PROGRAM; \
             $s.Description = 'Shared clipboard across your devices'; \
             $s.Save()",
        ])
        .env("CARRYOVER_LINK", shortcut_path()?)
        .env("CARRYOVER_PROGRAM", program)
        .status()?;
    anyhow::ensure!(status.success(), "PowerShell couldn't make the shortcut");
    Ok(())
}

#[cfg(windows)]
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Carryover";

/// The entry in Settings › Apps.
#[cfg(windows)]
fn register(program: &Path) -> Result<()> {
    let program_text = program.display().to_string();
    let folder = program
        .parent()
        .context("no install folder")?
        .display()
        .to_string();
    let size_kb = std::fs::metadata(program).map_or(0, |meta| meta.len() / 1024);
    let values: [(&str, &str, String); 9] = [
        ("DisplayName", "REG_SZ", "Carryover".into()),
        ("DisplayVersion", "REG_SZ", crate::update::VERSION.into()),
        ("Publisher", "REG_SZ", "Carryover".into()),
        ("DisplayIcon", "REG_SZ", format!("{program_text},0")),
        ("InstallLocation", "REG_SZ", folder),
        (
            "UninstallString",
            "REG_SZ",
            format!("\"{program_text}\" uninstall"),
        ),
        ("NoModify", "REG_DWORD", "1".into()),
        ("NoRepair", "REG_DWORD", "1".into()),
        ("EstimatedSize", "REG_DWORD", size_kb.to_string()),
    ];
    for (name, kind, data) in values {
        anyhow::ensure!(
            autostart::reg(&[
                "add",
                UNINSTALL_KEY,
                "/v",
                name,
                "/t",
                kind,
                "/d",
                &data,
                "/f"
            ])?,
            "couldn't add Carryover to Settings › Apps"
        );
    }
    Ok(())
}

/// Keeps the version in Settings › Apps current after a self-update.
pub fn note_version() {
    #[cfg(windows)]
    if autostart::reg(&["query", UNINSTALL_KEY]).unwrap_or(false) {
        let _ = autostart::reg(&[
            "add",
            UNINSTALL_KEY,
            "/v",
            "DisplayVersion",
            "/t",
            "REG_SZ",
            "/d",
            crate::update::VERSION,
            "/f",
        ]);
    }
}

/// Sets the login item and starts the installed program with its tray icon,
/// apart from this process.
pub fn start(program: &Path, login: bool) -> Result<()> {
    autostart::set_enabled_for(program, login)?;
    #[cfg(target_os = "linux")]
    spawn_linux(program, &[])?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        std::process::Command::new(program)
            .args(["run", "--background", "--tray"])
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("couldn't start Carryover")?;
    }
    #[cfg(target_os = "macos")]
    {
        let uid = unsafe { libc_getuid() };
        let domain = format!("gui/{uid}");
        let service = format!("{domain}/{}", autostart::LABEL);
        let _ = std::process::Command::new("launchctl")
            .args(["bootout", &service])
            .stderr(std::process::Stdio::null())
            .status();
        if login {
            let plist = autostart::plist()?;
            // Unloading finishes in the background, so loading right away
            // can fail.
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let loaded = std::process::Command::new("launchctl")
                    .arg("bootstrap")
                    .arg(&domain)
                    .arg(&plist)
                    .stderr(std::process::Stdio::null())
                    .status()?
                    .success();
                if loaded {
                    break;
                }
                anyhow::ensure!(Instant::now() < deadline, "couldn't start Carryover");
                std::thread::sleep(Duration::from_millis(500));
            }
        } else {
            spawn_mac(program, &[])?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

/// Starts the program with the menu bar icon, logging where the login item
/// would.
#[cfg(target_os = "macos")]
pub fn spawn_mac(program: &Path, extra: &[&str]) -> Result<()> {
    let home = std::env::var_os("HOME").context("no home folder")?;
    let log = PathBuf::from(home).join("Library/Logs/CopyBlip.log");
    std::fs::create_dir_all(log.parent().context("no log folder")?)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    std::process::Command::new(program)
        .args(["run", "--tray"])
        .args(extra)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(file)
        .spawn()
        .context("couldn't start Carryover")?;
    Ok(())
}

/// The .app this program is inside, if any.
#[cfg(target_os = "macos")]
pub fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.parent()?.parent()?.parent()?;
    (bundle.extension()? == "app").then(|| bundle.to_owned())
}

/// False when the app runs from the disk image or Downloads, where it
/// shouldn't stay.
#[cfg(target_os = "macos")]
pub fn in_applications() -> bool {
    let Some(bundle) = bundle() else {
        // A bare program, as when developing.
        return true;
    };
    let home_apps = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications"));
    bundle.starts_with("/Applications") || home_apps.is_some_and(|apps| bundle.starts_with(apps))
}

/// Copies the app into Applications and opens the copy. The caller exits.
#[cfg(target_os = "macos")]
pub fn move_to_applications() -> Result<()> {
    let bundle = bundle().context("Carryover isn't running from an app")?;
    let target = PathBuf::from("/Applications/Carryover.app");
    stop_running();
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .context("couldn't replace the Carryover already in Applications")?;
    }
    let copied = std::process::Command::new("ditto")
        .arg(&bundle)
        .arg(&target)
        .status()?
        .success();
    anyhow::ensure!(copied, "couldn't copy Carryover to Applications");
    // It was already opened once, from the download, so the copy shouldn't
    // ask again.
    let _ = std::process::Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&target)
        .status();
    let opened = std::process::Command::new("open")
        .arg("-n")
        .arg(&target)
        .status()?
        .success();
    anyhow::ensure!(opened, "couldn't open Carryover from Applications");
    Ok(())
}

/// Removes Carryover from this PC, as Settings › Apps › Uninstall does.
/// Keeps the group in %APPDATA%\CopyBlip, so reinstalling rejoins it.
#[cfg(windows)]
pub fn uninstall() -> Result<()> {
    stop_running();
    kill_others();
    autostart::set_enabled_for(&program()?, false)?;
    let _ = std::fs::remove_file(shortcut_path()?);
    let _ = autostart::reg(&["delete", UNINSTALL_KEY, "/f"]);
    // This program can't delete itself while running, so a short-lived
    // command does it once this exits.
    use std::os::windows::process::CommandExt;
    let folder = program()?.parent().context("no install folder")?.to_owned();
    // It retries for up to 30 seconds, since this stays open while its
    // message shows. Raw, since cmd.exe doesn't read the escaping Rust gives
    // arguments.
    let folder = folder.display();
    hidden("cmd.exe")
        .raw_arg(format!(
            "/c for /l %i in (1,1,30) do (ping -n 2 127.0.0.1 >nul & rmdir /s /q \"{folder}\" 2>nul & if not exist \"{folder}\" exit)"
        ))
        .spawn()?;
    Ok(())
}

/// Starts a Linux desktop process with its output in the config folder.
#[cfg(target_os = "linux")]
pub fn spawn_linux(program: &Path, extra: &[&str]) -> Result<()> {
    spawn_linux_in(program, &crate::config::default_dir()?, extra)
}

#[cfg(target_os = "linux")]
pub fn spawn_linux_in(program: &Path, dir: &Path, extra: &[&str]) -> Result<()> {
    spawn_linux_run(program, dir, true, extra)
}

#[cfg(target_os = "linux")]
pub fn spawn_linux_run(program: &Path, dir: &Path, tray: bool, extra: &[&str]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    std::fs::create_dir_all(&dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("copyblip.log"))?;
    let mut command = std::process::Command::new(program);
    command.arg("run");
    if tray {
        command.arg("--tray");
    }
    command
        .process_group(0)
        .arg("--config-dir")
        .arg(dir)
        .args(extra)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log)
        .spawn()
        .context("couldn't start Carryover")?;
    Ok(())
}
