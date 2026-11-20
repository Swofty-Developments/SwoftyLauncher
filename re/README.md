# re/: F1_25.exe reverse-engineering program

The shared knowledge base, rules and tools for every agent working on the game's native code. The end goal is engine-level mods (a 24 to 26-car grid, new home-menu behaviour). The rules are in [`PROTOCOL.md`](PROTOCOL.md); this page is the map.

## Layout

```
re/
  PROTOCOL.md          rules: evidence levels L0-L3, gates, naming, recording, verification, concurrency, tests, hooks
  TRACE_PLUGIN.md      runtime trace plugin spec (config schema, log format, safety limits)
  bindings/<sub>/<Name>.json  one binding per file: row, signature, verify rows and local proofs (PROTOCOL §11)
  .cache/records.json  the record checks keyed by path and SHA-256 of the bytes (gitignored; SWOFTY_RECORD_CACHE=0 off, =<file> elsewhere)
  db/                  the knowledge base (plain text, one build: sha256 d873a1c8...)
    functions.tsv        addr name sig level subsystem proto evidence notes
    globals.tsv          data, vtables (vtable[N]) and hook/trace code sites (site_*)
    structs/*.h          one struct per file, explicit offsets, per-field level + evidence
    signatures.json      byte signatures: unique match, anchor, resolve, target, builds verified on
    anchors.json         per-signature string anchor (string, func_rva, func_offset, durable) for build-agnostic resolution
    db_keys.tsv          game-DB keys the code reads (fnv1a64 key, path, reader, sites)
    strings_anchors.tsv  strings and djb2 hashes that locate functions (with refs)
    subsystems.tsv       subsystem id, name prefix, current owner
    subsystems/*.md      one map per subsystem; "## Inbox" at the end is append-only for non-owners
    verify.tsv           independent verification log (append-only)
    changes.log          journal of every kb.py write (who changed what)
  decomp/<addr9>.c     decompiler output with KB names applied (regenerable; cite by address)
  tests/               runtime test requests T###.md (+ T###.trace.json), TEMPLATE.md, SESSION-*.runtime.trace.json
  traces/T###/<run>/   logs from real sessions (L3 evidence)
  tools/               see below
  ghidra/              master project (never opened), slots/s0..s3 pool, clones/, scripts/*.java, demo/
  trace_plugin/        swofty_trace.dll source, build.sh, Wine test
```

## Onboarding in five minutes

1. Read `PROTOCOL.md` (§1–2 levels and gates, §4 how to record, §6 concurrency). Pick an agent name and pass it as `--agent <you>` to every write.
2. Use the pinned Python: `re/tools/py <script>` (numpy, pefile and capstone; already pinned to the allowed cores).
3. Claim your lane: `re/tools/py re/tools/kb.py --agent <you> claim session`. Read `re/db/subsystems/<id>.md` and `re/tools/kb.py find '<regex>'`.
4. Decompile with the shared names: `re/tools/decomp 0x1424a2350 0x14245fb70` → `re/decomp/1424a2350.c`. Up to 4 agents can run at once; never open a Ghidra project yourself.
5. Record what you find **only** through `kb.py add|set|note|verify|test-new` (it locks, validates and journals). Run `re/tools/py re/tools/kb_lint.py` and `re/tools/py re/tools/sigcheck.py` before you finish; both must pass.

## Tools (`re/tools/`)

