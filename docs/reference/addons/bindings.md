---
title: Adding a binding
description: Add a new F1 25 binding for your engine plugin - one file under re/bindings, proven on your own game exe, merged by CI.
---

# Adding a binding

A binding names one place in `F1_25.exe` that your [mixins](/reference/addons/hooking) can hook. A new binding is one JSON file under `re/bindings/<subsystem>/<Name>.json`. A pull request that adds only that file and passes every check merges itself.

## Before you start

```sh
git clone https://github.com/<your-github-login>/SwoftyLauncher.git && cd SwoftyLauncher   # your fork
python3 -m pip install pefile numpy capstone    # binding.py new and verify read the exe with these
python3 re/tools/binding.py --help              # every command; binding.py <command> --help for its options
```

- **The game.** `binding.py new`, `verify` and `agree` run on the current Steam build (F1 25 1.0.145.2472, build 26566308) or on any other build in `re/db/builds`. Records hold addresses on the knowledge-base (KB) build, Steam 1.0.141.7865 (build 25251167). On any other build, `new` takes the KB address from the bindings just below and just above yours, which must have moved by the same amount between the two builds. The record keeps where it was made and that reasoning in `twin`. When the two bindings moved by different amounts, `new` writes nothing and names them; pick another address or ask in an issue.
- **The address and why it is that place.** Find the function, instruction or global in your own disassembly of your build. Every address you pass, `--evidence` included, is on that build. `--evidence` is `kind:value` tokens joined by `;`:

  ```text
  decomp:own disasm 0x1436a3ab0-0x1436a422c    the code you read (L2 needs it)
  xref:0x1436a3b74,0x1436a3b83                 the callers or the instructions that use it (L2 needs it)
  str:<anchor id>                              a row in re/db/strings_anchors.tsv
  imm:<hex>                                    a constant that ties the code to the claim
  pat:clone-of-0x141746710                     a pattern or name match
  doc:re/research/<note>.md#<section>          a written argument
  ```

  L2, the default, needs `decomp:` and `xref:`. `--level L1` takes any one token (re/PROTOCOL.md §1).
- **The name.** A function is `<Prefix>_<Verb>_<Noun>` and a global `g_`, `k_`, `vt_` or `site_<Prefix>_<Noun>`; the prefix is the subsystem's `prefix` column in `re/db/subsystems.tsv` and picks the folder.
- **Your GitHub login.** `--author` is the account that opens the pull request.

`re/tools/py` runs the tools with a Python that has the three modules: `$SWOFTY_PY`, the pinned Nix one, or `python3` once `pip` installed them. `check`, `doctor` and `status` run on any Python 3.10+. On Windows, run `python re\tools\binding.py ...` in place of `re/tools/py re/tools/binding.py ...`.

## The whole flow

```sh
# 1. Make the record. binding.py writes the signature, checks it and proves it on your exe.
#    Nothing is written when a check fails; each error is followed by its fix.
re/tools/py re/tools/binding.py new site_Sess_PitEntryDecision 0x1436a3b83 --kind site \
    --author your-github-login --exe ~/F1_25.exe \
    --evidence "decomp:own disasm 0x1436a3ab0-0x1436a422c;xref:0x1436a3b74,0x1436a3b83" \
    --notes "call that decides whether the AI pits this lap; rcx = car record"

# 2. Run the pull request check on your checkout: merges itself, needs review, or fails, and why.
python3 re/tools/binding.py doctor

# 3. Commit only that file and open a pull request.
git switch -c bindings/pit-entry-decision
git add re/bindings/session/site_Sess_PitEntryDecision.json
git commit -m "feat(re): add the AI pit entry decision site"
git push -u origin bindings/pit-entry-decision    # then open the pull request against main
```

```text
binding.py doctor: 1 record file(s) and 0 other file(s) changed against 3f1c2a9b0d4e, pull request by your-github-login
  re/bindings/session/site_Sess_PitEntryDecision.json
verdict: auto: this pull request would merge itself: every changed file is a binding record that passes the checks
after the merge:
  site_Sess_PitEntryDecision: not exported; needs an agree row from a second contributor (binding.py agree)
```

On a build other than the KB build, `new` prints the KB twin before it writes the file:

```text
binding.py new: made on steam-26566308; the target 0x142e91ae0 is KB 0x142e8f550: Sess_TyreAllocation_SwapSets (0x2e90a50) and Sess_Veh_UpdateWheelMap (0x2e92af0) on either side both sit 0x2590 above their KB addresses
wrote re/bindings/session/Sess_Tyre_ReadCompoundSets.json (session.tyre_read_compound_sets, match 0x142e8f550, proof on 46009bd31f49d93a)
```

