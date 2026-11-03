#[cfg(any(target_os = "macos", windows))]
pub mod app;
#[cfg(any(target_os = "macos", windows))]
pub mod autostart;
pub mod clipboard;
pub mod config;
pub mod discovery;
pub mod hub;
pub mod protocol;
pub mod relay;
#[cfg(any(target_os = "macos", windows))]
pub mod tray;
pub mod update;
#[cfg(any(target_os = "macos", windows))]
pub mod window;
