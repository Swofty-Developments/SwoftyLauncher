---
title: Adding a binding
description: Add a new F1 25 binding for your engine plugin - one file under re/bindings, proven on your own game exe, merged by CI.
---

# Adding a binding

A binding names one place in `F1_25.exe` that your [mixins](/reference/addons/hooking) can hook. A new binding is one JSON file under `re/bindings/<subsystem>/<Name>.json`. A pull request that adds only that file and passes every check merges itself.

## The whole flow

```sh
# 1. Make the record. binding.py writes the signature, checks it and proves it on your exe.
re/tools/py re/tools/binding.py new site_Sess_PitEntryDecision 0x1436a3b83 --kind site \
    --author your-github-login --exe ~/F1_25.exe \
    --evidence "decomp:own disasm 0x1436a3ab0-0x1436a422c;xref:0x1436a3b74,0x1436a3b83" \
    --notes "call that decides whether the AI pits this lap; rcx = car record"

# 2. Run what CI runs.
python3 re/tools/binding.py check

# 3. Commit only that file and open a pull request.
git add re/bindings/session/site_Sess_PitEntryDecision.json
git commit -m "feat(re): add the AI pit entry decision site"
```

`re/tools/py` is the pinned Python with `pefile`, `numpy` and `capstone`, which `new` needs to make the signature. `check`, `verify` and `status` run on any Python 3.10+.

| `--kind` | binding | signature |
|---|---|---|
| `func` | a function you wrap | `func_start` at the function |
| `site` | an instruction you run code at | `mid` at the instruction, with `expect` bytes |
| `data` | a global (`--type`, `--via <insn>`) | `riprel` through the instruction at `--via` |

Options: `--subsystem` when the name's prefix is ambiguous, `--proto "<C prototype>"` for a `func`, `--level L1` for a pattern match without a read decompile, `--match-from <va>` to start the pattern at an earlier instruction, `--func-start` and `--data1-reason` for a `.data1` site (PROTOCOL.md §2).

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
| L1, or L2 without an agree row | not exported |
| L2 with an agree row from someone other than the author | `experimental` |
| L3 with a passed trace test (maintainer) | `stable` |

A second contributor re-derives the binding from the code alone (PROTOCOL.md §5), proves it on their own exe and adds the agree row:

```sh
re/tools/py re/tools/binding.py agree site_Sess_PitEntryDecision --exe ~/F1_25.exe --by other-login \
    --evidence "decomp:own disasm 0x1436a3ab0;xref:0x1436a3b74" --notes "same call, rcx is the car record"
```

That pull request merges itself too. The next launcher build exports the binding as `experimental`, and your addon can hook it.

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
- `binding.py check` passes, and the repository's required CI checks pass.

Anything else gets a normal review. The workflow's summary lists the reason.

## Checks CI runs without the game

- The file is at most 64 KiB of UTF-8 JSON with no duplicate keys, `NaN` or `Infinity`, exact key sets, one-line strings.
- The row passes `kb.py`'s rules: naming, subsystem prefix, level evidence tokens.
- The pattern parses, is 12 to 256 bytes, and neither starts nor ends with a wildcard.
- `target` is `match + anchor` for `addr`; the rel32 or disp32 operand is wildcarded for `rel32` and `riprel`; `expect` agrees with the pattern's bytes.
- The signature kind fits the binding kind, the section is `.xtls` (or `.data1` with a reason), the address is inside the KB image.
- Each proof's digest matches the signature and its exe hash; a proof on the KB build found the record's `match` and `target`.
- After merging, the `main` job runs `binding.py check` over every record again.

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
| Two accounts by one person | an experimental binding is used only by addons that name it, and those addons run native code already. Maintainers can demote a record (`kb.py set ... level=L1`) or revert it; `bindings: auto-merge` marks every self-merged PR. |
| Two PRs add the same name | both write the same path, so the second one conflicts and stays open. Two PRs at one address under different names are caught by the `main` job and `test_db_unique`. |

## Repository settings this needs

- Allow auto-merge.
- Protect `main`: require a pull request, require the CI checks to pass, do not allow bypassing.
- A `bindings: auto-merge` label.