A failing check prints the problem, the file and the fix:

```text
verdict: fail: this pull request would fail the binding check, because:
  - error: re/bindings/session/site_Sess_PitEntryDecision.json: row: L2 needs xref:
    fix: add the missing xref: token to --evidence (tokens are kind:value joined by ;, ...), or pass --level L1
```

| `--kind` | binding | signature |
|---|---|---|
| `func` | a function you wrap | `func_start` at the function; `call` through its most distinctive call site when the function's own bytes repeat elsewhere in the exe |
| `site` | an instruction you run code at | `mid` at the instruction, with `expect` bytes |
| `data` | a global (`--type`, `--via <insn>`) | `riprel` through the instruction at `--via` |

Options: `--subsystem` when the name's prefix is ambiguous, `--proto "<C prototype>"` for a `func`, `--level L1` for a pattern match without a read decompile, `--match-from <va>` to start the pattern at an earlier instruction when the bytes at the address are not unique, `--func-start` and `--data1-reason` for a `.data1` site (PROTOCOL.md §2). `binding.py new --help` describes each one.

## What the file looks like

```json
{
 "schema": 1,
 "name": "site_Sess_PitEntryDecision",
 "table": "globals",
 "author": "your-github-login",
 "row": {
  "addr": "0x1436a3b83",
  "type": "code",
  "level": "L2",
  "subsystem": "session",
  "evidence": "decomp:own disasm 0x1436a3ab0-0x1436a422c;xref:0x1436a3b74,0x1436a3b83",
  "notes": "call that decides whether the AI pits this lap; rcx = car record"
 },
 "signature": {
  "id": "session.pit_entry_decision",
  "section": ".xtls",
  "pattern": "48 89 DA E8 ?? ?? ?? ?? C5 FA 5C FE",
  "match": "0x1436a3b80",
  "anchor": "0x3",
  "resolve": {"kind": "addr"},
  "target": "0x1436a3b83",
  "kind": "mid",
  "expect": "E8",
  "names": ["site_Sess_PitEntryDecision"],
  "verified": [{"build": "d873a1c8d2044cef", "date": "2026-11-17", "by": "your-github-login", "how": "static"}],
  "used_by": []
 },
 "verify": [],
 "proofs": [
  {
   "by": "your-github-login",
   "date": "2026-11-17",
   "tool": "binding.py verify",
   "exe": {"sha256": "d873a1c8d2044cef2fd76c99c9d5a97649c5b815d055540995105f46619865ae", "size": 408063872, "pe_timestamp": 1789118616},
   "result": {"result": "resolved", "method": "scan", "matches": 1, "match": "0x1436a3b80", "target": "0x1436a3b83"},
   "sigcheck": "passed",
   "digest": "<sha256 of the signature fields and the exe hash>"
  }
 ]
}
```

A record made on another build has four more signature fields; `match`, `target` and `row.addr` stay KB addresses:

```json
  "verified": [{"build": "46009bd31f49d93a", "date": "2026-11-18", "by": "your-github-login", "how": "static"}],
  "made_on": "46009bd31f49d93a",
  "made_on_match": "0x142e91ae0",
  "made_on_target": "0x142e91ae0",
  "twin": "made on steam-26566308; the target 0x142e91ae0 is KB 0x142e8f550: Sess_TyreAllocation_SwapSets (0x2e90a50) and Sess_Veh_UpdateWheelMap (0x2e92af0) on either side both sit 0x2590 above their KB addresses"
```

Its proof on that build must find `made_on_match` and `made_on_target`, and the set for that build uses `made_on_match` as the match hint. With `SWOFTY_KB_EXE` set to a KB `F1_25.exe`, `new` also checks the twin against the KB bytes and adds the result to `twin`.

`row` holds the same columns as `re/db/functions.tsv` or `globals.tsv` (PROTOCOL.md §4), `signature` the same object as `re/db/signatures.json` (§7). `bindgen.py`, `kb_lint.py`, `sigcheck.py` and `kb.py` read records together with the shared tables.

## From merged to usable

A merged record is in the knowledge base but not yet in `bindings.json`. The existing export gate decides when it ships:

```sh
python3 re/tools/binding.py status
```

