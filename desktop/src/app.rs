//! What the tray and window control: settings that apply while running, and
//! the snapshot the window shows.
use crate::config::{self, Config};
use crate::hub::{Hub, unix_now};
use crate::protocol::GroupKey;
use crate::update::{self, State, Updater};
use crate::{autostart, relay};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Targets = Arc<Mutex<Vec<String>>>;

pub struct App {
    pub hub: Arc<Hub>,
    pub updater: Arc<Updater>,
    pub log: PathBuf,
    dir: PathBuf,
    key: GroupKey,
    config: Mutex<Config>,
    relay: Mutex<Option<relay::Handle>>,
    /// Saved peers being dialed. Emptying a list stops its dialer.
    peers: Mutex<HashMap<String, Targets>>,
}

/// A request from the window.
#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Request {
    Ready,
    Pause { on: bool },
    Login { on: bool },
    AutoUpdate { on: bool },
    Relay { url: String },
    AddPeer { address: String },
    RemovePeer { address: String },
    Forget { device: String },
    OpenLog,
    CheckUpdate,
    InstallUpdate,
}

impl App {
    /// Starts dialing saved peers and the saved relay, unless `relay`
    /// overrides it, and checking for updates.
    pub fn start(
        hub: Arc<Hub>,
        dir: PathBuf,
        config: Config,
        relay: Option<String>,
        log: PathBuf,
    ) -> Result<Arc<Self>> {
        let key = config.key()?;
        let updater = Updater::start(config.auto_update);
        let app = Arc::new(Self {
            hub,
            updater,
            log,
            key,
            relay: Mutex::new(None),
            peers: Mutex::new(HashMap::new()),
            config: Mutex::new(config),
            dir,
        });
        let peers = app.config.lock().unwrap().peers.clone();
        for peer in peers {
            app.dial(&peer);
        }
        app.use_relay(relay);
        Ok(app)
    }

    fn dial(&self, peer: &str) {
        let targets = Arc::new(Mutex::new(vec![peer.to_owned()]));
        self.peers
            .lock()
            .unwrap()
            .insert(peer.to_owned(), targets.clone());
        self.hub.dial(targets);
    }

    fn use_relay(&self, url: Option<String>) {
        let mut relay = self.relay.lock().unwrap();
        if let Some(old) = relay.take() {
            old.stop();
        }
        if let Some(url) = url {
            let device = self.config.lock().unwrap().device.clone();
            *relay = Some(relay::start(self.hub.clone(), url, &self.key, &device));
        }
    }

    pub fn invite_link(&self) -> String {
        self.config.lock().unwrap().invite_link()
    }

    fn save(&self, change: impl FnOnce(&mut Config)) -> Result<()> {
        let mut config = self.config.lock().unwrap();
        change(&mut config);
        config.update(&self.dir)
    }

    pub fn handle(&self, request: Request) -> Result<()> {
        match request {
            Request::Ready => {}
            Request::Pause { on } => self.hub.set_paused(on),
            Request::Login { on } => autostart::set_enabled(on)?,
            Request::AutoUpdate { on } => {
                self.save(|config| config.auto_update = on)?;
                self.updater.set_automatic(on);
            }
            Request::Relay { url } => {
                let url = match url.trim() {
                    "" => None,
                    url => Some(config::normalize_relay(url)?),
                };
                self.save(|config| config.relay = url.clone())?;
                self.use_relay(url);
            }
            Request::AddPeer { address } => {
                let address = config::normalize_peer(&address)?;
                if self.config.lock().unwrap().peers.contains(&address) {
                    bail!("{address} is already added");
                }
                self.save(|config| config.peers.push(address.clone()))?;
                self.dial(&address);
            }
            Request::RemovePeer { address } => {
                self.save(|config| config.peers.retain(|peer| *peer != address))?;
                if let Some(targets) = self.peers.lock().unwrap().remove(&address) {
                    targets.lock().unwrap().clear();
                }
            }
            Request::Forget { device } => self.hub.forget(&device),
            Request::OpenLog => open(&self.log)?,
            Request::CheckUpdate => self.updater.check_now(),
            Request::InstallUpdate => self.updater.install_now(),
        }
        Ok(())
    }

