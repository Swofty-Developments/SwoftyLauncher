#!/usr/bin/env bash
# Checks every tracked C and C++ source under native/ against native/.clang-format with clang-format 21, which skips
# the vendored and generated files native/.clang-format-ignore lists. CLANG_FORMAT names the clang-format to run.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
CF=${CLANG_FORMAT:-clang-format}
version=$("$CF" --version)
if [[ $version != *"version 21."* ]]; then
    echo "format_check.sh: needs clang-format 21, $CF is: $version" >&2
    exit 1
fi
git ls-files -z -- '*.c' '*.cc' '*.cpp' '*.h' '*.hpp' '*.inc' | xargs -0 "$CF" --dry-run -Werror
