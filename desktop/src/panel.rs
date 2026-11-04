//! Windows: the panel the notification area icon opens, in place of a menu,
//! like the Wi-Fi and volume flyouts. Unlike a menu it stays open while you
//! use it, and closes when you click elsewhere. It never takes the focus,
//! so the hidden icons flyout it may open from stays open with it.
//! Drawn from `panel.html`.
use crate::app::App;
use crate::window::UserEvent;
use anyhow::Result;
use serde_json::Value;
use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{Window as TaoWindow, WindowBuilder, WindowId};
use windows::Win32::Foundation::HWND;
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
    visible: Cell<bool>,
    shown_at: Instant,
    /// The hidden icons flyout, when the icon was clicked there and it
    /// stayed open. The panel closes when it does.
    overflow: Option<isize>,
    /// Set once the panel is clicked. Windows closes the flyout on any
    /// click outside it, so from then on the panel stays without it.
    detached: bool,
}

impl Panel {
    pub fn new(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
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
        never_activate(&window);
        *HOOK_PROXY.lock().unwrap() = Some(proxy.clone());
        PANEL_HWND.store(window.hwnd(), Ordering::Relaxed);
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
            visible: Cell::new(false),
            shown_at: Instant::now(),
            overflow: None,
            detached: false,
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    fn hwnd(&self) -> HWND {
        HWND(self.window.hwnd() as _)
    }

    /// Shows the panel beside the icon at `anchor`, without taking the
    /// focus, so the hidden icons flyout it may have opened from stays open
    /// with it.
    pub fn show_at(&mut self, anchor: tray_icon::Rect, app: &App) {
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_TOPMOST, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SetWindowPos, ShowWindow,
        };
        self.anchor = Some(anchor);
        self.shown = None;
        self.refresh(app);
        self.place();
        let hwnd = self.hwnd();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        self.visible.set(true);
        self.shown_at = Instant::now();
        self.overflow = None;
        self.detached = false;
        CLICKED_INSIDE.store(false, Ordering::Relaxed);
        watch_clicks(true);
        self.call("opened", &[]);
    }

    pub fn hide(&self) {
        use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_HIDE);
        }
        self.visible.set(false);
        watch_clicks(false);
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

    /// Whether to close the panel, checked often while it's open. Clicks
    /// elsewhere close it through [`watch_clicks`]. This catches the rest:
    /// the hidden icons flyout closing, as with Escape, or another app
    /// coming to the front, as with Alt+Tab.
    pub fn should_hide(&mut self) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        if CLICKED_INSIDE.load(Ordering::Relaxed) {
            self.detached = true;
            self.overflow = None;
        }
        let settled = self.shown_at.elapsed() > Duration::from_millis(300);
        // Windows may close the flyout itself when an icon in it is
        // clicked. Only a flyout still open once the panel has settled is
        // one to close with.
        if !self.detached && self.overflow.is_none() && settled {
            self.overflow = overflow_flyout();
        }
        if let Some(flyout) = self.overflow
            && !unsafe { IsWindowVisible(HWND(flyout as _)) }.as_bool()
        {
            return true;
        }
        settled && another_app_in_front()
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
        let (screen_width, screen_height) =
            (monitor.size().width as f64, monitor.size().height as f64);
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

/// Whether an app other than Carryover and the Windows shell (the taskbar
/// and its flyouts) is in front. Nothing is, briefly, while a click moves
/// the focus.
fn another_app_in_front() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetShellWindow, GetWindowThreadProcessId,
    };
    unsafe {
        let front = GetForegroundWindow();
        if front.is_invalid() {
            return false;
        }
        let mut process = 0u32;
        GetWindowThreadProcessId(front, Some(&mut process));
        let mut shell = 0u32;
        GetWindowThreadProcessId(GetShellWindow(), Some(&mut shell));
        process != std::process::id() && process != shell
    }
}

/// The hidden icons flyout by the clock, if it's open: Windows 11's, then
/// Windows 10's.
fn overflow_flyout() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindowVisible};
    use windows::core::w;
    [
        w!("TopLevelWindowForOverflowXamlIsland"),
        w!("NotifyIconOverflowWindow"),
    ]
    .into_iter()
    .filter_map(|class| unsafe { FindWindowW(class, None) }.ok())
    .find(|window| unsafe { IsWindowVisible(*window) }.as_bool())
    .map(|window| window.0 as isize)
}

/// Clicks on the panel mustn't take the focus from the hidden icons flyout,
/// which would close it.
fn never_activate(window: &TaoWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, SetWindowLongW, WS_EX_NOACTIVATE,
    };
    let hwnd = HWND(window.hwnd() as _);
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as i32);
    }
}

/// Where the click watcher sends Hide, and the panel it watches.
static HOOK_PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);
static PANEL_HWND: AtomicIsize = AtomicIsize::new(0);
static HOOK: AtomicIsize = AtomicIsize::new(0);
/// Whether the panel was clicked since it opened.
static CLICKED_INSIDE: AtomicBool = AtomicBool::new(false);

/// While the panel is open, watches mouse clicks anywhere, and hides the
/// panel on one outside it. The panel never has the focus, so losing it
/// can't be the signal.
fn watch_clicks(on: bool) {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        HHOOK, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL,
    };
    let current = HOOK.swap(0, Ordering::Relaxed);
    if current != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(current as _));
        }
    }
    if on {
        let module = unsafe { GetModuleHandleW(None) }.ok();
        let hook =
            unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), module.map(Into::into), 0) };
        match hook {
            Ok(hook) => HOOK.store(hook.0 as isize, Ordering::Relaxed),
            Err(error) => eprintln!("couldn't watch clicks outside the panel: {error}"),
        }
    }
}

unsafe extern "system" fn mouse_hook(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetWindowRect, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
        WM_RBUTTONDOWN,
    };
    let button = matches!(
        wparam.0 as u32,
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
    );
    if code >= 0 && button {
        let point = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt };
        let mut rect = RECT::default();
        let panel = HWND(PANEL_HWND.load(Ordering::Relaxed) as _);
        let inside = unsafe { GetWindowRect(panel, &mut rect) }.is_ok()
            && (rect.left..rect.right).contains(&point.x)
            && (rect.top..rect.bottom).contains(&point.y);
        if inside {
            CLICKED_INSIDE.store(true, Ordering::Relaxed);
        } else if let Some(proxy) = HOOK_PROXY.lock().unwrap().as_ref() {
            let _ = proxy.send_event(UserEvent::Panel(r#"{"cmd":"hide"}"#.to_owned()));
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Rounded corners on Windows 11, like its own flyouts. Earlier versions
/// ignore this and keep square ones.
fn round_corners(window: &TaoWindow) {
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
