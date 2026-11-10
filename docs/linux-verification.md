# Linux parity verification

Target: KDE Plasma on Bazzite, x86_64. Verification performed on November 10, 2026.

The final x86_64 release archive passed the isolated UI/lifecycle and real clipboard/sync checks. `cargo test --locked` passed 56 tests (39 library tests plus 17 protocol/sync integration tests); the graphical clipboard test also passed when run explicitly under Xvfb.

The popup page (`panel.html`), main window, setup page, panel command parsing and application actions are shared with Windows. Platform code handles tray activation, window positioning/focus, clipboard transport, installation, and login startup.

## Passed checks

| Area | Evidence |
| --- | --- |
| Actual Bazzite/KDE desktop | The final release executable loaded the host's native libraries, registered with the real Plasma StatusNotifierWatcher, displayed the popup for Activate and ContextMenu, received keyboard focus, dismissed on native Escape, and toggled closed on a second activation. Popup behavior passed after unlocking the session. A subsequent blank-setup report exposed a gap: mapping and accessibility checks alone did not establish that pixels were painted. The NVIDIA DMA-BUF fallback was then added and verified against real setup pixels on the host, without a test environment override. It used a temporary group and did not write the host clipboard. |
| Full setup and main UI | Real GTK/WebKit controls were exercised through AT-SPI and keyboard/mouse input in an isolated X11 desktop: invalid invite rejection, first-device setup, install, opening the window, invite link and QR page. |
| Popup controls | Same Windows popup page; left/context activation, pause/resume while remaining open, click-to-toggle, Escape, and focus-loss dismissal. |
| Settings | Login on/off, saved automatic-update choice, opening the correct log through the desktop opener, rejection of an unsigned update feed. |
| Recent | Real text and image clipboard changes appeared in history; clicking history restored exact text/PNG bytes; clear-history confirmation removed stored entries. Existing unit tests also verify thumbnail generation and history retention limits. |
| Group lifecycle | Leaving removed pairing/history and disabled startup; the wizard reopened and successfully rejoined the original synthetic group. |
| Installation lifecycle | The actual wizard installed the program and launcher; tests invoked the generated login and application launcher entries, including a custom profile path containing spaces. Graphical uninstall stopped syncing and removed the executable, launcher and login entry while retaining the group. Additional isolated unit checks cover upgrades, repeated removal and special characters in paths. |
| Background CLI | `run --background --tray` starts the configured profile and can be reopened through the launcher. |
| Real agent-to-agent sync | Two agent processes with separate X11 servers discovered each other over mDNS. Both preexisting clipboards stayed unchanged on connection. Unicode text, trailing newlines and exact PNGs synced in both directions without echoes. |
| Native Wayland clipboard | Headless Sway: external clipboard owners, Unicode, newlines, and exact PNG read/write. |
| Privacy and formats | UTF-8 priority, legacy X11 Latin-1, PNG, unsupported-format rejection, and unit checks that advertised private markers prevent content selection. |
| Updates | A local HTTP feed signed with a test key passed manifest verification, platform selection, download/checksum validation, atomic replacement, executable permissions and restart argument preservation. A corrupted download was rejected. The production verification key is unchanged. |
| Windows compatibility | `cargo check --locked --target x86_64-pc-windows-gnu` passed after the shared panel/update changes. |

## Blank-window regression (NVIDIA)

The setup page was present in the accessibility tree but painted as a solid blank surface. A host WebKit probe reproduced GBM buffer errors; disabling the DMA-BUF renderer produced the welcome text and controls. Carryover now selects that compatible renderer before starting GTK/WebKit when `/sys/module/nvidia` exists, while respecting explicit user overrides.

`python3 scripts/test-linux-rendering.py target/release/copyblip` verifies actual setup pixels on the unlocked host and fails a uniformly blank surface. The fixed release rendered 2,459 distinct pixel colors in its 560 × 580 setup client area. This check supplements the existing DOM and popup-behavior tests; the earlier verification did not catch this rendering failure.

## Connected-device tray regression

A real paired device exposed a panic in the Linux tray loop: it tried to insert device rows into an empty native menu, even though Linux uses the StatusNotifier popup instead. Native menu creation and updates are now compiled only for the platforms that use that code. The desktop UI regression test starts a real peer, waits for its device row to appear, and verifies the tray process survives the device-list refresh. Linux also uses the orange Carryover app icon, making it distinct from KDE's clipboard manager. After updating the installed and downloaded copies, the host agent was restarted and verified to remain running, registered with Plasma as Carryover with its 64×64 icon, and connected to the user's paired device without the panic.

## Repeatable checks

From `desktop/`, use a disposable development container for the desktop tests. They own clipboard contents, graphical sessions and app ports. Test dependencies include Xvfb, xauth, xclip, Openbox, xdotool, dbus-x11, at-spi2-core, python3-pyatspi, Sway and wl-clipboard.

```sh
cargo test --locked
cargo build --locked
xvfb-run -a env -u WAYLAND_DISPLAY cargo test --locked --lib clipboard::linux::tests::real_clipboard -- --ignored
xvfb-run -a scripts/test-linux-clipboard.sh target/debug/copyblip
python3 scripts/test-linux-sync.py target/debug/copyblip
scripts/test-linux-desktop.sh target/debug/copyblip
python3 scripts/test-linux-wayland.py target/debug/copyblip
```

Sway must run as an unprivileged user. The optional `python3 scripts/test-linux-plasma.py target/debug/copyblip` runs a brief popup check on the actual Plasma host and refuses to run if another Carryover agent owns the control port. It requires the host's Python GI bindings, xprop and xwininfo. It temporarily shows the test popup; it doesn't install the application or write the clipboard. The host must remain idle during that check so keyboard focus stays with the test popup.

Linux CI runs the unit/protocol tests, X11 clipboard tests, two-agent sync, full desktop flow and native Wayland clipboard checks.

## Limits of the evidence

- A physical Windows peer and iPhone weren't available for new end-to-end testing. Wire protocol and existing simulated phone/peer tests pass; this is not a claim of fresh physical-device interoperability testing.
- No real published Linux update was installed. The complete download/replace/restart path was tested using a local feed and a test signing key; unsigned feeds were rejected by the running UI.
- Actual logout/login wasn't performed. The generated login entry was executed and verified, but the user's desktop session wasn't ended.
- Mixed-DPI/multiple-monitor placement has geometry unit coverage, not an exhaustive physical display matrix. Linux's window manager controls focus, positioning, decorations and taskbar behavior, so literal pixel-for-pixel Windows shell behavior isn't guaranteed.
- aarch64 packaging is supported by the script and manifest selection, but only x86_64 was built and run. GNOME Wayland is outside this KDE-focused parity target.
