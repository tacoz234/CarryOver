use anyhow::Result;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

#[cfg(any(windows, test))]
pub mod dib;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

/// What a copy holds. When a copy has both text and a picture, the text wins,
/// which matches what most apps paste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
}

impl Content {
    pub fn digest(&self) -> [u8; 32] {
        let (kind, bytes) = match self {
            Content::Text(text) => (b't', text.as_bytes()),
            Content::Image(png) => (b'i', png.as_slice()),
        };
        Sha256::new()
            .chain_update([kind])
            .chain_update(bytes)
            .finalize()
            .into()
    }

    /// "text" or "an image", for status messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Content::Text(_) => "text",
            Content::Image(_) => "an image",
        }
    }

    /// For logs, which never include the contents themselves.
    pub fn describe(&self) -> String {
        match self {
            Content::Text(text) => format!("{} bytes of text", text.len()),
            Content::Image(png) => format!("a {} byte image", png.len()),
        }
    }
}

pub trait Clipboard: Send + Sync {
    /// Cheap value that changes with the clipboard. Must not read its contents,
    /// because a read is what triggers the macOS privacy alert.
    fn change_token(&self) -> u64;
    /// `None` when there's nothing syncable or the owner marked it as not for syncing.
    fn read(&self) -> Result<Option<Content>>;
    fn write(&self, content: &Content) -> Result<()>;
    /// Anything the user should know about clipboard access before syncing.
    fn startup_note(&self) -> Option<String> {
        None
    }
}

/// In-process clipboard for tests.
#[derive(Default)]
pub struct MemoryClipboard {
    state: Mutex<(u64, Option<Content>)>,
}

impl MemoryClipboard {
    /// Simulates the user copying text.
    pub fn copy(&self, text: &str) {
        self.set(Content::Text(text.to_owned()));
    }

    /// Simulates the user copying an image.
    pub fn copy_image(&self, png: &[u8]) {
        self.set(Content::Image(png.to_vec()));
    }

    fn set(&self, content: Content) {
        let mut state = self.state.lock().unwrap();
        state.0 += 1;
        state.1 = Some(content);
    }

    /// The text on the clipboard, if it holds text.
    pub fn current(&self) -> Option<String> {
        match self.content() {
            Some(Content::Text(text)) => Some(text),
            _ => None,
        }
    }

    pub fn content(&self) -> Option<Content> {
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

    fn read(&self) -> Result<Option<Content>> {
        Ok(self.content())
    }

    fn write(&self, content: &Content) -> Result<()> {
        self.set(content.clone());
        Ok(())
    }
}
