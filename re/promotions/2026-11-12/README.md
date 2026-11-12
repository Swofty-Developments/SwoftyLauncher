# My Team promotions (2026-11-12)

Raises the 40 My Team rows from #995 that have a verified `func_start` signature and an `agree` row
from a blind verifier to L2. No other row changes.

| owner | lane | rows |
|---|---|---|
| career-re | career | 37 |
| re-career-teams | game | 2 |
| - (unowned) | gamedb | 1 |

`applied.tsv`: owner, addr, name, subsystem, signature id, verifier, agree row (V-number), decision.

The 39 rows in owned lanes were raised with `kb.py --force` (agent `vmknolle-myteam`, not the verifier),
so they need the lane owner's review: reject the PR, or drop single rows, if an owner disagrees.
The rows can be demoted again with `py -3 re/tools/kb.py --agent <owner> set functions <addr> level=L1`.

Bindings: `bindgen.py` exports 2731 bindings instead of 2691 (+40, all experimental), sigcheck 2731/2735
(the 4 misses already fail on main). Only the 40 new entries were added to the generated headers.

All findings are static (decompile + disassembly), nothing is runtime-tested.
