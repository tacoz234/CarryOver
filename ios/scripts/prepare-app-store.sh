#!/bin/sh
# Archive and export Carryover. --upload sends the build to App Store Connect;
# it never creates a review submission or releases the app.
set -eu
case "${1:-}" in ""|--upload) ;; *) echo "Usage: $0 [--upload]" >&2; exit 2;; esac
root=$(cd "$(dirname "$0")/.." && pwd)
# Prefer full Xcode without changing the machine's global developer selection.
if [ -z "${DEVELOPER_DIR:-}" ]; then
  export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
fi
out="$root/build/app-store"
mkdir -p "$out"
xcodebuild -project "$root/CopyBlip.xcodeproj" -scheme CopyBlip \
  -configuration Release -destination 'generic/platform=iOS' \
  -archivePath "$out/Carryover.xcarchive" -derivedDataPath "$out/derived" \
  -allowProvisioningUpdates archive
options="$root/app-store/ExportOptions.plist"
if [ "${1:-}" = --upload ]; then
  cp "$options" "$out/UploadOptions.plist"
  /usr/libexec/PlistBuddy -c 'Set :destination upload' "$out/UploadOptions.plist"
  options="$out/UploadOptions.plist"
fi
xcodebuild -exportArchive -archivePath "$out/Carryover.xcarchive" \
  -exportPath "$out/export" -exportOptionsPlist "$options" \
  -allowProvisioningUpdates
