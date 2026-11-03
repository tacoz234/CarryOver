#[cfg(any(target_os = "macos", windows))]
pub mod app;
#[cfg(any(target_os = "macos", windows))]
pub mod autostart;
#[cfg(any(target_os = "macos", windows))]
pub mod avatar;
pub mod clipboard;
pub mod config;
pub mod discovery;
pub mod history;
pub mod hub;
pub mod protocol;
#[cfg(any(target_os = "macos", windows))]
pub mod tray;
pub mod update;
#[cfg(any(target_os = "macos", windows))]
pub mod window;
