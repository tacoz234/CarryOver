use crate::protocol::{GROUP_KEY_LEN, GroupKey};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 47820;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub device: String,
    group_key: String,
    /// Addresses to connect to besides those found on the local network,
    /// such as Tailscale names, as `host:port`.
    #[serde(default)]
    pub peers: Vec<String>,
    /// Relay base URL, for devices on different networks.
    #[serde(default)]
    pub relay: Option<String>,
}

impl Config {
    pub fn new(key: &GroupKey) -> Self {
        Self {
            device: hex::encode(rand::random::<[u8; 8]>()),
            group_key: hex::encode(key),
            peers: Vec::new(),
            relay: None,
        }
    }

    /// A link that joins a device to this group and relay. Scanned as a QR code
    /// by the iPhone's camera, or pasted into `copyblip join`.
    pub fn invite_link(&self) -> String {
        let mut link = format!("copyblip://join?code={}", self.group_key);
        if let Some(relay) = &self.relay {
            let https = relay
                .replacen("wss://", "https://", 1)
                .replacen("ws://", "http://", 1);
            link.push_str(&format!("&relay={https}"));
        }
        link
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

/// Accepts `host`, `host:port`, an IPv4 or IPv6 address, or `[ipv6]:port`,
/// and returns `host:port`, using the default port if none was given.
pub fn normalize_peer(address: &str) -> Result<String> {
    let address = address.trim();
    ensure!(
        !address.is_empty() && !address.contains('/'),
        "{address:?} isn't a host or address"
    );
    if address.parse::<std::net::SocketAddr>().is_ok() {
        return Ok(address.to_owned());
    }
    if let Ok(ip) = address.parse::<std::net::IpAddr>() {
        return Ok(std::net::SocketAddr::new(ip, DEFAULT_PORT).to_string());
    }
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port: u16 = port
                .parse()
                .with_context(|| format!("{port:?} isn't a port number"))?;
            Ok(format!("{host}:{port}"))
        }
        _ => Ok(format!("{address}:{DEFAULT_PORT}")),
    }
}

/// Accepts `https://` or `wss://` relay URLs, and `http://` or `ws://` for
/// local testing. Returns the WebSocket form without a trailing slash.
pub fn normalize_relay(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    let rest = |scheme: &str| url.strip_prefix(scheme).filter(|rest| !rest.is_empty());
    if let Some(rest) = rest("https://").or_else(|| rest("wss://")) {
        Ok(format!("wss://{rest}"))
    } else if let Some(rest) = rest("http://").or_else(|| rest("ws://")) {
        Ok(format!("ws://{rest}"))
    } else {
        bail!("relay address should start with https://")
    }
}

/// Accepts an invite link or a bare pairing code. Returns the key and the
/// relay, if the link named one.
pub fn parse_invite(text: &str) -> Result<(GroupKey, Option<String>)> {
    let text = text.trim();
    let Some(query) = text.strip_prefix("copyblip://join?") else {
        return Ok((parse_code(text)?, None));
    };
    let mut code = None;
    let mut relay = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("code", value)) => code = Some(parse_code(value)?),
            Some(("relay", value)) => relay = Some(normalize_relay(value)?),
            _ => {}
        }
    }
    Ok((code.context("invite link has no code")?, relay))
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
    fn peers_get_a_port_when_missing() {
        assert_eq!(normalize_peer("macmini").unwrap(), "macmini:47820");
        assert_eq!(normalize_peer(" macmini:5000 ").unwrap(), "macmini:5000");
        assert_eq!(normalize_peer("100.64.0.1").unwrap(), "100.64.0.1:47820");
        assert_eq!(
            normalize_peer("fd7a:115c::1").unwrap(),
            "[fd7a:115c::1]:47820"
        );
        assert_eq!(
            normalize_peer("[fd7a:115c::1]:9").unwrap(),
            "[fd7a:115c::1]:9"
        );
        assert!(normalize_peer("host:notaport").is_err());
        assert!(normalize_peer("").is_err());
        assert!(normalize_peer("https://example.com").is_err());
    }

    #[test]
    fn relay_urls_become_websocket_urls() {
        assert_eq!(
            normalize_relay("https://relay.example.dev/").unwrap(),
            "wss://relay.example.dev"
        );
        assert_eq!(
            normalize_relay("http://127.0.0.1:8787").unwrap(),
            "ws://127.0.0.1:8787"
        );
        assert!(normalize_relay("relay.example.dev").is_err());
        assert!(normalize_relay("https://").is_err());
    }

    #[test]
    fn invite_links_round_trip() {
        let mut config = Config::new(&[7; 32]);
        assert_eq!(
            parse_invite(&config.invite_link()).unwrap(),
            ([7; 32], None)
        );
        config.relay = Some("wss://relay.example.dev".into());
        let link = config.invite_link();
        assert!(link.ends_with("&relay=https://relay.example.dev"));
        let (key, relay) = parse_invite(&format!(" {link}\n")).unwrap();
        assert_eq!(
            (key, relay.as_deref()),
            ([7; 32], Some("wss://relay.example.dev"))
        );
        assert_eq!(parse_invite(&hex::encode([7u8; 32])).unwrap().0, [7; 32]);
        assert!(parse_invite("copyblip://join?relay=https://x.dev").is_err());
        assert!(parse_invite("copyblip://join?code=abc").is_err());
    }

    #[test]
    fn older_configs_without_new_fields_still_load() {
        let old = r#"{"device":"ab","group_key":"00"}"#;
        let config: Config = serde_json::from_str(old).unwrap();
        assert!(config.peers.is_empty() && config.relay.is_none());
    }
}
