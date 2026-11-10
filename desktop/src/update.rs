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
const FEED: &str = "https://github.com/tacoz234/CarryOver/releases/latest/download";

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
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
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
        .user_agent(format!("Carryover/{VERSION}"))
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
    check_at(&feed(), &public_key()?)
}

fn check_at(feed: &str, key: &VerifyingKey) -> Result<Option<Release>> {
    let manifest = get(&format!("{feed}/{MANIFEST}"), MAX_MANIFEST)?;
    let signature = get(&format!("{feed}/{MANIFEST}.sig"), 1024)?;
    let manifest = verify(&manifest, &signature, key)?;
    Ok(release_for(manifest, VERSION, platform()))
}

/// Downloads the release's program and checks it against the signed manifest.
pub fn download(release: &Release) -> Result<Vec<u8>> {
    download_at(release, &feed())
}

fn download_at(release: &Release, feed: &str) -> Result<Vec<u8>> {
    let program = &release.program;
    ensure!(
        !program.name.contains(['/', '\\']) && !program.name.starts_with('.'),
        "update file name is unsafe"
    );
    let bytes = get(&format!("{feed}/{}", program.name), MAX_PROGRAM)?;
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
    #[cfg(target_os = "macos")]
    if bundle_for_executable(path).is_some() {
        return replace_bundle_executable(path, program);
    }
    let new = path.with_file_name(".copyblip-update");
    std::fs::write(&new, program)?;
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    // Renaming swaps the file in one step. The running program keeps the old one.
    std::fs::rename(&new, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&new);
    })?;
    Ok(())
}

/// Only the actual bundle executable is eligible, never arbitrary files in it.
#[cfg(target_os = "macos")]
fn bundle_for_executable(path: &Path) -> Option<&Path> {
    let macos = path.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then_some(bundle)
}

#[cfg(target_os = "macos")]
fn valid_bundle_signature(bundle: &Path) -> Result<bool> {
    Ok(std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?
        .success())
}

/// Sign an unmapped staging copy, then atomically replace only its executable.
/// The live Info.plist and resources stay byte-identical, retaining their seal
/// and bundle identity. Never re-sign the executing file in place on arm64.
#[cfg(target_os = "macos")]
fn replace_bundle_executable(path: &Path, program: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let bundle = bundle_for_executable(path).context("not a bundle executable")?;
    let suffix = hex::encode(rand::random::<[u8; 8]>());
    // Prefer a sibling staging directory to keep the atomic rename on the
    // same volume, including apps installed on an external disk. Fall back to
    // temporary storage if the app's parent isn't writable; a cross-volume
    // rename then fails safely without touching the installed executable.
    let sibling = bundle
        .parent()
        .context("no app parent folder")?
        .join(format!(".carryover-update-{suffix}"));
    let work = if std::fs::create_dir(&sibling).is_ok() {
        sibling
    } else {
        let temporary = std::env::temp_dir().join(format!("carryover-update-{suffix}"));
        std::fs::create_dir(&temporary)?;
        temporary
    };
    struct Cleanup(Option<std::path::PathBuf>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
    let mut cleanup = Cleanup(Some(work.clone()));
    let staged = work.join("Carryover.app");
    ensure!(
        std::process::Command::new("/usr/bin/ditto")
            .arg(bundle)
            .arg(&staged)
            .status()?
            .success(),
        "couldn't stage the app for updating"
    );
    let relative = path.strip_prefix(bundle)?;
    let executable = staged.join(relative);
    std::fs::write(&executable, program)?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))?;
    ensure!(
        std::process::Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-"])
            .arg(&staged)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?
            .success(),
        "couldn't sign the staged update"
    );
    ensure!(
        valid_bundle_signature(&staged)?,
        "the staged app signature is invalid"
    );

    // Keep both staging and backup outside the live bundle: even an extra
    // temporary file inside it invalidates its seal if the app crashes.
    let old = work.join("copyblip-old");
    (|| -> Result<()> {
        std::fs::copy(path, &old).context("couldn't preserve the old executable")?;
        std::fs::rename(&executable, path)
            .context("couldn't atomically install the staged update")?;
        if !valid_bundle_signature(bundle).unwrap_or(false) {
            let rollback = std::fs::rename(&old, path);
            if let Err(error) = rollback {
                cleanup.0 = None;
                return Err(error).with_context(|| {
                    format!(
                        "couldn't restore the app; recovery executable kept at {}",
                        old.display()
                    )
                });
            }
            anyhow::bail!("updated app signature is invalid; restored the old executable");
        }
        Ok(())
    })()
}

