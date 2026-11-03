#!/bin/sh
# Builds the Mac and Windows programs, signs a manifest for them, and
# publishes them as a GitHub release, where installed copies find them.
#
#   scripts/publish-release.sh "What changed"
#   scripts/publish-release.sh --stage DIR "What changed"   # build and sign only
#
# Bump `version` in Cargo.toml first: copies only update to newer versions.
# Needs the signing key (see src/update.rs) and MinGW: brew install mingw-w64
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
key=${COPYBLIP_RELEASE_KEY:-"$HOME/.config/copyblip/release-signing-key"}
repo=${COPYBLIP_RELEASE_REPO:-tacoz234/copyblip-releases}
stage_only=
if [ "${1:-}" = "--stage" ]; then
  stage=$2
  stage_only=1
  shift 2
fi
notes=${1:-}
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)

[ -f "$key" ] || { echo "No signing key at $key" >&2; exit 1; }
cargo build --quiet --release
cargo build --quiet --release --target x86_64-pc-windows-gnu

if [ -z "$stage_only" ]; then
  stage=$(mktemp -d)
  trap 'rm -rf "$stage"' EXIT
fi
mkdir -p "$stage"
cp target/release/copyblip "$stage/copyblip-macos-$(uname -m | sed s/arm64/aarch64/)"
cp target/x86_64-pc-windows-gnu/release/copyblip.exe "$stage/copyblip-windows-x86_64.exe"
target/release/copyblip release-manifest --key "$key" --version "$version" --notes "$notes" "$stage"

if [ -n "$stage_only" ]; then
  echo "Staged Carryover $version in $stage"
  exit 0
fi
gh release create "v$version" --repo "$repo" --title "Carryover $version" --notes "${notes:-Carryover $version}" \
  "$stage"/copyblip-* "$stage/manifest.json" "$stage/manifest.json.sig"
echo "Published Carryover $version. Installed copies pick it up within 6 hours, or at their next start."
