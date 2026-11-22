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

A set whose handlers keep state derives one object from `loader::HookSet` (`runtime/loader/hook_set.hpp`): its `MixinGate`, the lock over the subsystem's state, live and off with the reason, the bind step, and an `Original` for each wrapped game function:

```cpp
struct CategoryHooks : runtime::loader::HookSet<std::recursive_mutex> {
    runtime::loader::Original<Game_CreateSessionDlcSelectTab_BuildTiles_t> build_tiles;
    runtime::loader::Original<Game_CreateSessionDlcSelectTab_HandleCommand_t> handle_command;
    runtime::loader::Original<Game_Network_CategoryOptionUsing_t> option_using;
};

CategoryHooks hooks;

void build_entry(void *vm)
{
    hooks.build_tiles(vm);
    if (!hooks.live() || hooks.off())
        return;
    std::lock_guard<std::recursive_mutex> g(hooks.lock);
    // ... add the addon boxes
}

std::vector<CoreMixins> car_category_mixins()
{
    std::vector<CoreMixins> out;
    if (hooks.gate.offered() || hooks.off() || !code_ready())
        return out;
    hooks.gate.offer();
    std::string why;
    if (!CategoryHooks::bind({{"Game_CreateSessionDlcSelectTab_BuildTiles", &b.build}, {"g_Vm_Registry", &b.registry}}, why)) {
        hooks.turn_off(why);
        swofty::log("[car-category] addon boxes off: %s", why.c_str());
        return out;
    }
    out.push_back({MIXIN, {{"Game_CreateSessionDlcSelectTab_BuildTiles", SWOFTY_MIXIN_WRAP, 100, (void *)&build_entry,
                            hooks.build_tiles.slot()}}});
    return out;
}

void car_category_commit()
{
    if (hooks.off() || !hooks.gate.settle())
        return;
    if (core_mixins_active(MIXIN))
        hooks.set_live();
    else
        hooks.turn_off("the Car Category hooks did not install (see the [core] lines for " + std::string(MIXIN) + ")");
}
```

The career contents and their tile route, the car category boxes, the box grid and the save splice keep their hook state this way. `native/test/hook_set_test.cpp` pins the bind step's reason, `needs <ids>, which this build does not resolve`.

A set whose parts addons ask for one by one keeps a `loader::MixinPart` per part: the mixin's name and label, wanted, its `MixinGate`, live and whether its late registration was logged. The career wizard's five controls and the character refs and voices are parts:

```cpp
runtime::loader::MixinPart ref{REF_MIXIN, "ref"}, voice{VOICE_MIXIN, "voice"};

std::vector<CoreMixins> characters_mixins()
{
    std::vector<CoreMixins> out;
    if (ref.offer()) // wanted, and offered for the first time
        out.push_back({REF_MIXIN, {/* ... */}});
    return out;
}

void characters_commit()
{
    bool any = false;
    for (runtime::loader::MixinPart *p : {&ref, &voice})
        any |= p->settle([p] { return core_mixins_active(p->name); });
    if (any)
        swofty::log("[characters] setup: character refs %s, voices %s", ref.state(), voice.state()); // armed, off, not used
}
```

`usable(core_mixins_closed())` answers whether an addon's entry can still take effect, and `first_late()` is true once, for the line a registration after the last commit logs.

Two hooks stay outside these objects. `diagnostics/tyre_models.cpp` keeps a `MixinGate` and its one original: a `HookSet` holds its reason as a `std::string`, whose exit-time destructor adds the unit to the exit section of `tools/core_init_order.txt`. `diagnostics/crash/dred.cpp` places its D3D12CreateDevice hook with `safetyhook::InlineHook::create` in the crash handler's attach step (`crash_attach`), on d3d12.dll's export from `GetProcAddress` with no signature or build gate; it logs through the crash log's own line function, and its line says only `hooked` or `NOT hooked`.

A safetyhook mid hook or inline hook goes on through `loader::install` (`runtime/loader/hook_install.hpp`). Given a `HookSite` it checks the site's build gate, finds its signature and places the hook; given an address it only places the hook:

