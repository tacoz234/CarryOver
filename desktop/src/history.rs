//! Clips this computer sent or received, for the window's Recent list, like
//! the iPhone app's. Kept in a folder: `history.json`, plus each image as
//! `<id>.png` with a small `<id>.thumb.png` for the window.
use crate::clipboard::Content;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How many clips to keep, matching the iPhone app.
pub const LIMIT: usize = 24;
const THUMBNAIL_SIZE: u32 = 320;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Sent,
    Received,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub direction: Direction,
    /// Names of the devices it went to, or the one it came from.
    pub peers: Vec<String>,
    /// None for images, which are saved as files.
    pub text: Option<String>,
}

pub struct History {
    folder: PathBuf,
    items: Vec<Item>,
}

impl History {
    /// Loads the history in `folder`, creating it if needed.
    pub fn open(folder: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&folder);
        let items = std::fs::read(folder.join("history.json"))
            .ok()
            .and_then(|json| serde_json::from_slice(&json).ok())
            .unwrap_or_default();
        Self { folder, items }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Newest first.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Adds an item whose image, if any, was already saved with `save_image`.
    pub fn add(&mut self, item: Item) {
        self.items.retain(|old| old.id != item.id);
        self.items.insert(0, item);
        for old in self.items.split_off(LIMIT.min(self.items.len())) {
            remove_files(&self.folder, &old.id);
        }
        self.save();
    }

    /// The full clip, for copying it again.
    pub fn content(&self, id: &str) -> Option<Content> {
        let item = self.items.iter().find(|item| item.id == id)?;
        match &item.text {
            Some(text) => Some(Content::Text(text.clone())),
            None => std::fs::read(image_path(&self.folder, id)).ok().map(Content::Image),
        }
    }

    pub fn thumbnail(&self, id: &str) -> Option<Vec<u8>> {
        std::fs::read(thumbnail_path(&self.folder, id)).ok()
    }

    pub fn delete(&mut self, id: &str) {
        self.items.retain(|item| item.id != id);
        remove_files(&self.folder, id);
        self.save();
    }

    pub fn clear(&mut self) {
        for item in std::mem::take(&mut self.items) {
            remove_files(&self.folder, &item.id);
        }
        self.save();
    }

    fn save(&self) {
        let result = serde_json::to_vec_pretty(&self.items)
            .map_err(anyhow::Error::from)
            .and_then(|json| Ok(std::fs::write(self.folder.join("history.json"), json)?));
        if let Err(error) = result {
            eprintln!("couldn't save recent clips: {error:#}");
        }
    }
}

/// Saves an image and its thumbnail. Slow for big images, so it runs before
/// the history is locked.
pub fn save_image(folder: &Path, id: &str, png: &[u8]) -> Result<()> {
    std::fs::write(image_path(folder, id), png)?;
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .context("couldn't read the image")?;
    image
        .thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE)
        .save_with_format(thumbnail_path(folder, id), image::ImageFormat::Png)?;
    Ok(())
}

fn image_path(folder: &Path, id: &str) -> PathBuf {
    folder.join(format!("{id}.png"))
}

fn thumbnail_path(folder: &Path, id: &str) -> PathBuf {
    folder.join(format!("{id}.thumb.png"))
}

fn remove_files(folder: &Path, id: &str) {
    let _ = std::fs::remove_file(image_path(folder, id));
    let _ = std::fs::remove_file(thumbnail_path(folder, id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("copyblip-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    fn text(id: &str) -> Item {
        Item {
            id: id.into(),
            at: 1,
            direction: Direction::Sent,
            peers: vec!["mac".into()],
            text: Some(format!("clip {id}")),
        }
    }

    #[test]
    fn keeps_the_newest_and_survives_reopening() {
        let folder = folder("limit");
        let mut history = History::open(folder.clone());
        for n in 0..LIMIT + 3 {
            history.add(text(&n.to_string()));
        }
        assert_eq!(history.items().len(), LIMIT);
        assert_eq!(history.items()[0].id, (LIMIT + 2).to_string());
        let reopened = History::open(folder.clone());
        assert_eq!(reopened.items(), history.items());
        assert_eq!(
            reopened.content(&(LIMIT + 2).to_string()),
            Some(Content::Text(format!("clip {}", LIMIT + 2)))
        );
        assert_eq!(reopened.content("0"), None);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn images_get_thumbnails_and_are_removed_with_their_item() {
        let folder = folder("images");
        let mut history = History::open(folder.clone());
        let mut png = Vec::new();
        image::RgbaImage::new(1000, 500)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        save_image(&folder, "pic", &png).unwrap();
        history.add(Item {
            text: None,
            ..text("pic")
        });
        let thumbnail = image::load_from_memory(&history.thumbnail("pic").unwrap()).unwrap();
        assert_eq!((thumbnail.width(), thumbnail.height()), (320, 160));
        assert_eq!(history.content("pic"), Some(Content::Image(png)));
        history.delete("pic");
        assert!(history.items().is_empty());
        assert!(!folder.join("pic.png").exists() && !folder.join("pic.thumb.png").exists());
        let _ = std::fs::remove_dir_all(folder);
    }
}
