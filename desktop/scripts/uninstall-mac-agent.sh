#!/bin/sh
# Stops copyblip and removes the login item. Keeps the group so a reinstall
# stays paired with your other devices.
set -eu
label=com.coledeterman.copyblip
launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
rm -f "$HOME/Library/LaunchAgents/$label.plist" "$HOME/Library/Application Support/CopyBlip/copyblip"
echo "CopyBlip stopped and removed from login items."
