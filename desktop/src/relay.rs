//! Reaches devices on other networks through a CopyBlip relay. See
//! docs/relay.md. The relay only passes bytes along: each relayed pipe runs the
//! same encrypted protocol as a direct connection.
use crate::hub::Hub;
use crate::protocol::{GroupKey, Role, relay_room};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const KEEPALIVE: Duration = Duration::from_secs(30);
const REDIAL: Duration = Duration::from_secs(30);
const MAX_RETRY: Duration = Duration::from_secs(60);
/// Writes are split so no message comes near the relay's size limit.
const CHUNK: usize = 256 * 1024;

#[derive(Deserialize)]
#[serde(untagged)]
enum Event {
    Devices { devices: Vec<String> },
    Pipe { pipe: String },
}

struct Relay {
    hub: Arc<Hub>,
    base: String,
    device: String,
    dialing: Mutex<HashSet<String>>,
    stopped: AtomicBool,
}

/// Stops listening on the relay when told to. Pipes already open stay up
/// until they drop.
pub struct Handle(Arc<Relay>);

impl Handle {
    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Relaxed);
    }
}

/// Keeps this device listening on the relay, and dials other computers there
/// that aren't connected directly.
pub fn start(hub: Arc<Hub>, url: String, key: &GroupKey, device: &str) -> Handle {
    let relay = Arc::new(Relay {
        hub,
        base: format!("{url}/v1/rooms/{}", relay_room(key)),
        device: device.to_owned(),
        dialing: Mutex::new(HashSet::new()),
        stopped: AtomicBool::new(false),
    });
    let listener = relay.clone();
    thread::spawn(move || listener.listen_forever());
    Handle(relay)
}

impl Relay {
    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    fn listen_forever(self: Arc<Self>) {
        let mut delay = Duration::from_secs(1);
        let mut reported = false;
        while !self.stopped() {
            let started = Instant::now();
            match self.listen() {
                Ok(()) => {}
                Err(error) if !reported => {
                    eprintln!("relay unavailable, will keep retrying: {error:#}");
                    reported = true;
                }
                Err(_) => {}
            }
            if started.elapsed() > KEEPALIVE {
                delay = Duration::from_secs(1);
                reported = false;
            }
            // Sleeps in steps so stopping doesn't wait out a long retry.
            let until = Instant::now() + delay;
            while Instant::now() < until && !self.stopped() {
                thread::sleep(Duration::from_millis(250));
            }
            delay = (delay * 2).min(MAX_RETRY);
        }
    }

