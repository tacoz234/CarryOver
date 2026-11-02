use anyhow::Result;
use std::sync::Mutex;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

pub trait Clipboard: Send + Sync {
    /// Cheap value that changes with the clipboard. Must not read its contents,
    /// because a read is what triggers the macOS privacy alert.
    fn change_token(&self) -> u64;
    /// `None` when there's no text or the owner marked it as not for syncing.
    fn read_text(&self) -> Result<Option<String>>;
    fn write_text(&self, text: &str) -> Result<()>;
    /// Anything the user should know about clipboard access before syncing.
    fn startup_note(&self) -> Option<String> {
        None
    }
}

/// In-process clipboard for tests.
#[derive(Default)]
pub struct MemoryClipboard {
    state: Mutex<(u64, Option<String>)>,
}

impl MemoryClipboard {
    /// Simulates the user copying something.
    pub fn copy(&self, text: &str) {
        let mut state = self.state.lock().unwrap();
        state.0 += 1;
        state.1 = Some(text.to_owned());
    }

    pub fn current(&self) -> Option<String> {
        self.state.lock().unwrap().1.clone()
    }

    pub fn writes(&self) -> u64 {
        self.state.lock().unwrap().0
    }
}

impl Clipboard for MemoryClipboard {
    fn change_token(&self) -> u64 {
        self.state.lock().unwrap().0
    }

    fn read_text(&self) -> Result<Option<String>> {
        Ok(self.current())
    }

    fn write_text(&self, text: &str) -> Result<()> {
        self.copy(text);
        Ok(())
    }
}
