#!/bin/sh
# Keep Xcode's simulated signing metadata: Keychain needs the app identity.
# Usage: run-simulator.sh <simulator-UDID>
set -eu
[ "$#" = 1 ] || { echo "Usage: $0 <simulator-UDID>" >&2; exit 1; }
root=$(cd "$(dirname "$0")/.." && pwd)
device=$1
build=${COPYBLIP_SIMULATOR_BUILD_DIR:-"$root/build/simulator"}
xcodebuild -project "$root/CopyBlip.xcodeproj" -scheme CopyBlip \
  -configuration Debug -sdk iphonesimulator -derivedDataPath "$build" build
app="$build/Build/Products/Debug-iphonesimulator/CopyBlip.app"
# CODE_SIGNING_ALLOWED=NO removes this section, breaking Keychain at runtime.
xcrun otool -l "$app/CopyBlip" | grep -q '__entitlements' || {
  echo 'Missing simulator app identity. Rebuild with Xcode signing enabled.' >&2
  exit 1
}
xcrun simctl bootstatus "$device" -b
xcrun simctl terminate "$device" com.coledeterman.CopyBlip 2>/dev/null || true
xcrun simctl install "$device" "$app"
xcrun simctl launch "$device" com.coledeterman.CopyBlip
