use crate::protocol::{GROUP_KEY_LEN, GroupKey};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub device: String,
    group_key: String,
}

impl Config {
    pub fn new(key: &GroupKey) -> Self {
        Self {
            device: hex::encode(rand::random::<[u8; 8]>()),
            group_key: hex::encode(key),
        }
    }

    pub fn key(&self) -> Result<GroupKey> {
        parse_code(&self.group_key)
    }

    pub fn pairing_code(&self) -> &str {
        &self.group_key
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("config.json");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("no group set up in {}, run init or join", dir.display()))?;
        serde_json::from_str(&text).context("config.json is malformed")
    }

    pub fn save(&self, dir: &Path, force: bool) -> Result<()> {
        let path = dir.join("config.json");
        if path.exists() && !force {
            bail!("{} already holds a group, pass --force to replace it", path.display());
        }
        fs::create_dir_all(dir)?;
        write_private(&path, &serde_json::to_vec_pretty(self)?)
    }
}

pub fn parse_code(code: &str) -> Result<GroupKey> {
    let cleaned: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    let bytes = hex::decode(cleaned).context("pairing code isn't valid")?;
    ensure!(bytes.len() == GROUP_KEY_LEN, "pairing code is the wrong length");
    let mut key = [0u8; GROUP_KEY_LEN];
    key.copy_from_slice(&bytes);
    Ok(key)
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents)?;
    Ok(())
}

/// %APPDATA% is only readable by the user by default.
#[cfg(not(unix))]
fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    Ok(fs::write(path, contents)?)
}

pub fn default_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    Ok(base.context("can't find the user's app data folder")?.join("CopyBlip"))
}
