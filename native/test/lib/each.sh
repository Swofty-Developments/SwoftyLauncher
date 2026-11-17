#!/usr/bin/env bash
# each.sh COMMAND...: runs each argument as a command of its own, in order, and stops at the first that fails.
set -e
for command in "$@"; do
    "$command"
done
