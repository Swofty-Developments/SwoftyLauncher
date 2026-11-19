# F1_25.exe reverse-engineering protocol

Every agent working on the exe follows this file. `re/tools/kb_lint.py` and `re/tools/sigcheck.py` enforce the mechanical parts; a run that fails either is not done.

BRIEF.md rules still apply: never launch the game, never write into the game install, never touch the EGO GameDatabases Editor, pin heavy work with `taskset -c 2-7,10,11,14-31 nice -n 10`.

The KB describes exactly one build: `F1_25.exe` sha256 `d873a1c8d2044cef2fd76c99c9d5a97649c5b815d055540995105f46619865ae` (`re/db/signatures.json` → `build`). All addresses are VAs with image base `0x140000000`.
Every other build the loader supports is an overlay on that KB in `re/db/builds/` (§10).

## 1. Evidence levels

| level | meaning | minimum evidence tokens |
|---|---|---|
| **L0** guess | a hunch, a likely name, a hypothesis | none |
| **L1** static-inferred | pattern, name, string or constant match: an assert string names it, a hash constant matches, it looks like a clone of a known function | one of `str:` `imm:` `pat:` `doc:` `decomp:` |
| **L2** static-proven | decompiled and read; callers/refs listed; the data flow that justifies the claim is written down | `decomp:` **and** `xref:` (and a `kb:` or `doc:` pointer when the data-flow argument is longer than the notes field). Code sites (`site_*`): `decomp:` of the containing function. Struct fields: `decomp:` plus `xref:` (the accessing instructions) or `doc:`/`kb:`. DB keys: `decomp:` of the reader (its `sites` column is the xref list). |
| **L3** runtime-confirmed | observed in a trace log from a real game session | everything L2 needs **plus** `trace:` |

Rules:
- A claim is only as strong as its weakest input. An L2 claim that depends on an L1 struct field is L1.
- Levels go down as readily as up. Contradicting evidence demotes immediately (§5).
- L3 is per build. After a game update every L3 item is treated as L2 until its trace is re-run (§8).

## 2. What each level may drive

| action | requirement |
|---|---|
| Record it in the KB | any level |
| Name it in Ghidra (via the KB) | L1+ (L0 goes in `notes` only, the name stays `<Prefix>_Unk_<addr>`) |
| **Trace-only hook** (reads registers/memory, logs, changes nothing) | target L1+, signature passes `sigcheck.py`, target in `.xtls` (never `.data1`), target is an instruction boundary, no branch lands inside the 5-byte patch window. One site carries one hook: core loads first, so a trace hook on a site core already patched is refused (`already_patched`); trace such a site in a session without core, or trace a neighbouring signature |
| **Behaviour hook** (writes registers/memory, calls game code, swaps vtables) | every function it calls or hooks, every field it reads or writes: **L2**, plus an `agree` row in `verify.tsv` from a different agent (§5), made at the item's current level or above, that is still the newest verify row over the item's name and address (a later `partial` or `disagree` ends it; a raise needs a new `agree` at the new level; `re/db/export_exceptions.json` lists the reviewed exceptions bindgen exports anyway, and `re/db/level_baseline.json` the bindings that exported on a lower-level `agree` before 2026-11-18, which only shrinks; `re/db/export_drops.json`, approved by the repository owner alone, deprecates or drops a binding whose newest row is a `partial` or `disagree`) |
| **Ship** (enabled by default in the launcher) | every item above **L3** on the shipping build, and the hook's own T### test passed |

`.data1` bodies are black boxes: they may be called (L2 on the thunk) but never hooked or patched, with one exception. A behaviour site inside a `.data1` body is allowed when the code it relies on has no `.xtls` route: the body is decompiled and read on the Steam build (where `.data1` is plaintext), and its signature declares `"section": ".data1"`, a `data1_reason` naming why no `.xtls` site works and, where `.pdata` does not cover the body, a `func_start` for the boundary check. Core binds such a site only from a `.data1` signature. The EA build's `.data1` is packed, so these sites never resolve there.

## 3. Naming

Names are ASCII, `[A-Za-z0-9_]`, at most 64 characters, unique across `functions.tsv` and `globals.tsv`.

