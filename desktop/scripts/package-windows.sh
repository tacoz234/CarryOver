#!/bin/sh
# Builds a Windows zip already joined to this Mac's group and relay.
# Needs the MinGW toolchain: brew install mingw-w64
# The zip holds the group key, so delete it once it's on the PC.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/target/CopyBlip-Windows.zip"}
cd "$root"
cargo build --quiet --release --target x86_64-pc-windows-gnu
cargo build --quiet --release
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/CopyBlip"
cp target/x86_64-pc-windows-gnu/release/copyblip.exe "$stage/CopyBlip/"
# Joins with this Mac's invite link, so the PC gets its own device ID.
target/release/copyblip invite | grep '^copyblip://' \
  | target/release/copyblip --config-dir "$stage/CopyBlip" join >/dev/null 2>&1
for file in setup.cmd uninstall.cmd README.txt; do
  # Windows wants CRLF line endings in batch files.
  perl -pe 's/\n/\r\n/' "scripts/windows/$file" > "$stage/CopyBlip/$file"
done
rm -f "$out"
(cd "$stage" && zip -qr "$out" CopyBlip)
echo "Wrote $out"
