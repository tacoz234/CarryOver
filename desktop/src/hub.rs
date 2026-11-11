//! Moves clips between the local clipboard and connected peers.
use crate::clipboard::{Clipboard, Content};
use crate::history::{Direction, History};
use crate::protocol::{
    GroupKey, MAX_IMAGE, MAX_TEXT, Message, Opener, Presence, Role, Sealer, handshake,
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const PING_INTERVAL: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RETRY: Duration = Duration::from_secs(10);
const SEEN_LIMIT: usize = 512;

type Hash = [u8; 32];

fn too_large(content: &Content) -> bool {
    match content {
        Content::Text(text) => text.len() > MAX_TEXT,
        Content::Image(png) => png.len() > MAX_IMAGE,
    }
}

fn to_message(id: String, content: Content) -> Message {
    match content {
        Content::Text(text) => Message::Clip { id, text },
        Content::Image(png) => Message::Image { id, png },
    }
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
    peers: HashMap<u64, Peer>,
    last_activity: Option<Activity>,
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
                let _ = peer.sender.send(message.clone());
            }
        }
    }
}

struct Peer {
    device: String,
    name: String,
    sender: Sender<Message>,
    /// The devices this peer last said it knows about.
    reported: Vec<Presence>,
}

/// The last clip sent or received, for the tray menu.
#[derive(Clone, Debug)]
pub struct Activity {
    pub description: String,
    pub at: Instant,
}

/// A snapshot for the tray menu and window.
pub struct Status {
    /// Names of connected devices, without duplicates.
    pub connected: Vec<String>,
    pub paused: bool,
    pub last_activity: Option<Activity>,
    /// Every device that has connected, newest first.
    pub devices: Vec<DeviceStatus>,
}

pub struct DeviceStatus {
    pub id: String,
    pub name: String,
    /// Connected here, or to a device connected here.
    pub connected: bool,
    /// When connected only through another device, that device's name.
    pub via: Option<String>,
    /// Seconds since the Unix epoch when it was last connected.
    pub last_seen: u64,
}

/// A device that has connected before, kept so the window can list devices
/// that are away, like an iPhone between opens.
#[derive(Clone, Serialize, Deserialize)]
struct Known {
    name: String,
    last_seen: u64,
}

#[derive(Default)]
struct Devices {
    known: BTreeMap<String, Known>,
    /// Where to save `known`. Unset in tests and plain `run`.
    path: Option<PathBuf>,
}

impl Devices {
    fn saw(&mut self, device: &str, name: &str) {
        self.known.insert(
            device.to_owned(),
            Known {
                name: name.to_owned(),
                last_seen: unix_now(),
            },
        );
        self.save();
    }

