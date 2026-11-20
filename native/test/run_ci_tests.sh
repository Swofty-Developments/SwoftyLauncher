#!/usr/bin/env bash
# CI's native test step: every host and Wine test in native/test/manifest.py through run_tests.sh, NATIVE_TEST_JOBS
# compiles and host tests at a time (default: one per CPU the step may run on; tools/pin.sh pins nothing under CI)
# and NATIVE_WINE_JOBS Wine tests beside them. With `host` it runs and reports only the host tests, with `wine` only
# the Wine tests, and with no argument both. A line reports each test as it ends; then each test's output follows in
# a log group, and the step fails if any test failed. The Wine tests need Wine and an xvfb-run that gives every call
# its own display, as ci.yml sets up.
set -uo pipefail
part=${1:-}
case "$part" in
    '') targets=(test test-wine) ;;
    host) targets=(test) ;;
    wine) targets=(test-wine) ;;
    *)
        echo "usage: $0 [host | wine]" >&2
        exit 2
        ;;
esac
cd "$(dirname "$(readlink -f "$0")")/.."
test/run_tests.sh "${targets[@]}"
rc=$?
failed=()
while IFS=$'\t' read -r name kind title; do
    [ -z "$part" ] || [ "$kind" = "$part" ] || continue
    dir=build/test/$name
    status=ok
    if [ ! -f "$dir/ok" ]; then
        status=FAILED
        failed+=("$title")
    fi
    echo "::group::$status $(cat "$dir/seconds" 2>/dev/null || echo ?)s $title"
    cat "$dir/log" 2>/dev/null
    echo "::endgroup::"
done <build/test/tests.tsv
for title in "${failed[@]}"; do
    echo "::error title=Native test failed::$title"
done
[ $rc -eq 0 ] && [ ${#failed[@]} -eq 0 ]
