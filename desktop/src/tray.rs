//! The menu bar icon on macOS and the notification area icon on Windows, and
//! the event loop that also runs the main window.
#[cfg(not(target_os = "linux"))]
use crate::app::ago;
use crate::app::{App, Request};
#[cfg(not(target_os = "linux"))]
use crate::avatar::{self, Kind};
#[cfg(not(target_os = "linux"))]
use crate::hub::DeviceStatus;
use crate::hub::Status;
use crate::update::{self, State};
use crate::window::{UserEvent, Window};
#[cfg(not(target_os = "linux"))]
use anyhow::Result;
#[cfg(not(target_os = "linux"))]
use image::ImageFormat;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
#[cfg(not(target_os = "linux"))]
use tray_icon::menu::PredefinedMenuItem;
#[cfg(not(target_os = "linux"))]
use tray_icon::menu::{CheckMenuItem, Icon as MenuIcon, IconMenuItem, Menu, MenuEvent, MenuItem};
#[cfg(not(target_os = "linux"))]
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../../assets/tray-mac.png");
#[cfg(windows)]
const ICON: &[u8] = include_bytes!("../../assets/tray-windows.png");

/// The most devices listed in the menu. The window lists them all.
#[cfg(not(target_os = "linux"))]
const MAX_DEVICES: usize = 8;

#[cfg(not(target_os = "linux"))]
struct Items {
    menu: Menu,
    /// The title, which also says when syncing is paused.
    header: MenuItem,
    /// One row per device, between the header and Add a Device.
    devices: Vec<IconMenuItem>,
    /// Stands in for the device rows before any device has connected.
    no_devices: MenuItem,
    activity: MenuItem,
    open: MenuItem,
    add_device: IconMenuItem,
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
    // Raw mouse and keyboard input isn't used, and on Windows each mouse
    // movement would otherwise wake the loop, which can stall scrolling.
    event_loop.set_device_event_filter(tao::event_loop::DeviceEventFilter::Always);
    let proxy = event_loop.create_proxy();
    let control_proxy = proxy.clone();
    if let Err(error) = crate::control::listen(move |command| {
        let _ = control_proxy.send_event(UserEvent::Control(command));
    }) {
        eprintln!("couldn't listen for other launches: {error:#}");
    }
    crate::install::note_version();