| tool | use |
|---|---|
| `binding.py` | add a binding as one file under `bindings/`: `new`, `verify` (proof on your exe), `agree`, `check` (what CI runs), `status`, `unfold` |
| `kb.py` | the only writer of `db/*.tsv`: `add`, `set` (`col=`, `col+=`), `get`, `find`, `claim`/`release`, `note`, `verify`, `test-new`, `import`; `export <dir>` writes every table with the binding records merged in, for tools outside Python |
| `kb_lint.py` | schema, uniqueness, naming, evidence rules per level, cross-references, struct layouts, exe string bytes; prints KB counts. `--structural` runs only the rules that need no decomp files and no exe (what CI runs) |
| `sigcheck.py` | every signature unique on the KB build, target resolved and re-checked, hook sites on instruction boundaries; `--exe new.exe` for a game update, `--make <va> --kind mid` for a new candidate, `--sigs file` to test candidates |
| `decomp` | Ghidra decompile through the slot pool with the KB applied (`RE_GHIDRA_POOL`, `RE_GHIDRA_XMX`, `RE_GHIDRA_PROJECT`) |
| `ghidra_apply [dir]` | apply functions/globals/structs/protos/comments to one project, or to every slot and clone. The Ghidra tools read `ghidra/kb`, which `ghidra_lib.sh` refreshes with `kb.py export` (`RE_KB=<dir>` reads another folder) |
| `ghidra_export <dir> <out.tsv>` | names typed in Ghidra that the KB lacks, as L0 rows for `kb.py import` |
| `ghidra_clone <name>` | a private persistent project for one agent |
| `trace_build.py` | authoring trace configs (hooks by signature id) → runtime `swofty/trace.json` |
| `pe_index.py`, `xref.py`, `disx.py`, `sigmk.py`, `annot.py` | section map, `.pdata` bounds, rel32/RIP-relative xref scan, disassembly, signature maker, string annotation (copied from `work/exe/tools`, which stays in place for the agents already using it) |
| `anchormk.py`, `anchorcheck.py`, `anchorlib.py` | write `db/anchors.json` (string anchor per signature), prove each anchor resolves to the signature target on the Steam image, and the shared mirror of the core's runtime string-anchor resolver |

## Ghidra concurrency (tested)

A local Ghidra project cannot be opened twice: a second `analyzeHeadless` on the same project fails after about 2.5 s with `LockException: Unable to lock project!`, and `-readOnly` fails the same way. Instead, `re/ghidra/master` is a pristine copy of `work/exe/ghidra/f1`, and `re/tools/decomp` takes one of four writable clones (`slots/s0..s3`, flock per slot, extra runs wait). It applies the KB only when the KB's hash changed since that slot's last apply.
- Three concurrent runs took about 13 s each, including first-time slot creation. Output was byte-identical across slots, and each JVM peaked at 1.4–1.6 GB.
- A normal run takes 4–8 s at 0.5 GB. The heap cap is 8 GB per JVM, so 32 GB is the worst case.
- `ghidra/demo/before_*.c` and `after_*.c` show the same functions without and with the KB applied, e.g. `FUN_140846270(_DAT_146ca54f8, …)` becomes `Vm_Set_String(g_Vm_Registry, …, "name", &item->id, 1)`.

## Current state of the KB (2026-10-17)

| table | L0 | L1 | L2 | L3 |
|---|---|---|---|---|
| functions (119) | 21 | 57 | 34 | 7 |
| globals (27: 5 data, 15 vtables, 7 code sites) | 0 | 4 | 20 | 3 |
| struct fields (15 structs) | 0 | 15 | 47 | 9 |
| db_keys (1,108; 92 with names) | 0 | 1,102 | 6 | 0 |
| strings_anchors (47) | 1 | 3 | 43 | 0 |

- **Signatures:** 16, all unique on the KB build. The 8 ids behind the core mod's 4 patterns were also matched at runtime in T000. The 6 `session.*`/`gamedb.*` signatures exist for the grid tests.
- **Sources seeded:**
  - `research/exe.md` (home menu, tile/item classes, VM, protection);
  - `research/grid24-cap.md` (the participant pipeline and the `_instancecount_` cap);
  - `research/ui.md` §5 (the djb2 command dispatch);
  - the exe agent's decompiles and `db_key_immediates.txt`.
- **Bulk names:** 321 functions at L1 named from pretty-function strings: 243 `game::` in the `game` lane and 78 `neon::` in `core`, with 461 string anchors. The exe has no `ego::` strings. The rules are in `db/subsystems/game.md`.
- **L3 items** rest on `traces/T000/run1/loader.log`, the first real-game run of the loader and core mod. It covers the home-menu MidHook site, the Career adder and vtable, base populate, `TileList.items`, and `g_Vm_Registry`.
- **Tests:**
  - T000: run, retroactive record.
  - T001: does B24 select 24 entrants? Queued.
  - T002: trace plugin smoke test in the real game. Queued.
  - T001 and T002 are batched in `tests/SESSION-1.runtime.trace.json`.
- **Verification:** one `partial` row. Nothing at L2 has been independently verified yet, so no new behaviour hook is cleared under PROTOCOL §2 until verifiers run.
- **Thinnest areas:** `assets` (eaid resolution not analysed), `gamedb` vtable slots (L0), and the per-car pools beyond the session vector (26-car audit, `subsystems/session.md`).
