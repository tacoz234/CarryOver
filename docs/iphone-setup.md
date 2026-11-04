# Setting up Carryover on iPhone

Once set up, opening Carryover is all it takes:

- **You copied something on the iPhone:** Carryover sends it to your computers.
- **Otherwise:** Carryover gets whatever you last copied on a computer and puts it on the iPhone clipboard.

Then paste as usual. First launch shows a setup wizard; the permission guide is also available from **Setup Guide** in Settings (the gear button).

## Join or start a group

1. Open Carryover and tap **Get Started**.
2. Choose **I already use Carryover** to join your other devices, or **This is my first device** to create a group, then tap **Create Group**.
3. To join, open **Add a Device** on your other device. Tap **Scan QR Code** inside Carryover on the iPhone, allow camera access, and point it at that device's QR code. Review the invite and tap **Join Group**. You can also paste an invite link or pairing code.
4. If you create a group, the completion screen shows a QR code and **Copy Invite Link** for adding your next device.
5. Use the permissions checklist to test paste access and check the local connection. Open Settings and choose **Paste from Other Apps → Allow** to stop repeated paste prompts, then continue to **Open Carryover**.

The scanner accepts only valid Carryover invites and doesn't join until you confirm **Join Group**. Camera permission is optional; if scanning is unsupported or access is denied, paste an invite instead. Existing paired installations keep their group and skip the wizard.

## 1. Allow pasting without asking

iOS normally asks "Allow Paste?" each time an app reads something copied in another app. Allowing it once in Settings stops the prompt.

1. Open **Settings › Apps › Carryover**, or tap **Open Carryover Settings** in the guide.
2. Tap **Paste from Other Apps**.
3. Choose **Allow**.

If **Paste from Other Apps** isn't listed, iOS hasn't asked Carryover about pasting yet. Copy some text, open Carryover, tap **Allow Paste** once, and the setting will appear.

The alert's **Allow Paste** approves a paste request; it doesn't select the persistent **Allow** preference. Setup and **Settings → Setup Guide** include **Test Paste Access**: copy text in Notes, return, and tap the test. It reads and discards the text without sending or saving it. A green check means that read worked, not that iOS's persistent setting was detected; iOS doesn't expose that setting to apps. Checks reset after returning from another app or Settings. Automatic syncing pauses while the checklist is open.

The checklist also shows actual camera authorization and whether an authenticated local connection succeeded. A failed connection does not by itself mean Local Network permission was denied; Wi-Fi and whether the computer is running also matter.

If you tap **Don't Allow** on the prompt, Carryover won't send that copy and will open this guide. It doesn't ask again for the same copy.

## 2. Allow Local Network

Carryover finds your computers on your Wi-Fi. The first time it looks, iOS asks permission. Choose **Allow**.

If you chose **Don't Allow**, turn on **Local Network** under **Settings › Apps › Carryover**.

## 3. Use it

- **iPhone to computer:** copy, open Carryover, then paste on the computer.
- **Computer to iPhone:** copy on the computer, open Carryover, then paste on the iPhone.

The iPhone and computer need to be on the same Wi-Fi, with Carryover running on the computer.

If something didn't sync, the main screen has buttons to send or get manually. The send button is iOS's own paste button, so it never shows the paste prompt.

## Good to know

- **Handoff:** if your Mac uses the same Apple account as your iPhone, Universal Clipboard may move copies between them before Carryover does. That's harmless. When the iPhone sends back what the Mac already has, the Mac doesn't rewrite its clipboard.
- **Conflicts:** if you copied on both devices since last opening Carryover, the iPhone's copy wins.

## Running in Simulator

Use `ios/scripts/run-simulator.sh <simulator-UDID>` to build, install and open the app. Keep Xcode signing enabled even for Simulator: `CODE_SIGNING_ALLOWED=NO` removes the simulated app identity, causing Keychain saves (both joining and creating groups) to fail with error -34018. This is a build configuration issue, not a camera or Wi-Fi permission.

`ios/scripts/test-simulator-keychain.sh <booted-simulator-UDID>` checks the real Keychain save/readback path with an isolated test service. Camera scanning still requires a supported physical iPhone.
