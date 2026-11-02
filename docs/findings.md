# Findings

Last updated 2026-10-30, tested on macOS 27.2. There was no Xcode, iPhone or Windows machine available.

## Tested

- **Sync between two agent processes works.** They found each other over mDNS in about 1.25 seconds. Clips arrived in about 200 ms, including emoji and 500 KB of text. These runs used private pasteboards.
- **The real macOS clipboard works in both directions.**
- **Clips marked concealed are not synced**, which is what password managers rely on.
- **Bad input is handled.** Random bytes sent to the port don't stop the agent, and devices in other groups get nothing.

## macOS clipboard permission

Per the macOS 27 SDK (`NSPasteboard.h`), reading the general clipboard from code asks the user by default, and System Settings can set Always Allow or Always Deny for each app. Checking `changeCount` doesn't read contents, so the agent only reads after a change.

This Mac reports `alwaysAllow`, even for a brand new binary started outside T3 Code, so the alert has never been seen here. Another Mac may behave differently.

## Mac as a bridge to iPhone: unresolved

The idea is that writing to the Mac's clipboard lets Universal Clipboard carry it to the iPhone, so the iPhone needs no app.

- One write was advertised over Handoff. About 200 ms later a nearby device requested the types over Rapport, and the system logged the item as resumed on another device.
- Every later write in the logged runs reached the Handoff daemon but wasn't advertised. Plain `pbcopy` behaved the same as CopyBlip, so CopyBlip's writes aren't the cause. The reason is unknown.
- The logs redact the device's identity, so it may not have been an iPhone.

To settle it, write to the clipboard on the Mac and paste on the iPhone:

```sh
swiftc spikes/mac-bridge/probe.swift -o /tmp/copyblip-probe
/tmp/copyblip-probe write "copyblip test" --hold 60
```

Try several times with both devices awake and unlocked. The probe restores the previous clipboard when it ends.

## iPhone app (open the app to sync)

The Mac bridge failed when tried with a real iPhone on 2026-10-30: the Mac never advertised the write, and nothing arrived. The app takes a different approach. Opening it sends the iPhone clipboard if it changed, and otherwise fetches the latest clip.

Tested in the iOS 27.2 Simulator against a real agent:

- Mac copy, then opening the app: the text lands on the iPhone clipboard.
- Opening again with nothing new: shows "Up to date" and doesn't rewrite the clipboard.
- iPhone copy, then opening the app: the read was refused, because the headless Simulator can't show the "Allow Paste" prompt. The app explains how to allow it and leaves the unsent copy alone.
- The Swift networking code passes interop tests against two live agents. This covers sending, fetching, rejecting a wrong group, and Bonjour discovery.

On a real iPhone 17 Pro (2026-11-02):

- iPhone to Mac works through CopyBlip. The agent logged the clip from the phone.
- Mac to iPhone worked, but Universal Clipboard carried it, because the Mac and iPhone share an Apple account. The app then sent the same text back, which is harmless. The fetch path still needs a check with Handoff turned off.
- Reported as slow, with pop-ups. Remembered addresses now skip the 1–3 second network search.
- Once the agent stopped, the app blamed the pairing code. It now only does that when the encryption check fails.

## Still needs an iPhone and Xcode

1. Does a Back Tap Shortcut read and send the clipboard without a paste prompt or opening the app?
2. Can a Notification Service Extension write to the clipboard when a push arrives?

## Not explained

During one early run, each agent logged two incoming connections at startup that didn't speak the protocol. They were rejected, and didn't reproduce in 5 more agent runs or against a plain listener. The log now records the source address if they return.
