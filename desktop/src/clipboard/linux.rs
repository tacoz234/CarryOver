//! Clipboard access through the desktop's selection tools. They keep owning
//! received clips after a short-lived `copyblip clip` process exits.
use super::{Clipboard, Content};
use anyhow::{Context, Result, ensure};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;

#[derive(Clone, Copy)]
enum Backend {
    Wayland,
    X11,
}

pub struct LinuxClipboard {
    backend: Backend,
    state: Mutex<(u64, Option<Content>)>,
}

impl LinuxClipboard {
    pub fn new() -> Result<Self> {
        // Prefer the native Wayland selection: XWayland can miss copies from
        // native applications. Never silently fall back when its tools are absent.
        let (backend, tools) = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            (Backend::Wayland, &["wl-copy", "wl-paste"][..])
        } else {
            ensure!(
                std::env::var_os("DISPLAY").is_some(),
                "no graphical session: set WAYLAND_DISPLAY or DISPLAY"
            );
            (Backend::X11, &["xclip"][..])
        };
        for tool in tools {
            Command::new(tool)
                .arg(if matches!(backend, Backend::Wayland) {
                    "--version"
                } else {
                    "-version"
                })
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .with_context(|| {
                    format!(
                        "install {} to access the Linux clipboard",
                        if matches!(backend, Backend::Wayland) {
                            "wl-clipboard"
                        } else {
                            "xclip"
                        }
                    )
                })?;
        }
        Ok(Self {
            backend,
            state: Mutex::new((0, None)),
        })
    }

    fn reader(&self, target: &str) -> Command {
        match self.backend {
            Backend::Wayland => {
                let mut command = Command::new("wl-paste");
                command.arg("--no-newline");
                if target == "TARGETS" {
                    command.arg("--list-types");
                } else {
                    command.args(["--type", target]);
                }
                command
            }
            Backend::X11 => {
                let mut command = Command::new("xclip");
                command.args(["-selection", "clipboard", "-out", "-target", target]);
                command
            }
        }
    }

    fn read_selection(&self) -> Result<Option<Content>> {
        let targets = self
            .reader("TARGETS")
            .output()
            .context("couldn't query clipboard formats")?;
        // An empty selection has no owner, which both tools report as failure.
        if !targets.status.success() {
            return Ok(None);
        }
        let targets = String::from_utf8_lossy(&targets.stdout);
        let Some((target, text)) = preferred_target(&targets) else {
            return Ok(None);
        };
        let output = self
            .reader(target)
            .output()
            .context("couldn't read clipboard")?;
        ensure!(
            output.status.success(),
            "clipboard owner couldn't provide {target}"
        );
        Ok(Some(
            if target == "STRING" && matches!(self.backend, Backend::X11) {
                // ICCCM STRING is Latin-1, unlike UTF8_STRING and MIME text.
                Content::Text(output.stdout.into_iter().map(char::from).collect())
            } else if text {
                Content::Text(
                    String::from_utf8(output.stdout).context("clipboard text isn't UTF-8")?,
                )
            } else {
                Content::Image(output.stdout)
            },
        ))
    }
}

fn preferred_target(targets: &str) -> Option<(&str, bool)> {
    // Match the Windows backend's password-manager/history opt-outs using
    // the native hints emitted by KDE applications and password managers.
    // Do this before requesting any text or image bytes.
    if targets.lines().any(|line| {
        matches!(
            line.trim(),
            "x-kde-passwordManagerHint" | "application/x-nspasteboard-concealed-type"
        )
    }) {
        return None;
    }
    for target in [
        "text/plain;charset=utf-8",
        "UTF8_STRING",
        "text/plain",
        "STRING",
        "image/png",
    ] {
        if targets.lines().any(|line| line.trim() == target) {
            return Some((target, target != "image/png"));
        }
    }
    None
}

impl Clipboard for LinuxClipboard {
    fn change_token(&self) -> u64 {
        let mut state = self.state.lock().unwrap();
        if let Ok(content) = self.read_selection() {
            if state.1 != content {
                state.0 = state.0.wrapping_add(1);
                state.1 = content;
            }
        }
        state.0
    }

    fn read(&self) -> Result<Option<Content>> {
        self.read_selection()
    }

    fn write(&self, content: &Content) -> Result<()> {
        let (mime, bytes) = match content {
            Content::Text(text) => ("text/plain;charset=utf-8", text.as_bytes()),
            Content::Image(png) => ("image/png", png.as_slice()),
        };
        let mut command = match self.backend {
            Backend::Wayland => {
                let mut command = Command::new("wl-copy");
                command.args(["--type", mime]);
                command
            }
            Backend::X11 => {
                let mut command = Command::new("xclip");
                let target = if matches!(content, Content::Text(_)) {
                    "UTF8_STRING"
                } else {
                    mime
                };
                command.args(["-selection", "clipboard", "-in", "-target", target]);
                command
            }
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("couldn't start clipboard owner")?;
        let written = child
            .stdin
            .take()
            .context("clipboard input isn't available")?
            .write_all(bytes);
        let status = child.wait()?;
        written?;
        ensure!(
            status.success(),
            "couldn't set clipboard; check your graphical session"
        );
        Ok(())
    }

    fn startup_note(&self) -> Option<String> {
        Some("Linux clipboard syncing uses the regular clipboard, not the primary selection. Wayland requires a compositor supporting wl-clipboard (such as KDE Plasma or Sway).".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_utf8_text_to_images_and_ignores_unknown_formats() {
        assert_eq!(
            preferred_target("image/png\nUTF8_STRING\ntext/plain;charset=utf-8\n"),
            Some(("text/plain;charset=utf-8", true))
        );
        assert_eq!(
            preferred_target("TARGETS\nimage/png\n"),
            Some(("image/png", false))
        );
        assert_eq!(preferred_target("text/html\nimage/jpeg\n"), None);
    }

    #[test]
    fn private_clipboards_are_skipped_before_reading_contents() {
        for hint in [
            "x-kde-passwordManagerHint",
            "application/x-nspasteboard-concealed-type",
        ] {
            assert_eq!(
                preferred_target(&format!("text/plain;charset=utf-8\nimage/png\n{hint}\n")),
                None
            );
        }
    }

    // Run under an isolated X server: xvfb-run cargo test --lib -- --ignored.
    #[test]
    #[ignore = "requires a graphical session and owns its clipboard"]
    fn real_clipboard_detects_external_changes_and_preserves_images() {
        let clipboard = LinuxClipboard::new().unwrap();
        clipboard.write(&Content::Text("first\n".into())).unwrap();
        let initial = clipboard.change_token();
        assert_eq!(clipboard.change_token(), initial);
        assert_eq!(
            clipboard.read().unwrap(),
            Some(Content::Text("first\n".into()))
        );
        let other = LinuxClipboard::new().unwrap();
        other.write(&Content::Text("second — λ".into())).unwrap();
        assert_ne!(clipboard.change_token(), initial);
        assert_eq!(
            clipboard.read().unwrap(),
            Some(Content::Text("second — λ".into()))
        );
        let image = Content::Image(include_bytes!("../../../assets/icon.png").to_vec());
        other.write(&image).unwrap();
        assert_eq!(clipboard.read().unwrap(), Some(image));
    }
}
