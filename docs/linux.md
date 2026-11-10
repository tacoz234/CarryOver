# Linux (KDE Plasma / Bazzite)

KDE Plasma is the target for Windows-style behavior. Linux uses the same setup, main-window and popup HTML and the same application commands as Windows: pairing, devices, Recent, copying history again, pause/resume, login startup, update controls, log access, and leaving/rejoining a group.

Clicking the tray icon opens the popup instead of a menu. It stays open while you use its controls, and closes on Escape, another click on the icon, or focus moving to another window. Installation runs through the app's wizard. A launcher action provides graphical uninstall; pairing and history are retained when uninstalling.

See [verification results](linux-verification.md) for what was actually tested and the remaining limits.

## Install a built package

Extract `Carryover-Linux-x86_64.tar.gz` (or the aarch64 build), then open `Carryover/copyblip`. The wizard installs it for your user and joins or creates a group. `sh Carryover/install.sh` opens the same wizard if your file manager doesn't launch executables. No root access is needed for the app installation.

On Bazzite/KDE, the runtime libraries and `wl-clipboard` are required. The x86_64 build was launched on the development Bazzite host with its existing GTK 3 and WebKitGTK 4.1 runtimes. This archive is dynamically linked, not a self-contained AppImage. Other distributions need compatible glibc, GTK 3, WebKitGTK 4.1, and D-Bus runtimes; X11 sessions also need `xclip`.

The installed executable lives in `${XDG_DATA_HOME:-~/.local/share}/Carryover/copyblip`. The application launcher entry includes **Uninstall Carryover** as an action. You can also run the installed executable with `uninstall`, or run `sh "${XDG_DATA_HOME:-$HOME/.local/share}/Carryover/uninstall.sh"`. The app asks for confirmation, stops the running agent, removes its launcher and login entry, and retains your group and history for reinstalling.

## Build

Install a current stable Rust toolchain and native dependencies. For Ubuntu 24.04 / Debian with WebKitGTK 4.1:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libdbus-1-dev wl-clipboard xclip
cd desktop
cargo build --release
./target/release/copyblip
```

On Fedora, the development packages are `gcc gcc-c++ pkgconf-pkg-config gtk3-devel webkit2gtk4.1-devel libayatana-appindicator-gtk3-devel dbus-devel`, plus `wl-clipboard` and `xclip`.

On immutable desktops such as Bazzite, build in a development container, then run the archive on the graphical host with its runtime libraries. Development dependencies don't need to be layered onto the host. The packaging script is `desktop/scripts/package-linux.sh`; it produces the archive and `copyblip-linux-<architecture>` for signed release manifests. Build on the oldest glibc distribution you intend to support.

## Desktop behavior

- **KDE Wayland:** native Wayland clipboard access uses `wl-copy`/`wl-paste`. GTK windows prefer XWayland when `DISPLAY` is available so the tray popup can be positioned at Plasma's activation coordinates. XWayland must be available for that positioning behavior. An explicit `GDK_BACKEND` environment setting is respected.
- **NVIDIA rendering:** Carryover automatically disables WebKit's DMA-BUF renderer when the NVIDIA kernel driver is loaded, preventing a blank window with an otherwise loaded page. An explicit `WEBKIT_DISABLE_DMABUF_RENDERER` setting is respected. Other GPU drivers keep the default renderer.
- **X11:** clipboard access uses `xclip`. The regular clipboard is synced; the primary selection used for middle-click paste is separate.
- **Tray:** the app implements StatusNotifierItem activation directly, including left-click and context activation. Plasma provides the host. Desktops without a StatusNotifier host need a tray extension or can use the application launcher for the main window.
- **Private clips:** advertised KDE password-manager and concealed clipboard markers are skipped before reading text/image content, matching Windows' handling of its native exclusion markers.
- **Other Wayland desktops:** native clipboard transport is tested with Sway, but GNOME Wayland isn't supported by this backend. GNOME isn't the parity target for this implementation.

Configuration, pairing, history, and logs live in `${XDG_CONFIG_HOME:-~/.config}/CopyBlip`. Login entries live in the sibling `autostart` directory. Relative XDG paths are ignored. A custom `--config-dir` is preserved through setup, launcher and login startup, and leaving/rejoining a group. The terminal commands (`init`, `join`, `invite`, `run`, and `clip`) work on Linux; `run --background` starts a detached process and writes to the profile's `copyblip.log`.

Discovery uses multicast DNS (UDP 5353); syncing uses TCP 47820 by default. Allow these on your trusted local network if your firewall blocks discovery or connections.

Signed updates recognize `linux-x86_64` and `linux-aarch64`, verify the signed manifest and binary checksum, replace the executable atomically, and restart with the same arguments. A real release must include the Linux artifact before installed Linux copies can update. The Mac publishing script doesn't build Linux artifacts: stage the Linux executable alongside the other platforms before generating the manifest. Packaging here doesn't publish a release.
