#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod app;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod autostart;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod avatar;
pub mod clipboard;
pub mod config;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod control;
pub mod discovery;
pub mod history;
pub mod hub;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod install;
#[cfg(target_os = "linux")]
pub mod linux_desktop;
#[cfg(target_os = "linux")]
pub mod linux_install;
#[cfg(any(windows, target_os = "linux"))]
pub mod panel;
pub mod protocol;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod setup;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod tray;
pub mod update;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub mod window;
