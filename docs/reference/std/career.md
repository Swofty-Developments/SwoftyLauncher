---
title: Career
description: Track F1 25 front-end flow states and the running career, reach its components through the game's registry and post on the game bus with swofty/career.hpp.
---

# Career

`swofty/career.hpp` tells your plugin which front-end flow state the game is in, whether a career is loaded, in which mode and whether Team HQ is idle; it also reaches the career's components, posts on the game's message bus and keeps the player out of a career mode on request.

```cpp [native/src/my_plugin.cpp]
#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/mixin.hpp>

namespace career = swofty::career;

// career.hub: a = 1 entered, 0 left; data is the career state, valid only during the callback
static void on_hub(const SwoftyEvent *ev, void *)
{
    auto *s = static_cast<const SwoftyCareerState *>(ev->data);
    if (!ev->a)
        return;
    swofty::log("[my-addon] Team HQ idle in %s, career %s", career::name(career::mode(*s)), s->key);
    // component() and post() are game calls: an events callback runs on the flow thread
    if (void *bank = career::component("CareerBankComponent"))
        swofty::log("[my-addon] bank at %p", bank);
}

// career.starting: data->blocked is 1 when a block cancelled the route
static void on_starting(const SwoftyEvent *ev, void *)
{
    auto *s = static_cast<const SwoftyCareerStart *>(ev->data);
    if (s && s->mode == SWOFTY_CAREER_MY_TEAM)
        swofty::log("[my-addon] My Team career start (route '%s') %s", s->route,
                    s->blocked ? "cancelled by the core; Driver Career only" : "NOT cancelled");
}

SWOFTY_ON_LOAD
{
    if (!career::available())                        // the flow-state site did not resolve on this build
        return;
    swofty::events::subscribe(SWOFTY_EVENT_CAREER_HUB, on_hub);
    swofty::events::subscribe(SWOFTY_EVENT_CAREER_STARTING, on_starting);

    career::State s{};                               // a late subscriber asks instead of waiting
    if (career::state(&s) == career::Result::ok && s.hub)
        swofty::log("[my-addon] already in Team HQ");

    // keep the player out of My Team for this session (API 7)
    career::Result r = career::block(career::Mode::my_team, "My Team not available",
                                     "This addon works in Driver Career only.");
    if (r != career::Result::ok)
        swofty::log("[my-addon] My Team is not blocked: %s", career::name(r));
}

SWOFTY_ADDON()
```

```json [swofty.json]
{
  "schema_version": 1,
  "id": "author.my-addon",
  "name": "My Addon",
  "version": "0.1.0",
  "author": "Your Name",
  "kind": ["native"],
  "compat": { "game": "f1_25" },
  "requires": [{ "id": "swofty.core", "version": ">=0.2.12" }],
  "operations": { "native": [{ "dll": "native/my_plugin.dll" }] }
}
```

`swofty/loader.log`, from the core's setup to a blocked My Team pick and a Driver Career load:

```text
[career] setup: live; flow states armed, Team HQ answer armed, career layout yes, entrants yes, bus yes, string key bound
[career] start gate: live on Vm_Run_Route (base+0x4188370), 0 block(s) held
[career] author.my-addon blocks my_team careers; the start gate is live
[career] career.starting: my_team (pick, route 'career_dlc_select_my_team'); blocked by author.my-addon
[career] route 'career_dlc_select_my_team' cancelled; the player stays on the menu and author.my-addon's popup is queued
[my-addon] My Team career start (route 'career_dlc_select_my_team') cancelled by the core; Driver Career only
[career] career.starting: driver_career (continue, route 'solo_career_continue_driver')
[career] Team HQ entered: the game reports not My Team (Career_Is_MyTeamGameMode)
[career] career.loaded: mode driver_career, key 'dc-00090000' (components live, component mode driver_career, Team HQ answer none)
[career] career.hub entered (mode driver_career)
[my-addon] Team HQ idle in driver_career, career dc-00090000
```

- **Header:** `swofty/career.hpp`, namespace `swofty::career`.
- **Table:** `SwoftyApi::career` (`SwoftyCareer`), API version 6 (`swofty_core.dll` 0.2.11); `block`, `unblock` and the `career.starting` event API version 7 (core 0.2.12).
- **Feature:** `career`, stable, always on.
- **Thread:** the getters, `block` and `unblock` from any thread. `entity`, `component`, `component_by_desc` and `post` call into the game: only on the flow thread (an [events](./events) or `on_frame` callback), never from a thread of your own.