    /// Everything the window shows, except the invite.
    pub fn snapshot(&self) -> Value {
        let status = self.hub.status();
        let config = self.config.lock().unwrap();
        let now = unix_now();
        let devices: Vec<Value> = status
            .devices
            .iter()
            .map(|device| {
                let seen = if device.connected {
                    "Connected".to_owned()
                } else {
                    format!(
                        "Last seen {}",
                        ago(Duration::from_secs(now.saturating_sub(device.last_seen)))
                    )
                };
                json!({
                    "id": device.id,
                    "name": device.name,
                    "connected": device.connected,
                    "seen": seen,
                })
            })
            .collect();
        let update = match self.updater.state() {
            State::Idle => json!({"state": "idle"}),
            State::Checking => json!({"state": "checking"}),
            State::UpToDate => json!({"state": "current"}),
            State::Available(release) => {
                json!({"state": "available", "version": release.version, "notes": release.notes})
            }
            State::Installing(version) => json!({"state": "installing", "version": version}),
            State::Ready(version) => json!({"state": "ready", "version": version}),
            State::Failed(message) => json!({"state": "failed", "message": message}),
        };
        json!({
            "version": update::VERSION,
            "name": self.hub.name(),
            "paused": status.paused,
            "login": autostart::is_enabled(),
            "autoUpdate": config.auto_update,
            "relay": config.relay.as_deref().map(https).unwrap_or_default(),
            "peers": config.peers,
            "devices": devices,
            "activity": status.last_activity.map(|activity| {
                format!("{} · {}", activity.description, ago(activity.at.elapsed()))
            }),
            "update": update,
        })
    }
}

/// Relays are saved in WebSocket form but shown as web addresses.
fn https(relay: &str) -> String {
    relay
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
}

pub fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{} min ago", elapsed.as_secs() / 60),
        3600..86400 => format!("{} hr ago", elapsed.as_secs() / 3600),
        _ => match elapsed.as_secs() / 86400 {
            1 => "yesterday".to_owned(),
            days => format!("{days} days ago"),
        },
    }
}

/// Opens a file in its default app.
pub fn open(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(windows)]
    let program = "explorer.exe";
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .with_context(|| format!("couldn't open {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_naturally() {
        assert_eq!(ago(Duration::from_secs(5)), "just now");
        assert_eq!(ago(Duration::from_secs(150)), "2 min ago");
        assert_eq!(ago(Duration::from_secs(7300)), "2 hr ago");
        assert_eq!(ago(Duration::from_secs(90_000)), "yesterday");
        assert_eq!(ago(Duration::from_secs(3 * 86400)), "3 days ago");
    }

    #[test]
    fn settings_apply_and_save() {
        let dir = std::env::temp_dir().join(format!("copyblip-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = Config::new(&[3; 32]);
        config.save(&dir, false).unwrap();
        let clipboard = Arc::new(crate::clipboard::MemoryClipboard::default());
        let hub = Hub::new([3; 32], config.device.clone(), "test".into(), clipboard);
        let app = App::start(hub, dir.clone(), config, None, dir.join("log")).unwrap();

        app.handle(Request::AddPeer {
            address: "127.0.0.1:9".into(),
        })
        .unwrap();
        assert!(
            app.handle(Request::AddPeer {
                address: "127.0.0.1:9".into()
            })
            .is_err()
        );
        assert!(
            app.handle(Request::Relay {
                url: "not a url".into()
            })
            .is_err()
        );
        app.handle(Request::Relay {
            url: "https://relay.example.dev/".into(),
        })
        .unwrap();
        app.handle(Request::AutoUpdate { on: false }).unwrap();
        app.handle(Request::Pause { on: true }).unwrap();

        let saved = Config::load(&dir).unwrap();
        assert_eq!(saved.peers, ["127.0.0.1:9"]);
        assert_eq!(saved.relay.as_deref(), Some("wss://relay.example.dev"));
        assert!(!saved.auto_update);
        let snapshot = app.snapshot();
        assert_eq!(snapshot["relay"], "https://relay.example.dev");
        assert_eq!(snapshot["paused"], true);

        app.handle(Request::RemovePeer {
            address: "127.0.0.1:9".into(),
        })
        .unwrap();
        app.handle(Request::Relay { url: " ".into() }).unwrap();
        let saved = Config::load(&dir).unwrap();
        assert!(saved.peers.is_empty() && saved.relay.is_none());
        // The removed peer's dialer was told to stop.
        assert!(app.peers.lock().unwrap().is_empty());
        assert!(app.relay.lock().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn window_requests_parse() {
        let request: Request =
            serde_json::from_str(r#"{"cmd":"addPeer","address":"macmini"}"#).unwrap();
        assert!(matches!(request, Request::AddPeer { address } if address == "macmini"));
        let request: Request = serde_json::from_str(r#"{"cmd":"pause","on":true}"#).unwrap();
        assert!(matches!(request, Request::Pause { on: true }));
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"rm -rf"}"#).is_err());
    }
}
