# Native guards

If you're adding a guard to the native core, this is how guards are built and tested. A guard stops a game function from
faulting on data it doesn't expect: a car an addon removed, a track with no track record, a save from an older build.
Every guard lives in `native/src/core/guards/<area>/`. Its decisions (is this entry usable, which row does it go in)
live in a `<name>_logic.hpp` header with no Windows or game code, which a host unit test includes.

A guard is built on one of four bases:

- `SiteGuard` (`guards/guard.hpp`): mixins on game code that the core's mixin installer puts in place. Most guards.
- `HookGuard` (`guards/guard.hpp`): safetyhook hooks the guard installs itself once the game code is ready. A guard
  with one mid hook declares it as a `MidHookSpec` (`guards/mid_hook.hpp`).
- `Guard` (`guards/guard.hpp`): mixins whose offer or log lines don't fit `SiteGuard`. You write `build()` and `settle()`.
- `OnDemandGuard` (`guards/on_demand.hpp`): a hook installed the first time a feature asks for it.

## SiteGuard

Use a `SiteGuard` when the guard is one or more site, wrap or patch mixins on bindings the core resolves. The ERS mode
guard (`guards/cars/ers_mode.cpp`) is the whole declaration below plus its handler, `mode_load`:

```cpp
guards::SiteGuard guard{guards::CoreSetId::ers_mode,
                        {
                            .set = MIXIN,
                            .name = "[ers] mode guard",
                            .env = "SWOFTY_ERS_MODE_GUARD",
                            .needs = {SITE},
                            .mixins = {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&mode_load, nullptr}},
                            .armed = "a car whose ERS mode is out of range gets its ERS reset to Medium instead of faulting",
                        }};
```

From that spec the guard logs four lines, all built by `guards/lines.hpp`:

```text
[ers] mode guard off (SWOFTY_ERS_MODE_GUARD=0)
[ers] mode guard off: needs site_Car_ErsGearModifierLoad, which this build does not resolve
[ers] mode guard armed: a car whose ERS mode is out of range gets its ERS reset to Medium instead of faulting
[ers] mode guard did not install (see the [core] lines)
```

The first two come from the offer: `SWOFTY_ERS_MODE_GUARD=0` or `off` turns the guard off, and a binding in `needs` that
the build doesn't resolve keeps its set back. The last two come from the settle, after the install pass that follows
the offer.

The rest of the spec covers what guards need beyond that:

```cpp
.keeps = {{TRACK_DONE, &track_done}},   // resolved first, into an address your handler uses
.needs = {TRACK_LOAD, TRACK_DONE},      // the order the needs line names them in
.settles_when_off = true,               // log the settle line even when the guard is off
.parts = {{                             // a further set, offered and settled on its own (guards/tracks/track_acclim.cpp)
    .set = PENALTY_MIXIN,
    .name = "[acclim] track acclimatisation penalty guard",
    .keeps = {{PENALTY_RETURN, &penalty_return}},
    .needs = {PENALTY_ENTRY, PENALTY_RETURN},
    .mixins = {{PENALTY_ENTRY, SWOFTY_MIXIN_SITE, 100, (void *)&penalty_entry, nullptr}},
    .armed = "a practice programme penalty flag with no targets returns instead of faulting",
}},
```

A part shares the guard's variable. It's offered when its own needs resolve, whether or not the guard's own set did, and
it logs its own needs, armed and did-not-install lines under its own name.

For optional mixins in the guard's own set, or a check after the needs resolve, subclass `SiteGuard` and override
`extend()` or `admit()`. The standings guard adds its record load site when the build has it
(`guards/career/standings_guard.cpp`):

```cpp
class StandingsGuard final : public guards::SiteGuard {
public:
    StandingsGuard() : SiteGuard(guards::CoreSetId::standings_guard, {/* spec */}) {}

private:
    void extend(runtime::CoreMixins &set) override
    {
        uintptr_t record_load_site = swofty::api->resolve(RECORD_LOAD);
        if (record_load_site) {
            record_next = record_load_site + RECORD_LOAD_LEN;
            set.mixins.push_back({RECORD_LOAD, SWOFTY_MIXIN_SITE, 100, (void *)&record_load, nullptr});
        } else
            swofty::log("[career] weekends track guard off: site_Career_WeekendsTrackRecordLoad is not resolved in this build");
    }
};
```

## HookGuard and MidHookSpec

Use a `MidHookSpec` when the guard installs one safetyhook mid hook itself, on a site it finds by byte scan
(`runtime::find`) or by binding. The teammate pairs guard (`guards/session/teammate_pairs_guard.cpp`):

```cpp
uintptr_t site() { return runtime::find(S_SORT, "strategy", "strategy"); }

const guards::MidHookSpec spec{
    .name = "[strategy] teammate pairs guard",
    .env = "SWOFTY_TEAMMATE_PAIRS_GUARD",
    .site = site,
    .destination = sort_site,
    .hook = h_sort,
    .armed = "a team with more than two cars pairs its first two for strategy instead of throwing",
};

}

SWOFTY_MID_HOOK_GUARD(teammate_pairs, "teammate pairs guard", spec)
```