    fn listen(self: &Arc<Self>) -> Result<()> {
        let mut socket = connect(&format!("{}/listen?device={}", self.base, self.device))?;
        tcp(&mut socket).set_read_timeout(Some(Duration::from_secs(1)))?;
        eprintln!("connected to relay");
        let mut devices = Vec::new();
        let mut last_ping = Instant::now();
        let mut last_redial = Instant::now();
        loop {
            if self.stopped() {
                let _ = socket.close(None);
                let _ = socket.flush();
                eprintln!("stopped using the relay");
                return Ok(());
            }
            if last_ping.elapsed() > KEEPALIVE {
                socket.send(Message::text("ping"))?;
                last_ping = Instant::now();
            }
            // Retries pipes that dropped while both computers stayed listening.
            if last_redial.elapsed() > REDIAL {
                self.dial_missing(&devices);
                last_redial = Instant::now();
            }
            match socket.read() {
                Ok(Message::Text(text)) => match serde_json::from_str(&text) {
                    Ok(Event::Devices { devices: list }) => {
                        devices = list;
                        self.dial_missing(&devices);
                    }
                    Ok(Event::Pipe { pipe }) => {
                        let relay = self.clone();
                        thread::spawn(move || relay.accept(&pipe));
                    }
                    // "pong" and anything newer.
                    Err(_) => {}
                },
                Ok(Message::Close(_)) => bail!("relay closed the connection"),
                Ok(_) => {}
                Err(tungstenite::Error::Io(error)) if is_timeout(&error) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// The device with the lower ID dials, so a pair only opens one pipe.
    fn dial_missing(self: &Arc<Self>, devices: &[String]) {
        for device in devices {
            if *device <= self.device || self.hub.is_connected(device) {
                continue;
            }
            if !self.dialing.lock().unwrap().insert(device.clone()) {
                continue;
            }
            let (relay, device) = (self.clone(), device.clone());
            thread::spawn(move || {
                if let Err(error) = relay.dial(&device) {
                    eprintln!("relayed connection ended: {error:#}");
                }
                relay.dialing.lock().unwrap().remove(&device);
            });
        }
    }

    fn dial(&self, to: &str) -> Result<()> {
        let mut socket = connect(&format!(
            "{}/dial?device={}&to={to}",
            self.base, self.device
        ))?;
        tcp(&mut socket).set_read_timeout(Some(CONNECT_TIMEOUT))?;
        // The relay says "ready" once the other computer has joined the pipe.
        loop {
            match socket.read()? {
                Message::Text(text) if text.as_str() == "ready" => break,
                Message::Close(_) => bail!("the other device didn't answer"),
                _ => {}
            }
        }
        self.hub.run_connection(bridge(socket)?, Role::Initiator)
    }

    fn accept(&self, pipe: &str) {
        let result = connect(&format!("{}/accept?pipe={pipe}", self.base))
            .and_then(bridge)
            .and_then(|stream| self.hub.run_connection(stream, Role::Responder));
        if let Err(error) = result {
            eprintln!("relayed connection ended: {error:#}");
        }
    }
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn connect(url: &str) -> Result<Socket> {
    let uri: tungstenite::http::Uri = url.parse().context("bad relay URL")?;
    let host = uri.host().context("relay URL has no host")?;
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("wss") {
            443
        } else {
            80
        });
    let address = (host, port)
        .to_socket_addrs()?
        .next()
        .context("couldn't look up the relay")?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    stream.set_nodelay(true)?;
    let (socket, _) = tungstenite::client_tls(url, stream).map_err(|error| match error {
        tungstenite::HandshakeError::Failure(error) => anyhow::Error::from(error),
        tungstenite::HandshakeError::Interrupted(_) => anyhow::anyhow!("relay handshake timed out"),
    })?;
    Ok(socket)
}

fn tcp(socket: &mut Socket) -> &mut TcpStream {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream,
        MaybeTlsStream::Rustls(stream) => stream.get_mut(),
        _ => unreachable!("only plain and rustls streams are enabled"),
    }
}

/// Connects a relayed pipe to a local socket, so the hub handles it like any
/// other connection. Returns the hub's end.
fn bridge(mut socket: Socket) -> Result<TcpStream> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let hub_end = TcpStream::connect(listener.local_addr()?)?;
    let (bridge_end, from) = listener.accept()?;
    // Another local program could race to connect to the listener.
    ensure!(from == hub_end.local_addr()?, "unexpected local connection");
    tcp(&mut socket).set_read_timeout(Some(POLL))?;
    bridge_end.set_nonblocking(true)?;
    thread::spawn(move || pump(socket, bridge_end));
    Ok(hub_end)
}

fn pump(mut socket: Socket, mut local: TcpStream) {
    let mut buffer = vec![0; CHUNK];
    let result: Result<()> = (|| loop {
        match local.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => socket.send(Message::binary(buffer[..n].to_vec()))?,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => write_all(&mut local, &bytes)?,
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error)) if is_timeout(&error) => {}
            Err(error) => return Err(error.into()),
        }
    })();
    let _ = result;
    let _ = socket.close(None);
    let _ = socket.flush();
    let _ = local.shutdown(Shutdown::Both);
}

/// `local` is non-blocking, so a full buffer means waiting for the hub to read.
fn write_all(local: &mut TcpStream, mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        match local.write(bytes) {
            Ok(0) => bail!("local connection closed"),
            Ok(n) => bytes = &bytes[n..],
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
