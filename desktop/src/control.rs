//! Lets a second launch talk to the copy that's already running: opening
//! Carryover from the Start menu or Finder shows the running copy's window,
//! and installing over a running copy asks it to quit first.
//!
//! A one-line command over a TCP connection to localhost only.
use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

const PORT: u16 = 47821;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// Show the main window.
    Open,
    /// Quit, so the program can be replaced.
    Quit,
}

impl Command {
    fn name(self) -> &'static str {
        match self {
            Command::Open => "open",
            Command::Quit => "quit",
        }
    }
}

/// Accepts commands on a background thread. Fails if another copy is
/// already listening.
pub fn listen(handle: impl Fn(Command) + Send + 'static) -> Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, PORT))?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let command = match line.trim() {
                "open" => Command::Open,
                "quit" => Command::Quit,
                _ => continue,
            };
            let _ = (&stream).write_all(b"ok\n");
            handle(command);
        }
    });
    Ok(())
}

/// Sends a command to the running copy. False if none is running.
pub fn send(command: Command) -> bool {
    let address = (Ipv4Addr::LOCALHOST, PORT).into();
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if writeln!(stream, "{}", command.name()).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

/// Whether a copy is running, without asking it to do anything.
pub fn is_running() -> bool {
    TcpStream::connect_timeout(
        &(Ipv4Addr::LOCALHOST, PORT).into(),
        Duration::from_millis(500),
    )
    .is_ok()
}
