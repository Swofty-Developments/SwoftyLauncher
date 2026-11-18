---
title: Native runtime
---

# Native runtime

The core's lifecycle, the checks that guard every read and write of game memory, the per-build proofs, the step lines, the crash handler and the career operations. The code is in `native/src/core/runtime/`, `native/src/core/diagnostics/` and `native/src/core/career/`. This page covers how the core starts, how to ask whether memory can be read, how to gate code on a game build, how a core mixin set is offered and committed, and where the crash handler and the career operations live.

## How the core starts

The loader drives the core through its exports, and `runtime::Runtime` (`runtime/lifecycle.hpp`) runs each subsystem at its point:

```cpp
SwoftyCoreAttach(api);   // Start::attach subsystems, the binding set, the binding pass
SwoftyCoreInit(...);     // once per addon
SwoftyCoreCommit();      // installs every mixin; Start::commit_code subsystems, then runtime::ready -> Start::ready
SwoftyCoreStage(stage);  // the window stage
```

Every subsystem is one row of the ordered table in `runtime/subsystems.cpp`:

```cpp
constexpr Subsystem SUBSYSTEMS[] = {
    {"crash", Start::attach, attach_crash},
    {"settings", Start::attach, attach_settings},
    {"crash: Ne_ReportMemoryError hook", Start::attach_code, crash_game_hooks},
    {"popup: message system hook", Start::commit_code, popup_setup},
    // ...
};
```

`attach_code` and `commit_code` rows wait for the game's code to read decrypted (`when_code_ready`), which on a packed build is a later pass. The core never unloads, so there is no shutdown step.

## Core mixin sets

A subsystem that hooks the game offers a core mixin set and settles it at the commit. `loader::FunctionSet` (`runtime/loader/core_sets.hpp`) registers the pair, and `loader::MixinGate` (`runtime/loader/mixin_gate.hpp`) keeps the offer and the commit to once each:

```cpp
loader::MixinGate mixin_gate;

std::vector<CoreMixins> tyre_models_mixins()
{
    if (mixin_gate.offered() || !code_ready())
        return {};
    mixin_gate.offer();
    // ... resolve, then return the set
}

void tyre_models_commit()
{
    if (!wanted || !mixin_gate.settle())
        return;
    swofty::log("[tyres] model check %s",
                loader::install_state(core_mixins_active(MIXIN), "armed: fitted tyre models are read 10 s and 60 s into each session"));
}

static loader::FunctionSet tyre_models_core_set{loader::CoreSetId::tyre_models, tyre_models_mixins, tyre_models_commit};
```

`install_state` gives the `armed: ...` text, or `did not install (see the [core] lines)` when the set's mixins did not install. `native/test/mixin_gate_test.cpp` pins the lines.

## Reading and writing game memory

Every check shares one protection rule, in `runtime/pages.hpp`. The checks differ only in how they ask Windows and how old the answer may be:

```cpp
runtime::readable(p, n);                    // the shared page table (mem_pages.cpp), answers kept a few ms
runtime::pages::readable_now(a, n);         // one VirtualQuery per page, no lock, no allocation: the crash handler
runtime::pages::readable_in_region(p, n);   // the n bytes end inside one readable region
runtime::pages::writable_in_region(p, n);   // one committed, writable region, not a guard page
runtime::pages::read_write_span(at, n);     // readable, and every region it crosses is read-write
```

None of them raises an exception, and none makes a read safe against a page that changes after the answer. Code that reads a game object reads through `swofty::mem::read`, which asks `readable` first.

## Gating code on a game build

A game update moves struct offsets, so code that reads or writes a struct field names the builds on which that layout was checked. `runtime/build_proof.hpp` knows each build by its PE header, and a gate names a proof and the bindings its code calls:

