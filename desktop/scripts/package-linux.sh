#!/bin/sh
# Build a Linux archive on Linux. Pass an output folder as the first argument.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[ "$(uname -s)" = Linux ] || { echo 'Build this package on Linux.' >&2; exit 1; }
arch=$(uname -m)
case "$arch" in x86_64|aarch64) ;; *) echo "Unsupported architecture: $arch" >&2; exit 1 ;; esac
output=${1:-"$root/target"}
mkdir -p "$output"
output=$(cd "$output" && pwd)
cargo build --release
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/Carryover"
cp target/release/copyblip "$stage/Carryover/copyblip"
cp "$root/../assets/icon.png" "$stage/Carryover/carryover.png"
cp scripts/install-linux.sh "$stage/Carryover/install.sh"
cp scripts/uninstall-linux.sh "$stage/Carryover/uninstall.sh"
cp "$root/../LICENSE" "$stage/Carryover/LICENSE"
# Raw program can also be included in a signed release manifest.
cp target/release/copyblip "$output/copyblip-linux-$arch"
tar -czf "$output/Carryover-Linux-$arch.tar.gz" -C "$stage" Carryover
echo "Created $output/Carryover-Linux-$arch.tar.gz"
