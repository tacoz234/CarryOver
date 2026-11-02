#!/bin/sh
# Installs copyblip as a login item that restarts if it stops.
# Undo with uninstall-mac-agent.sh.
set -eu
label=com.coledeterman.copyblip
root=$(cd "$(dirname "$0")/.." && pwd)
support="$HOME/Library/Application Support/CopyBlip"
plist="$HOME/Library/LaunchAgents/$label.plist"
log="$HOME/Library/Logs/CopyBlip.log"

cd "$root" && cargo build --release --quiet
mkdir -p "$support" "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
# A copy, so rebuilding or cleaning the repo doesn't break the running agent.
cp target/release/copyblip "$support/copyblip"

[ -f "$support/config.json" ] || "$support/copyblip" init

cat > "$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>ProgramArguments</key>
  <array><string>$support/copyblip</string><string>run</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>$log</string>
</dict>
</plist>
EOF

launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$plist"
echo "CopyBlip is running and will start at login. Log: $log"