```text
name                         level  status        proofs  verified by       needs
site_Sess_PitEntryDecision   L2     not exported       1  -                 an agree row from a second contributor (binding.py agree)
```

| record | exported as |
|---|---|
| L1, or L2 whose newest verify row is not `agree` | not exported |
| L2 whose newest verify row is an `agree` made at L1 | not exported |
| L2 whose newest verify row is an `agree` made at L2 or above, from someone other than the author | `experimental` |
| L3 with a passed trace test (maintainer) | `stable` |

The newest row is the latest by date over the binding's name and its address together, so a later `partial` or `disagree` takes an exported binding out of the next build until someone verifies it again. A binding the core's own code needs can stay exported through `re/db/export_exceptions.json`, which names the row it overrides and the reason; that file always needs review.

An `agree` counts only at the level it was made at or a lower one. Raising a binding from L1 to L2 takes it out of the build until someone adds an `agree` at L2, which checks the prototype and the L2 claims that an L1 `agree` never looked at. Struct fields and struct sizes follow the same rule with the level in their struct file. `re/db/level_baseline.json` lists the bindings that exported on a lower-level `agree` on 2026-11-18, each with the V-id of that row, and they keep exporting while it is still their newest row. The list only shrinks: a pull request that adds an entry or changes a V-id fails the check. `binding.py baseline` counts the entries and lists those no longer in use, and `binding.py baseline --prune` removes them; `binding.py doctor` prints how many are left.

An exported binding is part of the addon API, so the set main exports must not shrink. A pull request whose record change leaves an exported binding unexported fails the check. `binding.py exports` runs the bindgen of the commit where the branch leaves main, and the checkout's, and lists every binding main exports that the checkout does not; `binding.py doctor` runs it too. After a newer `partial` row, the binding exports again once the notes carry its corrections and someone who did not write them adds an `agree` row.

### Taking a binding out on purpose

When a blind round finds an exported binding wrong, the repository owner can take it out of the addon API with an entry in `re/db/export_drops.json`:

```json
{
 "schema": 1,
 "note": "...",
 "entries": {
  "func:Example_ShowFittedSet": {
   "mode": "deprecated",
   "since": "1.10.0",
   "covers": "V100013",
   "reason": "its fourth argument is a u8 wing flag, not a u32 model mask",
   "replacement": "Example_ShowFittedSet2"
  },
  "func:Example_ApplyCompound": {
   "mode": "dropped",
   "since": "1.10.0",
   "covers": "V100021",
   "reason": "it frees the wheel models a second time when the car is in the garage",
   "harm": "crash"
  }
 }
}
```

`deprecated` keeps the binding exporting for one release. Its `bindings.json` entry carries a `deprecated` object, and the headers mark it:

```json
{"id": "Example_ShowFittedSet", "kind": "func", "status": "experimental", "...": "...",
 "deprecated": {"since": "1.10.0", "reason": "its fourth argument is a u8 wing flag, not a u32 model mask",
                "replacement": "Example_ShowFittedSet2"}}
```

```cpp
[[deprecated("deprecated in 1.10.0, removed in the next release: its fourth argument is a u8 wing flag, not a u32 model mask; use Example_ShowFittedSet2")]] inline constexpr fn<"Example_ShowFittedSet", Example_ShowFittedSet_t> Example_ShowFittedSet{}; // L2 experimental
```

```rust
#[deprecated(since = "1.10.0", note = "deprecated in 1.10.0, removed in the next release: ...")]
pub const Example_ShowFittedSet: Binding = Binding { id: "Example_ShowFittedSet", kind: "func", status: Status::Experimental };
```

`dropped` stops exporting it: the headers lose it, and every bindings set lists it instead:

```json
"dropped": [{"id": "Example_ApplyCompound", "since": "1.10.0",
             "reason": "it frees the wheel models a second time when the car is in the garage"}]
```

