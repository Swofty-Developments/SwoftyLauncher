# Promotion requests from engine-re (2026-10-30)

Rows in owned lanes that engine-re decompiled, gave a unique signature and had checked by an
independent blind verifier (`agree` row in `verify.tsv`). Under the lane rule only the owner may
raise them to L2, so they enter upstream at L1 (through the open `re(discover)` / `re(core)` PRs)
and are listed here for the owner.

| owner | lane | rows | file |
|---|---|---|---|
| re-career-teams | game | 701 | `re-career-teams.tsv` |
| car-re | car | 41 | `car-re.tsv` |
| ai-re | ai | 9 | `ai-re.tsv` |
| overlay | render | 4 | `overlay.tsv` |

Each file: `addr`, `name`, `sig` (signature id in `signatures.json`), `verify` (verifier and V-number of the agree row).

To promote a row after its PR is merged: `py -3 re/tools/kb.py --agent <owner> set functions <addr> level=L2`.
Alternatively the maintainer can release the lane (`kb.py release <lane>`) so engine-re can promote them.

All findings are static (decompile + disassembly), nothing is runtime-tested.
