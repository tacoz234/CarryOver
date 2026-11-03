//! The menu bar icon on macOS and the notification area icon on Windows, and
//! the event loop that also runs the main window.
use crate::app::{App, Request, ago};
use crate::hub::Status;
use crate::update::{self, State};
use crate::window::{UserEvent, Window};
use anyhow::Result;
use image::ImageFormat;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../../assets/tray-mac.png");
#[cfg(windows)]
const ICON: &[u8] = include_bytes!("../../assets/tray-windows.png");

struct Items {
    connected: MenuItem,
    activity: MenuItem,
    open: MenuItem,
    add_device: MenuItem,
    pause: CheckMenuItem,
    update: MenuItem,
    quit: MenuItem,
}

/// Runs the menu and window on the main thread until Quit. Sync keeps
/// running on other threads.
pub fn run(app: Arc<App>, open_window: bool) -> ! {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        // A menu bar item only, with no Dock icon or app menu.
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();

    let items = Items {
        connected: MenuItem::new("Starting…", false, None),
        activity: MenuItem::new("Nothing synced yet", false, None),
        open: MenuItem::new("Open CopyBlip…", true, None),
        add_device: MenuItem::new("Add a Device…", true, None),
        pause: CheckMenuItem::new("Pause Syncing", true, false, None),
        update: MenuItem::new("Check for Updates", true, None),
        quit: MenuItem::new("Quit CopyBlip", true, None),
    };
    let mut tray: Option<TrayIcon> = None;
    let mut window: Option<Window> = None;
    let mut shown = Vec::new();
    // Once set, every later event repeats it, since the loop delivers a few
    // more events before ending and each would otherwise reset the flow.
    let mut exiting: Option<i32> = None;

    event_loop.run(move |event, target, flow| {
        if let Some(code) = exiting {
            *flow = ControlFlow::ExitWithCode(code);
            return;
        }
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1));
        let mut open_tab: Option<&'static str> = None;
        let mut exit: Option<i32> = None;
        match event {
            // macOS needs the loop running before the icon is created.
            Event::NewEvents(StartCause::Init) => {
                #[cfg(target_os = "macos")]
                edit_menu();
                match build(&items) {
                    Ok(icon) => tray = Some(icon),
                    Err(error) => eprintln!("couldn't show the tray icon: {error:#}"),
                }
                if open_window {
                    open_tab = Some("devices");
                }
            }
            Event::UserEvent(UserEvent::Request(body)) => {
                let request = serde_json::from_str::<Request>(&body);
                match request {
                    Ok(Request::Ready) => {
                        if let Some(window) = &mut window {
                            window.loaded(&app);
                        }
                    }
                    Ok(request) => {
                        let saved = match &request {
                            Request::Relay { url } if url.trim().is_empty() => {
                                Some("Relay turned off.")
                            }
                            Request::Relay { .. } => Some("Relay saved."),
                            _ => None,
                        };
                        let result = app.handle(request);
                        if let Some(window) = &mut window {
                            match result {
                                Ok(()) => saved.into_iter().for_each(|text| window.toast(text)),
                                Err(error) => window.toast(&format!("{error:#}")),
                            }
                            window.refresh(&app);
                        }
                    }
                    Err(error) => eprintln!("unknown request from the window: {error}"),
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } if window
                .as_ref()
                .is_some_and(|window| window.id() == window_id) =>
            {
                window = None;
            }
            _ => {}
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == items.open.id() {
                open_tab = Some("devices");
            } else if event.id == items.add_device.id() {
                open_tab = Some("add");
            } else if event.id == items.pause.id() {
                app.hub.set_paused(items.pause.is_checked());
            } else if event.id == items.update.id() {
                match app.updater.state() {
                    State::Available(_) => app.updater.install_now(),
                    _ => {
                        app.updater.check_now();
                        open_tab = Some("settings");
                    }
                }
            } else if event.id == items.quit.id() {
                eprintln!("quit from the tray");
                exit = Some(0);
            }
        }
        // On Windows, clicking the icon opens the window and right-clicking
        // shows the menu.
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
                && cfg!(windows)
            {
                open_tab = Some("devices");
            }
        }

        if let Some(tab) = open_tab {
            if window.is_none() {
                match Window::open(target, proxy.clone(), Some(tab)) {
                    Ok(opened) => window = Some(opened),
                    Err(error) => eprintln!("couldn't open the window: {error:#}"),
                }
            } else if let Some(window) = &mut window {
                window.show_tab(tab);
            }
            if let Some(window) = &window {
                window.focus();
            }
        }

        if let State::Ready(version) = app.updater.state() {
            match update::relaunch() {
                Ok(code) => {
                    eprintln!("restarting into CopyBlip {version}");
                    exit = Some(code);
                }
                Err(error) => eprintln!("couldn't restart after updating: {error:#}"),
            }
        }

        if let Some(code) = exit {
            // Dropping the icon removes it right away on Windows, rather than
            // when the mouse next passes over it.
            window = None;
            tray = None;
            exiting = Some(code);
            *flow = ControlFlow::ExitWithCode(code);
            return;
        }

        if let Some(tray) = &tray {
            let status = app.hub.status();
            let lines = vec![
                connected_line(&status),
                activity_line(&status),
                update_line(&app.updater.state()),
                status.paused.to_string(),
            ];
            // Only touches the menu when something changed, to avoid flicker.
            if lines != shown {
                items.connected.set_text(&lines[0]);
                items.activity.set_text(&lines[1]);
                items.update.set_text(&lines[2]);
                items.pause.set_checked(status.paused);
                let _ = tray.set_tooltip(Some(tooltip(&status)));
                shown = lines;
            }
        }
        if let Some(window) = &mut window {
            window.refresh(&app);
        }
    })
}

