//! The setup wizard, shown when Carryover is opened before it's ready: a
//! download that was just double-clicked on Windows, or a Mac or PC that
//! isn't in a group yet. It installs the program, joins or starts a group,
//! and starts Carryover with its tray icon, then gets out of the way.
use crate::config::Config;
use crate::control::{self, Command};
use crate::install;
use crate::window::qr_svg;
use anyhow::Result;
use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::{WebView, WebViewBuilder};

const PAGE: &str = include_str!("setup.html");
const ICON: &[u8] = include_bytes!("../../assets/icon-rounded.png");

/// What this computer still needs.
pub struct Plan {
    /// Copy the program into place (Windows) or point the login item at
    /// this copy (macOS).
    pub install: bool,
    /// Join or start a group.
    pub pair: bool,
    /// Where the group's config goes.
    pub dir: PathBuf,
}

enum UserEvent {
    Request(String),
    Finished(Result<Value, String>),
    /// Moving to Applications didn't work.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Failed(String),
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
enum Request {
    Ready,
    /// Set up, with an invite to join or none to start a new group.
    Finish {
        invite: Option<String>,
        login: bool,
    },
    MoveToApplications,
    /// Close the wizard and show Carryover's window.
    Open,
    Close,
}

pub fn run(plan: Plan) -> ! {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        // A normal app while the wizard is open, with a Dock icon, even
        // though the bundle is a menu bar app.
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Regular);
        app_menu();
    }
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Set Up Carryover")
        .with_inner_size(LogicalSize::new(560.0, 580.0))
        .with_resizable(false)
        .with_maximizable(false)
        .with_window_icon(window_icon())
        .build(&event_loop);
    let window = match window {
        Ok(window) => window,
        Err(error) => fail(&format!("Carryover couldn't open its window: {error}")),
    };
    let ipc = proxy.clone();
    let builder = WebViewBuilder::new()
        .with_html(PAGE)
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |request| {
            let _ = ipc.send_event(UserEvent::Request(request.into_body()));
        });
    let webview = crate::window::build_webview(builder, &window);
    let webview = match webview {
        Ok(webview) => webview,
        Err(error) => no_webview(&error.to_string()),
    };
    window.set_focus();

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::Request(body)) => match serde_json::from_str(&body) {
                Ok(Request::Ready) => call(&webview, "init", &init(&plan)),
                Ok(Request::Finish { invite, login }) => {
                    let proxy = proxy.clone();
                    let dir = plan.dir.clone();
                    let (install, pair) = (plan.install, plan.pair);
                    std::thread::spawn(move || {
                        let result = finish(dir, install, pair, invite, login)
                            .map_err(|error| format!("{error:#}"));
                        let _ = proxy.send_event(UserEvent::Finished(result));
                    });
                }
                Ok(Request::MoveToApplications) => move_to_applications(proxy.clone()),
                Ok(Request::Open) => {
                    window.set_visible(false);
                    std::thread::spawn(|| {
                        // The new copy may still be starting.
                        let deadline = Instant::now() + Duration::from_secs(10);
                        while !control::send(Command::Open) && Instant::now() < deadline {
                            std::thread::sleep(Duration::from_millis(300));
                        }
                        std::process::exit(0);
                    });
                }
                Ok(Request::Close) => *flow = ControlFlow::Exit,
                Err(error) => eprintln!("unknown request from setup: {error}"),
            },
            Event::UserEvent(UserEvent::Finished(Ok(done))) => call(&webview, "done", &done),
            Event::UserEvent(UserEvent::Finished(Err(message)) | UserEvent::Failed(message)) => {
                call(&webview, "failed", &Value::from(message))
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *flow = ControlFlow::Exit,
            _ => {}
        }
    })
}

/// What the page needs to know to choose its steps and wording.
fn init(plan: &Plan) -> Value {
    #[cfg(target_os = "linux")]
    let (platform, needs_move) = ("linux", false);
    #[cfg(windows)]
    let (platform, needs_move) = ("windows", false);
    #[cfg(target_os = "macos")]
    let (platform, needs_move) = ("macos", !install::in_applications());
    let icon = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ICON)
    );
    json!({
        "platform": platform,
        "install": plan.install,
        "pair": plan.pair,
        "needsMove": needs_move,
        "icon": icon,
        "version": crate::update::VERSION,
    })
}

