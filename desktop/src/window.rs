//! The main window: devices, adding a device, and settings. A web view drawn
//! from `window.html`, which talks to `App` through `window.ipc`.
use crate::app::App;
use anyhow::Result;
use serde_json::Value;
use tao::dpi::LogicalSize;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Window as TaoWindow, WindowBuilder, WindowId};
use wry::{WebView, WebViewBuilder};

const PAGE: &str = include_str!("window.html");

/// Messages from the web view to the event loop.
#[derive(Debug)]
pub enum UserEvent {
    Request(String),
}

pub struct Window {
    window: TaoWindow,
    webview: WebView,
    shown: Option<Value>,
    /// The page to show once loaded.
    tab: Option<&'static str>,
}

impl Window {
    pub fn open(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        tab: Option<&'static str>,
    ) -> Result<Self> {
        let window = WindowBuilder::new()
            .with_title("CopyBlip")
            .with_inner_size(LogicalSize::new(760.0, 540.0))
            .with_min_inner_size(LogicalSize::new(600.0, 420.0))
            .build(target)?;
        let webview = WebViewBuilder::new()
            .with_html(PAGE)
            .with_devtools(cfg!(debug_assertions))
            .with_ipc_handler(move |request| {
                let _ = proxy.send_event(UserEvent::Request(request.into_body()));
            })
            .build(&window)?;
        Ok(Self {
            window,
            webview,
            shown: None,
            tab,
        })
    }

    /// The page finished loading and asked for its contents.
    pub fn loaded(&mut self, app: &App) {
        if let Err(error) = self.set_invite(&app.invite_link()) {
            eprintln!("couldn't draw the invite: {error:#}");
        }
        self.shown = None;
        self.refresh(app);
        if let Some(tab) = self.tab.take() {
            self.show(tab);
        }
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.set_visible(true);
        self.window.set_minimized(false);
        self.window.set_focus();
    }

    /// Sends a fresh snapshot, if anything changed since the last one.
    pub fn refresh(&mut self, app: &App) {
        let snapshot = app.snapshot();
        if self.shown.as_ref() != Some(&snapshot) {
            self.call("render", std::slice::from_ref(&snapshot));
            self.shown = Some(snapshot);
        }
    }

    /// Switches page, or remembers it until the page has loaded.
    pub fn show_tab(&mut self, tab: &'static str) {
        if self.shown.is_some() {
            self.show(tab);
        } else {
            self.tab = Some(tab);
        }
    }

    fn show(&self, tab: &str) {
        self.call("show", &[Value::from(tab)]);
    }

    pub fn toast(&self, message: &str) {
        self.call("toast", &[Value::from(message)]);
    }

    fn set_invite(&self, link: &str) -> Result<()> {
        self.call(
            "setInvite",
            &[Value::from(qr_svg(link)?), Value::from(link)],
        );
        Ok(())
    }

    /// Calls a function on the page. Arguments go through JSON, so they can't
    /// break out of the call.
    fn call(&self, function: &str, arguments: &[Value]) {
        let arguments: Vec<String> = arguments.iter().map(Value::to_string).collect();
        let script = format!(
            "window.{function} && window.{function}({})",
            arguments.join(",")
        );
        if let Err(error) = self.webview.evaluate_script(&script) {
            eprintln!("couldn't update the window: {error}");
        }
    }
}

/// The invite as a QR code, black on white with a quiet zone, in SVG.
pub fn qr_svg(text: &str) -> Result<String> {
    let code = qrcode::QrCode::new(text)?;
    let width = code.width();
    let side = width + 8;
    let mut path = String::new();
    for (index, color) in code.to_colors().iter().enumerate() {
        if *color == qrcode::Color::Dark {
            let (x, y) = (index % width + 4, index / width + 4);
            path.push_str(&format!("M{x} {y}h1v1h-1z"));
        }
    }
    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side} {side}" shape-rendering="crispEdges"><rect width="{side}" height="{side}" fill="#fff"/><path d="{path}" fill="#000"/></svg>"##
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_qr_has_a_quiet_zone() {
        let svg = qr_svg("copyblip://join?code=00").unwrap();
        // Version 2 codes are 25 modules wide, plus 4 on each side.
        assert!(svg.contains(r#"viewBox="0 0 33 33""#), "{svg}");
        // The top-left finder pattern starts right after the quiet zone.
        assert!(svg.contains("M4 4h1v1h-1z"));
    }
}