`site` returns the address to hook, or 0 after logging why there's none. `install_mid_hook` logs the off line, the
`<name>: the hook failed: <error>` line (`<name>: <failed_at>: the hook failed: <error>` when you set `failed_at`) and
the armed line. Set `off_value` when the guard's variable takes other values than `0` and `off`.

A hook guard that installs several hooks, or has failure lines of its own, writes its setup function and registers it
as a `HookGuard` with `SWOFTY_HOOK_GUARD(id, what, setup)` instead (`guards/career/rnd_suppliers_guard.cpp`).

Both macros add one SDK on-load entry that arms the next hook guard in `HookGuardId` order (`guards/order.hpp`). The
k-th entry the SDK runs arms the k-th hook guard. On-load entries run in reverse link order: lld lays the DLL's static
constructors out in link order (the sources `native/tools/core_link_order.txt` lists, in its order, then the rest
sorted by path), the mingw crt calls them from the last, and the SDK lists each on-load entry as it's constructed. A
new hook guard takes the `HookGuardId` slot that matches where its source falls in that reverse order, and its name
goes in `HOOKED` in `native/test/guard_registry_test.cpp` at the same slot. Pin a `MidHookSpec` guard's off,
failed and armed lines in `added.txt` under `[hook <id>]`.

Every guard source's `swofty::log` formats are pinned in `native/test/guard_lines/logged.txt`, in source order under
`[log <path>]`, and `guard_lines` fails when a source logs other formats or a source that logs has no section. A change
to a guard's log text changes its section in the same commit.

Both macros also name the guard's `CoreRank` (`native/src/core/runtime/loader/core_rank.hpp`) under its `HookGuardId`
name, so a new hook guard adds that name to `CoreRank` too, where its on-load entry runs among the other registering
units (the `explicit` section of `native/tools/core_init_order.txt`); the core build fails on a missing name, and
`tools/init_check.py` fails it when the ranks give another order than the run order.

## Guard

Subclass `Guard` and write `build()` and `settle()` yourself when the lines or the offer don't fit a spec. Each
`Guard` in the tree holds a shape the specs don't:

- `myteam_vehicle_heal` arms for the career load alone when its session site doesn't install, and keeps a status
  string that it reports later.
- `myteam_vehicle_keep` reads two test variables, names a consequence in its off and needs lines, and records at
  settle whether it installed.
- `heap_sizes` has one variable and one size choice per heap, offers only the sites the raised heaps need, and runs
  only at the early load stage.
- `gp_continue` arms with either or both of its two sites and says which.
- `leaderboard` offers four sets under three variables, and its tab set arms with either of two activations.
- `pit_box` offers two sets under two variables, and the tyre rule's needs line reads "this build does not resolve X".
- `quali_tyres` waits until loaded content has qualifying tyres (`ready()`), offers each of three sets that resolves,
  and settles them in one combined line.
- `track_list` offers its count-lines set outside its variable when either of two sites resolves, and logs that set's
  settle line only when it doesn't install.
- `track_select` names only the first missing binding in its needs line and records at settle whether it installed,
  which its handler reads.
- `theatre_list` settles its three sets in one line ("recent list armed, saved list armed, rows armed").
- `tile_list_select` reads "armed;" rather than "armed:" in its settle lines.

`settle()` asks `offered_set(set)` whether `build()` offered a set rather than keeping a flag of its own; a flag the
handler reads at runtime, such as `track_select`'s installed one, stays in the guard.

`ready()` holds back the offer until it returns true. The core offers every set once its code is ready, in `CoreSetId`
order, and settles each offered set once, in `COMMIT_ORDER` (`runtime/loader/core_set_order.hpp`).

## OnDemandGuard

Use an `OnDemandGuard` when a feature installs the guard's hook the first time it needs it. `arm()` runs `install()`
once, from whichever thread calls first, and every caller gets its result. The dropped scores guard
(`guards/career/standings_drop.cpp`) arms when an addon first sets a rule:

```cpp
class DropGuard final : public guards::OnDemandGuard {
protected:
    bool install() override
    {
        // env_off, runtime::find, safetyhook::InlineHook::create, then the armed line
        return true;
    }
};

constinit DropGuard guard;

int standings_drop_set(const std::string &data_group, uint32_t best)
{
    if (best && !guard.arm())
        return SWOFTY_ASSET_LATE;
    // ...
}
```

## Add a guard

A new `SiteGuard` is its spec and its handler, plus its id in two files and its lines in a third.

1. Give it a core set. In `native/src/core/runtime/loader/core_set_order.hpp`, add the id to `CoreSetId` and its name
   to `CORE_SET_NAMES` at the same position, and add the id to `COMMIT_ORDER` where it should settle. Add the name in
   the same places in `OFFERED` and `SETTLED` in `native/test/guard_registry_test.cpp`.
