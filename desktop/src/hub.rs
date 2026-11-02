//! Moves clips between the local clipboard and connected peers.
use crate::clipboard::Clipboard;
use crate::protocol::{GroupKey, MAX_TEXT, Message, Opener, Role, Sealer, handshake};
use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const PING_INTERVAL: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RETRY: Duration = Duration::from_secs(10);
const SEEN_LIMIT: usize = 512;

type Hash = [u8; 32];

fn hash(text: &str) -> Hash {
    Sha256::digest(text.as_bytes()).into()
}

#[derive(Default)]
struct State {
    seen: HashSet<String>,
    seen_order: VecDeque<String>,
    /// The clipboard contents we last sent or applied, so applying a clip
    /// doesn't look like a new copy to the watcher.
    last_hash: Option<Hash>,
    /// Answers `Latest` requests.
    last_clip: Option<Message>,
    peers: HashMap<u64, Sender<Message>>,
    next_connection: u64,
}

impl State {
    /// Returns false if this clip was already handled.
    fn mark_seen(&mut self, id: &str) -> bool {
        if !self.seen.insert(id.to_owned()) {
            return false;
        }
        self.seen_order.push_back(id.to_owned());
        if self.seen_order.len() > SEEN_LIMIT
            && let Some(oldest) = self.seen_order.pop_front()
        {
            self.seen.remove(&oldest);
        }
        true
    }

    fn broadcast(&self, message: &Message, except: Option<u64>) {
        for (connection, peer) in &self.peers {
            if Some(*connection) != except {
                let _ = peer.send(message.clone());
            }
        }
    }
}

pub struct Hub {
    key: GroupKey,
    device: String,
    name: String,
    clipboard: Arc<dyn Clipboard>,
    state: Mutex<State>,
}

impl Hub {
    pub fn new(key: GroupKey, device: String, name: String, clipboard: Arc<dyn Clipboard>) -> Arc<Self> {
        Arc::new(Self {
            key,
            device,
            name,
            clipboard,
            state: Mutex::new(State::default()),
        })
    }

    pub fn peer_count(&self) -> usize {
        self.state.lock().unwrap().peers.len()
    }

    /// Only changes after startup are sent. Syncing on launch or connect would
    /// overwrite clipboards on other devices with stale content.
    pub fn watch_clipboard(self: &Arc<Self>) {
        let hub = self.clone();
        thread::spawn(move || {
            let mut token = hub.clipboard.change_token();
            loop {
                thread::sleep(POLL_INTERVAL);
                let current = hub.clipboard.change_token();
                if current != token {
                    token = current;
                    hub.local_change();
                }
            }
        });
    }

    fn local_change(&self) {
        let text = match self.clipboard.read_text() {
            Ok(Some(text)) => text,
            Ok(None) => return,
            Err(error) => {
                eprintln!("could not read clipboard: {error:#}");
                return;
            }
        };
        if text.len() > MAX_TEXT {
            eprintln!("skipped a {} byte copy, the limit is {MAX_TEXT}", text.len());
            return;
        }
        let digest = hash(&text);
        let mut state = self.state.lock().unwrap();
        if state.last_hash == Some(digest) {
            return;
        }
        state.last_hash = Some(digest);
        let id = hex::encode(rand::random::<[u8; 16]>());
        state.mark_seen(&id);
        eprintln!("sending {} bytes to {} connection(s)", text.len(), state.peers.len());
        let message = Message::Clip { id, text };
        state.broadcast(&message, None);
        state.last_clip = Some(message);
    }

