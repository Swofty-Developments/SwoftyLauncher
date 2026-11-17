#!/usr/bin/env bash
# The Windows entry point (Git Bash or MSYS2 with zig and ninja on PATH): the same build as build.sh.
set -euo pipefail
exec "$(dirname "$(readlink -f "$0")")/build.sh" "$@"
