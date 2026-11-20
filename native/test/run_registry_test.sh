#!/usr/bin/env bash
# Runs the registry test on the programs native/test/manifest.py builds for it into the folder given as $1. Run it as
# native/test/run_tests.sh registry, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh registry
OUT=$1
CASES=../../packages/sdk/std/tests/cases
"$OUT/pe_exports_test" "$OUT/with_register.dll" "$OUT/without_register.dll"
python3 "$CASES/../check_cases.py"
"$OUT/corpus_test" "$CASES"
"$OUT/corpus_test" --fake "$CASES"
python3 registry/ref.py "$CASES"
python3 registry/random_stacks.py --runner "$OUT/corpus_test" --count "${REGISTRY_STACKS:-300}" --seed "${REGISTRY_SEED:-1606}"
"$OUT/corpus_test" --bench "$OUT/perf.log"
"$OUT/older_test"
"$OUT/older_batch_test"
"$OUT/career_scope_test"
"$OUT/index_load_test"
"$OUT/declare_all_test"
"$OUT/header_schemas_test"
"$OUT/career_wait_test"
"$OUT/tolerant_edit_test"
"$OUT/iterate_test"