    fn remote_clip(&self, connection: u64, id: String, text: String) {
        if text.len() > MAX_TEXT {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if !state.mark_seen(&id) {
            return;
        }
        let digest = hash(&text);
        let message = Message::Clip { id, text };
        if state.last_hash == Some(digest) {
            // Already on this clipboard, e.g. the phone sending back something it
            // got through Universal Clipboard. Every device that set last_hash
            // also shared it, so there's nothing to write or forward.
            state.last_clip = Some(message);
            return;
        }
        state.last_hash = Some(digest);
        // Forwarded so devices that can only reach each other through us still sync.
        state.broadcast(&message, Some(connection));
        state.last_clip = Some(message.clone());
        drop(state);
        let Message::Clip { text, .. } = &message else {
            unreachable!()
        };
        eprintln!("received {} bytes", text.len());
        if let Err(error) = self.clipboard.write_text(text) {
            eprintln!("could not write clipboard: {error:#}");
        }
    }

    pub fn serve(self: &Arc<Self>, listener: TcpListener) {
        let hub = self.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let hub = hub.clone();
                thread::spawn(move || {
                    let peer = stream.peer_addr();
                    if let Err(error) = hub.run_connection(stream, Role::Responder) {
                        eprintln!("connection from {peer:?} ended: {error:#}");
                    }
                });
            }
        });
    }

    /// Keeps a connection to `targets` up, retrying until the list is emptied.
    pub fn dial(self: &Arc<Self>, targets: Arc<Mutex<Vec<SocketAddr>>>) {
        let hub = self.clone();
        thread::spawn(move || {
            let mut delay = Duration::from_millis(250);
            loop {
                let addresses = targets.lock().unwrap().clone();
                if addresses.is_empty() {
                    return;
                }
                for address in addresses {
                    if let Ok(stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                        delay = Duration::from_millis(250);
                        if let Err(error) = hub.run_connection(stream, Role::Initiator) {
                            eprintln!("connection to {address} ended: {error:#}");
                        }
                        break;
                    }
                }
                thread::sleep(delay);
                delay = (delay * 2).min(MAX_RETRY);
            }
        });
    }

    fn run_connection(&self, mut stream: TcpStream, role: Role) -> Result<()> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let (mut sealer, mut opener) = handshake(&mut stream, &self.key, role)?;
        sealer.send(
            &mut stream,
            &Message::Hello {
                device: self.device.clone(),
                name: self.name.clone(),
            },
        )?;
        // Nothing is shared until the peer proves it holds the group key.
        let Message::Hello { device, name } = opener.recv(&mut stream)? else {
            bail!("peer skipped its hello");
        };
        if device == self.device {
            return Ok(());
        }
        stream.set_read_timeout(Some(READ_TIMEOUT))?;

        let (sender, receiver) = mpsc::channel();
        let connection = {
            let mut state = self.state.lock().unwrap();
            let connection = state.next_connection;
            state.next_connection += 1;
            state.peers.insert(connection, sender);
            connection
        };
        eprintln!("connected to {name}");

        let writer_stream = stream.try_clone()?;
        let writer = thread::spawn(move || write_loop(writer_stream, sealer, receiver));
        let result = match self.read_loop(&mut stream, &mut opener, connection) {
            // The peer hung up between frames, which is how phones finish.
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::UnexpectedEof) =>
            {
                Ok(())
            }
            other => other,
        };

        self.state.lock().unwrap().peers.remove(&connection);
        let _ = stream.shutdown(Shutdown::Both);
        let _ = writer.join();
        eprintln!("disconnected from {name}");
        result
    }

    fn read_loop(&self, stream: &mut TcpStream, opener: &mut Opener, connection: u64) -> Result<()> {
        loop {
            match opener.recv(stream)? {
                Message::Clip { id, text } => self.remote_clip(connection, id, text),
                Message::Latest => {
                    let state = self.state.lock().unwrap();
                    if let (Some(clip), Some(peer)) = (&state.last_clip, state.peers.get(&connection)) {
                        let _ = peer.send(clip.clone());
                    }
                }
                Message::Ping => {}
                Message::Hello { .. } => bail!("unexpected hello"),
            }
        }
    }
}

fn write_loop(mut stream: TcpStream, mut sealer: Sealer, messages: mpsc::Receiver<Message>) {
    loop {
        let message = match messages.recv_timeout(PING_INTERVAL) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => Message::Ping,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if sealer.send(&mut stream, &message).is_err() {
            let _ = stream.shutdown(Shutdown::Both);
            return;
        }
    }
}
