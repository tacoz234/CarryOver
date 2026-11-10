#!/bin/sh
# Real clipboard smoke test, isolated from the user's clipboard with xvfb-run.
set -eu
binary=${1:-target/debug/copyblip}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
unset WAYLAND_DISPLAY
text='Carryover Linux — clipboard test'
"$binary" clip "$text"
[ "$("$binary" clip)" = "$text" ]
[ "$(xclip -selection clipboard -out -target UTF8_STRING)" = "$text" ]
# A third-party clipboard owner, with an exact trailing newline.
printf 'from xclip\n' | xclip -selection clipboard -target UTF8_STRING
"$binary" clip > "$work/actual"
printf 'from xclip\n\n' > "$work/expected"
cmp "$work/expected" "$work/actual"
# Legacy X11 STRING is Latin-1, not UTF-8.
printf 'caf\351' | xclip -selection clipboard -target STRING
[ "$("$binary" clip)" = 'café' ]
# A PNG copied by Carryover can be retrieved by both Carryover and xclip.
root=$(cd "$(dirname "$0")/../.." && pwd)
"$binary" clip --image "$root/assets/icon.png"
"$binary" clip --save-image "$work/image.png"
cmp "$root/assets/icon.png" "$work/image.png"
xclip -selection clipboard -out -target image/png > "$work/external.png"
cmp "$root/assets/icon.png" "$work/external.png"
# Unsupported formats must not be interpreted as text.
printf '<b>html only</b>' | xclip -selection clipboard -target text/html
if "$binary" clip > "$work/unsupported" 2>&1; then
  echo 'Unsupported clipboard content was accepted' >&2
  exit 1
fi
echo 'Linux X11 text, image, and unsupported-format checks passed.'