```cpp
// career/career_start.cpp: no build gate, a line of its own for each step
auto r = loader::install(h_run, {S_RUN_ROUTE, "start gate", "career"}, reinterpret_cast<void *>(&run_route_hook));
if (r.step == loader::Installed::Step::not_found)
    swofty::log("[career] start gate: Vm_Run_Route did not resolve; career.starting never fires and blocks cannot "
                "cancel a route");

// diagnostics/session_job_probe.cpp: an address the probe resolved itself
if (auto r = runtime::loader::install(hook, at, FUNC, on_entry); !r)
    swofty::log("[session-job] 0x1445acb00 entry probe: the hook failed: %s", r.error.c_str());

// a gated site whose hook set turns off with the reason
if (!runtime::loader::install(hooks, h_settings, runtime::loader::HookSite{S_SETTINGS, "direct", "direct", &RACE_LAPS},
                              reinterpret_cast<void *>(&settings_detour)))
    swofty::log("[direct] race laps off: %s", hooks.why().c_str());
```

| `Installed::step` | `why` |
|---|---|
| `placed` | empty; `at` is the hooked address |
| `not_proven` | `<feature> is not proven on this build`, after `runtime::holds` logs its `[build]` line |
| `not_found` | `<what> did not resolve on this build` |
| `failed` | `the hook on <what> at base+0x<rva> failed: <error>`, with `error` the `runtime::hook_error` text, or `at 0x<address>` outside the game's image |

A site with lines of its own reads `step`, `at` and `error`. The career start gate, the session job probe and the Team HQ hub tab builders install this way.

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

## Deprecated and dropped bindings

A `bindings.json` entry can carry `"deprecated": {"since", "reason", "replacement"?}`, and the set a top-level `"dropped": [{"id", "since", "reason", "replacement"?}]`. `runtime/loader/binding_policy.hpp` reads both when the set loads, and `BindingSet::note_use` answers each resolve. The set logs its counts at load when either is not 0, and an id its line on its first resolve:

```text
[core] bindings.json: 2 binding(s) deprecated, 1 dropped
[bind] id=Game_Old deprecated since=1.10.0 replacement=Game_New reason="split in two by the 1.0.145 patch"
[bind] id=Game_Gone dropped since=1.10.0 replacement=- reason="the game no longer has it"
```

- A deprecated id still resolves. A dropped id resolves to 0, like an id the set does not list, and an addon with a mixin on it is dropped with `needs <id>, which <file> drops`.
- `since` and `replacement` are printable ASCII without a space, `"` or `\`. `reason` is printable ASCII, 1 to 300 characters, without `"` or `\`.
- A malformed entry logs one `[core] <file>: ...` line at load and is left out: its binding resolves with no notice, or nothing is dropped. A `dropped` id the set still lists is malformed.
- `native/test/binding_policy_test.cpp` covers each line and the malformed entries.

## Code the core patches

The server's memory challenges hash regions of the game's code and read-only data, so they skip every byte the core writes there. `native/build.sh` writes `native/release/swofty-core-<version>-patches.json`, outside the game folder and not in player installs. The release workflow attaches it to the GitHub release as `swofty-core-<version>-patches.json`, so the server fetches it by core version. Each site per supported build:

```json
{"schema": 1, "module": "swofty.core", "version": "1.10.0", "hook_length": 32, "jump_span": 14,
 "builds": [{"exe_sha256": "d873...", "pe_timestamp": 1789118616, "exe_size": 408063872, "distribution": "Steam",
             "packed": false, "set": "bindings.json", "resolved_here": false}],
 "sites": [{"build": "d873...", "owner": "core/frontend/home_tiles.cpp", "name": "FeHome_Handle_Command",
            "source": "binding", "write": "inline", "rva": 29047840, "length": 32, "exact": false},
           {"build": "d873...", "owner": "core/frontend/hidden_tiles.cpp", "name": "S_LAN_TILE", "source": "signature",
            "write": "bytes", "rva": null, "pattern": "4C 8D 05 ?? ...", "section": ".xtls", "anchor": 17,
            "resolve": "addr", "length": 9, "exact": true}]}
```