- A binding is dropped without a deprecation release only when it crashes or corrupts state: `"harm": "crash"` or `"harm": "corrupts state"`. Any other binding is deprecated in one release and dropped in a later one, and its `dropped` entry keeps `"deprecated"` set to the release that deprecated it.
- `since` is the release the mode ships in. A new or changed mode ships after the release the base branch is at (`[workspace.package] version` in `Cargo.toml`).
- `covers` is the V-id of the `partial` or `disagree` row that ends the export. The entry holds only while that row is the binding's newest; a pull request that leaves an entry not holding fails the check.
- `reason` is one line of printable ASCII without `"` or `\`, at most 300 characters. `replacement` is optional.
- A binding the launcher's own native code (`native/src`) names, by its header constant, its `_t` type or a resolve string, leaves the addon API only in a pull request that also moves that code off it. A deprecated one moves to its `replacement`, so a binding core names cannot be deprecated before a replacement exists. Comments do not count.

A binding whose name is wrong is renamed in place, and its old name stays for one release as a deprecated alias:

```sh
python3 re/tools/binding.py rename Ne_Property_SetS64IfChanged Ne_Property_SetUIDBRefIfChanged
```

The record moves to its new name with its signature, its build overlay entries and `function_names.tsv`. The verify rows about the old name stay in the record as `retired:<old name>`, and the rows about the address carry over. The new name exports on an `agree` about itself. A `deprecated` entry keyed by the old name, covering the newest retired row and naming the new record as `replacement`, exports the old name as an alias of the replacement at the same address, and its `bindings.json` entry names the replacement in `alias`:

```json
  "func:Ne_Property_SetS64IfChanged": {
   "mode": "deprecated",
   "since": "1.10.0",
   "covers": "V100063",
   "reason": "renamed: every caller sets a UIDBRef weak object reference through it and the compare is object identity, not a numeric S64 compare",
   "replacement": "Ne_Property_SetUIDBRefIfChanged"
  }
```

Only the repository owner approves a change to `re/db/export_drops.json`. The `bindings` check fails on such a pull request until the owner has approved its head commit, or opened it; re-run the check after approving. The check reads the owner's reviews from the API with the read-only token.

```sh
python3 re/tools/binding.py drops                    # each entry's state; due deprecations; exits 1 on stale or overdue ones
python3 re/tools/binding.py drops --release 1.10.0   # the lines the 1.10.0 release notes carry
```

The tests fail when `docs/releases/<since>.md` exists without the line for an entry of that release, and when a binding deprecated in one release is still deprecated at a later one.

A second contributor re-derives the binding from the code alone (PROTOCOL.md §5), proves it on their own exe and adds the agree row:

```sh
re/tools/py re/tools/binding.py agree site_Sess_PitEntryDecision --exe ~/F1_25.exe --by other-login \
    --evidence "decomp:own disasm 0x1436a3ab0;xref:0x1436a3b74" --notes "same call, rcx is the car record"
