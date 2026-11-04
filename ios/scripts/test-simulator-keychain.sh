#!/bin/sh
# Regression check for the simulator's missing-entitlement save failure.
# Uses the real Keychain implementation with an isolated service name.
# Usage: test-simulator-keychain.sh <booted-simulator-UDID>
set -eu
[ "$#" = 1 ] || { echo "Usage: $0 <booted-simulator-UDID>" >&2; exit 1; }
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
sed 's/com.coledeterman.CopyBlip/com.coledeterman.CopyBlip.regression-probe/g' "$root/CopyBlip/Keychain.swift" > "$work/Keychain.swift"
cat > "$work/main.swift" <<'SWIFT'
import Foundation
import Security
let data = Data(repeating: 7, count: 32)
let saved = Keychain.save(data)
let loaded = Keychain.load() == data
Keychain.delete()
print("Keychain save=\(saved), readback=\(loaded)")
exit(saved && loaded ? 0 : 1)
SWIFT
cat > "$work/identity.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>application-identifier</key><string>A6GZ8F6PF5.com.coledeterman.CopyBlip.regression-probe</string>
</dict></plist>
PLIST
arch=$(uname -m)
[ "$arch" != x86_64 ] || arch=x86_64
xcrun --sdk iphonesimulator swiftc -target "$arch-apple-ios17.0-simulator" \
  "$work/Keychain.swift" "$work/main.swift" -o "$work/unsigned"
if xcrun simctl spawn "$1" "$work/unsigned"; then
  echo 'Expected an app without an identity to fail the Keychain check.' >&2
  exit 1
fi
xcrun --sdk iphonesimulator swiftc -target "$arch-apple-ios17.0-simulator" \
  "$work/Keychain.swift" "$work/main.swift" \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __entitlements -Xlinker "$work/identity.plist" \
  -o "$work/identified"
xcrun simctl spawn "$1" "$work/identified"
echo 'PASS: simulator Keychain saves and reads back with the app identity.'
