//! Linux popup using the exact page and command contract used on Windows.
use crate::app::App;
use crate::window::UserEvent;
use anyhow::Result;
use gtk::prelude::*;
use serde_json::Value;
use std::cell::Cell;
use std::time::{Duration, Instant};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::platform::unix::{WindowBuilderExtUnix, WindowExtUnix};
use tao::window::{Window as TaoWindow, WindowBuilder, WindowId};
use wry::{WebView, WebViewBuilder};

const WIDTH: f64 = 340.0;

pub struct Panel {
    window: TaoWindow,
    webview: WebView,
    shown: Option<Value>,
    anchor: Option<tray_icon::Rect>,
    height: f64,
    visible: Cell<bool>,
    focused: Cell<bool>,
    shown_at: Instant,
}

impl Panel {
    pub fn new(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
        let window = WindowBuilder::new()
            .with_title("Carryover Panel")
            .with_inner_size(LogicalSize::new(WIDTH, 420.0))
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top(true)
            .with_visible(false)
            .with_skip_taskbar(true)
            .build(target)?;
        window.gtk_window().set_skip_pager_hint(true);
        let escape_proxy = proxy.clone();
        window
            .gtk_window()
            .connect_key_press_event(move |_, event| {
                if event.keyval() == gtk::gdk::keys::constants::Escape {
                    let _ = escape_proxy.send_event(UserEvent::Panel(r#"{"cmd":"hide"}"#.into()));
                    return gtk::glib::Propagation::Stop;
                }
                gtk::glib::Propagation::Proceed
            });
        let builder = WebViewBuilder::new()
            .with_html(include_str!("../panel.html"))
            .with_background_color((36, 36, 36, 255))
            .with_devtools(cfg!(debug_assertions))
            .with_ipc_handler(move |request| {
                let _ = proxy.send_event(UserEvent::Panel(request.into_body()));
            });
        let webview = crate::window::build_webview(builder, &window)?;
        Ok(Self {
            window,
            webview,
            shown: None,
            anchor: None,
            height: 420.0,
            visible: Cell::new(false),
            focused: Cell::new(false),
            shown_at: Instant::now(),
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }
    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }
    pub fn show_at(&mut self, anchor: tray_icon::Rect, app: &App) {
        self.anchor = Some(anchor);
        self.shown = None;
        self.refresh(app);
        self.place();
        // Queue visibility through Tao, then request focus after GTK processes
        // that queue. An immediate set_focus sees the still-hidden widget.
        self.window.set_visible(true);
        let window = self.window.gtk_window().clone();
        gtk::glib::idle_add_local_once(move || {
            if window.is_visible() {
                window.present();
                window.child_focus(gtk::DirectionType::TabForward);
            }
        });
        self.visible.set(true);
        self.focused.set(false);
        self.shown_at = Instant::now();
        self.call("opened", &[]);
    }
    pub fn hide(&self) {
        self.window.set_visible(false);
        self.visible.set(false);
    }
    pub fn set_height(&mut self, height: f64) {
        if !height.is_finite() {
            return;
        }
        self.height = height.clamp(120.0, 720.0);
        self.window
            .set_inner_size(LogicalSize::new(WIDTH, self.height));
        self.place();
    }
    pub fn should_hide(&mut self) -> bool {
        let active = self.window.gtk_window().is_active();
        if active {
            self.focused.set(true);
        }
        self.focused.get() && !active && self.shown_at.elapsed() > Duration::from_millis(300)
    }
    pub fn refresh(&mut self, app: &App) {
        let snapshot = app.snapshot();
        if self.shown.as_ref() != Some(&snapshot) {
            self.call("render", std::slice::from_ref(&snapshot));
            self.shown = Some(snapshot);
        }
    }
    pub fn loaded(&mut self, app: &App) {
        self.shown = None;
        self.refresh(app);
    }
    fn place(&self) {
        let Some(anchor) = self.anchor else { return };
        let monitor = self
            .window
            .available_monitors()
            .find(|m| {
                let p = m.position();
                let s = m.size();
                (p.x as f64..p.x as f64 + s.width as f64).contains(&anchor.position.x)
                    && (p.y as f64..p.y as f64 + s.height as f64).contains(&anchor.position.y)
            })
            .or_else(|| self.window.primary_monitor());
        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let p = monitor.position();
            let s = monitor.size();
            let (x, y) = position(
                (anchor.position.x, anchor.position.y),
                (p.x as f64, p.y as f64, s.width as f64, s.height as f64),
                (WIDTH * scale, self.height * scale),
                12.0 * scale,
            );
            self.window.set_outer_position(PhysicalPosition::new(x, y));
        }
    }
    fn call(&self, function: &str, arguments: &[Value]) {
        let arguments = arguments
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(",");
        if let Err(error) = self.webview.evaluate_script(&format!(
            "window.{function} && window.{function}({arguments})"
        )) {
            eprintln!("couldn't update the panel: {error}");
        }
    }
}

fn position(
    anchor: (f64, f64),
    screen: (f64, f64, f64, f64),
    size: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    let (left, top, width, height) = screen;
    let x =
        (anchor.0 - size.0 / 2.0).clamp(left + gap, (left + width - size.0 - gap).max(left + gap));
    let y = if anchor.1 > top + height / 2.0 {
        anchor.1 - size.1 - gap
    } else {
        anchor.1 + gap
    };
    (
        x,
        y.clamp(top + gap, (top + height - size.1 - gap).max(top + gap)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_stays_inside_small_scaled_and_negative_origin_monitors() {
        for screen in [
            (0., 0., 1920., 1080.),
            (-1920., -200., 1920., 1080.),
            (0., 0., 240., 200.),
        ] {
            for anchor in [
                (screen.0, screen.1),
                (screen.0 + screen.2, screen.1 + screen.3),
            ] {
                let (x, y) = position(anchor, screen, (340., 420.), 12.);
                assert!(x >= screen.0 && y >= screen.1);
                if screen.2 > 364. {
                    assert!(x + 340. <= screen.0 + screen.2);
                }
                if screen.3 > 444. {
                    assert!(y + 420. <= screen.1 + screen.3);
                }
            }
        }
    }
}