```

An agree row that exports the binding also changes the generated headers in `native/include/swofty` (`game.rs` and `game/<subsystem>.hpp`), and main's tests fail when the tracked headers differ from what `bindgen.py` writes. `binding.py doctor` names the headers that change. Regenerate them and commit them in the same pull request:

```sh
python3 re/tools/bindgen.py --no-exe --json /tmp/bindings.json --symbols /tmp/symbols.tsv
git add native/include/swofty
```

`bindgen.py` writes `game.rs` through `rustfmt`, so run it where `rustfmt` is installed (`nix develop` has it). A pull request that changes the headers waits for a maintainer's review, who checks they are what `bindgen.py` writes. After the merge, the next launcher build exports the binding as `experimental`, and your addon can hook it.

Proving an existing record again, for example after a game update, adds a proof without changing anything else:

```sh
python3 re/tools/binding.py verify site_Sess_PitEntryDecision --exe ~/F1_25.exe --by your-github-login
```

## What merges itself

The `bindings` workflow labels the pull request `bindings: auto-merge` and turns on auto-merge when:

- every changed file is `re/bindings/<subsystem>/<Name>.json`, added or modified, at most 25 files;
- a new record is level L1 or L2, has no verify rows, and has a proof by the pull request's author on a build in `re/db/builds`;
- a changed record only gains one agree row and one proof, both by the pull request's author, who is neither the record's author nor an earlier verifier;
- the name, address, signature id, pattern and target are new to the knowledge base;
- no code in `native/src`, `addons`, `packages`, `templates` or `apps` already names the binding;
- the records leave what `bindgen.py` writes to `native/include/swofty` unchanged (a binding that starts exporting changes it, so its agree row gets a review);
- `binding.py check` passes, and the repository's required CI checks pass.

Anything else gets a normal review. The workflow's summary lists each reason with its fix, the same lines `binding.py doctor` prints.

## The pull request

- Only record files: a docs fix, an addon or a tool change goes in a pull request of its own.
- Up to 25 records; split larger sets.
- Title it like a commit, for example `feat(re): add the AI pit entry decision site`. The pull request template's checklist is for code changes and a record-only pull request leaves it out.
- In the description, say what the binding is for and which addon will use it.
- A pull request that needs review waits for a maintainer; the summary on the pull request says why.

## Checks CI runs without the game

- The file is at most 64 KiB of UTF-8 JSON with no duplicate keys, `NaN` or `Infinity`, exact key sets, one-line strings.
- The row passes `kb.py`'s rules: naming, subsystem prefix, level evidence tokens.
- The pattern parses, is 12 to 256 bytes, and neither starts nor ends with a wildcard.
- The anchor is under 256 bytes after the match and `expect` is 1 to 16 concrete bytes; rows that need more stay in the tables, where a maintainer owns them.
- `target` is `match + anchor` for `addr`; the rel32 or disp32 operand is wildcarded for `rel32` and `riprel`; `expect` agrees with the pattern's bytes.
- No other signature has the same section, pattern bytes, anchor, resolve and kind. The pattern is compared as parsed bytes and the offsets as numbers, the way the core scans once per pattern and resolves each binding from that match.
- The signature kind fits the binding kind, the section is `.xtls` (or `.data1` with a reason), the address is inside the KB image.
- Each proof's digest matches the signature and its exe hash; a proof on the KB build found the record's `match` and `target`, and a proof on the build in `made_on` found `made_on_match` and `made_on_target`.
- The name and address have no verify rows yet, so an old verdict cannot back a new record. A record's own verify rows are about its name, its address or its signature id, and the export gate reads only the first two.
- `kb_lint.py --structural` passes over the base branch with the PR's records added: no drift between rows, signatures, structs, verify rows and tests.
- After merging, the `main` job runs `binding.py check` and `kb_lint.py --structural` over `main` again.

## Threat model

| threat | defence |
|---|---|
| A fork's pull request runs code with a write token (`pull_request_target` checkout trap) | `validate` checks out the base branch only, reads the PR's records as git blobs into a scratch folder and parses them with the base branch's `binding.py`; its token is read-only and it uses no secrets. `automerge` checks nothing out. |
| The PR edits the workflow or the tools to pass itself | `automerge` lists the files from the API itself and refuses any path outside `re/bindings/*/*.json`; the workflow file and tools that ran are the base branch's. |
| A new commit lands between the check and the merge | auto-merge is pinned with `--match-head-commit` to the commit `validate` read; a new push re-runs `validate`, and `withdraw` turns auto-merge off when the PR stops qualifying. GitHub also disables auto-merge when someone without write access pushes. |
| A hostile record crashes or exploits the parser | size cap before parsing, strict UTF-8 JSON with no duplicate keys or `NaN`, recursion caught, exact key sets and string limits; no `eval`, pickle or YAML. Paths from the API are matched against `re/bindings/<subsystem>/<Name>.json` before they touch the file system, and only regular blobs (mode 100644) are read. |
| A record changes a binding players already rely on | only new names, addresses, signature ids, patterns and targets merge themselves; changing or deleting a record needs review. |
| A new name switches on code that already names it | a binding named anywhere in shipped code needs review. |
| A fabricated proof | CI cannot run the game, so a proof is a consistency check, not evidence. A record with one proof is not exported. Export needs an agree row from a second GitHub account (or a maintainer), and stable needs a maintainer's L3 trace test. |
| One person with two GitHub accounts | The agree row only proves a second account, so one person with two accounts can take a new binding to `experimental` alone. What they get is an experimental binding that no shipped code names (a name already used in shipped code needs review): nothing calls it unless an addon names it in a hook, and that addon already runs its own native code. They cannot change an existing binding (review) or reach `stable`: stable needs L3, and only a maintainer sets L3, after a trace test the lead runs passes on the KB build (`re/PROTOCOL.md` §1, §8). Maintainers can demote a record (`kb.py set ... level=L1`) or revert it; `bindings: auto-merge` marks every self-merged PR. |
| A PR takes a binding out of the addon API | `re/db/export_drops.json` is outside `re/bindings`, so it never merges itself, and `validate` fails until the repository owner (`github.repository_owner`, from the base branch's workflow) has approved the PR's head commit or opened the PR. |
| Two PRs add the same name | both write the same path, so the second one conflicts and stays open. Two PRs at one address under different names are caught by the `main` job and `test_db_unique`. |

## Repository settings this needs

- Allow auto-merge.
- Protect `main`: require a pull request, require the CI checks to pass (the `bindings` workflow's `validate` among them, which carries the owner's approval of export drops), do not allow bypassing.
- A `bindings: auto-merge` label.
