# One entrant, another car, for one event (static)

vmknolle (engine-re round), 2026-11-20. Build: Steam d873a1c8 (the KB build). Everything here is STATIC: own Ghidra
decomps of the functions named below, capstone disassembly and rel32 caller scans of `work/exe/F1_25.exe`, and the
existing KB records. Nothing is runtime-confirmed. Labels: **CONFIRMED-static** = read in the code;
**HYPOTHESIS** = inferred, needs the T-request named.

Question from the season add-ons: give ONE entrant (team / driver row) a different vehicle (car model, vehicle
package) for one event only, natively, before the car assets load. Example: 1978 Brabham BT46B only at Anderstorp,
the BT46 at every other round.

## Verdict

- **Data only: no.** Which car an entrant drives is the entrant row's `vehicle` label, parsed at boot into the
  0x88 `EntrantRecord` (+0x5c). Swofty writes those rows from `content.package` teams at boot (`Applies::boot`).
  Event selectors (`.when(track/session_type/year)`) choose when a *handler* runs, and `.in(vehicle_package(...))`
  selects the cars of a package for a handler; neither changes which vehicle an entrant gets. A layer with `.in()`
  on a boot registry (`vehicle`, `content.package`) is refused (layers.md). See [data](#2-what-swofty-data-can-say-today).
- **Data-only workaround (GP single race only):** a variant package (its own box, e.g. "F1 1978 - Anderstorp")
  whose Brabham team drives a `brabham_bt46b` vehicle variant. No championship or career round can use it.
- **Native: small, one session applier on an existing hook.** The car is resolved again at **every session load**
  from the current championship session's own copy of the entrant records. Writing the entrant's vehicle ref
  (+0x5c, and the livery key +0x64) in that copy before the descriptor fill swaps the car natively, before any car
  asset is requested. The hook point is the entry of `Game_ChampionshipGameMode_BuildSessionSettings` (0x1407b7b90),
  which the core already inline-hooks (`direct_start.cpp`, race laps). See [route](#3-the-native-route).

## 1. Where the game picks the car per entrant

Load order, one session load (Grand Prix and career both run the championship mode):

| # | step | what it does with the car | evidence | level |
|---|---|---|---|---|
| 1 | boot: `Sess_Load_AvailableEntrantLists` -> `Sess_Parse_EntrantList` 0x142453c40 -> `Sess_Parse_Entrant` 0x142452e60 | each `entrants.entrant_lists.<list>.entrant_<i>` row -> one 0x88 `EntrantRecord`; `vehicle` label -> stat id packed at **+0x5c** (`Ent_Pack_VehicleRef`, store 0x142453717), `vehicle_customisation` -> **+0x64**, team stat id +0x58/+0x5a | KB L2 + own decomp (store `*(param_1+0x5c)` line 323) | CONFIRMED-static |
| 2 | career creation | career rows (CareerEntity +0x2e0..+0x2e8, 0x88 stride) are seeded from the list and saved | KB (`Career_Rebuild_TeamUpgradesFromRows`, `Sess_Weekend_Start` notes) | CONFIRMED-static (KB) |
| 3 | each session: `Sess_Weekend_Start` 0x140af0460 | copies the entrants (career rows with a car, or the GP list) into the **current** session's vector: weekend +0x220 + idx(+0x238)*0xa0, vector at Session +0x18 {alloc, begin +0x20, end +0x28}, via `Sess_ParticipantVec_PushBack` (field copy of the 0x88 record, +0x5c included). A session with +0x88 clear takes the previous session's classified entrants | own decomp (index +0x238, stride 0xa0, call 0x140af2ab0) | CONFIRMED-static |
| 4 | `Sess_Loader_SetupTrackAssets` 0x142d48b70 -> `Sess_Loader_FillSessionDescriptor` 0x1407d11a0 (call 0x142d48bb2) | fills the session descriptor (game manager +0x1a8): walks the mode's components (0x14290fb80) and calls slot 0x80 | own decomp; new L1 record | CONFIRMED-static |
| 5 | championship slot 0x80 = `Game_ChampionshipGameMode_BuildSessionSettings` 0x1407b7b90 | collects pointers to the current Session's records (+0x20..+0x28), then per entrant: `Sess_ResolveEntityHandle(entrants, rec+0x5c)` (**site 0x1407b86ac**, null = entrant skipped), `Sess_Build_SessionParticipant(part, rec, ...)` (0x1407b882f), push into descriptor **+0xad0** (0x1407b8844) | own decomp + disasm; new L1 records | CONFIRMED-static |
| 6 | `Sess_Build_SessionParticipant` 0x142e0c700 (-> .data1 0x14f2dcbd0) | re-reads rec+0x5c (vehicle), rec+0x64 (livery preset through `Ent_Get_VehicleCustomisation_Thunk`, skipped when u16 +0x66 = 0x3fff), rec+0x60, rec+0x80/+0x81; builds the 0x6780-byte participant with 0x142e0a530 | own decomp | CONFIRMED-static |
| 7 | `Sess_Loader_StartFrontendAssetLoad` 0x142d47210 -> `Game_VehicleAssetManager_ChangeResidentAssets` 0x140818e30 (call 0x142d47ce8, after step 4 at line 99 of the same function) | loads the asset groups named in each participant (+0x5c58, +0x5fa0, +0x60b8 / +0x62e8 by asset type) of descriptor +0xad0 | own decomp | CONFIRMED-static |
| 8 | session enter: `Sess_Build_SessionVehicleClassLists` 0x14098f400 | walks the session entrants (holder +0x18 vector) and resolves +0x5c again (`site_Sess_SessionEnter_VehicleLookup`), then an unchecked lookup in 0x142458710 | KB L2 | CONFIRMED-static (KB) |

**Answer to Q1.** The table read is `entrants.entrant_lists` (row `vehicle` -> `entrants.vehicles` stat id), stored as
`EntrantRecord.vehicle_ref` (+0x5c). The vehicle row then gives package, model and asset groups (0x142e0a530,
`Sess_Loader_CollectVehicleAssetGroups` / `Ai_CarProfile_ResolveNames` read `asset_data` / `base_car`). The *source*
is read once (boot list, or the career rows), but the car is **resolved per session load** from the session's own
copy (steps 3-6), and again at session enter (step 8). CONFIRMED-static.

Side facts:

- `Sess_Match_EntrantIdentity_Thunk` (0x1431146d0, the compare BuildSessionSettings, the weekend's unique list and the
  career use) compares +0x48 and the name block, **not** +0x5c: a swapped car keeps the entrant's identity, results,
  standings and team. CONFIRMED-static.
- The loader keep checks (0x142d48040, 0x1436f8340, 0x143575170) fill a *temporary* descriptor with the same
  function and compare it with the live one. A swap must therefore be deterministic per session (same answer for
  every fill), or a session could reload its cars. HYPOTHESIS that a vehicle change between two sessions of one
  weekend forces a car reload rather than a crash (T334).
- Time Trial uses its own fill (`Sess_TimeTrialMode_FillDescriptor`), which draws distinct vehicle refs of whole
  lists; out of scope.

## 2. What Swofty data can say today

| tool | does it swap a car for one event? | why |
|---|---|---|
| `content.package` `teams[].vehicle` | no (whole box, every round) | `Applies::boot`, written into the entrant list rows once |
| `cars::vehicles` layer `.in(...)` | refused | boot registry (layers.md: "a layer with .in() on a boot registry is refused") |
| `events::on<E>().when(track(...), year(...))` | no data path | scopes a handler; no event payload exposes the session entrants' vehicle ref |
| `.in(vehicle_package(...))` | no | per-car handler filter, selects cars that already drive that package |
| `career::seasons` layer | no | season data; the round's entrant rows are not per-round |
| variant package + own box | **yes, GP only** | the box's list has the BT46B; not usable inside one championship or career |

Missing: a **session-scope** record that names (package, team, seats, vehicle, livery, when) and a core applier that
writes it into the current session's entrant copies before the descriptor fill.

## 3. The native route

**Smallest safe point: the entry of `Game_ChampionshipGameMode_BuildSessionSettings` (0x1407b7b90)**, before
`site_Sess_ChampFill_VehicleLookup` (0x1407b86ac).

- It runs per session load, inside `Sess_Loader_SetupTrackAssets`, before `ChangeResidentAssets` (asset streaming)
  and before any car is spawned. CONFIRMED-static.
- The core already hooks it inline (`native/src/core/testing/direct_start.cpp`, `h_settings`, signature
  `game.championship_game_mode_build_session_settings`, same 1358 instructions on Steam 26566308 and EA 1.0.145 per
  that file). A second user should share one detour (a small dispatcher), not install a second inline hook.
- The current weekend comes from the core's existing reader (`apply/moments.cpp` `game::weekend()`: `out.at`, track
  stat id, session id); the session = weekend +0x220 + (+0x238)*0xa0; its records = Session +0x20..+0x28 (0x88).

What it must swap, per matching record (team stat id u16 +0x5a = the team's row; driver u16 +0x52 when seats are named):

| field | write | why |
|---|---|---|
| +0x5c vehicle ref | `(stat_id << 16) | kind 0` of the override vehicle (a boot-registered `entrants.vehicles` row) | the car, its package, model, asset groups, AI asset data and VTF |
| +0x64 vehicle customisation | the override livery's key, or 0x3fff0000 (none: the vehicle's own livery) | the BT46 preset may not fit the BT46B model (step 6 resolves +0x64 independently of +0x5c) |

Keep the write in the session copy only (never the career rows +0x2e0 or the boot list), write the same values on
every fill of that session (determinism, keep checks), and put the original values back at `session.end` /
`session.unload` and before `career.saving`, so a save never holds the override ref. A pre-detour only; the
original function then runs unchanged.

Hook-free alternative (HYPOTHESIS, T333): the core's `session_load` moment fires at `StateLoadSession` Entering; if
`Sess_Weekend_Start` has filled the current session by then and the descriptor fill runs later (the session trace
suggests the fill happens during Updating), an `Applies::session` applier can do the same write without touching
BuildSessionSettings. The detour is the safer default because the fill always passes through it.

### Risks

| risk | what happens | mitigation | level |
|---|---|---|---|
| career save mid-weekend / mid-session save | session copies are part of the weekend; a save taken while swapped stores the BT46B ref; a later load without the add-on gives `ResolveEntityHandle` = 0: BuildSessionSettings skips the entrant (missing car), and the session-enter class build has an unchecked lookup (0x142458792) = possible crash | restore before `career.saving` and at session end; career stat-id map already guards ids | HYPOTHESIS (T335) |
| livery / garage per-box archive | +0x64 preset of the BT46 entry applied to the BT46B model; `team.garage` art is listed per car asset group, so the BT46B group must list the box's garage archive too | override sets its own livery or 0x3fff0000; the BT46B vehicle shares the box's garage record | CONFIRMED-static (+0x64 path), HYPOTHESIS (garage) |
| AI performance | AI data follows the participant's vehicle (`ai.asset_data.<vehicle>`, `Ai_CarProfile_ResolveNames` from the participant): the BT46B needs its own AI slot (`cars::Vehicle.ai`); career R&D upgrades are keyed by team + the career row's vehicle (BT46) | give the BT46B vehicle an AI slot; R&D unchanged (team key) | CONFIRMED-static (AI path), HYPOTHESIS (R&D fit on a swapped car) |
| replay / flashback | in-session state uses the live participant; saved replays may store the session descriptor | none needed within a session | HYPOTHESIS |
| online | network entrants (kind 2) are replicated; a local-only swap desyncs peers | applier off in online modes (`mode` scope) | HYPOTHESIS |
| HUD team names | team stat id +0x58 is not touched, so team name, colours, badge and standings stay Brabham | none | CONFIRMED-static |
| between sessions | P/Q/R of one weekend get the same answer if `when` has no `session_types`; a swap for the race only changes the car between sessions, which the loader keep check sees as a different descriptor | prefer whole-weekend scope; test the session-only case | HYPOTHESIS (T334) |
| entrant without a stock vehicle row | a vehicle ref whose stat id has no row resolves to null: entrant dropped | the core refuses an override whose vehicle is not a registered boot vehicle | CONFIRMED-static (null check at 0x1407b86b1) |

## 4. Proposed data form

One new session registry next to the box appliers (`drs_for_box`, `weekend_for_box`, ...), keyed like a package
team, using the event selectors for `when`:

```cpp
namespace swofty::content {
struct VehicleOverride {                           // one entrant's car for some events (session.vehicle_override)
    Ref<Package> package;                          // the box / career list it applies in
    std::string team;                              // the package's team key: "brabham"
    std::vector<Ref<drivers::Identity>> drivers;   // seats it applies to; empty = every seat of the team
    Ref<cars::Vehicle> vehicle;                    // a boot-registered vehicle: "swofty.f1-1978/brabham_bt46b"
    Ref<liveries::Livery> livery;                  // empty: the vehicle's own livery (+0x64 = none)
    events::Selector when;                         // track(), year(), session_type(); all must match
};
inline Registry<VehicleOverride> vehicle_overrides{"team.vehicle_override", { .applies = Applies::session }};
}
```

```json
"team.vehicle_override": {
  "brabham_bt46b_anderstorp": {
    "package": "swofty.f1-1978/f1_1978",
    "team": "brabham",
    "drivers": [],
    "vehicle": "swofty.f1-1978/brabham_bt46b",
    "livery": "",
    "when": { "track": "<provider>/anderstorp", "year": 1978, "session_types": [] }
  }
}
```

Rules: the vehicle must be a `cars::Vehicle` registered at boot (it gets its stat id, package and asset groups then,
even if no list uses it); `when.session_types` empty = the whole weekend (recommended); two overrides on one seat =
load-order winner, logged; refused in online modes and Time Trial.

Agreed with the season car builders (2026-11-20, offline only):
- Vehicle keys follow the season build schema: package and vehicle share the key `f<yy>_<row>`, e.g.
  `vmknolle.season-1978/f78_<brabham-row>_bt46b`; the vehicle brings its own `ai.asset_filename`, the package its
  own `resource_path` / `ai_package_folder`. The BT46B needs its own AI/handling data (fan grip, no rear wing).
- Single-driver swaps (1969 Lotus 63 / McLaren M9A / Matra MS84, 1981 Lotus 88) use `drivers` with exactly one entry.
- A THIRD car for a team (extra entrant) is out of scope here: the override only changes an existing seat's
  vehicle. An extra entrant per event needs its own entrant/list route (open RE topic).

## T-requests

T333 (moment order), T334 (BT46B at Anderstorp, GP), T335 (career save and restore). See `re/tests/`.
