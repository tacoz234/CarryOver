//! Windows: the panel the notification area icon opens, in place of a menu,
//! like the Wi-Fi and volume flyouts. Unlike a menu it stays open while you
//! use it, and closes when you click elsewhere. Drawn from `panel.html`.
use crate::app::App;
use crate::window::UserEvent;
use anyhow::Result;
use serde_json::Value;
use std::time::{Duration, Instant};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{Window as TaoWindow, WindowBuilder, WindowId};
use wry::{WebView, WebViewBuilder};

const PAGE: &str = include_str!("panel.html");
const WIDTH: f64 = 340.0;
/// Space between the panel and the taskbar.
const GAP: f64 = 12.0;

pub struct Panel {
    window: TaoWindow,
    webview: WebView,
    shown: Option<Value>,
    /// The icon the panel opened from, in physical pixels.
    anchor: Option<tray_icon::Rect>,
    height: f64,
    shown_at: Instant,
    /// Whether the panel has come to the front since it was shown.
    was_in_front: bool,
}

impl Panel {
    pub fn new(target: &EventLoopWindowTarget<UserEvent>, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let height = 420.0;
        let window = WindowBuilder::new()
            .with_title("Carryover")
            .with_inner_size(LogicalSize::new(WIDTH, height))
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top(true)
            .with_visible(false)
            .with_skip_taskbar(true)
            // On by default, and it keeps a frame around the panel, which
            // shows as a white edge.
            .with_undecorated_shadow(false)
            .with_background_color(BACKGROUND)
            .build(target)?;
        round_corners(&window);
        let webview = WebViewBuilder::new()
            .with_html(PAGE)
            .with_background_color(BACKGROUND)
            .with_devtools(cfg!(debug_assertions))
            .with_ipc_handler(move |request| {
                let _ = proxy.send_event(UserEvent::Panel(request.into_body()));
            })
            .build(&window)?;
        Ok(Self {
            window,
            webview,
            shown: None,
            anchor: None,
            height,
            shown_at: Instant::now(),
            was_in_front: false,
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    /// Shows the panel beside the icon at `anchor`.
    pub fn show_at(&mut self, anchor: tray_icon::Rect, app: &App) {
        self.anchor = Some(anchor);
        self.shown = None;
        self.refresh(app);
        self.place();
        self.window.set_visible(true);
        self.window.set_focus();
        bring_to_front(&self.window);
        self.shown_at = Instant::now();
        self.was_in_front = false;
        self.call("opened", &[]);
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// The page reports its height after drawing, so the panel fits it.
    pub fn set_height(&mut self, height: f64) {
        let height = height.clamp(120.0, 720.0);
        if (height - self.height).abs() < 1.0 {
            return;
        }
        self.height = height;
        self.window.set_inner_size(LogicalSize::new(WIDTH, height));
        self.place();
    }

    /// Whether to close the panel: another app came to the front, as when
    /// clicking elsewhere. Carryover's own main window doesn't count, so
    /// the panel stays open beside it.
    pub fn should_hide(&mut self) -> bool {
        if foreground_is_ours() {
            self.was_in_front = true;
            return false;
        }
        // Windows may refuse to bring the panel to the front. Then it stays
        // open until the icon is clicked again, rather than vanishing.
        self.was_in_front && self.shown_at.elapsed() > Duration::from_millis(200)
    }

    /// Sends a fresh snapshot, if anything changed since the last one.
    pub fn refresh(&mut self, app: &App) {
        let snapshot = app.snapshot();
        if self.shown.as_ref() != Some(&snapshot) {
            self.call("render", std::slice::from_ref(&snapshot));
            self.shown = Some(snapshot);
        }
    }

    /// The page asked for its contents after loading.
    pub fn loaded(&mut self, app: &App) {
        self.shown = None;
        self.refresh(app);
    }

    /// Above the icon when the taskbar is at the bottom of the screen, and
    /// below it when the taskbar is at the top, kept on the icon's screen.
    fn place(&self) {
        let Some(anchor) = self.anchor else { return };
        let (x, y) = (anchor.position.x, anchor.position.y);
        let monitor = self
            .window
            .available_monitors()
            .find(|monitor| {
                let (position, size) = (monitor.position(), monitor.size());
                (position.x as f64..position.x as f64 + size.width as f64).contains(&x)
                    && (position.y as f64..position.y as f64 + size.height as f64).contains(&y)
            })
            .or_else(|| self.window.primary_monitor());
        let Some(monitor) = monitor else { return };
        let scale = monitor.scale_factor();
        let (left, top) = (monitor.position().x as f64, monitor.position().y as f64);
        let (screen_width, screen_height) = (monitor.size().width as f64, monitor.size().height as f64);
        let (width, height, gap) = (WIDTH * scale, self.height * scale, GAP * scale);
        let center = x + anchor.size.width as f64 / 2.0;
        let px = (center - width / 2.0).clamp(left + gap, left + screen_width - width - gap);
        let py = if y > top + screen_height / 2.0 {
            y - height - gap
        } else {
            y + anchor.size.height as f64 + gap
        };
        self.window.set_outer_position(PhysicalPosition::new(px, py));
    }

    fn call(&self, function: &str, arguments: &[Value]) {
        let arguments: Vec<String> = arguments.iter().map(Value::to_string).collect();
        let script = format!(
            "window.{function} && window.{function}({})",
            arguments.join(",")
        );
        if let Err(error) = self.webview.evaluate_script(&script) {
            eprintln!("couldn't update the panel: {error}");
        }
    }
}

/// The panel's dark mode background, shown before the page draws.
const BACKGROUND: (u8, u8, u8, u8) = (36, 36, 36, 255);

fn bring_to_front(window: &TaoWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    unsafe {
        let _ = SetForegroundWindow(HWND(window.hwnd() as _));
    }
}

fn foreground_is_ours() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut process = 0u32;
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return false;
        }
        GetWindowThreadProcessId(window, Some(&mut process));
    }
    process == std::process::id()
}

/// Rounded corners on Windows 11, like its own flyouts. Earlier versions
/// ignore this and keep square ones.
fn round_corners(window: &TaoWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    let preference = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(window.hwnd() as _),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const _,
            std::mem::size_of_val(&preference) as u32,
        );
    }
}

/// A request from the panel.
#[derive(serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Request {
    Ready,
    /// The page's height, to fit the panel to.
    Size { height: f64 },
    /// Open the main window at a tab.
    Open { tab: String },
    Pause { on: bool },
    CheckUpdate,
    InstallUpdate,
    Hide,
    Quit,
}

/// The main window's tab for a name from the page.
pub fn tab(name: &str) -> &'static str {
    match name {
        "add" => "add",
        "settings" => "settings",
        "recent" => "recent",
        _ => "devices",
    }
}