Every call that returns a `swofty::career::Result` uses the shared [result codes](./#result-codes). The same facts arrive as [flow and career events](./events#flow-and-career-events); the getters answer an addon that subscribes late. To add a career of your own, with its own tile and save slots, or to tell which career is loaded by its data group, see [Career contents](./career-content).

## available

```cpp
if (!swofty::career::available())   // false: the core cannot track the flow states on this build
    return;
```

`available()` is `true` when the core tracks the flow states on this build. It is settled before any `SWOFTY_ON_LOAD` runs. In `SwoftyAddonInit` (the check you pass to `SWOFTY_ADDON`) it is `true` when this core can track them, and the core's career setup settles it.

## The career state

```cpp
swofty::career::State s{};                          // SwoftyCareerState
if (swofty::career::state(&s) == swofty::career::Result::ok && s.loaded)
    swofty::log("[my-addon] %s career %s, %u human(s)", swofty::career::name(swofty::career::mode(s)), s.key,
                s.humans);
```

`state(out)` returns `ok` with the current state, also when no career is loaded (`loaded` is 0 then).

```c
typedef struct SwoftyCareerState {
    uint32_t loaded;          // a career is loaded
    uint32_t mode;            // SWOFTY_CAREER_NONE, _MY_TEAM (1), _DRIVER (2)
    uint32_t hub;             // Team HQ is idle for the mode
    uint32_t humans;          // human entrants in the championship
    uint32_t player_driver;   // the human entrant's packed driver key, when there is one human
    uint32_t player_team;     // and its packed team key
    uint32_t has_player;
    uint32_t components;      // SWOFTY_CAREER_COMPONENTS_UNSEEN, _LIVE, _FALLBACK
    uint32_t component_mode;  // what the components say
    uint32_t team_hq_mode;    // what Team HQ answered when it opened
    uint32_t flow_career;     // the flow states alone say a career is up
    uint32_t reserved;
    char key[32];             // "mt-<team>-<driver>" or "dc-<driver>", "" until the player is known
} SwoftyCareerState;
```

How the core decides the mode:

- **Components first.** A `CareerComponent` means a loaded career. `CareerBankComponent` or `MyTeamFacilitiesComponent` means My Team. `DriverCareerPlayerDriverStatisticsComponent` means Driver Career.
- **Then Team HQ.** When the components have no answer, the core uses what Team HQ answered as it opened (`Career_Is_MyTeamGameMode`, read at `site_Career_TeamHqIsMyTeamResult`). Both career modes use the same `StateTeamHQ` and `StateTeamHQUpdate` states.
- **Then the flow states.** If Team HQ has been up for 4 seconds and the components still show no career, `components` turns to `_FALLBACK` and the flow states decide whether a career is loaded.
- **Polling.** The core reads the components every 250 ms for 8 seconds after a flow-state change and every 30 seconds otherwise. In a session it reads them only after the session states change, not after pause screens.

```text
[career] Team HQ entered: the game reports My Team (Career_Is_MyTeamGameMode)
[career] career.loaded: mode my_team, key 'mt-00000001-00c50000' (components live, component mode my_team, Team HQ answer none)
[career] career.hub entered (mode my_team)
[career] career.hub left (mode driver_career)
[career] career.unloaded (mode driver_career)
```

`career.loaded` fires once the mode is known. A change of mode or career key while a career is loaded fires `career.unloaded` and then `career.loaded`.

## Flow states

```cpp
namespace career = swofty::career;

career::Flow last{};                                // SwoftyFlowState
if (career::flow(&last) == career::Result::ok)      // late before the first transition
    swofty::log("[my-addon] %s %s -> %s", last.name, career::name(career::Status(last.from)),
                career::name(career::Status(last.to)));

career::Status st;
uint64_t age_ms = 0;
// by name or by hash; invalid (st is none) for a state the core has not seen yet
if (career::flow_status("StateTeamHQUpdate", &st, &age_ms) == career::Result::ok)
    swofty::log("[my-addon] Team HQ %s for %llu ms", career::name(st), (unsigned long long)age_ms);

constexpr uint32_t hq = career::state_hash("StateTeamHQUpdate");   // djb2, the hash the game keys states by
```

- `flow(out)`: `ok` with the last flow transition; `late` before the first.
- `flow_status(hash or name, status, age_ms = nullptr)`: `ok` with a state's status and how long it has held it; `invalid` for a state the core has not seen.
- `state_hash(name)`: the djb2 hash the game keys flow states by (`constexpr`).

```c
typedef struct SwoftyFlowState {
    uint32_t hash;          // djb2 of the state name
    uint32_t from, to;      // SWOFTY_FLOW_NONE, _ENTERING, _UPDATING, _EXITING
    uint32_t named;         // name is the real state name
    uint64_t transitions;   // transitions seen so far
    char name[64];          // "StateTeamHQUpdate", or "#0e245469" when unreadable
} SwoftyFlowState;
```

The core installs one read-only site, `site_FeFlow_SetStateStatus`, in the setter every flow-state change goes through. It tracks up to 256 distinct states. The game keeps state names XOR-encoded until it decodes them, so the core decodes a copy itself. A state it cannot name shows as `#` and the hash:

```text
[career] flow "StateTeamHQUpdate" None -> Entering
[career] flow "StateTeamHQUpdate" Updating -> Exiting
[career] flow "#0776f07b" None -> Entering (first sighting)
```

## Weekend diagnostics

Right after each `career.hub entered` line, the core logs the career's current weekend: the round, whether the weekend is not started, in progress or complete, the current session and how many of its sessions are complete. A weekend in progress with no session complete gets its own line, `weekend already started, session never ran`:

```text
[career] career.hub entered (mode driver_career)
[career] Team HQ weekend: round 4/17, weekend not started, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/silverstone
[career] career.hub entered (mode driver_career)
[career] weekend already started, session never ran: round 4/17, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/silverstone
```

After each of these lines the core logs the player's calendar items running that day, the item the game's calendar check picks and whether the items keep Team HQ's Advance off, as `Team HQ calendar`, in the same form as `hub opened` below. An end of season, race weekend, dynamic break or season break running that day keeps Advance off, in any state:

```text
[career] weekend already started, session never ran: round 4/17, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/silverstone
[career] Team HQ calendar: day 2045, season 0, 2 of 58 calendar items running today (driver 0x00020000, calendar 0x2a1f40e10); the calendar check reads: stay in the hub (item 1, RaceWeekend, decides); Team HQ's Advance is off (item 1, RaceWeekend, keeps it off)
[career]   item 1/2: RaceWeekend (0x0a) days 2045-2048 state 1 id 12 at 0x2a1f6c2a0 vt 0x1456d10d0
[career]   item 2/2: FreeTime (0x06) days 2030-2059 state 1 id 3 at 0x2a1f6c340 vt 0x1456d10d0
```

The launcher reads the `weekend already started, session never ran` line to learn [a save stuck in a started race weekend](/reference/saves#a-save-stuck-in-a-started-race-weekend).

A career session enters `SessionStateCareerHub` (`#42843edb`) right after `StateSession` and leaves it in the same frame. As the hub opens, the core logs the player's calendar items running that day and the item the game's calendar check picks. When `StateSession` ends within 2 seconds of entering after the hub opened, the core logs `session ended <N> ms after start`, with the weekend and the calendar items again, before `session.end`:

```text
[career] hub opened: day 2045, season 0, 2 of 58 calendar items running today (driver 0x00020000, calendar 0x2a1f40e10); the calendar check reads: stay in the hub (item 1, RaceWeekend, decides)
[career]   item 1/2: RaceWeekend (0x0a) days 2045-2048 state 1 id 12 at 0x2a1f6c2a0 vt 0x1456d10d0
[career]   item 2/2: FreeTime (0x06) days 2030-2059 state 1 id 3 at 0x2a1f6c340 vt 0x1456d10d0
[career] session ended 590 ms after start (StateSession exited; the career hub opened 545 ms after start): round 4/17, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/silverstone; day 2045, season 0, 2 of 58 calendar items running today (driver 0x00020000, calendar 0x2a1f40e10); the calendar check reads: stay in the hub (item 1, RaceWeekend, decides)
[career]   item 1/2: RaceWeekend (0x0a) days 2045-2048 state 1 id 12 at 0x2a1f6c2a0 vt 0x1456d10d0
[career]   item 2/2: FreeTime (0x06) days 2030-2059 state 1 id 3 at 0x2a1f6c340 vt 0x1456d10d0
[events] session.end (StateSession exited)
```

A calendar can start a race weekend inside the season break. The stock 2026 season's break, `season_break_2026`, runs 15 days from 27 July, and Historical Calendar 1.4.8 has a round on 30 July in 2016, 2017, 2019 to 2022, 2024 and 2025. The break starts first and is item 1: the calendar check sends the session to Team HQ as practice 1 opens, and the game crashes at `F1_25.exe+0x1f8be5c`. As the hub opens, the core sets every started season break listed ahead of a started race weekend running that day to ignored (state 4), the state the game gives a season break inside a dynamic break, and logs the calendar again:

```text
[career] hub opened: day 170, season 0, 2 of 29 calendar items running today (driver 0x00090000, calendar 0x345517370); the calendar check reads: go to Team HQ (item 1, SeasonBreak, decides)
[career]   item 1/2: SeasonBreak (0x0c) days 166-180 state 1 id 36 at 0x16ef13c0 vt 0x1456d31f0
[career]   item 2/2: RaceWeekend (0x0a) days 169-172 state 1 id 41 at 0x16ef13e0 vt 0x1456d31f0
[career] season break over race weekend: SeasonBreak (0x0c) days 166-180 state 1 id 36 at 0x16ef13c0 vt 0x1456d31f0 runs over the race weekend days 169-172 (id 41) that the session is in; set to ignored (state 4), so the race weekend decides
[career] hub calendar after the season break fix: day 170, season 0, 2 of 29 calendar items running today (driver 0x00090000, calendar 0x345517370); the calendar check reads: stay in the hub (item 2, RaceWeekend, decides)
[career]   item 1/2: SeasonBreak (0x0c) days 166-180 state 4 id 36 at 0x16ef13c0 vt 0x1456d31f0
[career]   item 2/2: RaceWeekend (0x0a) days 169-172 state 1 id 41 at 0x16ef13e0 vt 0x1456d31f0
```

The rest of the break doesn't run. A break with any other item the check weighs ahead of the weekend, or with no started weekend running that day, is left as it is. `SWOFTY_HUB_BREAK_FIX=0` turns the fix off, and the line then ends `left as it is (SWOFTY_HUB_BREAK_FIX=0)`.

The ignored break still keeps Team HQ's Advance off on the days after that weekend, and the game's own break flow only starts a break that has not started. At each Team HQ entry, when the only items keeping Advance off are ignored season breaks, the core moves each one back so it ends the day before and logs the calendar again:

```text
[career] Team HQ calendar: day 172, season 0, 1 of 31 calendar items running today (driver 0x00c90000, calendar 0x344755b18); the calendar check reads: go to Team HQ (no running activity decides); Team HQ's Advance is off (item 1, SeasonBreak, keeps it off)
[career]   item 1/1: SeasonBreak (0x0c) days 166-180 state 4 id 36 at 0x17121580 vt 0x1456d31f0
[career] ignored season break over Team HQ: SeasonBreak (0x0c) days 166-180 state 4 id 36 at 0x17121580 vt 0x1456d31f0 keeps Team HQ's Advance off on day 172 with no race weekend running; to days 157-171: moved back so it ends the day before
[career] Team HQ calendar after the season break move: day 172, season 0, 0 of 31 calendar items running today (driver 0x00c90000, calendar 0x344755b18); the calendar check reads: go to Team HQ (no running activity decides); Team HQ's Advance is on (nothing running today keeps it off)
```

A race weekend, dynamic break, end of season or a break in another state running that day leaves the break as it is, and so does `SWOFTY_HUB_BREAK_FIX=0`.

An add-on calendar can also start a second race weekend inside the first (F2 Revamp's 32 rounds). The first weekend finishes, but its calendar item stays started, the check keeps answering that the hub stays, and the session load finds the race already ran on every pass. At the first pass of that load loop, when the weekend that decides is complete in every session and a race weekend that has not started begins inside its days, the core sets the finished item to completed (state 2), the state the game's own finish writes, and logs the calendar again:

```text
[career] finished race weekend over the next one: RaceWeekend (0x0a) days 28-31 state 1 id 12 at 0x16ef13c0 vt 0x1456d31f0 is complete (round 2, 7 of 7 sessions) but still started while RaceWeekend (0x0a) days 29-32 state 0 id 14 at 0x16ef13e0 vt 0x1456d31f0 starts inside it; set to completed (state 2), so the hub moves on
[career] hub calendar after the finished weekend fix: day 29, season 0, 2 of 39 calendar items running today (driver 0x00090000, calendar 0x345517370); the calendar check reads: go to Team HQ (no running activity decides)
```

A stock calendar, a weekend that is not complete and an overlapping item that is not a race weekend are left as they are, and so is every item when `SWOFTY_HUB_OVERLAP_FIX=0`.

All of these come from the flow-state site and the Team HQ entry. Apart from the three calendar writes, they read the game without writing to it. The calendar reads use `ScheduleComponent` (`seasons_begin`, `seasons_end`, `current_season`, `season_over`), `ScheduleSeason`, `DriverCalendar`, `CalendarActivity` (`day`, `state`, `activity`), `ActivityDef` (`id`, `type`, `duration`) and `CareerTimeComponent.current_minutes` from the generated game headers; the core does not compile without one of them. The weekend reads need `vt_Career_Championship`, `vt_Career_Season` and `vt_Career_Weekend`; the calendar reads need the career components. A build without them logs `the current weekend is not readable` or `the calendar is not readable (<reason>)` in the same lines.

## Components

```cpp
// Progressive Career: the player's driver statistics, by descriptor (stats_mem.hpp)
void *p = swofty::career::component_by_desc(k_Career_DriverCareerPlayerDriverStatisticsComponentDesc.get());
if (!p)
    p = swofty::career::component_by_desc(k_Career_DriverStatisticsComponentDesc.get());

// by class name, or by the descriptor's binding id: the same lookup
void *bank = swofty::career::component("CareerBankComponent");
void *same = swofty::career::component("k_Career_CareerBankComponentDesc");

void *championship = swofty::career::entity();     // the championship entity, or nullptr
```

- `entity()`: the championship entity (the career entity in a career), or `nullptr`.
- `component(type)`: a component of the entity, or `nullptr`. `type` is the class name (`"CareerBankComponent"`) or the descriptor's binding id (`"k_Career_CareerBankComponentDesc"`). It is also `nullptr` when no career is loaded, the descriptor has no binding, or the entity lacks the component.
- `component_by_desc(desc)`: the same, by the address of the component's static type descriptor.

`component` resolves `k_Career_<Class>Desc` from the bindings, so it finds every component class the KB has a descriptor for. It calls the registry the game's own components use: `get(registry, &desc, 1)` at vtable slot `+0x10` of the registry embedded in `CareerEntity`.

## The message bus

```cpp
// Career Editor: a cash transaction on the live bus (economy.cpp), trimmed
BankTransactionMessage m{};
m.vtable = vt_Career_BankTransactionMessage.get();   // the message's vtable in the game image
// ... the rest of the fields, exactly as the game's own posters fill them ...
swofty::career::Result r = swofty::career::post(&m); // every subscriber has run when it returns
if (r == swofty::career::Result::late)
    swofty::log("[my-addon] not ready: message bus inactive");
```

`post(msg)` hands your message to `Ne_Post_BusMessage`, which runs every subscriber before it returns. It returns `ok` once every subscriber has handled the message, `late` when the bus is missing or inactive, and `invalid` for a message without a vtable in the game image. Build the message exactly as the game's own posters do.

## Blocking a career mode

```cpp
// R&D: Tyres & Thermal Management (rnd_myteam_block.cpp), trimmed
SWOFTY_ON_LOAD
{
    namespace career = swofty::career;
    career::Result r = career::block(career::Mode::my_team, "My Team not available",
                                     "R&D: Tyres & Thermal Management only works in Driver Career.");
    if (r != career::Result::ok)                     // unavailable on a core older than API 7
        swofty::log("[rnd-tyres] My Team is NOT blocked: %s", career::name(r));
}

// later, to let the player in again; invalid when this addon held no block on the mode
swofty::career::unblock(swofty::career::Mode::my_team);
```

- `block(mode, title = {}, body = {})`: `ok`, and from now on the core cancels starting or loading a career in `mode` and shows the [popup](./popup) with `title` and `body`. `invalid` for `Mode::none`; `unavailable` on a core older than API 7.
- `unblock(mode)`: `ok` once this addon's block on `mode` is lifted; `invalid` when it held none.

The Career menu starts and loads careers through fixed front-end routes. The core runs every route through a wrap on `Vm_Run_Route`, the game's route runner, and knows these:

```text
route                                  screen                                                     mode            Start
career_dlc_select_my_team              fe_career                                                  My Team         pick: the season or save choice comes next
career_dlc_select_driver               fe_career                                                  Driver Career   pick
career_wizard_my_team_career           fe_career, fe_career_dlc_select, career_continue_my_team   My Team         new_career
career_wizard_my_team_season_length    fe_career, fe_career_dlc_select, career_continue_my_team   My Team         new_career
career_wizard_driver_career            fe_career, fe_career_dlc_select, career_continue_driver    Driver Career   new_career
career_wizard_driver_career_to_f1      fe_career, fe_career_dlc_select, career_continue_driver    Driver Career   new_career
solo_career_continue_my_team           fe_career, fe_career_dlc_select                            My Team         continue_career
solo_career_continue_driver            fe_career, fe_career_dlc_select                            Driver Career   continue_career
to_manage_career_my_team               career_wizard_career_type_select                           My Team         continue_career
to_manage_career_driver                career_wizard_career_type_select                           Driver Career   continue_career
```

For each of them the core fires `career.starting` (see [events](./events#flow-and-career-events)) with a `SwoftyCareerStart`:

```c
typedef struct SwoftyCareerStart {
    uint32_t mode;      // SWOFTY_CAREER_MY_TEAM (1) or SWOFTY_CAREER_DRIVER (2)
    uint32_t kind;      // SWOFTY_CAREER_START_PICK (0), _NEW (1), _CONTINUE (2)
    uint32_t blocked;   // a block cancelled the route
    uint32_t reserved;
    char route[64];
} SwoftyCareerStart;
```

What a block does:

- When an addon holds a block on the route's mode, the core returns from the wrap without running the route. No step of the route has run, so the screen the player pressed on stays open and nothing of the chosen career loads.
- The core then queues the blocking addon's popup, at most once every 1.5 seconds per mode. A block with an empty title and body cancels without a popup. If the popup library is unavailable, the route is still cancelled and the core log says so.
- A block lasts for the session. Each addon holds at most one block per mode; calling `block` again replaces its text.
- When several addons block the same mode, the first one's popup shows.
- A block goes away with `unblock`, or when the addon's `SwoftyAddonInit` fails.

```text
[career] swofty.rnd-tyres-thermal blocks my_team careers; the start gate is live
[career] career.starting: my_team (pick, route 'career_dlc_select_my_team'); blocked by swofty.rnd-tyres-thermal
[career] route 'career_dlc_select_my_team' cancelled; the player stays on the menu and swofty.rnd-tyres-thermal's popup is queued
```

With `SWOFTY_UI_DIAG=1` the core also logs every other route it runs:

```text
[career] route 'open' (not a career start)
```

## Names

```cpp
namespace career = swofty::career;
career::name(career::Mode::my_team);      // "my_team"; also "driver_career", "none"
career::name(career::Status::updating);   // "Updating"; also "None", "Entering", "Exiting"
career::name(career::Start::pick);        // "pick"; also "new", "continue"
career::name(career::Result::late);       // "late"; also "ok", "unavailable", "invalid"
career::mode(s);                          // the Mode of a State s
```

## Availability

The library needs the flow-state site. On a build where it does not resolve, the core drops only that site, `available()` returns `false` and every call returns `unavailable` or `nullptr`; the rest of the core keeps working.

The Team HQ answer site, each component descriptor, `g_Game_Manager`, `g_Game_MessageBus` and `Ne_Post_BusMessage` are looked up one at a time when first needed, and a missing one fails only the calls that need it. A core built without the KB structs it reads (`FlowState`, `CareerEntity`, `EntrantRecord`, `GameMessageBusHolder`) reports the matching parts unavailable. The core log's `[career] setup:` line lists what is live:

```text
[career] setup: live; flow states armed, Team HQ answer armed, career layout yes, entrants yes, bus yes, string key missing (unknown state names stay hashes)
[career] setup: the flow-state site is not installed, so the career library reports unavailable; flow states off, Team HQ answer off, career layout yes, entrants yes, bus yes, string key missing (unknown state names stay hashes)
```

The start gate needs `Vm_Run_Route`, found by its signature (`vm.run_route`). On a build where it does not match, `career.starting` never fires and `block` still returns `ok`, but no route is cancelled; the core log's `[career] start gate:` line says so:

```text
[career] start gate: Vm_Run_Route signature matched 0 time(s)
[career] start gate: Vm_Run_Route did not resolve; career.starting never fires and blocks cannot cancel a route
```

Requires `swofty.core` 0.2.11 or later (0.2.12 for `block`, `unblock` and `career.starting`). Declare it in your manifest:

```json [swofty.json]
"requires": [{ "id": "swofty.core", "version": ">=0.2.11" }]
```