- **Functions:** `<Prefix>_<Verb>_<Noun>[_<Qualifier>]`, PascalCase segments. Prefix is the subsystem's prefix from `subsystems.tsv`. Examples: `FeTile_Build_List`, `Vm_Set_String`, `FeHome_Add_CareerTile`.
- **Recovered real names** (from `__PRETTY_FUNCTION__`, asserts, log strings) go in `notes` as `cxx=game::FEHomeViewModel::onCommandRaised`. The KB name still follows the rule above (`FeHome_Handle_Command`), so names stay greppable by subsystem.
- **Thunks** to `.data1` bodies: `<name>_Thunk`; the body, if listed, is `<name>_Body` (notes `thunk=` / `body=`).
- **Vtable slots** (functions only reached through a vtable): `<Prefix>_<Class>_<Method>` e.g. `FeHome_CareerItem_Destroy`; role unknown: `<Prefix>_<Class>_Slot<off>` (`FeTile_ItemBase_Slot68`).
- **Unknowns:** `<Prefix>_Unk_<addr9>` (`FeTile_Unk_1415a92d0`). Never commit Ghidra's `FUN_`/`DAT_`/`LAB_` names.
- **Globals:** `g_<Prefix>_<Noun>` for data, `vt_<Prefix>_<Class>` for vtables (type `vtable[N]`), `k_<Prefix>_<Noun>` for constant data, `site_<Prefix>_<Noun>` for code sites that hooks or traces target (type `code`; they become Ghidra labels).
- **Structs:** PascalCase class-like names (`TileList`, `HomeItem`, `NeStr`). Recovered C++ class names are used as-is when known (`FEHomeViewModel`). Unknown structs: `Unk_<owner>_<size>`.
- **Fields:** snake_case. Unknown fields: `unk_<off>` (`unk_0x38`); padding `_pad_<off>`.
- **Signature ids:** `<subsystem>.<snake_name>` (`fe_home.career_call_anchor`).
- **Tests:** `T###` (zero-padded, allocated by `kb.py test-new`).

## 4. Recording a finding

The KB is plain text under `re/db/`. **Never edit `db/*.tsv`, `db/signatures.json` or another lane's files with an editor while other agents run.** Use `re/tools/kb.py` (flock on `db/.kb.lock`, validates, rewrites atomically, journals to `db/changes.log`):

```
python3 re/tools/kb.py --agent <you> add functions addr=0x14173df80 name=FeTile_Build_List level=L2 subsystem=fe_tiles \
    "evidence=decomp:work/exe/decomp/14173df80_home_buttons_build.c;xref:0x141bb2125,0x141a48cc5" "notes=..."
python3 re/tools/kb.py --agent <you> set functions 0x14173df80 level=L3 "evidence+=trace:T001/r1:12"
```

| file | key | columns |
|---|---|---|
| `functions.tsv` | `addr` | `addr name sig level subsystem proto evidence notes` |
| `globals.tsv` | `addr` | `addr name type level subsystem sig evidence notes` |
| `db_keys.tsv` | `key` + `reader` | `key path reader sites level subsystem evidence notes` |
| `strings_anchors.tsv` | `id` | `id addr enc text refs funcs level notes` |
| `subsystems.tsv` | `id` | `id prefix owner status description` |
| `verify.tsv` | append-only | `date verifier kind item claimed verdict evidence notes` |
| `signatures.json` | `signatures[].id` | see §7 |
| `structs/<Name>.h` | file | header block + one field per line, see below |
| `subsystems/<id>.md` | file | the owner's map of the subsystem; `## Inbox` at the end is append-only for everyone else |

Cell rules: tabs and newlines are forbidden in cells; `-` means empty; addresses are `0x14xxxxxxx` lowercase. `db_keys.path` uses `{<16 hex>}` for an FNV state whose prefix is not yet named (research/gamedb.md convention; kb_lint then skips the hash check) and `?` for an unknown path. `globals.type` is a C type, `vtable[N]` or `code`.

**Evidence tokens** (`;`-separated in `evidence`):