- `write` is `inline` or `mid` (a safetyhook jump over whole instructions), `bytes`, `call` (a call's rel32), `vtable` (8 bytes a slot) or `import` (an import address table slot).
- `length` counts the bytes written from `rva`. A hook's span is the whole instructions covering `jump_span` bytes. Without the exe it is `hook_length` and `exact` is false.
- A binding site takes its rva from the build's set: the KB build's `rva`, or `match + anchor` for an address binding with a `match`.
- A signature site (`runtime::Sig` and the `Target` structs with its layout) and every other binding site are left for a tool that holds the exe, with `rva` null:

```text
hit  = the one match of pattern in section ("*" is every executable section)
site = hit + anchor                         resolve "addr"
site = hit + next + int32 at (hit + anchor) resolve "rel32" or "riprel"
```

- An import row carries `"import": "<dll>!<function>"`, either part `*` for any. A binding row may carry the binding's `match` and `string_anchor`.
- `SWOFTY_STEAM_EXE` resolves that build's sites on the exe and decodes each hook's instructions, through `re/tools/py`. `re/tools/py native/tools/patch_sites.py --resolve patches.json --image <exe> --sets <folder of bindings*.json> --out <file>` does the same for a written file: each row of the exe's build gets its rva and decoded length, or an `unresolved` reason.
- Addon mixins are not in the file. The core installs them per session.

`native/tools/patch_sites.py` reads the core's mixin tables, the SDK mixin macros, the binding sets and `native/tools/patch_sites.txt`. The list names the targets of every other line in `native/src` that writes code: a safetyhook create, a gated install, a `VirtualProtect` that grants write access, a patch set write and a `WriteProcessMemory`, plus the callers its `helper` patterns match. `python3 native/tools/patch_sites.py --check` (the `patch_sites` host test) fails on a line it does not list, on an entry whose line is gone, on a target that is not a signature or a binding, and on a mixin kind used outside an entry it reads. The build refuses to write the file on the same failures. A new hook adds its line to the list with its write and targets:

```text
core/frontend/mfd.cpp: auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(update), reinterpret_cast<void *>(&update_hook)); => inline sig S_UPDATE
```

`native/test/patch_sites_test.py` covers the check and the file on a fixture tree, two sets and a synthetic exe.

## Step lines

A boot or a load logs each phase as one stable line that the launcher's tools read (`diagnostics/steps.hpp`):

```cpp
perf::Stopwatch steps;
start(Start::attach);
loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach subsystems");   // [perf] step 12.3 ms: attach subsystems
```

Attach-time lines go through the loader's `api->log`, because `swofty::log` has no API until the settings subsystem has attached. An addon init slower than `SLOW_ADDON_INIT_MS` adds a `[perf] slow addon init` line beside its step.

## The boot plan

The boot plan counts the new database keys every boot applier adds, before the game loads its database. The `gamedb` subsystem starts it at the ready stage, and the count of the content rows runs on a worker while the stage goes on (`apply/scheduler.cpp`):

```cpp
moments::start_boot_plan();          // scheduler().start_plan(true), or start_plan(false) with SWOFTY_PLAN_SYNC=1
if (edits.empty() && !moments::boot_may_work())
    without_database("not_needed", "");
// ...
scheduler().planned_keys();          // in reserve_target: the first reader waits for the worker
```

`start_plan` runs on the calling thread:

- Every boot applier's `prepare`: the stock teams and constructors, the venues and the track seeds.
- Every count that writes the store: the tracks' `database_keys`, which declares the record tracks' data variants.
- The hook test. When no `counts_off_thread()` applier `may_count`, those appliers count on this thread too, and `boot_may_work()` is the counted plan's `boot_work()`. While a worker counts, `boot_may_work()` is true, and the database load is hooked. The content rows `may_count` only when a row registry holds an addon record, which makes boot work in any case.

The `counts_off_thread()` appliers then count on a worker. These are the content rows, whose `database_keys` expands every content record against the game database file and only reads the store. Every reader of the plan waits for the worker first: `planned_keys`, `boot_work`, `run_boot` and `plan_boot`. At that wait the store's `generation()` is compared with its value when the worker started. If the store changed, the waiting thread forgets the worker's count and counts again. The content expansion reads no track registry, so it counts the same before and after the tracks' variants.

The plan logs these step lines:

```text
[perf] step 12.0 ms: boot plan prepare (24)
[perf] step 30.1 ms: boot plan counts on this thread (3)
[perf] step 1210.4 ms: boot plan count off the thread (21)
[perf] step 0.2 ms: boot plan wait for the count off the thread
[perf] step 1301.7 ms: boot plan on this thread (24)              (SWOFTY_PLAN_SYNC=1)
[perf] step 1190.3 ms: boot plan count again on this thread (21)  (the store changed during the count)
```

`native/test/apply/apply_test.cpp` covers the scheduler side. `native/test/run_content_test.sh` compares the database edits from both paths over Classic Seasons on the game database.

### The boot expansion made ahead

The boot applier for content rows (`apply/content/appliers.cpp`, `boot_ahead.hpp`) expands every addon record into database writes inside the game's database load. When the database load is hooked, a worker makes that expansion ahead and the boot adopts it.

- The worker takes the content lock only to read the game database file and the older addons' edits. It then works on a copy of the stat-id store (`ids::Store`) with every addon admitted, and holds the stat-id and pool log lines in a list.
- `run_boot` adopts the work when `ahead_refusal` finds nothing, and writes the held lines in the order the boot writes them. Otherwise it runs the same steps again on the live store.
- `SWOFTY_BOOT_AHEAD=0` turns the worker off and logs `[apply] boot ahead: SWOFTY_BOOT_AHEAD=0, so the boot expansion runs inside the database load`.

The boot refuses the work, and logs why, in these cases:

| Case | Check |
|---|---|
| the registry changed | the store's `generation()` differs from its value when the worker started (the boot's own context publish is left out) |
| the stat-id store changed | `ids::Store::commit` compares its generation under its lock |
| `SWOFTY_BOX_WORLDS` differs | the value the worker read against the value at the boot |
| the date changed | the day the stat ids are dated with |
| the older addons' edits changed | the edits the worker read against the edits at the boot |
| the room leaves an addon out | at the boot, or once the addon's rows are counted |
| the worker did not finish | it failed, or never got the content lock before the boot |

