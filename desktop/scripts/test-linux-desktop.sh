#!/bin/sh
# Run in a disposable container: this owns its X server, session bus and app ports.
# Dependencies: Xvfb, xauth, openbox, dbus-x11, at-spi2-core, python3-pyatspi.
set -eu
binary=${1:-target/debug/copyblip}
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export CARRYOVER_DESKTOP_TEST=isolated
export NO_AT_BRIDGE=0
export GTK_MODULES=atk-bridge
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
exec dbus-run-session -- xvfb-run -a -s '-screen 0 1280x800x24' sh -c '
  openbox >/tmp/carryover-test-openbox.log 2>&1 &
  exec python3 "$1" "$2"
' sh "$script_dir/test-linux-desktop.py" "$binary"
