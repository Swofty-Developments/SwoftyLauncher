#!/usr/bin/env bash
# The core's schema check (src/core/registry/schema.cpp): its unit test, then the agreement test, which
# validates every example and must-fail case under each registries folder with the core and with
# tools/std/validate.py and fails on any difference in verdicts or error pointers (schema/agree.py), and
# the pattern matcher against Python's re (schema/agree_patterns.py) and swofty::json::merge_patch against
# stdschema.merge_patch (schema/agree_merge.py).
# Runs the schema test on the programs native/test/manifest.py builds for it into the folder given as $1. Run it as
# native/test/run_tests.sh schema, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh schema
OUT=$1
REPO=$(readlink -f ../..)
"$OUT/schema_test" "${SCHEMA_FUZZ_ROUNDS:-20000}" | tee "$OUT/schema_test.log"
grep '^speed:' "$OUT/schema_test.log" | sed "s/^/$(date -u +%FT%TZ) /" >>"$OUT/perf.log"
# A sanitizer build runs the same cases and a longer fuzz loop where the host compiler supports it.
if [ -x "$OUT/schema_test_san" ]; then
    "$OUT/schema_test_san" "${SCHEMA_SAN_FUZZ_ROUNDS:-100000}"
else
    echo "schema: no host compiler with -fsanitize=address,undefined; skipped the sanitizer build"
fi

# Each registries folder that exists: the SDK's real schemas and the schema tooling's fixtures, plus any
# extra folders (relative to the repository) in SCHEMA_AGREE_ROOTS.
ROOTS=(packages/sdk/std/registries tools/std/tests/fixtures/registries ${SCHEMA_AGREE_ROOTS:-})
checked=0
for root in "${ROOTS[@]}"; do
    [[ -d $REPO/$root ]] || continue
    name=$(echo "$root" | tr / _)
    (cd "$REPO" && python3 tools/std/validate.py --registries "$root" --json >"$OUT/python_$name.json") || true
    status=0
    (cd "$REPO" && "$OUT/schema_agree" "$root" "$root" >"$OUT/core_$name.json") || status=$?
    if [[ $status -gt 1 ]]; then
        echo "schema: the core couldn't compile a schema under $root (exit $status)" >&2
        exit 1
    fi
    echo "== $root"
    python3 schema/agree.py "$OUT/python_$name.json" "$OUT/core_$name.json"
    checked=$((checked + 1))
done
# The pattern matcher against Python's re on random patterns and texts.
python3 schema/agree_patterns.py "$OUT/schema_agree" "${SCHEMA_PATTERN_ROUNDS:-20000}"
# The keyed merge rule against stdschema.merge_patch, per rule and on random bases and patches.
python3 schema/agree_merge.py "$OUT/schema_agree" "${SCHEMA_MERGE_ROUNDS:-5000}"
if [[ $checked -eq 0 ]]; then
    echo "schema: no registries folder to check" >&2
    exit 1
fi
