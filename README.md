<img src="assets/icon.svg" width="64" height="64" alt="">

# CopyBlip

A shared clipboard across your Mac, Windows and iPhone devices. Copy on one, paste on another.

Created in [T3 Code](https://t3.codes).

## Status

Early. Text only, run from a terminal.

| Part | State |
| --- | --- |
| Mac agent | Tested on macOS 27 with the real clipboard |
| Windows agent | Compiles, but has never been run on Windows |
| iPhone app | Runs on a real iPhone; iPhone to Mac confirmed |

## Try it

Build from `desktop/` with `cargo build --release`, then:

```sh
copyblip init          # first device, prints a pairing code
copyblip join          # every other device, asks for the code
copyblip run           # on every device
```

For the iPhone, open `ios/CopyBlip.xcodeproj` in Xcode and run it on your phone, then enter the pairing code and follow the [setup guide](docs/iphone-setup.md). Opening the app sends your iPhone clipboard if you copied something new, and otherwise gets the latest copy from your other devices.

On a Mac, `desktop/scripts/install-mac-agent.sh` keeps the agent running as a login item, and `uninstall-mac-agent.sh` removes it. The log is in `~/Library/Logs/CopyBlip.log`.

Devices on the same network find each other. Use `--peer host:port` to reach one over a VPN.

The pairing code is the group's only secret, so treat it like a password. Anyone who has it can read everything the group copies.

On Windows, allow `copyblip` through the firewall when asked. On macOS, choose Always Allow if asked about clipboard access.

## Docs

- [iPhone setup](docs/iphone-setup.md): permissions and how to use it
- [Findings](docs/findings.md): what's been tested and what iOS allows
- [Protocol](docs/protocol.md): the wire format
