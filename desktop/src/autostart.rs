//! Starting Carryover at login, with its tray icon.
//!
//! - **macOS:** a LaunchAgent, the same one `scripts/install-mac-agent.sh`
//!   writes. Turning it off removes the file but leaves this run going.
//! - **Windows:** a value under the current user's `Run` key.
#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
pub const LABEL: &str = "com.coledeterman.copyblip";

#[cfg(target_os = "macos")]
fn plist() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("no home folder")?;
    Ok(PathBuf::from(home).join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    plist().is_ok_and(|path| path.exists())
}

/// Starts at login with the menu bar icon. Restarts after a crash or an
/// update, but Quit keeps it stopped until the next login.
#[cfg(target_os = "macos")]
pub fn set_enabled(enabled: bool) -> Result<()> {
    let path = plist()?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        return Ok(());
    }
    let program = std::env::current_exe()?;
    let home = std::env::var("HOME")?;
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>run</string><string>--tray</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardErrorPath</key><string>{}/Library/Logs/CopyBlip.log</string>
</dict>
</plist>
"#,
        escape(&program.to_string_lossy()),
        escape(&home)
    );
    std::fs::create_dir_all(path.parent().context("no LaunchAgents folder")?)?;
    std::fs::write(path, plist)?;
    Ok(())
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new("reg.exe")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    Ok(status.success())
}

#[cfg(windows)]
pub fn is_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", "CopyBlip"]).unwrap_or(false)
}

#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<()> {
    if !enabled {
        if is_enabled() {
            anyhow::ensure!(
                reg(&["delete", RUN_KEY, "/v", "CopyBlip", "/f"])?,
                "couldn't remove the login item"
            );
        }
        return Ok(());
    }
    let program = std::env::current_exe()?;
    let command = format!("\"{}\" run --background --tray", program.display());
    anyhow::ensure!(
        reg(&[
            "add", RUN_KEY, "/v", "CopyBlip", "/t", "REG_SZ", "/d", &command, "/f"
        ])?,
        "couldn't add the login item"
    );
    Ok(())
}