`release_database` cancels a worker that has not started and waits for one that has. The registry digest line `[registry] boot rows digest` is the same with and without the worker.

`native/test/content/rows_test_plan.cpp` forces each case over a small team database. `native/test/content/check_boot_ahead.py` runs `rows_tool --ahead` on the game database for Classic Seasons, Classic Seasons over older addons' edits, a box world box, a room that leaves an addon out and each `--ahead-change`, and compares the database, the stat ids and the stat-id lines with the boot's own path.

### Registry writes after the register pass

`SwoftyCoreRegisterDone` ends the register pass (`Store::end_register`). On a boot registry, `add`, `variant`, `provide` and `layer` are refused as `late` after that. These calls are still accepted on a boot registry and change what the boot plan and the boot apply read:

| Call | Effect on a boot registry |
|---|---|
| `clear_layer` | drops an addon's layer |
| `tag_add` | adds a record to a tag a layer targets |
| `protect` | protects fields from other addons' layers |
| `invalidate` | makes a dynamic provider's records read again |
| `declare` | a newer minor replaces the registry's schema |

A call made after the plan's wait changes the boot apply but not the plan. The plan's counts size the value map and admit addons for room, so the boot can then lay out a different number of keys than the plan counted. A call that lands before the wait moves the store's generation, and the plan is counted again. `native/test/content/rows_test_plan.cpp` shows the current behaviour: `clear_layer` on `team.identity` after the register pass is accepted and changes the expansion. To close the gap, these calls would be refused as `late` on boot registries after the register pass. Addons would see that change.

## The crash handler

The crash handler in `diagnostics/crash/` is split by what runs when:

| File | Runs |
|---|---|
| `crash.cpp` | `crash_attach` and `crash_game_hooks` (`crash.hpp`), the two subsystem entry points |
| `capture.cpp` | the vectored handler, the top-level filter and the exit hooks, on fixed-size state that never allocates |
| `fatal_paths.cpp` | the game's own fatal paths (`Ne_ReportMemoryError`, fast fails) |
| `report_writer.cpp`, `report_sections.cpp` | the writer thread that turns one request into `crash-<time>.txt`, one step at a time |
| `report_memory.cpp`, `report_process.cpp` | the sections about memory (the exception, the memory at the registers, the Ne heap) and about the process (modules, threads, loader.log) |
| `report_text_logic.hpp` | the report text that reads no process state, which `test/report_text_test.cpp` pins |
| `symbolizer.cpp`, `minidump.cpp` | symbol names and the minidump for the report |
| `dred.cpp`, `dred_markers.cpp`, `dred_nodes.cpp` | DRED's arming and device hook, the removal markers between sessions, and a removed device's breadcrumbs, contexts and allocations (`dred_state.hpp`) |
| `stall_report.cpp`, `stall_walk.cpp`, `stall_frames.cpp` | the load stall report (`stall_report.hpp`), its unwinder and its frames' function tables and export names |

