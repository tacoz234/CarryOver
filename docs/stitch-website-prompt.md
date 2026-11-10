# Carryover website prompt for Stitch

Paste the brief below into Stitch. Attach `assets/icon-rounded.png` as the brand reference. Actual app screenshots can be supplied as additional references; use synthetic clip content in any public screenshot.

---

Design a polished, responsive product landing page for **Carryover**, an open-source app that shares an encrypted clipboard between macOS, Windows, Linux, and iPhone over a local network. The page should make people understand the product in seconds and help them download the right build.

## Brand and visual direction

Use the attached Carryover icon: a white clipboard with an arrow carrying content over it, on a warm orange/coral background. The name is “Carryover,” one word with a capital C.

Create the feel of a thoughtfully designed desktop utility: calm, useful, warm, and precise. Use a predominantly light interface with an off-white `#F5F5F7` background, white surfaces, charcoal `#1D1D1F` text, and muted gray secondary text. Use a coral gradient from `#FF9158` to `#F0533C` sparingly for illustrations and icon backgrounds. Primary buttons should use deep coral `#D03E26` with white text. Green is reserved for online status. Support a corresponding dark theme using charcoal surfaces and `#FF7A59` accents. Avoid purple/blue gradients, oversized glows, glassmorphism, and generic SaaS dashboard styling.

Use clear system-style typography, generous spacing, restrained shadows, subtle borders, rounded app surfaces, and simple device glyphs. Make the product UI the visual centerpiece. Show desktop and mobile layouts, including a thoughtfully stacked 390px-wide mobile version.

## Page structure and copy

1. **Navigation:** Carryover icon and wordmark; Features, How It Works, Downloads, FAQ, and GitHub. A clear Download button. Use a compact mobile menu.

2. **Hero:** Headline: **“Copy here. Paste there.”** Supporting copy: **“Share text and images between your Mac, Windows PC, Linux computer, and iPhone. Encrypted, over your local network.”** Primary CTA: “Download Carryover.” Secondary CTA: “View on GitHub.” Add a small “Free and open source · Early release” line.

   Show a realistic app composition: a desktop window with sidebar tabs Devices, Recent, Add a Device, and Settings; device rows named “Home Mac,” “Linux PC,” and “iPhone”; warm device avatars; status dots; and a compact orange tray icon/popup. The iPhone status must say “Syncs when opened.” Use a small text or image clip moving between devices as a restrained visual explanation. Use supplied screenshots when available; otherwise keep UI mockups consistent with these actual features. Never use real pairing codes or personal clipboard content.

3. **How it works:** Three concise steps: “Install on your devices,” “Pair with an invite link or QR code,” and “Copy, then paste.” State that devices must be on the same local network. Explain briefly that opening the iPhone app sends or retrieves the latest clip.

4. **Features:** Present a varied, editorial layout rather than a wall of identical cards. Cover text and image sharing, encrypted local connections without an account or hosted sync service, recent clip history, device status, pause/resume, and easy pairing. Show a realistic Recent view with synthetic sample text and an image thumbnail. Do not present cloud storage, internet syncing, file transfer, team workspaces, or background iPhone syncing as features.

5. **Downloads:** Make this section especially clear and usable. Show all platform choices; visitors must be able to choose manually.
   - **macOS · Apple silicon:** “Download for Mac.” The app is not notarized yet; link to installation guidance.
   - **Windows:** “Download for Windows.” Uses a setup wizard.
   - **Linux · x86_64 · KDE Plasma/Bazzite:** “Download for Linux.” Explain that compatible runtime libraries are required and link to the Linux guide. Do not claim support for every Linux desktop or GNOME Wayland.
   - **iPhone:** “iPhone setup guide,” visually a secondary action. The app currently requires building with Xcode. There is no App Store or TestFlight download to advertise.

   Include a concise note that the current Linux build is installed manually; automatic Linux updates are not yet available in the published signed update feed.

6. **FAQ:** Address same-network requirements, encryption and pairing, iPhone syncing when opened, Linux desktop compatibility, and early-release status. Explain that devices exchange encrypted clips directly and recent clips are stored locally. Invite links contain the group's shared secret and should only be shared with the user's own devices. Do not claim zero local storage, a security audit, or certifications.

7. **Footer:** Carryover, “Copy here. Paste there.”, GitHub, Releases, Setup Guides, and “GPL-3.0-or-later.” No pricing plans, fake testimonials, invented usage statistics, email capture, or sign-in flow.

## Real destinations

- GitHub: https://github.com/tacoz234/CarryOver
- All downloads: https://github.com/tacoz234/CarryOver/releases/latest
- Mac: https://github.com/tacoz234/CarryOver/releases/latest/download/Carryover-Mac.dmg
- Windows: https://github.com/tacoz234/CarryOver/releases/latest/download/Carryover-Setup.exe
- Linux: https://github.com/tacoz234/CarryOver/releases/latest/download/Carryover-Linux-x86_64.tar.gz
- Linux guide: https://github.com/tacoz234/CarryOver/blob/clipboard-sync-v0/docs/linux.md
- iPhone guide: https://github.com/tacoz234/CarryOver/blob/clipboard-sync-v0/docs/iphone-setup.md
- License: https://github.com/tacoz234/CarryOver/blob/clipboard-sync-v0/LICENSE

## Interaction and accessibility

Create clear hover, pressed, and keyboard-focus states; semantic heading order; accessible contrast; readable body text; and comfortable touch targets. Respect reduced-motion preferences. Keep animations subtle, such as a single clip moving between device illustrations. Download buttons must use the real destinations above; navigation should scroll to the corresponding sections. The result should feel ready to implement as a fast, lightweight public website, with honest support notes and no invented functionality.
