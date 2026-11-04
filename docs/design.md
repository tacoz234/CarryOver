# Design

How Carryover looks and talks, on iPhone, iPad, Mac and Windows. Change this alongside the code when the design changes.

## Principles

- **Opening it is the whole job.** On the iPhone, opening the app sends what you copied or gets what you copied elsewhere. Everything on screen is for when that didn't work, or for copying something again.
- **Devices first.** The people-free version of a contacts row: your devices as circles, with who's online at a glance.
- **Quiet until something's wrong.** Success is a short status line. Problems say what happened and what to do, in one sentence.
- **Same app everywhere.** The iPhone app and the computer window share colors, device pictures and words, so it's obvious they're one product.

## Name

**Carryover**, one word, capital C. Never "CarryOver" or "Carry Over".

Some internal names are still CopyBlip, the app's earlier name, so paired devices and updates keep working. Users don't see them, so leave them alone:

- bundle ID `com.coledeterman.CopyBlip`, the Keychain item and the Xcode project and folder names
- the `copyblip` command and program file, and the `copyblip-releases` repo that copies before 0.3.2 update from
- the settings folders, `~/Library/Application Support/CopyBlip` on Mac and `%APPDATA%\CopyBlip` and `%LOCALAPPDATA%\CopyBlip` on Windows, the log `~/Library/Logs/CopyBlip.log`, and the Windows startup entry
- the `_copyblip._tcp` network service, and the protocol's labels and `CBLP` header

Invite links are `carryover://join?code=…`. Every platform still opens `copyblip://` links from before the rename.

## Color

Coral, from light at the top to deep at the bottom.

| Use | Value | Where |
| --- | --- | --- |
| Gradient, top | `#FF9158` | Device circles, app icon, tray pictures |
| Gradient, bottom | `#F0533C` | Same |
| Accent, iPhone | `#E0482D` | Tint: buttons, links, icons on grey. Readable on white and in dark mode. |
| Accent, computer, light | `#D03E26` with white text | Selected tab, primary buttons, switches, links. White text on it is 4.6:1. |
| Accent, computer, dark | `#FF7A59` with `#2A0D05` text | The same, in dark mode |
| Device screen inset | `#FFC2A6` | The screen inside tray computer and phone pictures |
| Online | system green; `#28A745` light and `#32D74B` dark on computers | The online dot |
| Offline | grey dot, circle at 45% opacity | Computers that aren't connected |
| Danger | `#D70015` | Remove and delete on hover |

Keep the green dot as the only green. Avoid purple and other cool accents, which read as a different product.

Text, backgrounds and lines use the platform's own colors: system colors on iPhone, and on computers a neutral grey palette (`#F5F5F7` background, `#FFF` cards, `#1D1D1F` text, `#6E6E73` secondary text) with dark-mode equivalents in `desktop/src/window.html`.

## App icon

A white clipboard with an arrow hopping up over it and landing pointing down, carrying it over, on the coral gradient. `assets/make-icons.swift` draws every size from code, so there's no source image to keep:

- iOS app icon, 1024px, square and opaque (iOS rounds it)
- `assets/icon-rounded.png` for docs, `assets/icon.ico` for the Windows program
- `assets/tray-mac.png`, a black template image macOS recolors for light and dark menu bars
- `assets/tray-windows.png`, in color

Small sizes get thicker strokes and less padding so they stay legible at 16px.

## Device pictures

A circle with a white glyph for each device, chosen by name: `iphone`, `ipad` or anything else, which shows as a computer. Mac minis show as a box, other computers as a monitor.

- **Online:** a green dot at the top right, with a ring of the background color around it.
- **Offline:** the circle at 45% opacity with a grey dot.
- **Phones and tablets** never show offline. They can't stay connected in the background, so they're described as syncing when the app is opened instead.
- **Actions** in the same row, Add a Device and Get Latest, are grey circles with an accent-colored glyph.

## iPhone and iPad

One screen, top to bottom:

1. **Status line** in a rounded grey card: "Ready", "Sending…", "Sent to Mac", or an orange warning with what to do.
2. **Device row:** every device in the group, then Add a Device and Get Latest. Names go under each circle, in footnote size, up to two lines.
3. **Send Clipboard** card: a backup for when opening didn't send, using iOS's paste button so it never shows the paste prompt.
4. **Recent:** what was sent and received, newest first, up to 20. Images show as thumbnails, text as a short preview. Tap one to copy it again. Long press for Copy and Delete. The ••• menu clears them all.

Settings, from the gear at the top left, holds the setup guide, the device list and Leave Group.

**Sizes.** On iPhone the device row is four flexible columns and Recent is two. On iPad and other wide screens, tiles keep a fixed size, device circles at 112 to 128 points and Recent cards at 200 to 240 points, and more fit across rather than growing. The layout works in every orientation; iPads need that for multitasking.

## Computer window

A sidebar with Devices, Recent, Add a Device and Settings, and the version at the bottom. It's the same page on Mac and Windows, drawn in a web view.

- **Devices:** a card of rows. Each row has the device picture, its name and one line about it: "This computer", "Online", "Online through DESKTOP-GAMING", "Offline · last seen 3 days ago" or "Syncs when you open Carryover". Offline rows show Remove on hover. Dots line up on the right whether or not a row has Remove.
- **Recent:** cards in a grid, 150px and up. Clicking copies, and a toast confirms it. × on hover removes one. Clear All needs a second click to confirm, since web view dialogs don't show everywhere.
- **Add a Device:** a QR code for the iPhone's Camera app, and an invite link for computers.
- **Settings:** General (pause, open at login), Updates and Troubleshooting (open the log). Sections are uppercase grey labels over white cards, in the style of system settings.

## Tray menu

The menu bar on Mac, the notification area on Windows:

- A title: "Carryover", "Carryover · 2 online" or "Carryover · Paused"
- Up to eight devices, each with its picture and online dot, then Add a Device…
- What last synced, then Open Carryover…, Pause Syncing, updates and Quit Carryover

The tooltip on Windows says how many devices are connected.

## Words

- Plain words, short sentences, no jargon. "Your computer" rather than "the desktop agent", and "copy" and "paste" rather than "clipboard data".
- Say what happened and what to do: "Couldn't connect to your computer. Make sure Carryover is running on it."
- Title Case for buttons and menu items ("Add a Device", "Send Clipboard"), sentence case for everything else.
- An ellipsis ("…") on actions that open something else, like Open Carryover….
- Warn plainly about the one secret: "Anyone with this code can read what you copy, so only share it with your own devices."

## iPhone setup wizard

Four stages: welcome, connect your devices, permission checks, and completion. A small Carryover wordmark and four progress marks stay at the top. The welcome illustration uses native device symbols around a coral clipboard. Rounded headings, grouped backgrounds, white cards and a soft coral glow carry through the flow, with system colors adapting to dark mode.

The primary action stays at the bottom in a material footer, with Back alongside it on the pairing stage. Content scrolls independently for small screens and larger text. Pairing choices share a card pattern with a checkmark and coral outline. Scanning and pasting sit together in an invite card; errors appear beside the input and preserve its contents. Completion uses the same cards for the invite and permission tips. The primary button uses deep coral with white text for contrast.

The permission checklist is shared with Settings → Setup Guide. Green checks report actual camera authorization, a successful local connection, or a successful explicit paste test. The paste check never claims to detect the persistent Ask/Allow setting. The checklist pauses automatic clipboard syncing while it is visible, retains the user's input, and gives a Settings action plus retry instructions.
