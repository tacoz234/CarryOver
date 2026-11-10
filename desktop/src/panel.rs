//! The same popup page and commands on Windows and Linux.
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Panel;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Panel;

/// A request from the panel.
#[derive(serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Request {
    Ready,
    /// The page's height, to fit the panel to.
    Size {
        height: f64,
    },
    /// Open the main window at a tab.
    Open {
        tab: String,
    },
    Pause {
        on: bool,
    },
    CheckUpdate,
    InstallUpdate,
    Hide,
    Quit,
}

/// The main window's tab for a name from the page.
pub fn tab(name: &str) -> &'static str {
    match name {
        "add" => "add",
        "settings" => "settings",
        "recent" => "recent",
        _ => "devices",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_commands_and_tabs_are_shared() {
        for request in [
            r#"{"cmd":"ready"}"#,
            r#"{"cmd":"pause","on":true}"#,
            r#"{"cmd":"size","height":350}"#,
            r#"{"cmd":"open","tab":"recent"}"#,
            r#"{"cmd":"checkUpdate"}"#,
            r#"{"cmd":"installUpdate"}"#,
            r#"{"cmd":"hide"}"#,
            r#"{"cmd":"quit"}"#,
        ] {
            assert!(
                serde_json::from_str::<Request>(request).is_ok(),
                "{request}"
            );
        }
        assert_eq!(tab("recent"), "recent");
        assert_eq!(tab("untrusted"), "devices");
    }
}
