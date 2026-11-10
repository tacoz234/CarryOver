#!/bin/sh
# Compatibility entry point for the archive. Installation happens in the wizard.
set -eu
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$source_dir/copyblip" setup "$@"