```cpp
namespace bp = runtime::build_proof;

constexpr bp::Proof SESSION_LAPS{"the session settings' laps layout (+0xdf0, +0xdf8)",
                                 bp::proofs::VERIFIED_LAYOUT.builds};   // or bp::bit(bp::Build::kb) | ...
constexpr bp::Gate RACE_LAPS{"the direct start's race laps", &SESSION_LAPS};   // testing/direct_start.cpp

if (!runtime::holds(RACE_LAPS))   // logs once: "[build] the direct start's race laps off on <exe>: <proof> is proven on <builds> only"
    return;
```

A gate whose code calls bindings names them too, and holds only where each resolves:

```cpp
constexpr const char *LEVER_BINDINGS[] = {"Car_...", "Car_..."};
constexpr bp::Gate LEVERS{"live car levers", &CAR_LEVERS, LEVER_BINDINGS};   // "...: Car_... does not resolve"
```

- `runtime::holds(gate)` answers and logs one `[build]` line per gate the first time it refuses.
- `runtime::proven(gate)` answers without logging.
- `bp::proven_on(proof, runtime::running_build())` answers for a proof without bindings.

An exe the table does not list holds no proof, so gated code stays off on a game update until its layout is checked again with `re/tools/sigport.py --compare` and `re/tools/sigverify.py --disp`. Adding a build is one row in `KNOWN` and one `Build` value. `same_build()` (the KB build) and `verified_build()` (`proofs::VERIFIED_LAYOUT`) read the same table. `native/test/build_proof_test.cpp` covers the KB build, Steam 1.0.145 and an unknown build.

## Step lines

A boot or a load logs each phase as one stable line that the launcher's tools read (`diagnostics/steps.hpp`):

```cpp
perf::Stopwatch steps;
start(Start::attach);
loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach subsystems");   // [perf] step 12.3 ms: attach subsystems
```

Attach-time lines go through the loader's `api->log`, because `swofty::log` has no API until the settings subsystem has attached. An addon init slower than `SLOW_ADDON_INIT_MS` adds a `[perf] slow addon init` line beside its step.

## The crash handler

The crash handler in `diagnostics/crash/` is split by what runs when:

| File | Runs |
|---|---|
| `crash.cpp` | `crash_attach` and `crash_game_hooks` (`crash.hpp`), the two subsystem entry points |
| `capture.cpp` | the vectored handler, the top-level filter and the exit hooks, on fixed-size state that never allocates |
| `fatal_paths.cpp` | the game's own fatal paths (`Ne_ReportMemoryError`, fast fails) |
| `report_writer.cpp`, `report_sections.cpp` | the writer thread that turns one request into `crash-<time>.txt` |
| `symbolizer.cpp`, `minidump.cpp`, `dred.cpp` | symbol names, the minidump and the D3D12 DRED data for the report |
| `stall_report.cpp`, `stall_walk.cpp` | the load stall report |

Code that runs inside a handler reads with `pages::readable_now` and does not allocate or lock. The crash files do not include `runtime.hpp`; the few runtime calls they make are declared at the top of each file.

## Career operations

The career operations (`SwoftyCareerOps`, standard library 1.6) are in `career/ops/`. A batch of entrant writes is planned by pure headers and run by the executor:

| File | Role |
|---|---|
| `career_ops.cpp` | the API table and the state shared between calls |
| `lineup.cpp`, `lineup.hpp` | the line-up executor: one batch of moves, numbers, cars and looks, read back into a report |
| `entrants.cpp`, `entrants.hpp` | the entrant rows, one write per row, the contracts and the batch's finalise |
| `labels.cpp`, `tables.hpp` | the database's driver, look and team labels |
| `apply_teams.cpp`, `car_numbers.cpp` | `apply_teams` and `set_car_number` |
| `*_plan.hpp`, `driver_rows.hpp`, `fuel_logic.hpp` | the pure planners the host tests run (`run_career_ops_test.sh`) |

An operation runs on the flow thread (`flow_tid`) or before the first frame, and resolves its bindings once through `bind`.
