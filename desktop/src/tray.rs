//! The menu bar icon on macOS and the notification area icon on Windows.
use crate::hub::{Hub, Status};
use anyhow::Result;
use image::{ImageFormat, Rgba, RgbaImage};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../../assets/tray-mac.png");
#[cfg(windows)]
const ICON: &[u8] = include_bytes!("../../assets/tray-windows.png");

/// How long an invite QR code stays on disk, since it holds the group key.
const INVITE_LIFETIME: Duration = Duration::from_secs(300);

struct Items {
    connected: MenuItem,
    activity: MenuItem,
    pause: CheckMenuItem,
    add_device: MenuItem,
    open_log: MenuItem,
    quit: MenuItem,
}

/// Runs the menu on the main thread until Quit. Sync keeps running on other
/// threads.
pub fn run(hub: Arc<Hub>, invite_link: String, log: PathBuf) -> ! {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::new().build();
    #[cfg(target_os = "macos")]
    {
        // A menu bar item only, with no Dock icon or app menu.
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }

    let items = Items {
        connected: MenuItem::new("Starting…", false, None),
        activity: MenuItem::new("Nothing synced yet", false, None),
        pause: CheckMenuItem::new("Pause Syncing", true, false, None),
        add_device: MenuItem::new("Add a Device…", true, None),
        open_log: MenuItem::new("Open Log", true, None),
        quit: MenuItem::new("Quit CopyBlip", true, None),
    };
    let mut tray: Option<TrayIcon> = None;
    let mut shown = (String::new(), String::new());

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1));
        // macOS needs the loop running before the icon is created.
        if let Event::NewEvents(StartCause::Init) = event {
            match build(&items) {
                Ok(icon) => tray = Some(icon),
                Err(error) => eprintln!("couldn't show the tray icon: {error:#}"),
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == items.pause.id() {
                hub.set_paused(items.pause.is_checked());
            } else if event.id == items.add_device.id() {
                if let Err(error) = show_invite(&invite_link) {
                    eprintln!("couldn't show the invite: {error:#}");
                }
            } else if event.id == items.open_log.id() {
                open(&log);
            } else if event.id == items.quit.id() {
                eprintln!("quit from the tray");
                std::process::exit(0);
            }
        }
        if let Some(tray) = &tray {
            let status = hub.status();
            let lines = (connected_line(&status), activity_line(&status));
            // Only touches the menu when something changed, to avoid flicker.
            if lines != shown {
                items.connected.set_text(&lines.0);
                items.activity.set_text(&lines.1);
                let _ = tray.set_tooltip(Some(tooltip(&status)));
                shown = lines;
            }
        }
    })
}

fn build(items: &Items) -> Result<TrayIcon> {
    let menu = Menu::new();
    menu.append_items(&[
        &items.connected,
        &items.activity,
        &PredefinedMenuItem::separator(),
        &items.pause,
        &items.add_device,
        &items.open_log,
        &PredefinedMenuItem::separator(),
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
    let builder = builder.with_icon(icon);
    Ok(builder.build()?)
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

fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{} min ago", elapsed.as_secs() / 60),
        _ => format!("{} hr ago", elapsed.as_secs() / 3600),
    }
}

/// Saves the invite as a QR code image and opens it, for scanning with the
/// iPhone's camera. Deleted after a few minutes, since it holds the group key.
fn show_invite(link: &str) -> Result<()> {
    let path = std::env::temp_dir().join("CopyBlip Invite.png");
    qr_image(link)?.save_with_format(&path, ImageFormat::Png)?;
    open(&path);
    std::thread::spawn(move || {
        std::thread::sleep(INVITE_LIFETIME);
        let _ = std::fs::remove_file(path);
    });
    Ok(())
}

fn qr_image(text: &str) -> Result<RgbaImage> {
    let code = qrcode::QrCode::new(text)?;
    let width = code.width();
    let colors = code.to_colors();
    const SCALE: usize = 12;
    // A four-module quiet zone, which scanners need.
    let side = (width + 8) * SCALE;
    let mut image = RgbaImage::from_pixel(side as u32, side as u32, Rgba([255, 255, 255, 255]));
    for (index, color) in colors.iter().enumerate() {
        if *color == qrcode::Color::Dark {
            let (x, y) = ((index % width + 4) * SCALE, (index / width + 4) * SCALE);
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    image.put_pixel((x + dx) as u32, (y + dy) as u32, Rgba([0, 0, 0, 255]));
                }
            }
        }
    }
    Ok(image)
}

fn open(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();
    #[cfg(windows)]
    let result = Command::new("explorer.exe").arg(path).spawn();
    if let Err(error) = result {
        eprintln!("couldn't open {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_naturally() {
        assert_eq!(ago(Duration::from_secs(5)), "just now");
        assert_eq!(ago(Duration::from_secs(150)), "2 min ago");
        assert_eq!(ago(Duration::from_secs(7300)), "2 hr ago");
    }

    #[test]
    fn invite_qr_is_square_with_a_quiet_zone() {
        let image = qr_image("copyblip://join?code=00").unwrap();
        assert_eq!(image.width(), image.height());
        assert_eq!(image.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
        // The top-left finder pattern starts right after the quiet zone.
        assert_eq!(image.get_pixel(4 * 12, 4 * 12), &Rgba([0, 0, 0, 255]));
    }
}