| token | meaning |
|---|---|
| `decomp:<path>` | decompiled output, path relative to `f1-modding/` (`re/decomp/14173df80.c`, or legacy `work/exe/decomp/...`) |
| `xref:<addr>[,<addr>...]` | call sites / referencing instructions that were checked (callers for functions, readers/writers for globals and fields) |
| `str:<anchor-id>` | a row in `strings_anchors.tsv` |
| `imm:<hex>` | an immediate constant (djb2, FNV key, size) that ties the code to the claim |
| `pat:<short text>` | pattern/name reasoning, e.g. `pat:clone-of-0x141746710` |
| `doc:<path>#<section>` | prose argument in a research note |
| `kb:subsystems/<id>.md#<anchor>` | the data-flow write-up in the subsystem map |
| `trace:T###/<run>[:<line>]` | runtime log under `re/traces/T###/<run>/` |
| `sig:<id>` | (functions/globals carry it in the `sig` column instead) |

**Caller counts** come from the census, never from a count made by hand. `python3 re/tools/census.py show <build> <addr> [--sites]` prints an address's direct calls and the functions they sit in, its thunks and the calls through them, the jumps, the lea loads and the 8-byte pointers to it, from every executable section of the build (.data1 included). A note cites the stored row as `callers: census <build> <rva>` and gives no count of its own. `python3 re/tools/census.py update <build>` writes a row for every citation to `re/db/census/<build>.tsv`, and kb_lint fails a citation without a row. Once the KB build has a census, kb_lint warns about notes that count callers by hand (`census.py hand` lists them), and the bindings check fails a pull request whose record notes add such a count.

**Structs** (`structs/<Name>.h`), one struct per file, offsets explicit, one field per line:

```c
// struct: HomeItem
// subsystem: fe_tiles
// size: 0x148  L2 decomp:work/exe/decomp/141746710_... (alloc size)
// notes: base vtable vt_FeTile_ItemBase
struct HomeItem {
    /* 0x000 */ void **vtable;          // L2 decomp:...;xref:...
    /* 0x010 */ NeStr *id;              // L2 decomp:... route key ("career")
    /* 0x058 */ VmPropHolder title;     // L2 ...
};
```

Field line grammar: `/* 0xOFF */ <type> <name>[<count>]; // L<n> <evidence tokens> [free text]`. Gaps are allowed (tools pad them). Types: stdint names, `float`, `double`, `bool`, `char`, `void`, pointers, and other `structs/` names. `size: ?` when unknown.

## 5. Independent verification

A **verifier** is any agent other than the one who raised the item to its current level. It gets only the item row (address, claim, level), never the original write-up, and re-derives:

- **functions:** re-decompile (`re/tools/decomp`), confirm the claimed role from the code alone, re-list callers with `re/tools/xref.py`, check the `proto` arity and types against the decompile.
- **struct fields:** find at least one read and one write (or the allocation size for `size:`) independently of the cited decomp.
- **signatures:** `sigcheck.py` passes, and the target is the claimed function or instruction.
- **db keys:** recompute `fnv1a64(path)` and find the immediate at a listed site.

The verifier appends one `verify.tsv` row via `kb.py verify`: verdict `agree`, `disagree` or `partial`, with its own evidence tokens. On `disagree` the item is demoted to the highest level both sides accept, and the disagreement is written into the item's `notes` as `DISPUTE(Vnnn)` until the subsystem owner resolves it with new evidence (a new `verify` row). Nobody deletes `verify.tsv` rows. When the item a row is about is renamed to a different claim, retargeted or was never added, its `item` becomes `retired:<item>`, so the old verdict can never back a new item that reuses the name or address. `binding.py rename OLD NEW` renames a record this way: the file, its name, the build overlays' entries and `function_names.tsv` follow, the rows about the old name stay in the record as `retired:OLD`, and the rows about the address carry over. A `deprecated` entry for `OLD` in `re/db/export_drops.json` that covers the newest retired row and names `NEW` as its `replacement` keeps `OLD` exporting for one release as a deprecated alias of `NEW`, while `NEW` exports on its own agree. `binding.py check` fails on a build overlay entry still keyed by a retired name, which binds nothing: a branch that adds one after the rename moves it to the new name by hand.