Code that runs inside a handler reads with `pages::readable_now` and does not allocate or lock. The crash files do not include `runtime.hpp`; the few runtime calls they make are declared at the top of each file.

## The profiler

`diagnostics/perf.hpp` is the profiler's API (`perf::Scope`, `perf::Pause`, `mark`, `frame_claim`). Its state is one `Profiler` object (`perf/profiler.hpp`), defined in `perf.cpp`; its members are defined by concept in `diagnostics/perf/`, next to the parts with state of their own:

| File | Holds |
|---|---|
| `perf.cpp` | the `Profiler` object, its calibration, `attach`, `start` and `flush` |
| `perf/profiler.hpp` | `Profiler`: the probe names and per-thread counters, the frame clock and histograms, the hitch, slow call and timeline rings |
| `perf/probes.cpp` | probes, `enter` and `leave`, the per-thread counters, slow calls, `owner_of` and the `SwoftyPerf*` exports |
| `perf/frames.cpp` | frames from the frame token and the flow ticks, the histograms, hitches, `frame_claim` and the timeline marks |
| `perf/report.cpp` | `perf.log`, the minute lines and the crash report's probe text |
| `perf/vq.cpp` | the VirtualQuery calls from swofty modules and the process's CPU use per thread |
| `perf/sysmem.cpp` | RAM, commit, the game's working set and each adapter's VRAM |
| `perf/counters.hpp` | the relaxed counters and `ms_of`, which they all use |
| `perf_logic.hpp` | the text of every `[perf]` line, which `test/perf_logic_test.cpp` pins |

A new `[perf]` line is a function in `perf_logic.hpp` with a case in the test:

```cpp
swofty::log("%s", logic::hitch_line(ms, h.during, swofty_ms, h.top).c_str());
```

## Career namespaces

Code in `core/career/` is in `career::`. The entry points `runtime/runtime.hpp` declares stay in `runtime`: `career.cpp`, `career_saves.cpp` and `career_start.cpp` define them over unnamed-namespace state, and `career_wizard.cpp` over `career::wizard`:

| Namespace | Files |
|---|---|
| `career::state` | `career_state.hpp`: the flow-state and Team HQ tracking without the game |
| `career::game` | `career_entity.hpp`, `career_entity.cpp`: the championship entity, its components and entrants |
| `career::content` | `career_content_logic.hpp`, `content_state.hpp`, `career_content.cpp`, `content_*.cpp` |
| `career::characters` | `characters_logic.hpp`, `characters_state.hpp`, `characters.cpp`, `characters_hooks.cpp` |
| `career::hub` | `hub_state.hpp`, `hub_tab.cpp`, `hub_team_hq.cpp`, `hub_rnd.cpp` |
| `career::wizard` | `wizard_state.hpp`, `career_wizard.cpp`, `wizard_hooks.cpp` |
| `career::saves`, `career::start`, `career::ui` | `career_saves_logic.hpp`, `career_start.hpp`, `career_ui.hpp` |
| `career::save_data`, `career::save_splice` | `save_data.hpp`, `save_data.cpp`, `save_splice_logic.hpp`, `save_splice.cpp` |
| `career::save_diag`, `career::save_heap` | `save_diag_logic.hpp`, `save_heap_logic.hpp` |
| `career::ops` | `ops/`: the career operations below |
| `career::snapshots` | `snapshots/`: the career snapshot book, its entry format and the save bag |

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
| `report_queue.hpp` | `report_season` calls from off the flow thread, queued and run at the next flow tick (the snapshots host test) |

An operation runs on the flow thread (`flow_tid`) or before the first frame, and resolves its bindings once through `bind`. A `report_season` from another thread is queued and runs in the career operations' frame callback at the next flow tick.

## Garage namespaces

Code in `core/garage/` is in `garage::`. `garage_placement.cpp` defines `garage_placement_reset`, which `runtime/runtime.hpp` declares, and its core set in `runtime`, over `garage::edits`:

