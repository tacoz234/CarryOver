# Updates

## On computers

Carryover on Mac and Windows checks for a new version 15 seconds after it starts, then every 6 hours. With **Install updates automatically** on (the default), it downloads the update, swaps it in, and restarts itself, which takes about a second. With it off, the window shows an **Install** button and the tray menu shows **Install Update**.

**Settings › Updates › Check for Updates** checks right away.

Each update is checked before it's used:

- **Signed.** The release's `manifest.json` must carry a valid Ed25519 signature from the Carryover release key, whose public half is built into the app. Someone who controls the download site, or the network, still can't ship an update.
- **Matching.** The downloaded program must match the size and SHA-256 listed in the signed manifest.
- **Newer.** Only versions newer than the running one are installed, so an old release can't be replayed to downgrade.

Pairing codes and settings stay on each device. Mac app-bundle updates sign an isolated staging copy, atomically swap its executable, and verify the installed signature. If verification fails, the previous executable is restored. Resources, bundle identity and the existing Info.plist stay unchanged; the in-app version reports the updated program, while Finder's version changes when installing a new disk image. Bare-binary Mac installs keep their existing atomic update path.

Version 0.4.0 can also repair the bundle signature on startup after an older development build performed an executable-only update. If macOS refuses to open that damaged development bundle before startup runs, reinstall it from the current disk image. Public 0.3.2 Mac downloads were bare binaries, so they do not need this migration.

The iPhone app can't update itself. iOS only allows that through the App Store or TestFlight. Until then, rebuild it from Xcode.

## Publishing a release

On the Mac that holds the release key:

1. Bump `version` in `desktop/Cargo.toml`.
2. Run:

   ```sh
   desktop/scripts/publish-release.sh "What changed"
   ```

This builds the Mac and Windows programs and their installers (`Carryover-Setup.exe` and `Carryover-Mac.dmg`), writes and signs `manifest.json`, and publishes them as a release on [tacoz234/CarryOver](https://github.com/tacoz234/CarryOver/releases). Releases before 0.3.2 updated from the old `tacoz234/copyblip-releases` repo.

`--stage DIR` builds and signs into a folder without publishing, for testing with `COPYBLIP_UPDATE_FEED=http://127.0.0.1:<port>` pointed at a local web server serving that folder.

## The release key

The private key is at `~/.config/copyblip/release-signing-key`, outside the repo. **Back it up somewhere safe.** If it's lost, installed copies can't verify new releases, and each computer needs reinstalling by hand with a build that has a new key. If it leaks, anyone could ship an update, so rotate it the same way.

## Windows release smoke test

On Windows, `desktop/scripts/package-windows.ps1` builds the program and installer natively using the installed Windows Rust host target (MSVC or GNU). It does not require `sh` or Git Bash on PATH. Both builds use their lockfiles, and packaging restores any pre-existing `CARRYOVER_EXE` / `CARRYOVER_LOADER` environment values afterwards.

For an automatic-update smoke test, transfer a signed test feed from the release-key machine, serve it on localhost, and set `COPYBLIP_UPDATE_FEED` only for the test process. The test feed must contain an actually newer program version. Afterwards stop the test process, clear the override, and rerun the 0.4.0 installer to restore the release candidate while keeping pairing and history. Never publish the test version or replace the release key to make a test pass.