/// Does the setup: checks the invite before changing anything, then
/// installs, saves the group, and starts Carryover.
fn finish(
    dir: PathBuf,
    install: bool,
    pair: bool,
    invite: Option<String>,
    login: bool,
) -> Result<Value> {
    let joining = invite.is_some();
    let config = if pair {
        Some(match &invite {
            Some(text) => Config::from_invite(text).map_err(|_| {
                anyhow::anyhow!("That isn't a Carryover invite link. Copy it again from Add a Device on your other device.")
            })?,
            None => Config::new(&rand::random()),
        })
    } else {
        None
    };
    let program = if install {
        #[cfg(target_os = "linux")]
        {
            install::stop_running();
            crate::linux_install::install_from_in(&std::env::current_exe()?, &dir)?
        }
        #[cfg(not(target_os = "linux"))]
        install::install()?
    } else {
        install::program()?
    };
    if let Some(config) = &config {
        config.save(&dir, true)?;
    }
    #[cfg(target_os = "linux")]
    {
        crate::autostart::set_enabled_for_in(&program, login, &dir)?;
        install::spawn_linux_in(&program, &dir, &[])?;
    }
    #[cfg(not(target_os = "linux"))]
    install::start(&program, login)?;
    let link = Config::load(&dir)?.invite_link();
    Ok(json!({
        "newGroup": pair && !joining,
        "joined": joining,
        "qr": qr_svg(&link)?,
        "link": link,
    }))
}

#[cfg(target_os = "macos")]
fn move_to_applications(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || match install::move_to_applications() {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            let _ = proxy.send_event(UserEvent::Failed(format!("{error:#}")));
        }
    });
}

#[cfg(any(windows, target_os = "linux"))]
fn move_to_applications(_proxy: EventLoopProxy<UserEvent>) {}

/// Calls a function on the page with one JSON argument.
fn call(webview: &WebView, function: &str, argument: &Value) {
    let script = format!("window.{function} && window.{function}({argument})");
    if let Err(error) = webview.evaluate_script(&script) {
        eprintln!("couldn't update setup: {error}");
    }
}

/// Carryover's icon, for title bars and the taskbar.
pub(crate) fn window_icon() -> Option<tao::window::Icon> {
    let image = image::load_from_memory_with_format(ICON, image::ImageFormat::Png)
        .ok()?
        .resize(64, 64, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (width, height) = image.dimensions();
    tao::window::Icon::from_rgba(image.into_raw(), width, height).ok()
}

/// Windows without the WebView2 runtime can't draw any of Carryover's
/// windows. Windows 11 includes it; some Windows 10 PCs don't.
fn no_webview(error: &str) -> ! {
    #[cfg(windows)]
    {
        let install = message(
            "Carryover needs Microsoft Edge WebView2",
            "Carryover needs Microsoft's WebView2 Runtime to show its windows, and this PC \
             doesn't have it.\n\nOpen Microsoft's download page now? Install the \
             \"Evergreen Bootstrapper\", then open Carryover again.",
            true,
        );
        if install {
            let _ = std::process::Command::new("explorer.exe")
                .arg("https://developer.microsoft.com/microsoft-edge/webview2/")
                .spawn();
        }
        eprintln!("couldn't create the web view: {error}");
        std::process::exit(1);
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fail(&format!("Carryover couldn't open its window: {error}"))
}

pub fn fail(text: &str) -> ! {
    eprintln!("{text}");
    #[cfg(target_os = "linux")]
    message("Carryover", text, false);
    #[cfg(windows)]
    message("Carryover", text, false);
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "display alert \"Carryover\" message (item 1 of argv)",
                "-e",
                "end run",
            ])
            .arg(text)
            .status();
    }
    std::process::exit(1)
}

/// A standard Windows message box. With `ask`, it has Yes and No, and
/// returns whether Yes was chosen.
#[cfg(windows)]
pub fn message(title: &str, text: &str, ask: bool) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO, MessageBoxW,
    };
    use windows::core::HSTRING;
    let style = if ask {
        MB_YESNO | MB_ICONQUESTION
    } else {
        MB_OK | MB_ICONINFORMATION
    };
    let answer = unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), style) };
    answer == IDYES
}

/// An app menu with Quit, and an Edit menu so Cut, Copy and Paste
/// shortcuts reach the invite field.
#[cfg(target_os = "macos")]
fn app_menu() {
    use tray_icon::menu::{Menu, PredefinedMenuItem, Submenu};
    let app = Submenu::with_items("Carryover", true, &[&PredefinedMenuItem::quit(None)]);
    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ],
    );
    let menu = Menu::new();
    if let (Ok(app), Ok(edit)) = (app, edit)
        && menu.append_items(&[&app, &edit]).is_ok()
    {
        menu.init_for_nsapp();
        // The menu lives as long as the app.
        std::mem::forget(menu);
    }
}

#[cfg(target_os = "linux")]
pub fn message(title: &str, text: &str, ask: bool) -> bool {
    use gtk::prelude::*;
    if gtk::init().is_err() {
        eprintln!("{title}: {text}");
        return false;
    }
    let dialog = gtk::MessageDialog::builder()
        .text(title)
        .secondary_text(text)
        .message_type(if ask {
            gtk::MessageType::Question
        } else {
            gtk::MessageType::Info
        })
        .buttons(if ask {
            gtk::ButtonsType::YesNo
        } else {
            gtk::ButtonsType::Ok
        })
        .modal(true)
        .build();
    let result = dialog.run();
    dialog.close();
    result == gtk::ResponseType::Yes
}