/// Repairs development bundles updated by an older executable-only updater.
/// Public 0.3.2 installs were bare binaries and do not need this migration.
#[cfg(target_os = "macos")]
pub fn repair_bundle_signature() -> Result<()> {
    let path = std::env::current_exe()?;
    if let Some(bundle) = bundle_for_executable(&path)
        && !valid_bundle_signature(bundle)?
    {
        replace_bundle_executable(&path, &std::fs::read(&path)?).context(
            "couldn't repair the app signature; reinstall Carryover from its disk image",
        )?;
    }
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
    spawn_updated(
        &std::env::current_exe()?,
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
    )
}

fn spawn_updated(program: &Path, arguments: &[std::ffi::OsString]) -> Result<i32> {
    let mut command = std::process::Command::new(program);
    command.args(arguments);
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
                eprintln!("installing Carryover {}", release.version);
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
                eprintln!("Carryover {} is available", release.version);
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
    #[cfg(target_os = "macos")]
    fn signed_test_bundle() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "carryover-bundle-test-{}",
            hex::encode(rand::random::<[u8; 8]>())
        ));
        let bundle = root.join("Carryover.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::create_dir_all(bundle.join("Contents/Resources")).unwrap();
        std::fs::write(
            bundle.join("Contents/Resources/test.txt"),
            b"sealed resource",
        )
        .unwrap();
        std::fs::write(bundle.join("Contents/Info.plist"), br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.coledeterman.carryover.update-test</string><key>CFBundleExecutable</key><string>copyblip</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>0.4.0</string></dict></plist>"#).unwrap();
        std::fs::copy("/usr/bin/true", bundle.join("Contents/MacOS/copyblip")).unwrap();
        assert!(
            std::process::Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&bundle)
                .status()
                .unwrap()
                .success()
        );
        assert!(valid_bundle_signature(&bundle).unwrap());
        bundle
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn updating_a_bundle_preserves_its_signature_and_identity() {
        let bundle = signed_test_bundle();
        let path = bundle.join("Contents/MacOS/copyblip");
        let info = std::fs::read(bundle.join("Contents/Info.plist")).unwrap();
        let resources =
            std::fs::read(bundle.join("Contents/_CodeSignature/CodeResources")).unwrap();
        replace(&path, &std::fs::read("/bin/echo").unwrap()).unwrap();
        assert!(valid_bundle_signature(&bundle).unwrap());
        assert_eq!(
            std::fs::read(bundle.join("Contents/Info.plist")).unwrap(),
            info
        );
        assert_eq!(
            std::fs::read(bundle.join("Contents/_CodeSignature/CodeResources")).unwrap(),
            resources
        );
        let output = std::process::Command::new(&path)
            .arg("updated")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "updated");
        std::fs::remove_dir_all(bundle.parent().unwrap()).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_bundle_signing_keeps_the_working_app() {
        let bundle = signed_test_bundle();
        let path = bundle.join("Contents/MacOS/copyblip");
        let before = std::fs::read(&path).unwrap();
        assert!(replace(&path, b"not a Mach-O executable").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(valid_bundle_signature(&bundle).unwrap());
        std::fs::remove_dir_all(bundle.parent().unwrap()).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn repairs_an_old_executable_only_bundle_update() {
        let bundle = signed_test_bundle();
        let path = bundle.join("Contents/MacOS/copyblip");
        std::fs::copy("/bin/echo", &path).unwrap();
        assert!(!valid_bundle_signature(&bundle).unwrap());
        replace_bundle_executable(&path, &std::fs::read(&path).unwrap()).unwrap();
        assert!(valid_bundle_signature(&bundle).unwrap());
        std::fs::remove_dir_all(bundle.parent().unwrap()).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_live_bundle_verification_restores_the_old_executable() {
        let bundle = signed_test_bundle();
        let path = bundle.join("Contents/MacOS/copyblip");
        let before = std::fs::read(&path).unwrap();
        // The live resource seal must not be silently replaced to bless edits.
        std::fs::write(
            bundle.join("Contents/Resources/test.txt"),
            b"changed resource",
        )
        .unwrap();
        assert!(replace(&path, &std::fs::read("/bin/echo").unwrap()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_dir_all(bundle.parent().unwrap()).unwrap();
    }

    /// Run explicitly against a locally served, signed newer-version manifest:
    /// COPYBLIP_UPDATE_FEED=http://127.0.0.1:PORT cargo test bundle_update_from_signed_feed -- --ignored
    #[test]
    #[ignore = "requires a locally served signed update feed"]
    #[cfg(target_os = "macos")]
    fn bundle_update_from_signed_feed() {
        assert!(
            std::env::var("COPYBLIP_UPDATE_FEED")
                .unwrap()
                .starts_with("http://127.0.0.1:")
        );
        let release = check().unwrap().expect("feed must contain a newer release");
        let program = download(&release).unwrap();
        let bundle = signed_test_bundle();
        let path = bundle.join("Contents/MacOS/copyblip");
        replace(&path, &program).unwrap();
        assert!(valid_bundle_signature(&bundle).unwrap());
        let output = std::process::Command::new(&path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .starts_with("copyblip ")
        );
        std::fs::remove_dir_all(bundle.parent().unwrap()).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_signed_feed_download_replace_and_restart() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::time::Instant;
        let dir = std::env::temp_dir().join(format!("carryover-update-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("restarted");
        let program = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            marker.display()
        )
        .into_bytes();
        let secret = [17; 32];
        let key = SigningKey::from_bytes(&secret).verifying_key();
        let manifest = serde_json::to_vec(&Manifest {
            version: "99.0.0".into(),
            notes: "Linux update fixture".into(),
            files: BTreeMap::from([(
                platform().into(),
                Program {
                    name: "copyblip-linux-test".into(),
                    size: program.len() as u64,
                    sha256: hex::encode(Sha256::digest(&program)),
                },
            )]),
        })
        .unwrap();
        for corrupt in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let feed = format!("http://{}", listener.local_addr().unwrap());
            let files = BTreeMap::from([
                ("/manifest.json".to_owned(), manifest.clone()),
                (
                    "/manifest.json.sig".to_owned(),
                    sign(&manifest, &secret).into_bytes(),
                ),
                (
                    "/copyblip-linux-test".to_owned(),
                    if corrupt {
                        vec![0; program.len()]
                    } else {
                        program.clone()
                    },
                ),
            ]);
            let server = std::thread::spawn(move || {
                for _ in 0..3 {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut request = String::new();
                    BufReader::new(&stream).read_line(&mut request).unwrap();
                    let path = request.split_whitespace().nth(1).unwrap();
                    let bytes = &files[path];
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .unwrap();
                    stream.write_all(bytes).unwrap();
                }
            });
            let release = check_at(&feed, &key).unwrap().unwrap();
            assert_eq!(release.version, "99.0.0");
            let download = download_at(&release, &feed);
            server.join().unwrap();
            let path = dir.join("copyblip");
            if corrupt {
                assert!(download.is_err());
                assert_eq!(std::fs::read(&path).unwrap(), program);
                continue;
            }
            std::fs::write(&path, b"old executable").unwrap();
            replace(&path, &download.unwrap()).unwrap();
            assert_eq!(
                spawn_updated(
                    &path,
                    &["--config-dir".into(), "profile with spaces".into()]
                )
                .unwrap(),
                0
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            while !marker.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                std::fs::read_to_string(&marker).unwrap(),
                "--config-dir\nprofile with spaces\n"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
