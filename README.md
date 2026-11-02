<img src="assets/icon-rounded.png" width="96" height="96" alt="CopyBlip icon">

# CopyBlip

A shared clipboard across your Mac, Windows and iPhone devices. Copy on one, paste on another.

Created in [T3 Code](https://t3.codes).

## Status

Early. Text and images, run from a terminal on computers.

| Part | State |
| --- | --- |
| Mac agent | Tested on macOS 27 with the real clipboard |
| Windows agent | Compiles, but has never been run on Windows |
| iPhone app | Runs on a real iPhone; iPhone to Mac confirmed |

## Try it

Build from `desktop/` with `cargo build --release`, then:

```sh
copyblip init          # first device, prints a pairing code
copyblip join          # other computers, asks for the invite link or code
copyblip invite        # shows a QR code to scan with an iPhone
copyblip run           # on every computer
```

For the iPhone, open `ios/CopyBlip.xcodeproj` in Xcode and run it on your phone, then enter the pairing code and follow the [setup guide](docs/iphone-setup.md). Opening the app sends your iPhone clipboard if you copied something new, and otherwise gets the latest copy from your other devices.

On a Mac, `desktop/scripts/install-mac-agent.sh` keeps the agent running as a login item with a menu bar icon, and `uninstall-mac-agent.sh` removes it. The log is in `~/Library/Logs/CopyBlip.log`.

For a PC, `desktop/scripts/package-windows.sh` builds a zip that's already joined to this Mac's group. On the PC, run `setup.cmd` from it. CopyBlip then appears in the notification area.

The menu bar and notification area icons show what's connected and what last synced. Their menus can pause syncing, show a QR code for adding a device, open the log, or quit.

Devices on the same network find each other. For other networks, use Tailscale or the relay; see [away from home](docs/away-from-home.md).

The pairing code is the group's only secret, so treat it like a password. Anyone who has it can read everything the group copies.

On Windows, allow `copyblip` through the firewall when asked. On macOS, choose Always Allow if asked about clipboard access.

## Docs

- [iPhone setup](docs/iphone-setup.md): permissions and how to use it
- [Away from home](docs/away-from-home.md): Tailscale and the relay
- [Relay](relay/README.md): deploying it to Cloudflare
- [Findings](docs/findings.md): what's been tested and what iOS allows
- [Protocol](docs/protocol.md): the wire format