    #[cfg(not(target_os = "linux"))]
    let mut items = Items {
        menu: Menu::new(),
        header: MenuItem::new("Carryover", false, None),
        devices: Vec::new(),
        no_devices: MenuItem::new("No devices yet", false, None),
        activity: MenuItem::new("Nothing synced yet", false, None),
        open: MenuItem::new("Open Carryover…", true, None),
        add_device: IconMenuItem::new("Add a Device…", true, Some(picture(Kind::Add, false)), None),
        pause: CheckMenuItem::new("Pause Syncing", true, false, None),
        update: MenuItem::new("Check for Updates", true, None),
        quit: MenuItem::new("Quit Carryover", true, None),
    };
    #[cfg(not(target_os = "linux"))]
    let mut tray: Option<TrayIcon> = None;
    #[cfg(target_os = "linux")]
    let mut tray: Option<crate::linux_desktop::Tray> = None;
    let mut window: Option<Window> = None;
    let mut shown = Vec::new();
    // Once set, every later event repeats it, since the loop delivers a few
    // more events before ending and each would otherwise reset the flow.
    let mut exiting: Option<i32> = None;
    // Windows: the panel the icon opens, made on first use.
    #[cfg(any(windows, target_os = "linux"))]
    let mut panel: Option<crate::panel::Panel> = None;
    // When the panel last hid, so the click on the icon that hid it doesn't
    // open it straight back up.
    #[cfg(any(windows, target_os = "linux"))]
    let mut panel_hidden_at: Option<Instant> = None;

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
                #[cfg(not(target_os = "linux"))]
                let built = build(&items);
                #[cfg(target_os = "linux")]
                let built = crate::linux_desktop::Tray::new(proxy.clone());
                match built {
                    Ok(icon) => tray = Some(icon),
                    Err(error) => eprintln!("couldn't show the tray icon: {error:#}"),
                }
                if open_window {
                    open_tab = Some("devices");
                }
            }
            Event::UserEvent(UserEvent::Control(command)) => match command {
                crate::control::Command::Open => open_tab = Some("devices"),
                crate::control::Command::Quit => {
                    eprintln!("quit for another launch");
                    exit = Some(0);
                }
            },
            Event::UserEvent(UserEvent::Request(body)) => {
                let request = serde_json::from_str::<Request>(&body);
                match request {
                    Ok(Request::Ready) => {
                        if let Some(window) = &mut window {
                            window.loaded(&app);
                        }
                    }
                    Ok(Request::LeaveGroup) => match app.handle(Request::LeaveGroup) {
                        Ok(()) => exit = Some(0),
                        Err(error) => {
                            if let Some(window) = &mut window {
                                window.toast(&format!("Couldn't leave the group: {error:#}"));
                                window.refresh(&app);
                            }
                        }
                    },
                    Ok(request) => {
                        let saved = match &request {
                            Request::CopyClip { .. } => {
                                Some("Copied to this computer's clipboard.")
                            }
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
            #[cfg(any(windows, target_os = "linux"))]
            Event::UserEvent(UserEvent::Panel(body)) => {
                use crate::panel::Request as PanelRequest;
                match serde_json::from_str::<PanelRequest>(&body) {
                    Ok(PanelRequest::Ready) => {
                        if let Some(panel) = &mut panel {
                            panel.loaded(&app);
                        }
                    }
                    Ok(PanelRequest::Size { height }) => {
                        if let Some(panel) = &mut panel {
                            panel.set_height(height);
                        }
                    }
                    Ok(PanelRequest::Open { tab }) => open_tab = Some(crate::panel::tab(&tab)),
                    Ok(PanelRequest::Pause { on }) => app.hub.set_paused(on),
                    Ok(PanelRequest::CheckUpdate) => app.updater.check_now(),
                    Ok(PanelRequest::InstallUpdate) => app.updater.install_now(),
                    Ok(PanelRequest::Hide) => {
                        if let Some(panel) = &panel {
                            panel.hide();
                            panel_hidden_at = Some(Instant::now());
                        }
                    }
                    Ok(PanelRequest::Quit) => {
                        eprintln!("quit from the panel");
                        exit = Some(0);
                    }
                    Err(error) => eprintln!("unknown request from the panel: {error}"),
                }
            }
            #[cfg(target_os = "linux")]
            Event::UserEvent(UserEvent::TrayActivate { x, y }) => {
                let just_hid =
                    panel_hidden_at.is_some_and(|at| at.elapsed() < Duration::from_millis(400));
                if panel.is_none() {
                    match crate::panel::Panel::new(target, proxy.clone()) {
                        Ok(made) => panel = Some(made),
                        Err(error) => {
                            eprintln!("couldn't open the panel: {error:#}");
                            open_tab = Some("devices");
                        }
                    }
                }
                if let Some(panel) = &mut panel {
                    if panel.is_visible() {
                        panel.hide();
                        panel_hidden_at = Some(Instant::now());
                    } else if !just_hid {
                        panel.show_at(
                            tray_icon::Rect {
                                position: (x as f64, y as f64).into(),
                                size: (1u32, 1u32).into(),
                            },
                            &app,
                        );
                    }
                }
            }
            #[cfg(target_os = "linux")]
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } if panel.as_ref().is_some_and(|panel| panel.id() == window_id) => {
                if let Some(panel) = &panel {
                    panel.hide();
                    panel_hidden_at = Some(Instant::now());
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

        #[cfg(not(target_os = "linux"))]
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == items.open.id()
                || items.devices.iter().any(|device| device.id() == &event.id)
            {
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
        // On Windows, clicking the icon, with either button, opens or
        // closes the panel.
        #[allow(unused_variables)]
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            #[cfg(windows)]
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left | tray_icon::MouseButton::Right,
                button_state: tray_icon::MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let just_hid =
                    panel_hidden_at.is_some_and(|at| at.elapsed() < Duration::from_millis(400));
                if panel.is_none() {
                    match crate::panel::Panel::new(target, proxy.clone()) {
                        Ok(made) => panel = Some(made),
                        Err(error) => {
                            eprintln!("couldn't open the panel: {error:#}");
                            open_tab = Some("devices");
                        }
                    }
                }
                if let Some(panel) = &mut panel {
                    if panel.is_visible() {
                        panel.hide();
                        panel_hidden_at = Some(Instant::now());
                    } else if !just_hid {
                        panel.show_at(rect, &app);
                    }
                }
            }
        }
        // While the panel is open, check often whether another app came to
        // the front, which closes it.
        #[cfg(any(windows, target_os = "linux"))]
        if let Some(panel) = &mut panel
            && panel.is_visible()
        {
            if panel.should_hide() {
                panel.hide();
                panel_hidden_at = Some(Instant::now());
            } else {
                *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
            }
        }

        if let Some(tab) = open_tab {
            #[cfg(target_os = "linux")]
            if let Some(panel) = &panel {
                panel.hide();
            }
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
                    eprintln!("restarting into Carryover {version}");
                    exit = Some(code);
                }
                Err(error) => eprintln!("couldn't restart after updating: {error:#}"),
            }
        }

        if let Some(code) = exit {
            // Dropping the icon removes it right away on Windows, rather than
            // when the mouse next passes over it.
            window = None;
            #[cfg(any(windows, target_os = "linux"))]
            {
                panel = None;
            }
            tray = None;
            exiting = Some(code);
            *flow = ControlFlow::ExitWithCode(code);
            return;
        }

        if let Some(tray) = &tray {
            let status = app.hub.status();
            #[cfg(target_os = "linux")]
            {
                // Linux uses StatusNotifier activation and the HTML popup;
                // there is no native menu to populate when peers connect.
                let text = tooltip(&status);
                if shown.first() != Some(&text) {
                    let _ = tray.set_tooltip(Some(text.clone()));
                    shown.clear();
                    shown.push(text);
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                let mut lines = vec![
                    header_line(&status),
                    activity_line(&status),
                    update_line(&app.updater.state()),
                    status.paused.to_string(),
                ];
                let listed = &status.devices[..status.devices.len().min(MAX_DEVICES)];
                lines.extend(listed.iter().map(device_key));
                // Only touches the menu when something changed, to avoid flicker.
                if lines != shown {
                    if lines[4..] != *shown.get(4..).unwrap_or_default() {
                        set_devices(&mut items, listed);
                    }
                    items.header.set_text(&lines[0]);
                    items.activity.set_text(&lines[1]);
                    items.update.set_text(&lines[2]);
                    items.pause.set_checked(status.paused);
                    let _ = tray.set_tooltip(Some(tooltip(&status)));
                    shown = lines;
                }
            }
        }
        if let Some(window) = &mut window {
            window.refresh(&app);
        }
        #[cfg(any(windows, target_os = "linux"))]
        if let Some(panel) = &mut panel
            && panel.is_visible()
        {
            panel.refresh(&app);
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn build(items: &Items) -> Result<TrayIcon> {
    let menu = items.menu.clone();
    menu.append_items(&[
        &items.header,
        &items.no_devices,
        &items.add_device,
        &PredefinedMenuItem::separator(),
        &items.activity,
        &PredefinedMenuItem::separator(),
        &items.open,
        &items.pause,
        &PredefinedMenuItem::separator(),
        &items.update,
        &items.quit,
    ])?;
    let icon = image::load_from_memory_with_format(ICON, ImageFormat::Png)?.to_rgba8();
    let (width, height) = icon.dimensions();
    let icon = Icon::from_rgba(icon.into_raw(), width, height)?;
    let builder = TrayIconBuilder::new().with_tooltip("Carryover");
    // On Windows the icon opens the panel instead, since a menu closes on
    // every click.
    #[cfg(target_os = "macos")]
    let builder = builder.with_menu(Box::new(menu));
    // macOS recolors template images for light and dark menu bars.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon);
    #[cfg(windows)]
    let builder = {
        let _ = menu;
        builder.with_icon(icon)
    };
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

/// Replaces the device rows, which sit right after the header.
#[cfg(not(target_os = "linux"))]
fn set_devices(items: &mut Items, devices: &[DeviceStatus]) {
    for row in items.devices.drain(..) {
        let _ = items.menu.remove(&row);
    }
    let _ = items.menu.remove(&items.no_devices);
    if devices.is_empty() {
        let _ = items.menu.insert(&items.no_devices, 1);
        return;
    }
    for (index, device) in devices.iter().enumerate() {
        let row = IconMenuItem::new(
            &device.name,
            true,
            Some(picture(Kind::of(&device.name), device.connected)),
            None,
        );
        let _ = items.menu.insert(&row, 1 + index);
        items.devices.push(row);
    }
}

#[cfg(not(target_os = "linux"))]
fn picture(kind: Kind, online: bool) -> MenuIcon {
    MenuIcon::from_rgba(avatar::draw(kind, online), avatar::SIZE, avatar::SIZE)
        .expect("avatars are drawn at their stated size")
}

/// What a device row shows, to tell when the rows need redrawing.
#[cfg(not(target_os = "linux"))]
fn device_key(device: &DeviceStatus) -> String {
    format!("{}\t{}\t{}", device.id, device.name, device.connected)
}

#[cfg(not(target_os = "linux"))]
fn header_line(status: &Status) -> String {
    if status.paused {
        "Carryover · Paused".to_owned()
    } else {
        match status
            .devices
            .iter()
            .filter(|device| device.connected)
            .count()
        {
            0 => "Carryover".to_owned(),
            n => format!("Carryover · {n} online"),
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn activity_line(status: &Status) -> String {
    match &status.last_activity {
        Some(activity) => format!("{} · {}", activity.description, ago(activity.at.elapsed())),
        None => "Nothing synced yet".to_owned(),
    }
}

#[cfg(not(target_os = "linux"))]
fn update_line(state: &State) -> String {
    match state {
        State::Available(release) => format!("Install Update ({})", release.version),
        State::Installing(version) | State::Ready(version) => format!("Updating to {version}…"),
        _ => "Check for Updates…".to_owned(),
    }
}

fn tooltip(status: &Status) -> String {
    if status.paused {
        "Carryover: paused".to_owned()
    } else {
        match status.connected.len() {
            0 => "Carryover: no other devices connected".to_owned(),
            1 => "Carryover: connected to 1 device".to_owned(),
            n => format!("Carryover: connected to {n} devices"),
        }
    }
}