    /// Learns about devices from a peer's report. Doesn't overwrite what
    /// this device saw itself with older news.
    fn merge(&mut self, reports: &[Presence], own: &str) {
        let mut changed = false;
        for report in reports.iter().filter(|report| report.device != own) {
            let known = self.known.entry(report.device.clone()).or_insert_with(|| {
                changed = true;
                Known {
                    name: report.name.clone(),
                    last_seen: 0,
                }
            });
            if report.last_seen > known.last_seen {
                known.last_seen = report.last_seen;
                known.name = report.name.clone();
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }

    fn save(&self) {
        if let Some(path) = &self.path {
            let result = serde_json::to_vec_pretty(&self.known)
                .map_err(anyhow::Error::from)
                .and_then(|json| Ok(std::fs::write(path, json)?));
            if let Err(error) = result {
                eprintln!("couldn't save the device list: {error:#}");
            }
        }
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

pub struct Hub {
    key: GroupKey,
    device: String,
    name: String,
    clipboard: Arc<dyn Clipboard>,
    state: Mutex<State>,
    paused: AtomicBool,
    devices: Mutex<Devices>,
    pub history: Mutex<History>,
}

impl Hub {
    pub fn new(
        key: GroupKey,
        device: String,
        name: String,
        clipboard: Arc<dyn Clipboard>,
    ) -> Arc<Self> {
        Arc::new(Self {
            key,
            device,
            name,
            clipboard,
            state: Mutex::new(State::default()),
            paused: AtomicBool::new(false),
            devices: Mutex::new(Devices::default()),
            history: Mutex::new(History::default()),
        })
    }

    /// While paused, copies here aren't sent and clips from others aren't applied.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        eprintln!("{}", if paused { "paused" } else { "resumed" });
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Loads and keeps saving the list of devices seen, at `path`.
    pub fn remember_devices(&self, path: PathBuf) {
        let known = std::fs::read(&path)
            .ok()
            .and_then(|json| serde_json::from_slice(&json).ok())
            .unwrap_or_default();
        *self.devices.lock().unwrap() = Devices {
            known,
            path: Some(path),
        };
    }

    /// Stops syncing and drops the devices and history it knew, without
    /// saving anything more to disk.
    pub fn leave(&self) {
        self.set_paused(true);
        *self.devices.lock().unwrap() = Devices::default();
        *self.history.lock().unwrap() = History::default();
    }

    /// Loads and keeps saving the clip history in `dir`.
    pub fn keep_history(&self, dir: PathBuf) {
        *self.history.lock().unwrap() = History::open(dir);
    }

    /// Puts a clip from the history back on the clipboard. It isn't sent
    /// again, since the other devices had it already.
    pub fn copy_again(&self, id: &str) -> Result<()> {
        let content = self.history.lock().unwrap().content(id)?;
        // Marked as already shared, so the watcher doesn't send it.
        self.state.lock().unwrap().last_hash = Some(content.digest());
        self.clipboard.write(&content)
    }

    /// Drops a device from the list. It reappears if it connects again.
    pub fn forget(&self, device: &str) {
        let mut devices = self.devices.lock().unwrap();
        devices.known.remove(device);
        devices.save();
    }

    /// What this device knows, as sent to peers.
    fn presence(&self, state: &State) -> Vec<Presence> {
        let live: HashSet<&str> = state
            .peers
            .values()
            .map(|peer| peer.device.as_str())
            .collect();
        self.devices
            .lock()
            .unwrap()
            .known
            .iter()
            .map(|(id, known)| Presence {
                device: id.clone(),
                name: known.name.clone(),
                connected: live.contains(id.as_str()),
                last_seen: known.last_seen,
            })
            .collect()
    }

    /// Tells peers what changed, after a connection starts or ends.
    fn announce(&self, except: Option<u64>) {
        let state = self.state.lock().unwrap();
        let message = Message::Devices {
            devices: self.presence(&state),
        };
        state.broadcast(&message, except);
    }

    fn reported(&self, connection: u64, devices: Vec<Presence>) {
        let mut state = self.state.lock().unwrap();
        self.devices.lock().unwrap().merge(&devices, &self.device);
        if let Some(peer) = state.peers.get_mut(&connection) {
            peer.reported = devices;
        }
    }

    pub fn status(&self) -> Status {
        let state = self.state.lock().unwrap();
        let mut connected: Vec<String> =
            state.peers.values().map(|peer| peer.name.clone()).collect();
        connected.sort();
        connected.dedup();
        let live: HashSet<&str> = state
            .peers
            .values()
            .map(|peer| peer.device.as_str())
            .collect();
        let mut devices: Vec<DeviceStatus> = self
            .devices
            .lock()
            .unwrap()
            .known
            .iter()
            .map(|(id, known)| {
                let direct = live.contains(id.as_str());
                // Only direct reports count, so a device that left can't be
                // kept online by peers repeating each other.
                let via = (!direct)
                    .then(|| {
                        state.peers.values().find(|peer| {
                            peer.reported
                                .iter()
                                .any(|report| report.device == *id && report.connected)
                        })
                    })
                    .flatten()
                    .map(|peer| peer.name.clone());
                DeviceStatus {
                    id: id.clone(),
                    name: known.name.clone(),
                    connected: direct || via.is_some(),
                    via,
                    last_seen: known.last_seen,
                }
            })
            .collect();
        devices.sort_by_key(|device| std::cmp::Reverse((device.connected, device.last_seen)));
        Status {
            connected,
            paused: self.paused.load(Ordering::Relaxed),
            last_activity: state.last_activity.clone(),
            devices,
        }
    }

    pub fn peer_count(&self) -> usize {
        self.state.lock().unwrap().peers.len()
    }

    pub fn is_connected(&self, device: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .peers
            .values()
            .any(|peer| peer.device == device)
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
        if self.paused.load(Ordering::Relaxed) {
            return;
        }
        let content = match self.clipboard.read() {
            Ok(Some(content)) => content,
            Ok(None) => return,
            Err(error) => {
                eprintln!("could not read clipboard: {error:#}");
                return;
            }
        };
        if too_large(&content) {
            eprintln!(
                "skipped {}, which is over the size limit",
                content.describe()
            );
            return;
        }
        let digest = content.digest();
        let mut state = self.state.lock().unwrap();
        if state.last_hash == Some(digest) {
            return;
        }
        state.last_hash = Some(digest);
        let id = hex::encode(rand::random::<[u8; 16]>());
        state.mark_seen(&id);
        eprintln!(
            "sending {} to {} connection(s)",
            content.describe(),
            state.peers.len()
        );
        state.last_activity = Some(Activity {
            description: format!("Sent {}", content.kind()),
            at: Instant::now(),
        });
        let message = to_message(id.clone(), content.clone());
        state.broadcast(&message, None);
        state.last_clip = Some(message);
        drop(state);
        self.history
            .lock()
            .unwrap()
            .add(Direction::Sent, None, &id, &content);
    }

    fn remote_clip(&self, connection: u64, id: String, content: Content) {
        if too_large(&content) || self.paused.load(Ordering::Relaxed) {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if !state.mark_seen(&id) {
            return;
        }
        let digest = content.digest();
        let message = to_message(id.clone(), content.clone());
        // Forwarded so devices that can only reach each other through us still
        // sync. That includes content already on this clipboard: a device that
        // was away or restarting when it first went around never got it, and
        // the phone may have reached only this computer. The ID keeps it from
        // looping, and devices that already have it don't rewrite it.
        state.broadcast(&message, Some(connection));
        state.last_clip = Some(message);
        if state.last_hash == Some(digest) {
            // Already on this clipboard, e.g. the phone sending back something
            // it got through Universal Clipboard.
            return;
        }
        state.last_hash = Some(digest);
        let from = state
            .peers
            .get(&connection)
            .map(|peer| peer.name.clone())
            .unwrap_or_default();
        state.last_activity = Some(Activity {
            description: format!("Received {} from {from}", content.kind()),
            at: Instant::now(),
        });
        drop(state);
        self.history
            .lock()
            .unwrap()
            .add(Direction::Received, Some(from), &id, &content);
        eprintln!("received {}", content.describe());
        if let Err(error) = self.clipboard.write(&content) {
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
    /// Targets are `host:port`, looked up on each attempt.
    pub fn dial(self: &Arc<Self>, targets: Arc<Mutex<Vec<String>>>) {
        let hub = self.clone();
        thread::spawn(move || {
            let mut delay = Duration::from_millis(250);
            loop {
                let addresses = targets.lock().unwrap().clone();
                if addresses.is_empty() {
                    return;
                }
                let resolved = addresses
                    .iter()
                    .filter_map(|target| target.to_socket_addrs().ok())
                    .flatten();
                for address in resolved {
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

    /// Runs one connection until it ends.
    fn run_connection(&self, mut stream: TcpStream, role: Role) -> Result<()> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let (mut sealer, mut opener) = handshake(&mut stream, &self.key, role)?;
        let devices = self.presence(&self.state.lock().unwrap());
        sealer.send(
            &mut stream,
            &Message::Hello {
                device: self.device.clone(),
                name: self.name.clone(),
                devices,
            },
        )?;
        // Nothing is shared until the peer proves it holds the group key.
        let Message::Hello {
            device,
            name,
            devices: reported,
        } = opener.recv(&mut stream)?
        else {
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
            state.peers.insert(
                connection,
                Peer {
                    device: device.clone(),
                    name: name.clone(),
                    sender,
                    reported: Vec::new(),
                },
            );
            connection
        };
        eprintln!("connected to {name}");
        self.devices.lock().unwrap().saw(&device, &name);
        self.reported(connection, reported);
        // Includes the new peer: its hello was built before it was added
        // here, so a change in between would otherwise never reach it.
        self.announce(None);

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
        self.devices.lock().unwrap().saw(&device, &name);
        self.announce(None);
        let _ = stream.shutdown(Shutdown::Both);
        let _ = writer.join();
        eprintln!("disconnected from {name}");
        result
    }

    fn read_loop(
        &self,
        stream: &mut TcpStream,
        opener: &mut Opener,
        connection: u64,
    ) -> Result<()> {
        loop {
            match opener.recv(stream)? {
                Message::Clip { id, text } => self.remote_clip(connection, id, Content::Text(text)),
                Message::Image { id, png } => self.remote_clip(connection, id, Content::Image(png)),
                Message::Latest => {
                    let state = self.state.lock().unwrap();
                    if let (Some(clip), Some(peer)) =
                        (&state.last_clip, state.peers.get(&connection))
                    {
                        let _ = peer.sender.send(clip.clone());
                    }
                }
                Message::Devices { devices } => self.reported(connection, devices),
                Message::Ping | Message::Unknown => {}
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
