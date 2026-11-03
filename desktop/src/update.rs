//! Updates the desktop app in place from signed releases.
//!
//! A release is a `manifest.json` listing one program per platform, with its
//! size and SHA-256, plus `manifest.json.sig`, an Ed25519 signature over the
//! manifest. The public key is built in, so only someone holding the release
//! key can ship an update, wherever the files are hosted. Programs are only
//! ever replaced with newer versions.
//!
//! Releases are made with `scripts/publish-release.sh`.
use anyhow::{Context, Result, ensure};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where releases are published. GitHub serves each file of the newest
/// release at this address. `COPYBLIP_UPDATE_FEED` overrides it for testing.
const FEED: &str = "https://github.com/tacoz234/copyblip-releases/releases/latest/download";

/// Verifies releases. The matching private key stays off the repo, in
/// `~/.config/copyblip/release-signing-key` on the machine that publishes.
const PUBLIC_KEY: &str = "38add7c844dd555e68f4c5394729654f0228251e38c28c4f2c2b76bace4ce140";

const MANIFEST: &str = "manifest.json";
/// Signed along with the manifest, so the signature can't be passed off as
/// one over anything else.
const CONTEXT: &[u8] = b"copyblip update manifest v1\n";
const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_PROGRAM: u64 = 128 * 1024 * 1024;
const FIRST_CHECK: Duration = Duration::from_secs(15);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    /// Keyed by `platform()`.
    pub files: BTreeMap<String, Program>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Program {
    /// The file's name in the release, next to the manifest.
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub program: Program,
}

/// This build's key in a manifest's `files`.
pub fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        _ => "unsupported",
    }
}

fn parse_version(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let mut numbers = [0; 3];
    for number in &mut numbers {
        *number = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

fn public_key() -> Result<VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(PUBLIC_KEY)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .context("this build has no update key")?;
    Ok(VerifyingKey::from_bytes(&bytes)?)
}

fn signed_message(manifest: &[u8]) -> Vec<u8> {
    [CONTEXT, manifest].concat()
}

/// Checks the signature, then parses the manifest.
pub fn verify(manifest: &[u8], signature: &[u8], key: &VerifyingKey) -> Result<Manifest> {
    let signature = hex::decode(String::from_utf8_lossy(signature).trim())
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
        .context("update signature is malformed")?;
    key.verify_strict(&signed_message(manifest), &signature)
        .context("update signature doesn't match")?;
    serde_json::from_slice(manifest).context("update manifest is malformed")
}

/// For publishing: returns the signature file's contents.
pub fn sign(manifest: &[u8], secret: &[u8; 32]) -> String {
    hex::encode(
        SigningKey::from_bytes(secret)
            .sign(&signed_message(manifest))
            .to_bytes(),
    )
}

/// The public key for a secret, as written in `PUBLIC_KEY`.
pub fn public_key_for(secret: &[u8; 32]) -> String {
    hex::encode(SigningKey::from_bytes(secret).verifying_key().to_bytes())
}

/// Picks this platform's program from a verified manifest, if it's newer.
pub fn release_for(manifest: Manifest, current: &str, platform: &str) -> Option<Release> {
    if !is_newer(&manifest.version, current) {
        return None;
    }
    let program = manifest.files.get(platform)?.clone();
    Some(Release {
        version: manifest.version,
        notes: manifest.notes,
        program,
    })
}

fn feed() -> String {
    std::env::var("COPYBLIP_UPDATE_FEED")
        .unwrap_or_else(|_| FEED.to_owned())
        .trim_end_matches('/')
        .to_owned()
}

fn get(url: &str, limit: u64) -> Result<Vec<u8>> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(format!("CopyBlip/{VERSION}"))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("couldn't download {url}"))?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()?)
}

/// Returns a newer release for this platform, if there is one.
pub fn check() -> Result<Option<Release>> {
    let feed = feed();
    let manifest = get(&format!("{feed}/{MANIFEST}"), MAX_MANIFEST)?;
    let signature = get(&format!("{feed}/{MANIFEST}.sig"), 1024)?;
    let manifest = verify(&manifest, &signature, &public_key()?)?;
    Ok(release_for(manifest, VERSION, platform()))
}

/// Downloads the release's program and checks it against the signed manifest.
pub fn download(release: &Release) -> Result<Vec<u8>> {
    let program = &release.program;
    ensure!(
        !program.name.contains(['/', '\\']) && !program.name.starts_with('.'),
        "update file name is unsafe"
    );
    let bytes = get(&format!("{}/{}", feed(), program.name), MAX_PROGRAM)?;
    ensure!(
        bytes.len() as u64 == program.size,
        "update download is the wrong size"
    );
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == program.sha256.to_lowercase(),
        "update download doesn't match its checksum"
    );
    Ok(bytes)
}

/// Replaces the running program on disk. It keeps running until restarted.
pub fn install(program: &[u8]) -> Result<()> {
    replace(&std::env::current_exe()?, program)
}

#[cfg(unix)]
fn replace(path: &Path, program: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let new = path.with_file_name(".copyblip-update");
    std::fs::write(&new, program)?;
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    // Renaming swaps the file in one step. The running program keeps the old one.
    std::fs::rename(&new, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&new);
    })?;
    Ok(())
}

