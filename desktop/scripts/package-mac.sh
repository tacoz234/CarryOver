#!/bin/sh
# Builds Carryover.app and Carryover-Mac.dmg, what people download on a Mac.
# Opening the app the first time shows its setup wizard, which offers to
# move it to Applications, joins or starts a group, and adds the login item.
#   package-mac.sh [out-folder]
# Runs on a Mac. The app is signed ad hoc, not with a Developer ID, so the
# first open needs System Settings › Privacy & Security › Open Anyway.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/target"}
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)

cargo build --quiet --release
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/Carryover.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/copyblip "$app/Contents/MacOS/copyblip"

# The icon, from the 1024px rounded one, at every size macOS asks for.
iconset="$work/Carryover.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
  sips -z $size $size ../assets/icon-rounded.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double ../assets/icon-rounded.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Carryover.icns"

# LSUIElement: a menu bar app with no Dock icon. The setup wizard shows one
# while it's open.
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Carryover</string>
  <key>CFBundleDisplayName</key><string>Carryover</string>
  <key>CFBundleIdentifier</key><string>com.coledeterman.carryover</string>
  <key>CFBundleExecutable</key><string>copyblip</string>
  <key>CFBundleIconFile</key><string>Carryover</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
codesign --force --deep --sign - "$app"

mkdir -p "$out"
rm -rf "$out/Carryover.app"
ditto "$app" "$out/Carryover.app"

# A disk image with the app beside a link to Applications, to drag onto.
image="$work/image"
mkdir "$image"
ditto "$app" "$image/Carryover.app"
ln -s /Applications "$image/Applications"
rm -f "$out/Carryover-Mac.dmg"
hdiutil create -quiet -volname Carryover -srcfolder "$image" -ov -format UDZO "$out/Carryover-Mac.dmg"
echo "Wrote $out/Carryover.app and $out/Carryover-Mac.dmg"
