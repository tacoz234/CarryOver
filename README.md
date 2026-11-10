<img src="assets/icon-rounded.png" width="96" height="96" alt="Carryover icon">

# Carryover

Encrypted clipboard sharing for macOS, Windows, Linux, and iPhone. Copy text and images on one device, then paste on another over your local network.

Created in [T3 Code](https://t3.codes).

## What it does

- **Text and images:** share text and PNG images between paired devices.
- **Local and encrypted:** devices discover each other on your network and exchange encrypted clips. No account or hosted sync service is required.
- **Easy pairing:** create a group, then add devices with a QR code or invite link.
- **Desktop controls:** a tray/menu bar icon, connected devices, recent clips, pause/resume, and login startup.

## Status

Early release. Desktop apps include graphical setup and a main window; terminal commands are also available. Support and testing vary by platform:

| Part | State |
| --- | --- |
| Mac agent | Tested on macOS 27 with the real clipboard |
| Windows agent | Runs on Windows 11; the window and updates aren't tested there yet |
| Linux agent | x86_64 KDE Plasma/Bazzite: Windows-style popup, setup and lifecycle; see [verification](docs/linux-verification.md) |
| iPhone app | Build with Xcode; syncs when opened. iPhone to Mac confirmed |

### Before you install

- Devices must be on the same local network. Carryover does not provide internet or remote-network syncing.
- The iPhone app currently requires an Xcode build; there is no App Store download. It syncs when opened, rather than continuously in the background.
- Linux desktop support targets KDE Plasma/Bazzite with compatible runtime libraries. The current published package is x86_64; GNOME Wayland is not supported. See the [Linux guide](docs/linux.md).

## Install

Download from the [latest release](https://github.com/tacoz234/CarryOver/releases/latest):

- **Windows:** `Carryover-Setup.exe`. Open it and follow the setup wizard. It installs Carryover just for you, with no administrator needed, adds it to the Start menu and to Settings › Apps (where you can uninstall it), and starts it in the notification area.
- **Mac:** `Carryover-Mac.dmg`. Open it, drag Carryover to Applications, and open it from there. The setup wizard adds it to the menu bar and, if you like, to your login items. The app isn't notarized yet, so the first time macOS blocks it: open **System Settings › Privacy & Security** and choose **Open Anyway**.

**Linux (KDE Plasma/Bazzite, x86_64):** download `Carryover-Linux-x86_64.tar.gz` from the latest release, extract it, and open `Carryover/copyblip` for the setup wizard. See [Linux setup](docs/linux.md) for dependencies and [verification](docs/linux-verification.md) for tested behavior.

The wizard asks whether this is your first device. If it is, it starts a group and shows a QR code for adding the next one. If not, paste the invite link from a device that already has Carryover (**Add a Device**, then **Show Link**).

For the iPhone, open `ios/CopyBlip.xcodeproj` in Xcode and run it on your phone, then follow the setup wizard. Join your other devices by scanning their QR code inside Carryover or pasting an invite link, or start a new group on your iPhone. See the [setup guide](docs/iphone-setup.md) for permissions. Opening the app sends your iPhone clipboard if you copied something new, and otherwise gets the latest copy from your other devices. Its main screen shows the devices you sync with and a Recent list of what you sent and received; tap an item to copy it again.

## Build it

Build from `desktop/` with `cargo build --release`. The program also works from a terminal:

```sh
copyblip init          # first device, prints a pairing code
copyblip join          # other computers, asks for the invite link or code
copyblip invite        # shows a QR code to scan with an iPhone
copyblip run           # on every computer
```

Build the installation packages with:

- **Windows:** `desktop/scripts/package-windows.ps1` from PowerShell on Windows. It builds natively without Git Bash. `package-windows.sh` also supports cross-building on a Mac with MinGW.
- **Mac:** `desktop/scripts/package-mac.sh` on a Mac.
- **Linux:** `desktop/scripts/package-linux.sh` on Linux; install the [native build dependencies](docs/linux.md#build) first.

Packages are written to `desktop/target`.

For development on a Mac, `desktop/scripts/install-mac-agent.sh` builds and runs the agent as a login item with a menu bar icon, and `uninstall-mac-agent.sh` removes it. The log is in `~/Library/Logs/CopyBlip.log`; on Windows it's `%APPDATA%\CopyBlip\copyblip.log`, and on Linux it's `${XDG_CONFIG_HOME:-~/.config}/CopyBlip/copyblip.log`.

On KDE Linux, look for the orange Carryover icon with a white clipboard and arrow; it is separate from KDE's clipboard manager.

The menu bar and notification area icons show what's connected and what last synced. On a Mac, the icon opens a menu. On Windows and KDE Linux, clicking it opens a panel that stays open while you use it, with your devices, Pause Syncing and updates, and closes when you click elsewhere. **Open Carryover** in either opens the main window, which has:

- **Devices:** every device in the group, with a green dot for the ones online, including ones connected through another computer.
- **Recent:** the last 20 clips sent or received, with image thumbnails. Click one to copy it again.
- **Add a Device:** a QR code for the iPhone and an invite link for computers.
- **Settings:** pause syncing, open at login, updates, and the log.

Mac and Windows installations update themselves from signed releases. The current Linux download is available for manual installation; automatic Linux updates require a future release with Linux included in the signed manifest. See [updates](docs/updates.md).

Devices discover each other automatically on the same local network, whether connected over Wi-Fi or Ethernet.

The pairing code is the group's only secret, so treat it like a password. Anyone who has it can read everything the group copies.

On Windows, allow `copyblip` through the firewall when asked. On macOS, choose Always Allow if asked about clipboard access.

## Docs

- [iPhone setup](docs/iphone-setup.md): permissions and how to use it
- [Linux setup](docs/linux.md): dependencies, installation, and desktop compatibility
- [Linux verification](docs/linux-verification.md): tested behavior and remaining limits
- [Updates](docs/updates.md): how computers update, and publishing a release
- [Findings](docs/findings.md): what's been tested and what iOS allows
- [Design](docs/design.md): colors, icon, layout and wording
- [Protocol](docs/protocol.md): the wire format

## License

Copyright © 2026 Cole Determan. Carryover is free software under the [GNU General Public License, version 3 or later](LICENSE): you can use, change and share it, and copies you share, changed or not, must be under the same license with their source available.

Contributions are welcome. By sending one, you agree it's under the same license, and that Cole Determan may also distribute it in builds on the App Store, whose terms add restrictions the GPL doesn't allow on its own.
