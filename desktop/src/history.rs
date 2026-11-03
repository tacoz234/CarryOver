//! The last few clips sent or received, for copying again from the window.
//!
//! Saved in the config folder when given one: `history/history.json` for the
//! list and text, with each image beside it as `<id>.png`. Kept in memory
//! otherwise, as in tests.
use crate::clipboard::Content;
use crate::hub::unix_now;
use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

pub const LIMIT: usize = 20;
/// Longest text shown on a card. The full text is copied.
const PREVIEW_CHARS: usize = 400;
/// Thumbnails are at most this many pixels on their long side.
const THUMBNAIL: u32 = 360;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Sent,
    Received,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    id: String,
    at: u64,
    direction: Direction,
    /// Who it came from, for received clips.
    #[serde(default)]
    from: Option<String>,
    /// Nil for images.
    #[serde(default)]
    text: Option<String>,
}

#[derive(Default)]
pub struct History {
    entries: VecDeque<Entry>,
    /// Image bytes when not saving to disk.
    images: HashMap<String, Vec<u8>>,
    /// Thumbnails as data URLs, made once per image.
    thumbnails: HashMap<String, String>,
    dir: Option<PathBuf>,
    /// Changes whenever the list does, so the window knows to redraw it.
    pub version: u64,
}

impl History {
    /// Loads a saved history from `dir`, and keeps saving there.
    pub fn open(dir: PathBuf) -> Self {
        let entries = std::fs::read(dir.join("history.json"))
            .ok()
            .and_then(|json| serde_json::from_slice(&json).ok())
            .unwrap_or_default();
        Self {
            entries,
            dir: Some(dir),
            version: 1,
            ..Self::default()
        }
    }

    pub fn add(&mut self, direction: Direction, from: Option<String>, id: &str, content: &Content) {
        let text = match content {
            Content::Text(text) => Some(text.clone()),
            Content::Image(png) => {
                if let Err(error) = self.save_image(id, png) {
                    eprintln!("couldn't save an image to the history: {error:#}");
                    return;
                }
                None
            }
        };
        self.remove(id);
        self.entries.push_front(Entry {
            id: id.to_owned(),
            at: unix_now(),
            direction,
            from,
            text,
        });
        while self.entries.len() > LIMIT {
            if let Some(old) = self.entries.pop_back() {
                self.forget_image(&old.id);
            }
        }
        self.changed();
    }

    fn save_image(&mut self, id: &str, png: &[u8]) -> Result<()> {
        match &self.dir {
            Some(dir) => {
                private_dir(dir)?;
                std::fs::write(dir.join(format!("{id}.png")), png)?;
            }
            None => {
                self.images.insert(id.to_owned(), png.to_vec());
            }
        }
        Ok(())
    }

