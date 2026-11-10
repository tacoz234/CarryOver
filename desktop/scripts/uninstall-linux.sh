#!/bin/sh
# Compatibility entry point; the app asks for confirmation and stops syncing.
set -eu
: "${HOME:?HOME is required}"
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
case "$data" in /*) ;; *) data="$HOME/.local/share" ;; esac
exec "$data/Carryover/copyblip" uninstall "$@"
