//! Linux desktop integration, separate from clipboard transport.
use crate::window::UserEvent;
use anyhow::{Context, Result};
use dbus::blocking::Connection;
use dbus::channel::{MatchingReceiver, Sender};
use dbus::message::MatchRule;
use dbus_crossroads::Crossroads;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
struct Item {
    activate: Box<dyn Fn(i32, i32) + Send>,
    tooltip: Arc<Mutex<String>>,
    icons: Pixmaps,
}

/// StatusNotifierItem gives us actual Activate/ContextMenu events. The old
/// AppIndicator tray adapter only exposed a menu on Linux.
pub struct Tray {
    tooltip: Arc<Mutex<String>>,
    dirty: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Tray {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let connection =
            Connection::new_session().context("couldn't connect to the desktop session bus")?;
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        connection.request_name(&name, false, true, false)?;
        let tooltip = Arc::new(Mutex::new("Carryover".into()));
        let stopped = Arc::new(AtomicBool::new(false));
        let dirty = Arc::new(AtomicBool::new(false));
        // Use the app's orange icon so it is distinct from KDE's clipboard manager.
        let image = image::load_from_memory(include_bytes!("../../assets/icon.png"))?
            .resize(64, 64, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let (width, height) = image.dimensions();
        let pixels = image
            .pixels()
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
            .collect();
        let item = Item {
            activate: Box::new(move |x, y| {
                let _ = proxy.send_event(UserEvent::TrayActivate { x, y });
            }),
            tooltip: tooltip.clone(),
            icons: vec![(width as i32, height as i32, pixels)],
        };
        let stop = stopped.clone();
        let changed = dirty.clone();
        let thread = std::thread::spawn(move || {
            let mut crossroads = Crossroads::new();
            let interface = crossroads.register(
                "org.kde.StatusNotifierItem",
                |b: &mut dbus_crossroads::IfaceBuilder<Item>| {
                    b.property::<String, _>("Category")
                        .get(|_, _| Ok("ApplicationStatus".into()));
                    b.property::<String, _>("Id")
                        .get(|_, _| Ok("carryover".into()));
                    b.property::<String, _>("Title")
                        .get(|_, _| Ok("Carryover".into()));
                    b.property::<String, _>("Status")
                        .get(|_, _| Ok("Active".into()));
                    b.property::<i32, _>("WindowId").get(|_, _| Ok(0));
                    b.property::<bool, _>("ItemIsMenu").get(|_, _| Ok(false));
                    b.property::<dbus::Path<'static>, _>("Menu")
                        .get(|_, _| Ok(dbus::Path::from("/")));
                    b.property::<String, _>("IconName")
                        .get(|_, _| Ok(String::new()));
                    b.property::<String, _>("IconThemePath")
                        .get(|_, _| Ok(String::new()));
                    b.property::<Pixmaps, _>("IconPixmap")
                        .get(|_, item| Ok(item.icons.clone()));
                    b.property::<String, _>("AttentionIconName")
                        .get(|_, _| Ok(String::new()));
                    b.property::<Pixmaps, _>("AttentionIconPixmap")
                        .get(|_, _| Ok(Vec::new()));
                    b.property::<String, _>("OverlayIconName")
                        .get(|_, _| Ok(String::new()));
                    b.property::<Pixmaps, _>("OverlayIconPixmap")
                        .get(|_, _| Ok(Vec::new()));
                    b.property::<(String, Pixmaps, String, String), _>("ToolTip")
                        .get(|_, item| {
                            Ok((
                                String::new(),
                                item.icons.clone(),
                                "Carryover".into(),
                                item.tooltip.lock().unwrap().clone(),
                            ))
                        });
                    for method in ["Activate", "SecondaryActivate", "ContextMenu"] {
                        b.method(method, ("x", "y"), (), |_, item, (x, y): (i32, i32)| {
                            (item.activate)(x, y);
                            Ok(())
                        });
                    }
                    b.method(
                        "Scroll",
                        ("delta", "orientation"),
                        (),
                        |_, _, _: (i32, String)| Ok(()),
                    );
                    b.signal::<(), _>("NewToolTip", ());
                },
            );
            crossroads.insert("/StatusNotifierItem", &[interface], item);
            connection.start_receive(
                MatchRule::new_method_call(),
                Box::new(move |message, connection| {
                    let _ = crossroads.handle_message(message, connection);
                    true
                }),
            );
            let mut watcher = String::new();
            let mut next_register = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                if Instant::now() >= next_register {
                    let bus = connection.with_proxy(
                        "org.freedesktop.DBus",
                        "/org/freedesktop/DBus",
                        Duration::from_millis(500),
                    );
                    let owner: Result<(String,), _> = bus.method_call(
                        "org.freedesktop.DBus",
                        "GetNameOwner",
                        ("org.kde.StatusNotifierWatcher",),
                    );
                    let owner = owner.map(|(name,)| name).unwrap_or_default();
                    if owner != watcher {
                        if owner.is_empty() {
                            watcher.clear();
                        } else {
                            let proxy = connection.with_proxy(
                                "org.kde.StatusNotifierWatcher",
                                "/StatusNotifierWatcher",
                                Duration::from_millis(500),
                            );
                            let result: Result<(), _> = proxy.method_call(
                                "org.kde.StatusNotifierWatcher",
                                "RegisterStatusNotifierItem",
                                (&name,),
                            );
                            if result.is_ok() {
                                watcher = owner;
                            }
                        }
                    }
                    next_register = Instant::now() + Duration::from_secs(2);
                }
                if changed.swap(false, Ordering::Relaxed) {
                    let signal = dbus::Message::new_signal(
                        "/StatusNotifierItem",
                        "org.kde.StatusNotifierItem",
                        "NewToolTip",
                    )
                    .unwrap();
                    let _ = connection.send(signal);
                }
                if let Err(error) = connection.process(Duration::from_millis(100)) {
                    eprintln!("Linux tray disconnected: {error}");
                    break;
                }
            }
        });
        Ok(Self {
            tooltip,
            stopped,
            dirty,
            thread: Some(thread),
        })
    }
    pub fn set_tooltip(&self, value: Option<String>) -> Result<()> {
        *self.tooltip.lock().unwrap() = value.unwrap_or_default();
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
