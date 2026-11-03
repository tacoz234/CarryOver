# Updates

## On computers

CopyBlip on Mac and Windows checks for a new version 15 seconds after it starts, then every 6 hours. With **Install updates automatically** on (the default), it downloads the update, swaps it in, and restarts itself, which takes about a second. With it off, the window shows an **Install** button and the tray menu shows **Install Update**.

**Settings › Updates › Check for Updates** checks right away.

Each update is checked before it's used:

- **Signed.** The release's `manifest.json` must carry a valid Ed25519 signature from the CopyBlip release key, whose public half is built into the app. Someone who controls the download site, or the network, still can't ship an update.
- **Matching.** The downloaded program must match the size and SHA-256 listed in the signed manifest.
- **Newer.** Only versions newer than the running one are installed, so an old release can't be replayed to downgrade.

Releases hold only the programs. Pairing codes and settings stay on each device.

The iPhone app can't update itself. iOS only allows that through the App Store or TestFlight. Until then, rebuild it from Xcode.

## Publishing a release

On the Mac that holds the release key:

1. Bump `version` in `desktop/Cargo.toml`.
2. Run:

   ```sh
   desktop/scripts/publish-release.sh "What changed"
   ```

This builds the Mac and Windows programs, writes and signs `manifest.json`, and publishes them as a release on [tacoz234/copyblip-releases](https://github.com/tacoz234/copyblip-releases). That repo is public and holds only release files. The source stays private.

`--stage DIR` builds and signs into a folder without publishing, for testing with `COPYBLIP_UPDATE_FEED=http://127.0.0.1:<port>` pointed at a local web server serving that folder.

## The release key

The private key is at `~/.config/copyblip/release-signing-key`, outside the repo. **Back it up somewhere safe.** If it's lost, installed copies can't verify new releases, and each computer needs reinstalling by hand with a build that has a new key. If it leaks, anyone could ship an update, so rotate it the same way.