Every row `kb.py verify` writes starts with its id `Vnnn`. From V100000 the ids come from ranges in `db/verify_ranges.tsv` (`first`, `last`, `owner`, `granted`, `notes`):

- A range belongs to one owner: a lane name such as `re-bindings`. It covers the verifier of that name and every verifier whose name starts with it and a `-`, such as `re-bindings-blind`. The longest matching owner wins.
- `kb.py verify` takes the next free id in the verifier's ranges, above every id that `verify.tsv` or a record already holds in that range whoever wrote it, or refuses when the verifier has none or they are used up. Each lane records under its own owner name (`std-libs`, `native-guards`, `port-live`, `load-game`, or a name that starts with it and a `-`), never under another lane's, even for a blind row another lane relays.
- A lead grants ranges on the trunk only, with `kb.py --agent LEAD grant-range OWNER [--size 10000]`, and lanes merge the trunk before writing rows. A lane that works on two branches at once takes a range per branch, for example `re-bindings` and `re-bindings-w2`.
- `kb_lint.py` fails on overlapping ranges, on an id from V100000 up that lies outside every range of its verifier's owner, and on an id above V10540 that two rows carry. Ids below V100000 keep the numbers they were given.

The row's `claimed` column is the item's level when the row is written: `kb.py verify` reads it from the item's row, and for a struct field or a struct size from its struct file. bindgen counts an `agree` only when its `claimed` level is the item's current level or above, so an item raised after its `agree` needs a new `agree` at the new level.

## 6. Concurrency

`kb.py` records every write in `db/changes.log` (JSON lines: time, agent, op, row before/after). That journal is how `kb.py verify` knows who raised an item (the seed rows are attributed to `exe-agent`, `grid24-cap`, or `re-infra` for the T000 promotions).


- **Lanes.** One agent owns a subsystem at a time: `kb.py claim <id>` / `kb.py release <id>` (owner column in `subsystems.tsv`). Only the owner edits `subsystems/<id>.md` above its `## Inbox` and the `structs/` files whose `subsystem:` is that id, and only the owner raises rows of that subsystem to L2+. Everyone may add L0/L1 rows anywhere and append notes to any inbox with `kb.py note <id> "text"`.
- **Shared tables** are written only through `kb.py` (serialised by flock, schema-checked before the atomic rename). `kb_lint.py` runs clean after every write session. Hand edits are allowed only to your own lane's `.md`/`.h` files.
- **Ghidra.** A local Ghidra project cannot be opened by two processes, even with `-readOnly` (`LockException: Unable to lock project`). So nobody opens `work/exe/ghidra` or `re/ghidra/master` directly. Decompile with `re/tools/decomp <addr>...`: it takes a free slot from the pool `re/ghidra/slots/s0..s3` (flock per slot; extra runs wait), applies the KB to the slot if the KB changed since that slot's last apply (names, protos, plate comments with level/evidence, `/kb` structs), writes `re/decomp/<addr9>.c` and releases the slot. Each run is 4–8 s, 0.5–1.6 GB RSS, heap capped at 8 GB (`RE_GHIDRA_XMX`); pool size `RE_GHIDRA_POOL`. A private persistent project: `re/tools/ghidra_clone <you>`, then `RE_GHIDRA_PROJECT=re/ghidra/clones/<you>`. Names typed in Ghidra are not knowledge until `re/tools/ghidra_export <project> out.tsv` and `kb.py import out.tsv functions`.
- **Decomp output** in `re/decomp/` is a regenerable artifact named by address only, so citations survive renames. Your own scratch goes under `work/exe/<you>/`, never in `re/`.

## 7. Signatures

`signatures.json` → `signatures[]`, one object per signature:

| field | meaning |
|---|---|
| `id` | `<subsystem>.<snake_name>` |
| `section` | section scanned (`.xtls`) |
| `pattern` | IDA-style bytes, `??` wildcards; wildcard every rel32/riprel displacement |
| `match` | VA of the unique match on the verified build |
| `anchor` | offset from the match start to the byte the consumer uses |
| `resolve` | how the target is derived: `{"kind":"addr"}` (target = match+anchor), `{"kind":"rel32","insn":<insn offset>}` (target = branch target of the call/jmp at match+insn whose rel32 is at match+anchor), `{"kind":"riprel","insn":<insn offset>,"len":<insn len>}` (target = RIP-relative address of the instruction whose disp32 is at match+anchor). Offsets are relative to the match; sigcheck decodes the instruction and checks that the operand really sits at `anchor` |
| `target` | the resolved VA on the verified build; `sigcheck.py` recomputes it |
| `expect` | optional hex bytes that must be at `match+anchor` |
| `names` | the `functions.tsv`/`globals.tsv` name(s) the target is |
| `kind` | `func_start`, `mid` (MidHook site), `call` or `data` |
| `verified` | list of `{build, date, by}`; `build` is the first 16 hex chars of the exe sha256 |
| `used_by` | files that consume it (`native/src/core/frontend/swofty_core.cpp`) |

Rules: a pattern must match **exactly once** in its section on every build listed in `verified`; it must not start or end on a wildcard; it is at least 12 bytes; a `mid` target must sit on an instruction boundary inside a `.pdata` function in `.xtls`; `.data1` bodies get signatures only for the §2 exception (`section` `.data1`, `data1_reason`, `func_start` when `.pdata` lacks the body). `re/tools/sitereg.py` makes, checks and records a site, data or function signature together with its KB row. `sigcheck.py --make <va> --kind mid|func_start` prints a candidate object; `sigcheck.py --sigs <file>` checks candidates before they go in; plain `sigcheck.py` is the gate.

## 8. Hook development workflow

1. **Static.** Get every function, global and field the hook needs to L2 (decomp + xrefs + data flow in the subsystem map). Add signatures, run `sigcheck.py`.
2. **Trace-only hook.** Write a `swofty/trace.json` entry (see `TRACE_PLUGIN.md`) for the exact site and the registers/fields your behaviour hook will rely on. File a test request (§9). The trace answers: is it hit, how often, with which register values.
3. **Behaviour hook.** Only after the trace confirms the assumptions (items to L3) and a verifier agreed on the L2 items. Keep the trace config active alongside it for the first runs.
4. **Every game update.** `sigcheck.py --exe <new exe>`. Every signature that still matches once gets a new `verified` entry after its target is re-checked against the decomp; anything that fails is `stale`. All L3 items drop to L2 until their tests are re-run on the new build. Nothing ships on a build with failing signatures. A build that ships beside the KB build instead of replacing it gets an overlay (§10).

## 9. Runtime test requests

The lead runs tests with the user, batched into one play session. Request one with `kb.py --agent <you> test-new "<question>"`, which allocates `re/tests/T###.md` from `re/tests/TEMPLATE.md`. Rules:

- **One question per test.** If you need two answers, file two tests; they can share a session.
- The test lists the KB items it would promote to L3, the exact build (exe sha, loader, plugin versions), the trace config, numbered user steps, and expected log lines as regexes with a pass/fail rule.
- No test may need the user to do anything risky to their saves or install beyond dropping files into `swofty/`.
- After the session the lead files logs under `re/traces/T###/<run>/`, and the test's owner records the verdict in the test file and promotes or demotes the items.

## 10. Builds

`re/db/builds/<build>.json` describes one game build the loader supports; `steam.json` is the KB build and `ea.json` the EA App build.

```
{ "schema": 1,
  "build": { "exe": "F1_25.exe", "sha256": "...", "size": 0, "pe_timestamp": 0, "image_base": "0x140000000",
             "distribution": "Steam|EA App", "packed": false },
  "note": "<optional build-level text>",
  "sections": { "<KB section>": "<this build's section>" },
  "overrides": { "<binding id>": { "pattern": "<bytes with ??>", "note": "<why the base pattern fails here>" } },
  "hints": { "<binding id>": { "match": "0x<rva of the pattern match start>", "func_rva": "0x<anchor function rva>" } },
  "unresolvable": { "<binding id>": "<reason>" } }
```

| field | meaning |
|---|---|
| `build` | the build's identity; `packed` is `true` when its code section is encrypted on disk and decrypts at runtime |
| `sections` | KB section name to this build's section name for every binding (EA: `.xtls` → `.rdata`); unlisted names stay |
| `overrides` | this build's `pattern` for a binding whose KB pattern does not match there; `note` is required, and `anchor`, `resolve`, `expect` and `section` may be replaced too |
| `hints` | `match` is the RVA where the binding's pattern match starts on this build; `func_rva` is its string anchor's function RVA; either is optional |
| `unresolvable` | bindings that cannot resolve on this build, each with its reason |