| Namespace | Files |
|---|---|
| `garage` | `garage_owner.hpp`, `garage_car_pose.hpp`, `garage_central.hpp` (with `crew_detail`), `garage_rig.hpp` (with `rig_detail`) |
| `garage::edits` | `garage_edits.hpp`, `garage_placement.cpp`, `garage_crew_edits.cpp`, `garage_member_edits.cpp`, `garage_car_edits.cpp` |
| `garage::members`, `garage::vehicle` | `garage_members.hpp`, `garage_vehicle.hpp` |
| `garage::crew_pose` | `garage_crew_pose.hpp` |
| `garage::steam_26566308` | `steam_26566308.hpp`: the authored pits of Steam build 26566308 |

## Units split across files

These units keep their state in the first file listed; the other files hold code by concept and reach that state through the unit's shared header:

| Files | Hold |
|---|---|
| `runtime/loader/mixin_manager.cpp` | the `MixinManager` object, registration, the core sets, `on_load` and addon init |
| `runtime/loader/mixin_install.cpp` | the install plan, dropped conflicts, chained groups, `install` and `install_late` |
| `runtime/loader/mixin_queries.cpp` | active core sets, code owners, overwrites, load indexes and the `runtime::` query calls |
| `career/career.cpp` | flow states, the Team HQ answer, the tracker and the `SwoftyCareer` table |
| `career/career_entity.cpp`, `career_entity.hpp` | the championship entity, its components and entrants, and the My Team vehicle heal |
| `career/save_splice.cpp` | the splice's store, its load and save sites, the career slot reads and its core set |
| `career/save_data.cpp`, `save_data.hpp` | the game's save-data objects: the root, a child's bag and its encoding, the string table, slot managers |
| `career/snapshots/snapshots.cpp` | the snapshots entry, the save and career moments and the reads |
| `career/snapshots/freeze.cpp`, `book_cache.hpp` | `Book::freeze`, `season_start` and `career_created` |
| `career/hub_tab.cpp`, `hub_state.hpp` | the hub tabs and their options, the page, the laptop builder, setup and `SwoftyCoreHub` |
| `career/hub_team_hq.cpp` | the Driver Career Team HQ builder |
| `career/hub_rnd.cpp` | the R&D tab's visibility on Team HQ and the laptop, and `hub_tick` |
| `career/career_content.cpp`, `content_state.hpp` | the content registry and its core sets |
| `career/content_hooks.cpp` | the router, gate and slot predicate wraps and `bind_all` |
| `career/content_api.cpp` | the addon table (`SwoftyCoreCareerContent`) |
| `career/content_tiles.cpp` | the content tiles and the Addons row |
| `garage/garage_placement.cpp`, `garage_edits.hpp` | the bays, the scene, sequence and update hooks, visits, the departure restore and the export tables |
| `garage/garage_crew_edits.cpp` | the garage root and central crew poses, reads and restore |
| `garage/garage_member_edits.cpp` | the mechanic list, reads, edits and restore |
| `garage/garage_car_edits.cpp` | the parked car edit, its verification and restore |
| `runtime/loader/bindings.cpp` | choosing and loading the binding set, its KB addresses and the binding report |
| `runtime/loader/binding_resolve.cpp` | resolving a binding by address, pattern or anchor, deferral and the retry passes |
| `runtime/settings.cpp` | settings.json's addon options, the settings API, the MFD rows and the commands |
| `runtime/settings_logic.hpp` | the option model, the requests.json text and an option's value (`test/settings_logic_test.cpp`) |
| `career/career_wizard.cpp`, `wizard_state.hpp` | the wizard entries and their records, the parts, the `SwoftyCareerWizard` table, the core sets and the tile routes |
| `career/wizard_hooks.cpp` | the start in F1, step and option, confirm route and driver tab wraps and sites |
| `career/characters.cpp`, `characters_state.hpp` | the character entries, the active group, the strings and the `SwoftyCharacters` table |
| `career/characters_hooks.cpp` | the character ref and voice wraps and the character list's rebuild |
| `career/characters_logic.hpp` | which swap and mute act in the active group (`test/characters_logic_test.cpp`) |
| `diagnostics/net_trace.cpp` | the socket trace's log, socket notes and setup |
| `diagnostics/net_trace/hooks.cpp`, `trace.hpp` | the ws2_32 hooks and their table |

No added file has a static object of its own; each follows its unit's first file in `tools/core_link_order.txt` when that file is listed.

The swofty_registry entry codec is header-only: `career/snapshots/items.hpp` holds the tagged items and their checksum, `format.hpp` the entry, its careers and seasons, and `bag.hpp` the property bag the save carries it in.
