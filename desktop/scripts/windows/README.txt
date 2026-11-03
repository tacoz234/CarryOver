CopyBlip for Windows

1. Extract this folder.
2. Double-click setup.cmd.
3. If this PC isn't in a group yet, setup asks whether to join one or start a
   new one. To join, paste the invite link from a computer already in the
   group: open CopyBlip there, choose Add a Device, then Show Link. If its
   window won't open, double-click invite.cmd in %LOCALAPPDATA%\CopyBlip on
   a PC, or run "copyblip invite" on a Mac.
4. Setup installs Microsoft's WebView2 runtime if the PC doesn't have it,
   since CopyBlip's window needs it.
5. If Windows Firewall asks, allow CopyBlip on private networks.

CopyBlip's icon appears in the notification area by the clock. Click it to
open CopyBlip, or right-click for its menu. CopyBlip keeps itself up to date.

If this package has a config.json, it holds your group's pairing key and
setup joins that group. Delete this folder and the zip after setup.

To remove CopyBlip, run uninstall.cmd.