    fn forget_image(&mut self, id: &str) {
        self.images.remove(id);
        self.thumbnails.remove(id);
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(format!("{id}.png")));
        }
    }

    /// The full clip, for copying again.
    pub fn content(&self, id: &str) -> Result<Content> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .context("that clip is no longer in the history")?;
        if let Some(text) = &entry.text {
            return Ok(Content::Text(text.clone()));
        }
        Ok(Content::Image(
            self.image(id).context("that image is gone")?,
        ))
    }

    fn image(&self, id: &str) -> Option<Vec<u8>> {
        match &self.dir {
            Some(dir) => std::fs::read(dir.join(format!("{id}.png"))).ok(),
            None => self.images.get(id).cloned(),
        }
    }

    pub fn remove(&mut self, id: &str) {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        if self.entries.len() != before {
            self.forget_image(id);
            self.changed();
        }
    }

    pub fn clear(&mut self) {
        let ids: Vec<String> = self.entries.drain(..).map(|entry| entry.id).collect();
        ids.iter().for_each(|id| self.forget_image(id));
        self.changed();
    }

    fn changed(&mut self) {
        self.version += 1;
        if let Some(dir) = &self.dir {
            let result = private_dir(dir)
                .and_then(|()| Ok(serde_json::to_vec(&self.entries)?))
                .and_then(|json| Ok(std::fs::write(dir.join("history.json"), json)?));
            if let Err(error) = result {
                eprintln!("couldn't save the history: {error:#}");
            }
        }
    }

    /// The list for the window, with text cut short and images as thumbnails.
    pub fn to_json(&mut self) -> Value {
        let entries = self.entries.clone();
        let items: Vec<Value> = entries
            .iter()
            .map(|entry| {
                let (preview, long) = match &entry.text {
                    Some(text) => {
                        let preview: String = text.chars().take(PREVIEW_CHARS).collect();
                        let long = preview.len() < text.len();
                        (Some(preview), long)
                    }
                    None => (None, false),
                };
                let thumbnail = entry
                    .text
                    .is_none()
                    .then(|| self.thumbnail(&entry.id))
                    .flatten();
                json!({
                    "id": entry.id,
                    "at": entry.at,
                    "direction": entry.direction,
                    "from": entry.from,
                    "text": preview,
                    "long": long,
                    "thumbnail": thumbnail,
                })
            })
            .collect();
        Value::Array(items)
    }

    fn thumbnail(&mut self, id: &str) -> Option<String> {
        if let Some(cached) = self.thumbnails.get(id) {
            return Some(cached.clone());
        }
        let made = make_thumbnail(&self.image(id)?).ok()?;
        self.thumbnails.insert(id.to_owned(), made.clone());
        Some(made)
    }
}

/// Clips can be anything copied, so only this user may read them. On
/// Windows, %APPDATA% is already private to the user.
fn private_dir(dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn make_thumbnail(png: &[u8]) -> Result<String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)?;
    let small = if image.width().max(image.height()) > THUMBNAIL {
        image.thumbnail(THUMBNAIL, THUMBNAIL)
    } else {
        image
    };
    let mut out = std::io::Cursor::new(Vec::new());
    small.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 0, 0, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn keeps_the_newest_and_drops_the_oldest() {
        let mut history = History::default();
        for n in 0..LIMIT + 5 {
            history.add(
                Direction::Sent,
                None,
                &n.to_string(),
                &Content::Text(format!("clip {n}")),
            );
        }
        let items = history.to_json();
        let items = items.as_array().unwrap();
        assert_eq!(items.len(), LIMIT);
        assert_eq!(items[0]["text"], format!("clip {}", LIMIT + 4));
        assert!(history.content("0").is_err());
    }

    #[test]
    fn images_get_thumbnails_and_copy_back_in_full() {
        let dir = std::env::temp_dir().join(format!("copyblip-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut history = History::open(dir.clone());
        let big = png(1200, 600);
        history.add(
            Direction::Received,
            Some("PC".into()),
            "img",
            &Content::Image(big.clone()),
        );
        let items = history.to_json();
        assert!(
            items[0]["thumbnail"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(items[0]["from"], "PC");
        assert_eq!(history.content("img").unwrap(), Content::Image(big));

        // Survives a restart.
        let mut reopened = History::open(dir.clone());
        assert_eq!(reopened.to_json().as_array().unwrap().len(), 1);
        reopened.clear();
        assert!(!dir.join("img.png").exists());
        assert!(
            History::open(dir.clone())
                .to_json()
                .as_array()
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn long_text_is_cut_short_for_the_card_only() {
        let mut history = History::default();
        let long = "x".repeat(PREVIEW_CHARS + 50);
        history.add(Direction::Sent, None, "a", &Content::Text(long.clone()));
        let items = history.to_json();
        assert_eq!(items[0]["text"].as_str().unwrap().len(), PREVIEW_CHARS);
        assert_eq!(items[0]["long"], true);
        assert_eq!(history.content("a").unwrap(), Content::Text(long));
    }
}
