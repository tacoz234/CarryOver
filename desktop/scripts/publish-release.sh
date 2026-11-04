#!/bin/sh
# Builds the Mac and Windows programs, signs a manifest for them, and
# publishes them as a GitHub release, where installed copies find them.
# Also attaches what people download to install: Carryover-Setup.exe and
# Carryover-Mac.dmg.
#
#   scripts/publish-release.sh "What changed"
#   scripts/publish-release.sh --stage DIR "What changed"   # build and sign only
#
# Bump `version` in Cargo.toml first: copies only update to newer versions.
# Needs the signing key (see src/update.rs) and MinGW: brew install mingw-w64
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
key=${COPYBLIP_RELEASE_KEY:-"$HOME/.config/copyblip/release-signing-key"}
# Space-separated, to publish to more than one repo.
repos=${COPYBLIP_RELEASE_REPO:-tacoz234/CarryOver}
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
# The installers. copyblip.exe alone won't start: it needs WebView2Loader.dll
# beside it, which Carryover-Setup.exe brings. Installed copies already have
# the DLL, so updates replace only the program. Not named copyblip-*, so the
# manifest skips them.
scripts/package-windows.sh "$stage/Carryover-Setup.exe"
scripts/package-mac.sh "$stage"
rm -rf "$stage/Carryover.app"

if [ -n "$stage_only" ]; then
  echo "Staged Carryover $version in $stage"
  exit 0
fi
for repo in $repos; do
  gh release create "v$version" --repo "$repo" --title "Carryover $version" --notes "${notes:-Carryover $version}" \
    "$stage/Carryover-Setup.exe" "$stage/Carryover-Mac.dmg" "$stage"/copyblip-* "$stage/manifest.json" "$stage/manifest.json.sig"
done
echo "Published Carryover $version. Installed copies pick it up within 6 hours, or at their next start."
