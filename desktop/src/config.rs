use crate::protocol::{GROUP_KEY_LEN, GroupKey};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 47820;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub device: String,
    /// LAN addresses supplied by the invite, as a fallback when mDNS is blocked.
    #[serde(default)]
    pub pairing_addresses: Vec<String>,
    group_key: String,
    /// Install updates as soon as they're found.
    #[serde(default = "yes")]
    pub auto_update: bool,
}

fn yes() -> bool {
    true
}

impl Config {
    pub fn new(key: &GroupKey) -> Self {
        Self {
            device: hex::encode(rand::random::<[u8; 8]>()),
            pairing_addresses: Vec::new(),
            group_key: hex::encode(key),
            auto_update: true,
        }
    }

    /// A link that joins a device to this group. Scanned as a QR code by the
    /// iPhone's camera, or pasted into `copyblip join`.
    pub fn invite_link(&self) -> String {
        let mut addresses = self.pairing_addresses.clone();
        if let Ok(interfaces) = if_addrs::get_if_addrs() {
            for interface in interfaces {
                let endpoint = std::net::SocketAddr::new(interface.ip(), DEFAULT_PORT).to_string();
                if local_endpoint(&endpoint) && !addresses.contains(&endpoint) {
                    addresses.push(endpoint);
                }
            }
        }
        let peers: String = addresses
            .iter()
            .filter(|address| local_endpoint(address))
            .take(16)
            .map(|address| format!("&peer={address}"))
            .collect();
        format!("carryover://join?code={}{}", self.group_key, peers)
    }

    pub fn from_invite(text: &str) -> Result<Self> {
        let mut config = Self::new(&parse_invite(text)?);
        config.pairing_addresses = text
            .split('?')
            .nth(1)
            .unwrap_or_default()
            .split('&')
            .filter_map(|pair| pair.strip_prefix("peer="))
            .filter(|address| local_endpoint(address))
            .take(16)
            .map(str::to_owned)
            .collect();
        Ok(config)
    }

    /// Saves changes to an existing config.
    pub fn update(&self, dir: &Path) -> Result<()> {
        write_private(&dir.join("config.json"), &serde_json::to_vec_pretty(self)?)
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
            bail!(
                "{} already holds a group, pass --force to replace it",
                path.display()
            );
        }
        fs::create_dir_all(dir)?;
        write_private(&path, &serde_json::to_vec_pretty(self)?)
    }
}

fn local_endpoint(text: &str) -> bool {
    match text.parse::<std::net::SocketAddr>() {
        Ok(std::net::SocketAddr::V4(address)) => {
            address.port() > 0 && (address.ip().is_private() || address.ip().is_link_local())
        }
        _ => false,
    }
}

/// Accepts an invite link or a bare pairing code, and returns the key.
/// Other parts of a link, like the relay older versions added, are ignored.
pub fn parse_invite(text: &str) -> Result<GroupKey> {
    // PowerShell starts text piped to a program with a byte order mark.
    let text = text.trim().trim_start_matches('\u{feff}');
    // copyblip:// is from before the app was renamed.
    let Some(query) = ["carryover://join?", "copyblip://join?"]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))
    else {
        return parse_code(text);
    };
    let code = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("code="))
        .context("invite link has no code")?;
    parse_code(code)
}

pub fn parse_code(code: &str) -> Result<GroupKey> {
    let cleaned: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    let bytes = hex::decode(cleaned).context("pairing code isn't valid")?;
    ensure!(
        bytes.len() == GROUP_KEY_LEN,
        "pairing code is the wrong length"
    );
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
    Ok(base
        .context("can't find the user's app data folder")?
        .join("CopyBlip"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_links_round_trip() {
        let link = Config::new(&[7; 32]).invite_link();
        assert_eq!(parse_invite(&format!(" {link}\n")).unwrap(), [7; 32]);
        assert_eq!(parse_invite(&hex::encode([7u8; 32])).unwrap(), [7; 32]);
        assert_eq!(
            parse_invite(&format!("\u{feff}{link}\r\n")).unwrap(),
            [7; 32]
        );
        // Links from versions that had a relay still work.
        assert_eq!(
            parse_invite(&format!("{link}&relay=https://relay.example.dev")).unwrap(),
            [7; 32]
        );
        // Links from before the rename still work.
        assert_eq!(
            parse_invite(&format!("copyblip://join?code={}", hex::encode([7u8; 32]))).unwrap(),
            [7; 32]
        );
        assert!(parse_invite("copyblip://join?relay=https://x.dev").is_err());
        assert!(parse_invite("copyblip://join?code=abc").is_err());
    }

    #[test]
    fn joining_keeps_only_lan_addresses() {
        let code = hex::encode([7; 32]);
        let config = Config::from_invite(&format!("carryover://join?code={code}&peer=192.168.50.117:47820&peer=10.0.0.8:47820&peer=example.com:80&peer=8.8.8.8:47820&peer=192.168.1.2:0")).unwrap();
        assert_eq!(config.key().unwrap(), [7; 32]);
        assert_eq!(
            config.pairing_addresses,
            ["192.168.50.117:47820", "10.0.0.8:47820"]
        );
        let saved = serde_json::to_vec(&config).unwrap();
        let restored: Config = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.pairing_addresses, config.pairing_addresses);
        assert!(
            Config::from_invite(&code)
                .unwrap()
                .pairing_addresses
                .is_empty()
        );
    }

    #[test]
    fn older_configs_still_load() {
        let old = r#"{"device":"ab","group_key":"00"}"#;
        let config: Config = serde_json::from_str(old).unwrap();
        assert!(config.auto_update);
        // Ones that saved addresses or a relay load too. They're dropped.
        let old = r#"{"device":"ab","group_key":"00","peers":["mac:1"],"relay":"wss://x.dev"}"#;
        assert!(serde_json::from_str::<Config>(old).is_ok());
    }
}