2. Write the guard. The ERS mode declaration above is the template: name the set, the log prefix, the variable, the
   bindings and the mixins.
3. Pin its lines in `native/test/guard_lines/added.txt`:

   ```text
   [my_guard]
   off [area] my guard off (SWOFTY_MY_GUARD=%s)
   unresolved [area] my guard off: needs site_Area_Thing, which this build does not resolve
   armed [area] my guard armed: what it does
   not_installed [area] my guard did not install (see the [core] lines)
   ```

4. Put the handler's decisions in `my_guard_logic.hpp` and test them in a host unit
   (`native/test/<name>_test.cpp`, listed in `native/test/manifest.py`).

The shared pieces a handler uses:

```cpp
guards::LogFirst<4> skipped{"skips"};               // the first 4 notes get a line; the 4th ends with the tail
guards::FirstKeys<uint32_t, 8> rounds{"rounds"};    // the first note of each of the first 8 keys gets a line
guards::FirstBytes seen;                            // first(id) is true once per one-byte id; lock around it

if (auto line = skipped.note())   // line.tail is "" or " (further skips are not logged)"
    swofty::log("[area] skipped an entry with no record%s", line.tail);

guards::read_if_readable(address, &out, sizeof out, nullptr);       // the core's page table, then a copy
guards::read_catching_faults(address, &out, sizeof out, nullptr);   // a copy that catches an access violation
```

`read_if_readable`, `write_if_readable` and `read_catching_faults` (`guards/memory.hpp`) have the signature the logic
headers take for memory access. `write_if_readable` asks whether the bytes are readable, not whether they're writable:
a readable page that isn't writable faults in its copy. Its one caller, `ai_line_set`'s repair, writes into the game's
AI build records on its heap. Code that may write anywhere else asks `runtime::writable_in_region` instead.

## What the tests guarantee

The guard tests run on the host (`native/test/run_tests.sh guard_registry guard_lines guard_skips guard_on_demand`) and
read the guard sources. They don't load the game.

- `guard_registry`: every core set registers exactly once, under its id, somewhere under `native/src/core`. The offer
  order and the commit order match 1.9.63's. Every hook guard registers once, and the hook guards' on-load entries
  run (in reverse link order) in `HookGuardId` order, which is the order 1.9.63 armed them in.
- `guard_lines`: every `SiteGuard` spec (and each of its parts) and every `MidHookSpec` renders, through
  `guards/lines.hpp`, exactly the lines pinned for it. `expected.txt` holds the lines the guards logged before they
  became specs, written by `guard_lines/extract.py` from the older sources. `added.txt` holds the lines of guards
  written as specs from the start. A spec with no pinned lines fails, and so does a guard pinned in both files.
- `guard_skips`: `LogFirst` logs its first N occurrences with the tail on the last; `FirstKeys` logs each of its first
  N keys once, across threads; `FirstBytes` reports each byte once with no cap; `switch_off` takes `0` and `off` only.
- `guard_on_demand`: `arm()` from 16 threads runs `install()` once and every caller sees its result.
- Each guard's own `<name>_test` checks its logic header: which entries it skips, which row it picks, which lines it
  logs once.

What they don't cover: whether a site still matches the running game build. The core checks that at load, through the
bindings and `runtime::find`, and the guard logs its needs or not-hooked line.

## Sites checked and left unguarded

- `Sess_VehiclePlacement_PlaceOnSlot` (0x142f343d0, callers: census steam 0x2f343d0) bounds the slot (unsigned, at
  most 31) but not the type, which indexes the [9][32] request table at placement + 0x38. Every game call passes a
  constant type: 3, 5, 6 or 7 (0x142a3c7b2 picks 5 or 6). The core never calls it. The box grid's wrap
  (`core/boxes/box_grid.cpp`) passes the game's type through and acts on type 5 only. The launcher's world extension
  (`packages/core/src/games/f1_25/activation/grid.rs`, `extend`) clones a world's own GridSlot, PitGarage, F1Garage and PitBox
  entries and sets only their id, so each added request keeps the type of the entry it copies. No std library code
  reaches the placement system.
- `Sess_VehiclePlacementRequest_Register` (0x142f60c80) stores the request at table[type][slot] from its +0x1c and
  +0x18 (slot = `VehPlacement.Id - 1`) with no bound. The ids the world extension adds stop at the Time Trial grid's
  entrant count (`entrants.entrant_lists.entrant_list_34`, 26 in the game data) for GridSlot and PitGarage, and at
  half of it for F1Garage and PitBox. `extend` refuses a count that needs an id past 32 and writes no entry
  (`refuses_more_cars_than_the_game_has_placement_slots`). The build refuses an addon's `patch-pkg` edit that sets
  `overrides.VehPlacement.Id` or `overrides.F1Garage.id` outside 1 to 32 (`PkgEdit::refusal`), with a warning that
  names the addon, the op file and the entry, and skips that op.
