#!/bin/sh
# Starts two desktop agents on private pasteboards and runs the Swift tests
# against them. Never touches the real clipboard.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root/desktop" && cargo build --quiet
agent="$root/desktop/target/debug/copyblip"
dir=$(mktemp -d)
trap 'kill $a $b 2>/dev/null; rm -rf "$dir"' EXIT

"$agent" --config-dir "$dir/a" init >/dev/null
"$agent" --config-dir "$dir/a" pairing-code | "$agent" --config-dir "$dir/b" join >/dev/null 2>&1
"$agent" --config-dir "$dir/a" run --port 0 --pasteboard copyblip.interop.a 2>"$dir/a.log" & a=$!
"$agent" --config-dir "$dir/b" run --port 0 --pasteboard copyblip.interop.b 2>"$dir/b.log" & b=$!

port() { sed -n 's/.*syncing on port //p' "$1"; }
# Waits for the agents to connect to each other.
peered() { grep -q '^connected to ' "$1"; }
for _ in $(seq 50); do
  [ -n "$(port "$dir/a.log")" ] && [ -n "$(port "$dir/b.log")" ] && peered "$dir/a.log" && peered "$dir/b.log" && break
  sleep 0.1
done

COPYBLIP_INTEROP="$("$agent" --config-dir "$dir/a" pairing-code) $(port "$dir/a.log") $(port "$dir/b.log")" \
  swift test --no-parallel --package-path "$root/ios/CopyBlipKit" "$@"
