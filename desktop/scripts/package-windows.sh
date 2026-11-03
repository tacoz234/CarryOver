#!/bin/sh
# Builds a Windows zip from a Mac or Linux computer. On a PC, use
# package-windows.ps1 instead.
#   package-windows.sh [--with-group] [out.zip]
# By default the zip carries no group, and setup asks to join or start one.
# --with-group joins the zip to this computer's group, so the zip
# holds the group key; delete it once it's on the PC.
# Needs the MinGW toolchain: brew install mingw-w64
set -eu
with_group=false
if [ "${1:-}" = --with-group ]; then
  with_group=true
  shift
fi
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/target/Carryover-Windows.zip"}
cd "$root"
cargo build --quiet --release --target x86_64-pc-windows-gnu
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/Carryover"
cp target/x86_64-pc-windows-gnu/release/copyblip.exe "$stage/Carryover/"
# The window's web view loads this at start; without it, Windows won't run
# copyblip.exe at all.
loader=$(ls -t target/x86_64-pc-windows-gnu/release/build/webview2-com-sys-*/out/x64/WebView2Loader.dll | head -n 1)
cp "$loader" "$stage/Carryover/"
if $with_group; then
  cargo build --quiet --release
  # Joins with this computer's invite link, so the PC gets its own device ID.
  target/release/copyblip invite | grep '^carryover://' \
    | target/release/copyblip --config-dir "$stage/Carryover" join >/dev/null 2>&1
fi
for file in setup.cmd invite.cmd uninstall.cmd README.txt; do
  # Windows wants CRLF line endings in batch files.
  perl -pe 's/\r?\n/\r\n/' "scripts/windows/$file" > "$stage/Carryover/$file"
done
rm -f "$out"
(cd "$stage" && zip -qr "$out" Carryover)
echo "Wrote $out"