fn build(items: &Items) -> Result<TrayIcon> {
    let menu = Menu::new();
    menu.append_items(&[
        &items.connected,
        &items.activity,
        &PredefinedMenuItem::separator(),
        &items.open,
        &items.add_device,
        &items.pause,
        &PredefinedMenuItem::separator(),
        &items.update,
        &items.quit,
    ])?;
    let icon = image::load_from_memory_with_format(ICON, ImageFormat::Png)?.to_rgba8();
    let (width, height) = icon.dimensions();
    let icon = Icon::from_rgba(icon.into_raw(), width, height)?;
    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("CopyBlip");
    // macOS recolors template images for light and dark menu bars.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon);
    #[cfg(windows)]
    let builder = builder.with_icon(icon).with_menu_on_left_click(false);
    Ok(builder.build()?)
}

/// Without an Edit menu, macOS doesn't route Cut, Copy and Paste shortcuts
/// to text fields. The menu bar itself stays hidden.
#[cfg(target_os = "macos")]
fn edit_menu() {
    use tray_icon::menu::Submenu;
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
    if let Ok(edit) = edit
        && menu.append(&edit).is_ok()
    {
        menu.init_for_nsapp();
        // The app menu lives as long as the app.
        std::mem::forget(menu);
    }
}

fn connected_line(status: &Status) -> String {
    match status.connected.as_slice() {
        [] => "No other devices connected".to_owned(),
        names => format!("Connected to {}", names.join(", ")),
    }
}

fn activity_line(status: &Status) -> String {
    match &status.last_activity {
        Some(activity) => format!("{} · {}", activity.description, ago(activity.at.elapsed())),
        None => "Nothing synced yet".to_owned(),
    }
}

fn update_line(state: &State) -> String {
    match state {
        State::Available(release) => format!("Install Update ({})", release.version),
        State::Installing(version) | State::Ready(version) => format!("Updating to {version}…"),
        _ => "Check for Updates…".to_owned(),
    }
}

fn tooltip(status: &Status) -> String {
    if status.paused {
        "CopyBlip: paused".to_owned()
    } else {
        match status.connected.len() {
            0 => "CopyBlip: no other devices connected".to_owned(),
            1 => "CopyBlip: connected to 1 device".to_owned(),
            n => format!("CopyBlip: connected to {n} devices"),
        }
    }
}
