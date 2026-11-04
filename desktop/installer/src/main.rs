//! Carryover-Setup.exe, what people download on Windows. It carries
//! copyblip.exe and the WebView2 loader that copyblip.exe needs beside it,
//! puts both in %LOCALAPPDATA%\CopyBlip, and opens Carryover's setup
//! wizard from there, which does the rest.
//!
//! copyblip.exe can't be the download itself: Windows won't start it
//! without WebView2Loader.dll in the same folder.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PROGRAM: &[u8] = include_bytes!(env!("CARRYOVER_EXE"));
const LOADER: &[u8] = include_bytes!(env!("CARRYOVER_LOADER"));

/// Where a running Carryover takes commands. See copyblip's control.rs.
const CONTROL_PORT: u16 = 47821;

fn main() {
    if let Err(message) = run() {
        alert(&message);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("This PC has no LOCALAPPDATA folder.")?;
    let folder = PathBuf::from(base).join("CopyBlip");
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Couldn't make {}: {error}", folder.display()))?;
    let program = folder.join("copyblip.exe");

    stop_running();
    write(&program, PROGRAM)?;
    write(&folder.join("WebView2Loader.dll"), LOADER)?;
    // Antivirus can remove the program right after it lands.
    if !program.exists() {
        return Err(blocked(&folder));
    }
    std::process::Command::new(&program)
        .arg("setup")
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                blocked(&folder)
            } else {
                format!("Couldn't open Carryover's setup: {error}")
            }
        })?;
    Ok(())
}

/// Stops a running Carryover so its files can be replaced: politely, then,
/// for versions too old to listen, by force.
fn stop_running() {
    if ask_to_quit() {
        let deadline = Instant::now() + Duration::from_secs(8);
        while is_listening() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = hidden("taskkill.exe")
        .args(["/f", "/im", "copyblip.exe"])
        .status();
}

fn ask_to_quit() -> bool {
    let address = (Ipv4Addr::LOCALHOST, CONTROL_PORT).into();
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if writeln!(stream, "quit").is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

fn is_listening() -> bool {
    TcpStream::connect_timeout(
        &(Ipv4Addr::LOCALHOST, CONTROL_PORT).into(),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// Writes a file, retrying while a copy that's still exiting holds it.
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(300)),
            Err(error) => {
                return Err(format!(
                    "Couldn't write {}: {error}\n\nIf Carryover is open, quit it from its icon \
                     by the clock, then run setup again.",
                    path.display()
                ));
            }
        }
    }
}

fn blocked(folder: &Path) -> String {
    format!(
        "Windows Security probably removed Carryover. Open Windows Security, then \
         Virus & threat protection, then Protection history, and allow Carryover. \
         Or add an exclusion for\n{}\nThen run setup again.",
        folder.display()
    )
}

fn hidden(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(windows)]
fn alert(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::HSTRING;
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from("Carryover Setup"),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn alert(text: &str) {
    eprintln!("{text}");
}
