#!/bin/sh
# Builds Carryover-Setup.exe, what people download on Windows. It carries
# copyblip.exe and the WebView2 loader that copyblip.exe needs beside it,
# unpacks them, and opens the setup wizard.
#   package-windows.sh [out.exe]
# Works on a Mac or Linux (needs MinGW: brew install mingw-w64), and on a PC
# with Rust's x86_64-pc-windows-gnu target.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/target/Carryover-Setup.exe"}
target=x86_64-pc-windows-gnu
cd "$root"
cargo build --quiet --release --target $target
CARRYOVER_EXE="$root/target/$target/release/copyblip.exe"
CARRYOVER_LOADER="$root/$(cd "$root" && ls -t target/$target/release/build/webview2-com-sys-*/out/x64/WebView2Loader.dll | head -n 1)"
export CARRYOVER_EXE CARRYOVER_LOADER
cargo build --quiet --release --target $target --manifest-path installer/Cargo.toml
mkdir -p "$(dirname "$out")"
cp "installer/target/$target/release/Carryover-Setup.exe" "$out"
echo "Wrote $out"