Rules:
- `re/tools/bindgen.py` writes one bindings set per overlay from the same gated bindings: `bindings.json` for the overlay whose `build.sha256` is the KB build, `bindings.<build>.json` for every other one.
- Every emitted binding carries a non-empty `pattern` in every set.
- Statuses are build-independent: `stable`, `experimental`, or `unresolvable` with a `reason` from the overlay.
- A hint is the pattern match start, never a resolved target; core verifies it by bytes and scans when it differs.
- The KB build's hints are each signature's `match` in `signatures.json`; an overlay hint replaces it.
- The KB build's overlay takes no overrides: its patterns are the signatures in `signatures.json`, and its `build` must agree with that file's `build`.
- A non-KB set carries no `rva`, and carries `match` and `string_anchor.func_rva` only where its overlay hints them.
- Each overlay has its own `pe_timestamp`, which core selects the set by.
- Overlay entries for bindings that are not exported are ignored and listed by bindgen.
- `re/tools/capture.py --write` updates an overlay's `hints` from a launch's `[bind]` log lines.
- A set whose overlay maps sections carries that map as its top-level `sections`; core sends scans of a KB section name by core code and addons there.
- `python3 -m unittest discover -s re/tools/tests` runs the generator, capture and verifier tests with the standard library; CI runs it in the native job.
- With `SWOFTY_STEAM_EXE` set to the KB exe, the tests also check that every exported pattern matches exactly once at its `match` and that the Steam set resolves by hint and by scan.
- With `SWOFTY_EA_DUMP` set to a decrypted EA image dump, the tests check that the EA set resolves by hint and by scan; with both set, they also check that each EA binding lands on the same code as on Steam.
- `re/tools/bindverify.py --build <name> --image <exe or flat dump>` resolves a set against a static image with core's rules: the byte-checked `match` hint, then a unique section scan, then the string anchor, then `resolve`, `expect` and the target section; a packed set also defers code that reads encrypted.
- `bindverify.py --compare <KB exe>` flags bindings whose match surroundings or callee differ from the KB build's, which catches a pattern that matches once on the wrong code.
- `native/build.sh` runs bindverify on the emitted sets when `SWOFTY_STEAM_EXE` or `SWOFTY_EA_DUMP` is set.
- `re/tools/py re/tools/bindgen.py --exe <KB exe>` runs sigcheck on every exported binding and drops any that fails.

## 11. Binding records

A binding can live in one file instead of the shared tables: `re/bindings/<subsystem>/<Name>.json` holds its `functions.tsv` or `globals.tsv` row, its signature, the verify rows about it and the local proofs contributors ran on their own exe. `kb.py` merges records into every table it reads, so the levels, gates and lint rules above apply unchanged. `kb.py set` and `kb.py verify` on a record's item write into the record. Tools that read the tables without `kb.py` (the Ghidra scripts) read `kb.py export <dir>`; never read `re/db/functions.tsv`, `globals.tsv` or `signatures.json` directly.

- `re/tools/binding.py new|verify|agree|check|status` is the contributor command; the page for addon authors is `docs/reference/addons/bindings.md`.
- A record's `author` and every proof's and verify row's `by`/`verifier` is a GitHub login. The author never verifies their own record (§5).
- A proof is `binding.py verify`'s result on one exe: the exe's sha256, size and timestamp, the resolution result with core's rules, the sigcheck result, and a digest over the signature fields that decide where the binding lands plus the exe hash. CI cannot rerun it; it checks the build is known, the digest matches and a KB-build proof found the recorded `match` and `target`.
- `.github/workflows/bindings.yml` merges a pull request by itself when it only adds L1/L2 records with a proof by their author, or only adds one agree row plus proof by a second account; everything else gets review.
- `binding.py unfold <Name>...` moves bindings from the shared tables into records; bindgen's output does not change (`re/tools/tests/test_binding_check.py`).
