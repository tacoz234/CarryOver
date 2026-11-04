#[cfg(any(target_os = "macos", windows))]
pub mod app;
#[cfg(any(target_os = "macos", windows))]
pub mod autostart;
#[cfg(any(target_os = "macos", windows))]
pub mod avatar;
pub mod clipboard;
pub mod config;
#[cfg(any(target_os = "macos", windows))]
pub mod control;
pub mod discovery;
pub mod history;
pub mod hub;
#[cfg(any(target_os = "macos", windows))]
pub mod install;
#[cfg(windows)]
pub mod panel;
pub mod protocol;
#[cfg(any(target_os = "macos", windows))]
pub mod setup;
#[cfg(any(target_os = "macos", windows))]
pub mod tray;
pub mod update;
#[cfg(any(target_os = "macos", windows))]
pub mod window;
