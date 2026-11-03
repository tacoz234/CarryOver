//! What the tray and window control: settings that apply while running, and
//! the snapshot the window shows.
use crate::autostart;
use crate::config::Config;
use crate::hub::{Hub, unix_now};
use crate::update::{self, State, Updater};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct App {
    pub hub: Arc<Hub>,
    pub updater: Arc<Updater>,
    pub log: PathBuf,
    dir: PathBuf,
    config: Mutex<Config>,
    /// Whether Carryover opens at login. Checking costs a process launch on
    /// Windows, too slow for every snapshot, so it's read once and kept here.
    login: AtomicBool,
}

/// A request from the window.
#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Request {
    Ready,
    Pause { on: bool },
    Login { on: bool },
    AutoUpdate { on: bool },
    Forget { device: String },
    CopyClip { id: String },
    DeleteClip { id: String },
    ClearHistory,
    OpenLog,
    CheckUpdate,
    InstallUpdate,
}

impl App {
    /// Starts checking for updates.
    pub fn start(hub: Arc<Hub>, dir: PathBuf, config: Config, log: PathBuf) -> Result<Arc<Self>> {
        let updater = Updater::start(config.auto_update);
        Ok(Arc::new(Self {
            hub,
            updater,
            log,
            config: Mutex::new(config),
            dir,
            login: AtomicBool::new(autostart::is_enabled()),
        }))
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
            Request::Login { on } => {
                autostart::set_enabled(on)?;
                self.login.store(on, Ordering::Relaxed);
            }
            Request::AutoUpdate { on } => {
                self.save(|config| config.auto_update = on)?;
                self.updater.set_automatic(on);
            }
            Request::Forget { device } => self.hub.forget(&device),
            Request::CopyClip { id } => self.hub.copy_again(&id)?,
            Request::DeleteClip { id } => self.hub.history.lock().unwrap().remove(&id),
            Request::ClearHistory => self.hub.history.lock().unwrap().clear(),
            Request::OpenLog => open(&self.log)?,
            Request::CheckUpdate => self.updater.check_now(),
            Request::InstallUpdate => self.updater.install_now(),
        }
        Ok(())
    }

    /// The clip history for the window. Sent only when it changes, since
    /// thumbnails make it large.
    pub fn history(&self) -> Value {
        self.hub.history.lock().unwrap().to_json()
    }

    /// Everything the window shows, except the invite and the history.
    pub fn snapshot(&self) -> Value {
        let status = self.hub.status();
        let config = self.config.lock().unwrap();
        let now = unix_now();
        let devices: Vec<Value> = status
            .devices
            .iter()
            .map(|device| {
                // Zero means it was only heard of, never seen connected.
                let ago_text = (device.last_seen > 0)
                    .then(|| ago(Duration::from_secs(now.saturating_sub(device.last_seen))));
                json!({
                    "id": device.id,
                    "name": device.name,
                    "connected": device.connected,
                    "via": device.via,
                    "ago": ago_text,
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
            "login": self.login.load(Ordering::Relaxed),
            "autoUpdate": config.auto_update,
            "devices": devices,
            "activity": status.last_activity.map(|activity| {
                format!("{} · {}", activity.description, ago(activity.at.elapsed()))
            }),
            "update": update,
            "historyVersion": self.hub.history.lock().unwrap().version,
        })
    }
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
        let app = App::start(hub, dir.clone(), config, dir.join("log")).unwrap();

        app.handle(Request::AutoUpdate { on: false }).unwrap();
        app.handle(Request::Pause { on: true }).unwrap();

        let saved = Config::load(&dir).unwrap();
        assert!(!saved.auto_update);
        assert_eq!(app.snapshot()["paused"], true);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn window_requests_parse() {
        let request: Request = serde_json::from_str(r#"{"cmd":"forget","device":"abc"}"#).unwrap();
        assert!(matches!(request, Request::Forget { device } if device == "abc"));
        let request: Request = serde_json::from_str(r#"{"cmd":"pause","on":true}"#).unwrap();
        assert!(matches!(request, Request::Pause { on: true }));
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"rm -rf"}"#).is_err());
    }
}