/// Windows can't overwrite a running program, but it can rename it.
#[cfg(windows)]
fn replace(path: &Path, program: &[u8]) -> Result<()> {
    let old = path.with_file_name("copyblip.old.exe");
    let new = path.with_file_name("copyblip.new.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::write(&new, program)?;
    std::fs::rename(path, &old)?;
    if let Err(error) = std::fs::rename(&new, path) {
        let _ = std::fs::rename(&old, path);
        anyhow::bail!("couldn't put the update in place: {error}");
    }
    Ok(())
}

/// Removes what an earlier update left behind.
pub fn clean_up() {
    if let Ok(path) = std::env::current_exe() {
        let _ = std::fs::remove_file(path.with_file_name("copyblip.old.exe"));
    }
}

/// Starts the program again, now updated, and returns the exit code this
/// process should end with. A LaunchAgent restarts it on a failure code, so
/// on macOS under launchd nothing is started here.
pub fn relaunch() -> Result<i32> {
    #[cfg(target_os = "macos")]
    if std::env::var("XPC_SERVICE_NAME").as_deref() == Ok(crate::autostart::LABEL) {
        return Ok(75);
    }
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.args(std::env::args_os().skip(1));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // A hidden console, which --background then lets go of.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .context("couldn't start the updated program")?;
    Ok(0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(String),
    /// Installed and waiting to restart.
    Ready(String),
    Failed(String),
}

/// Checks for updates in the background, and installs them when allowed.
pub struct Updater {
    state: Mutex<State>,
    automatic: AtomicBool,
    busy: AtomicBool,
}

impl Updater {
    pub fn start(automatic: bool) -> Arc<Self> {
        clean_up();
        let updater = Arc::new(Self {
            state: Mutex::new(State::Idle),
            automatic: AtomicBool::new(automatic),
            busy: AtomicBool::new(false),
        });
        let background = updater.clone();
        thread::spawn(move || {
            thread::sleep(FIRST_CHECK);
            loop {
                background.run(false);
                thread::sleep(CHECK_EVERY);
            }
        });
        updater
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    pub fn set_automatic(&self, automatic: bool) {
        self.automatic.store(automatic, Ordering::Relaxed);
    }

    /// Checks now, and installs if allowed.
    pub fn check_now(self: &Arc<Self>) {
        let updater = self.clone();
        thread::spawn(move || updater.run(false));
    }

    /// Installs the available update.
    pub fn install_now(self: &Arc<Self>) {
        let updater = self.clone();
        thread::spawn(move || updater.run(true));
    }

    fn set(&self, state: State) {
        *self.state.lock().unwrap() = state;
    }

    fn run(&self, chosen: bool) {
        if matches!(self.state(), State::Ready(_)) || self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let release = match self.state() {
            State::Available(release) if chosen => Some(release),
            _ => {
                self.set(State::Checking);
                match check() {
                    Ok(Some(release)) => Some(release),
                    Ok(None) => {
                        self.set(State::UpToDate);
                        None
                    }
                    Err(error) => {
                        eprintln!("couldn't check for updates: {error:#}");
                        self.set(State::Failed("Couldn't check for updates.".to_owned()));
                        None
                    }
                }
            }
        };
        if let Some(release) = release {
            // Development builds never replace themselves on their own.
            let automatic = self.automatic.load(Ordering::Relaxed) && !cfg!(debug_assertions);
            if chosen || automatic {
                self.set(State::Installing(release.version.clone()));
                eprintln!("installing CopyBlip {}", release.version);
                match download(&release).and_then(|program| install(&program)) {
                    Ok(()) => self.set(State::Ready(release.version)),
                    Err(error) => {
                        eprintln!("couldn't install the update: {error:#}");
                        self.set(State::Failed(format!(
                            "Couldn't install {}.",
                            release.version
                        )));
                    }
                }
            } else {
                eprintln!("CopyBlip {} is available", release.version);
                self.set(State::Available(release));
            }
        }
        self.busy.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "version": version,
            "files": {"macos-aarch64": {"name": "copyblip-macos-aarch64", "size": 3, "sha256": "ab"}}
        }))
        .unwrap()
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.9.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("0.3", "0.2.0"));
        assert!(!is_newer("0.3.0-beta", "0.2.0"));
    }

    #[test]
    fn only_signed_manifests_are_accepted() {
        let secret = [9; 32];
        let key = VerifyingKey::from_bytes(
            &hex::decode(public_key_for(&secret))
                .unwrap()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        let signed = manifest("0.3.0");
        let signature = sign(&signed, &secret);
        assert_eq!(
            verify(&signed, signature.as_bytes(), &key).unwrap().version,
            "0.3.0"
        );
        // Any change to the manifest breaks the signature.
        assert!(verify(&manifest("9.3.0"), signature.as_bytes(), &key).is_err());
        let other = sign(&signed, &[8; 32]);
        assert!(verify(&signed, other.as_bytes(), &key).is_err());
        assert!(verify(&signed, b"not hex", &key).is_err());
    }

    #[test]
    fn only_newer_releases_for_this_platform_count() {
        let parse = |version| serde_json::from_slice::<Manifest>(&manifest(version)).unwrap();
        let release = release_for(parse("0.3.0"), "0.2.0", "macos-aarch64").unwrap();
        assert_eq!(release.program.name, "copyblip-macos-aarch64");
        assert!(release_for(parse("0.2.0"), "0.2.0", "macos-aarch64").is_none());
        assert!(release_for(parse("0.3.0"), "0.2.0", "windows-x86_64").is_none());
    }

    #[test]
    fn the_built_in_key_is_valid() {
        public_key().unwrap();
    }

    #[test]
    fn installing_swaps_the_file() {
        let dir = std::env::temp_dir().join(format!("copyblip-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(if cfg!(windows) {
            "copyblip.exe"
        } else {
            "copyblip"
        });
        std::fs::write(&path, b"old").unwrap();
        replace(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
