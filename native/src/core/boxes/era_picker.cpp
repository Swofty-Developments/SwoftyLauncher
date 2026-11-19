#include "core/boxes/picker/picker.hpp"

#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/resolve.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

// Era picker addon row (swofty/era.hpp). The Grand Prix, Time Trial and split-screen category picker
// (the DynamicEraSelectTabViewModel, VM dynamic_era_select) shows its stock boxes in the VM tiles list;
// the launcher's UI build gives the screen a second row (scene slot addon_categories_row) whose spawner
// lists the VM addon_tiles list, selects into selected_addon_tile and raises addon_select_pressed.
// Addons register boxes with add_box; each names an entrant list of the host setup mode swofty_f1, which
// the core's own database rows add to every F1 mode and addons list-append their entrant lists onto.
//
// Build (site_FeTile_EraTilesBuilt, rsi = the view model, after the last stock box): the stock tiles stay
// in vm+0x690. The core keeps a second tile set laid out like it (era::Set) whose two handles hold the
// addon_tiles list (Vm_GetOrCreate_ListChild under the VM root) and the selected_addon_tile reference
// (Vm_Set_NodeRef). Each handle sits in its node's holder list and holds a reference. The Grand Prix and
// Time Trial each build their own picker view model, and before every build FeTile_Clear_EraTileSet_Thunk
// clears the set wherever its handles still hold nodes, another view model's too: it empties that
// addon_tiles list and unlinks and releases both handles. A handle overwritten while still linked would
// leave the old node's holder list running on into the new node's, so the game's teardown of the old view
// model would release the new nodes as well, and tearing the new picker down then made a pure call on its
// freed selected_addon_tile. A view model the game tears down unlinks and releases the handles itself.
// FeTile_Add_EraTile adds each box while vm+0x698 points at addon_tiles, so its VM node lands in the
// addon row and its Tile in the stock vector after the stock tiles, which the game frees with them.
// Boxes whose entrant list the screen's mode does not offer under swofty_f1 are skipped with one log line.
// On a Time Trial picker (the mode's name, *(*(mode+0x88)+0x278), is one the core reports as time_trial)
// a box with its own Time Trial entrant list gets that list as its tile's command, so its press selects
// and narrows to it; a box without one keeps its own list. Either way the box is skipped there, with one
// log line, when that list is not offered under swofty_f1 or holds more cars than a Time Trial session
// holds (era_logic.hpp); the car count is the list object's entrant records.
// The VM bool addon_tiles_visible says whether the addon row holds a box; the UI only moves Down from the
// stock row to the addon row while it is true.
//
// Press (FeTile_Press_EraTile, wrapped): addon_select_pressed runs the stock select_pressed path with
// vm+0x698, vm+0x6b8 and the vector start pointed at the addon row, so the pressed box raises
// entrant_list_selected with its entrant list exactly like a stock box. The stock handler does not
// bounds-check the child index against the vector, so the wrap refuses a press unless the vector still
// holds the stock and addon tiles of the last build of that view model.
//
// Narrowing (Game_F1WorldActivityContainer_SelectEntrantList, wrapped): every addon list shares the one
// host setup mode, whose entrant setup modes list all of them. Time Trial loads every car of every list
// the entrant setup mode offers and the Grand Prix takes its first list, so a press on an addon box
// leaves only that box's list in each swofty_f1 entrant setup mode until another box is pressed, and
// every press clears the setup's selected list, which the setup refill takes from the entrant setup
// mode's first list only while it is empty. A build of the boxes puts every list back for the
// availability checks, then narrows again to the addon list the setup has selected (setup +0xe18), or
// to the last addon box pressed while nothing is selected: a session can start from a rebuilt picker
// without a new press, and a Time Trial that loads all nine Classic Seasons lists overflows the fixed
// 43 MiB AUDIO_BUNDLE heap with their engine sound bundles.
//
// DRS per box: a narrowing also points every swofty_f1 game setup mode's DRS group (+0x1e0, a label holder) at the
// group the core added at boot for the box's rules.drs (content.package rules.drs). A box without a rule, a stock
// box press and a build that narrows to no addon list put the game's group back. The DRS system reads the current
// mode's group at session load, at a full safety car end and at a red flag restart; every other mode keeps its own.
// Points per box: the same narrowing points the race sessions of every swofty_f1 mode at the standings preset the
// boot apply added for the box's rules.points (logic::PointsSwaps); the same put backs restore the game's presets.
// Mandatory tyre change per box: the same narrowing writes the box's rules.mandatory_tyre_change into every
// swofty_f1 mode's 39 tyre regulation requirement flags (+0x8b) (logic::TyreChangeSwaps); the same put backs restore them.
// Tyre warmers per box: the same narrowing writes the box's rules.tyre_warmers into every swofty_f1 mode's
// has_tyre_warmers byte (+0x1c8) and the same put backs restore the game's value (logic::WarmerSwaps).
// Race time limit and weekend per box: the same narrowing writes the box's rules.race_time_limit_s into every swofty_f1
// mode's race_time_limit_in_seconds (+0xb4, logic::TimeLimitSwaps) and exchanges the weekend options (+0x190..+0x1a8)
// of the setup mode named by rules.weekend_mode with the first swofty_f1 mode (logic::WeekendSwaps); the same put
// backs undo both.
// Game options per box: the same narrowing sets the options store's safety car, formation lap, rules and penalties and
// corner cutting options to the box's rules.safety_car / formation_lap / penalties / corner_cutting (box_options.cpp)
// and the same put backs give the player's index back where an option still holds the box's.
// Grid rules per box: the same narrowing hands the box's rules.grid to box_grid.cpp, which re-ranks the weekend's
// qualifying classification; the same put backs clear it (the game's own grid).
// Qualifying tyres per box: the same narrowing hands the box's rules.qualifying_tyres option mask to the qualifying
// tyre hooks (quali_tyres_guard.cpp); the same put backs clear it.
//
// The three hooks are one core mixin set, offered at the first mixin commit after a box was registered,
// so a game without registered boxes runs none of them.
//
// SwoftyMod's split_season_picker off: the launcher's UI build leaves the screen with its single stock row,
// so the era picker is off from the start. available() says so and every box is refused as unavailable.
//
// Time Trial guard (era_logic.hpp): a stock box pressed on a Time Trial picker leaves swofty_f1 offering
// only its lists of at most 23 cars, none when no list fits; each Time Trial load counts the distinct cars of
// every list of the selected list's leaderboard that the Time Trial fill takes (era_logic.hpp) and leaves whole
// lists out, addon lists first, until they fit beside the game's 4 entries in the 26-slot table, and its unload
// puts them back; era_session_load (StateLoadSession, any mode) puts back what it held
// and, in Time Trial, logs the setup's resolved list and keeps a list past 23 cars out of the session;
// era_session_unloaded (StateUnloadSession Exiting) and every press put it back. The load's leaderboard cut is made on
// the swapped array, so each of them puts the cut back before the swap (logic::end_hold). The session vehicle guard is a
// separate set wrapping Sess_Setup_VehicleComponents, offered whether or not a box is registered.
//
// Setup mode guard: Sess_Apply_ContentAvailability rebuilds the game setup modes (it frees the old objects), so a
// set wrapping its entry drops the narrowing, DRS swaps and Time Trial hold the core keeps without writing them back.

namespace era_picker {

constinit Bound b{};
constinit Picker picker;
BoxRules box_rules;
VirtualLists virtual_lists;
constinit Filled filled;
constinit logic::GridLimit continue_grid;
constinit Hold hold;
constinit VehicleGuard vehicle_guard;
constinit TimeTrialGuard time_trial_guard;
constinit AliasGuard alias_guard;
constinit Unlisted unlisted;
constinit AvailabilityGuard availability_guard;
constinit SetupListGuard setup_list_guard;

constexpr size_t BOX_TIME_TRIAL = offsetof(SwoftyEraBox, time_trial_entrant_list);
constexpr size_t BOX_ONLY = offsetof(SwoftyEraBox, only);
static_assert(BOX_TIME_TRIAL == 0x28 && BOX_ONLY == 0x30);

constexpr const char *PARSE_LIST_FN = "Sess_Parse_EntrantList";
constexpr const char *PARSE_LIST_MIXIN = "swofty_core.dll (virtual entrant lists)";

constexpr const char *VEHICLE_MIXIN = "swofty_core.dll (session vehicle guard)";
constexpr const char *VEHICLE_FN = "Sess_Setup_VehicleComponents";

bool bind_all(std::string &why)
{
    std::string missing;
    b.site = runtime::resolve_or_note("site_FeTile_EraTilesBuilt", missing);
    b.press = runtime::resolve_or_note("FeTile_Press_EraTile", missing);
    b.select = runtime::resolve_or_note("Game_F1WorldActivityContainer_SelectEntrantList", missing);
    b.add = runtime::resolve_or_note("FeTile_Add_EraTile", missing);
    b.available = runtime::resolve_or_note("FeTile_Is_EraScenarioAvailable_Thunk", missing);
    b.list_child = runtime::resolve_or_note("Vm_GetOrCreate_ListChild", missing);
    b.node_ref = runtime::resolve_or_note("Vm_Set_NodeRef", missing);
    b.set_bool = runtime::resolve_or_note("Vm_Set_Bool", missing);
    b.clear = runtime::resolve_or_note("FeTile_Clear_EraTileSet_Thunk", missing);
    b.registry = runtime::resolve_or_note("g_Vm_Registry", missing);
    if (missing.empty())
        return true;
    why = "needs " + missing + ", which this build does not resolve";
    return false;
}

std::string caller(const void *from)
{
    std::string dll, id = settings_addon(from, dll);
    return id.empty() ? (dll.empty() ? std::string("unknown module") : dll) : id;
}

std::string text(const char *s) { return s ? s : ""; }

// SwoftyMod's split_season_picker, read once settings.json has it: off turns the era picker off before any
// box registers. Without the option (an older settings.json) the picker stays split.
bool split_off()
{
    static std::atomic<int> split{-1};
    if (split < 0) {
        SwoftySettingValue v{};
        if (settings_get(SPLIT_OPTION, SWOFTY_SETTING_TOGGLE, &v))
            split = v.toggle ? 1 : 0;
    }
    if (split != 0 || picker.off)
        return picker.off;
    std::lock_guard<std::mutex> g(picker.lock);
    if (!picker.off) {
        picker.off_why = "SwoftyMod's Split the season picker is off, so the picker keeps the game's single row";
        picker.off = true;
        swofty::log("[era] addon row off: %s", picker.off_why.c_str());
    }
    return true;
}

int api_available()
{
    split_off();
    return !picker.off.load();
}

int api_add_box(const SwoftyEraBox *desc)
{
    std::string who = caller(__builtin_return_address(0));
    if (!desc || desc->size < BOX_TIME_TRIAL) {
        swofty::log("[era] box from %s refused: no descriptor, or its size %u is below %zu", who.c_str(), desc ? desc->size : 0,
                    BOX_TIME_TRIAL);
        return SWOFTY_ASSET_INVALID;
    }
    split_off();
    logic::Box box{text(desc->entrant_list), text(desc->title), text(desc->image), desc->order, who, 0, {}};
    if (desc->size >= BOX_TIME_TRIAL + sizeof(desc->time_trial_entrant_list))
        box.time_trial = text(desc->time_trial_entrant_list);
    if (desc->size >= BOX_ONLY + sizeof(desc->only))
        box.only = desc->only;
    std::lock_guard<std::mutex> g(picker.lock);
    std::string why = logic::check(box, picker.boxes);
    if (!why.empty()) {
        swofty::log("[era] box %s from %s refused: %s", box.entrant_list.c_str(), who.c_str(), why.c_str());
        return SWOFTY_ASSET_INVALID;
    }
    box.seq = picker.boxes.size();
    picker.boxes.push_back(box);
    std::string state = picker.off    ? "off: " + picker.off_why
                        : picker.live ? "live from the next build of the picker"
                                      : "queued for the next mixin commit";
    std::string tt = box.time_trial.empty() ? std::string() : ", Time Trial list " + box.time_trial;
    if (box.only == logic::ONLY_GRAND_PRIX)
        tt += ", Grand Prix only";
    else if (box.only == logic::ONLY_TIME_TRIAL)
        tt += ", Time Trial only";
    swofty::log("[era] box %s from %s registered (title %s, image %s, order %d%s): %s", box.entrant_list.c_str(), who.c_str(),
                box.title.c_str(), logic::image_of(box).c_str(), box.order, tt.c_str(), state.c_str());
    return picker.off ? SWOFTY_ASSET_UNAVAILABLE : SWOFTY_ASSET_OK;
}

int api_count()
{
    std::lock_guard<std::mutex> g(picker.lock);
    return int(picker.boxes.size());
}

const SwoftyEra table = {api_available, api_add_box, api_count};

constexpr const char *CAREER_ALIAS_FN = "Ent_Deserialise_EntrantVector";
constexpr const char *CAREER_ALIAS_MIXIN = "swofty_core.dll (career id aliases)";

constexpr const char *AVAILABILITY_MIXIN = "swofty_core.dll (setup mode guard)";
constexpr const char *AVAILABILITY_FN = "Sess_Apply_ContentAvailability";

// An era tile press (0x1424b3640 -> 0x1424b2c90) collects the game mode's setup modes into a pointer vector
// with Sess_Collect_GameModeSetupModes, backed by a 256-byte arena on the caller's stack (0x1424b31af). The
// vector grows 1, 2, 3, 4, 6, 9, 13, so a game mode with a 10th matching setup mode asks the arena for 104 more
// bytes, gets null and the game memcpys to address 0. With the stack arena swapped for the game's default
// allocator the list grows on the heap; the vector keeps its allocator and the caller frees through it (vt+0x38).
constexpr const char *SETUP_LIST_MIXIN = "swofty_core.dll (era press setup mode list)";
constexpr const char *SETUP_LIST_FN = "Sess_Collect_GameModeSetupModes";

}

using namespace era_picker;

extern "C" __declspec(dllexport) const SwoftyEra *SwoftyCoreEra(void) { return &table; }

extern "C" __declspec(dllexport) const uint32_t SwoftyCoreEraOnly = SWOFTY_API_ERA;

namespace runtime {

const SwoftyEra *era_table() { return &table; }

std::vector<era::Box> era_boxes()
{
    std::lock_guard<std::mutex> g(picker.lock);
    return logic::ordered(picker.boxes);
}

void era_press(const char *entrant_list)
{
    void *entity = nullptr;
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_Manager") : 0, mgr = 0, ctx = 0;
    if (slot && mem::read(slot, mgr) && mgr)
        mem::read(mgr + offsetof(swofty::game::GameManager, mode_context), ctx);
    entity = reinterpret_cast<void *>(ctx);
    void *host = time_trial(entity) ? entity : nullptr;
    // The picker's own list of boxes only exists once the picker has been opened, so the name is taken as given: the narrowing finds
    // the list by name in the mode's setups and does nothing when it is not there.
    bool ours = entrant_list && *entrant_list;
    {
        std::lock_guard<std::mutex> g(hold.lock);
        put_back();
    }
    if (host && ours)
        ::narrow(host, entrant_list);
    picker.last_press = entrant_list ? entrant_list : "";
    picker.last_pick = ours ? picker.last_press : "";
    swofty::log("[era] box %s counts as pressed (a replay names it): %s",
                picker.last_press.empty() ? "none" : picker.last_press.c_str(),
                !ours  ? "no box named"
                : host ? "the Time Trial mode is narrowed to it"
                       : "the current mode is not Time Trial; nothing narrowed");
}

static bool entrant_cap_armed = false;

static std::vector<CoreMixins> tt_guard_mixins()
{
    std::vector<CoreMixins> out;
    if (vehicle_guard.gate.offered() || !code_ready())
        return out;
    vehicle_guard.gate.offer();
    entrant_cap_armed = install_entrant_cap();
    if (!swofty::api->resolve(VEHICLE_FN)) {
        swofty::log("[era] session vehicle guard off: needs %s, which this build does not resolve", VEHICLE_FN);
    } else {
        vehicle_guard.bound = true;
        out.push_back(
            {VEHICLE_MIXIN, {{VEHICLE_FN, SWOFTY_MIXIN_WRAP, 100, (void *)&vehicles_entry, (void **)&vehicle_guard.orig}}});
    }
    if (!swofty::api->resolve(AVAILABILITY_FN)) {
        swofty::log("[era] setup mode guard off: needs %s, which this build does not resolve", AVAILABILITY_FN);
    } else {
        availability_guard.bound = true;
        out.push_back(
            {AVAILABILITY_MIXIN,
             {{AVAILABILITY_FN, SWOFTY_MIXIN_WRAP, 100, (void *)&availability_entry, (void **)&availability_guard.orig}}});
    }
    if (!swofty::api->resolve(SETUP_LIST_FN) || !swofty::api->resolve("g_Ne_DefaultAllocator")) {
        swofty::log("[era] era press setup mode list off: needs %s and g_Ne_DefaultAllocator, which this build does "
                    "not resolve",
                    SETUP_LIST_FN);
    } else {
        setup_list_guard.bound = true;
        out.push_back({SETUP_LIST_MIXIN,
                       {{SETUP_LIST_FN, SWOFTY_MIXIN_WRAP, 100, (void *)&setup_list_entry, (void **)&setup_list_guard.orig}}});
    }
    if (!swofty::api->resolve(PARSE_LIST_FN) || !swofty::api->resolve("g_Ne_DefaultAllocator")) {
        swofty::log("[era] virtual entrant lists off: needs %s and g_Ne_DefaultAllocator, which this build does not "
                    "resolve",
                    PARSE_LIST_FN);
    } else {
        virtual_lists.parse_list_bound = true;
        out.push_back(
            {PARSE_LIST_MIXIN,
             {{PARSE_LIST_FN, SWOFTY_MIXIN_WRAP, 100, (void *)&parse_list_entry, (void **)&virtual_lists.parse_list_orig}}});
    }
    return out;
}

static std::vector<CoreMixins> career_alias_mixins()
{
    std::vector<CoreMixins> out;
    if (alias_guard.gate.offered() || !code_ready())
        return out;
    alias_guard.gate.offer();
    if (!swofty::api->resolve(CAREER_ALIAS_FN)) {
        swofty::log("[era] career id aliases off: needs %s, which this build does not resolve (a verify agree row on its "
                    "saved record layout is missing)",
                    CAREER_ALIAS_FN);
        return out;
    }
    alias_guard.bound = true;
    out.push_back({CAREER_ALIAS_MIXIN,
                   {{CAREER_ALIAS_FN, SWOFTY_MIXIN_WRAP, 100, (void *)&entrant_vector_entry, (void **)&alias_guard.orig}}});
    return out;
}

static void career_alias_commit()
{
    if (!alias_guard.gate.settle())
        return;
    if (alias_guard.bound)
        swofty::log("[era] career id aliases %s",
                    loader::install_state(core_mixins_active(CAREER_ALIAS_MIXIN),
                                          "armed: a career saved with earlier row ids has its entrants mapped to the "
                                          "current ids as it loads"));
}

static loader::FunctionSet career_alias_core_set{loader::CoreSetId::career_alias, career_alias_mixins, career_alias_commit};

static void tt_guard_commit()
{
    if (!vehicle_guard.gate.settle())
        return;
    if (entrant_cap_armed)
        swofty::log("[era] Time Trial entrant cap armed: a Time Trial session holds at most %zu entrants when the game "
                    "builds its participants, leaving out the lists' cars past that",
                    logic::TT_TABLE);
    if (vehicle_guard.bound)
        swofty::log("[era] session vehicle guard %s",
                    loader::install_state(core_mixins_active(VEHICLE_MIXIN),
                                          "armed: a session with more vehicles than its table of 26 sets up the first 26 "
                                          "instead of writing past it"));
    if (availability_guard.bound)
        swofty::log("[era] setup mode guard %s",
                    loader::install_state(core_mixins_active(AVAILABILITY_MIXIN),
                                          "armed: when the game rebuilds its setup modes the core drops the pointers it "
                                          "keeps into them"));
    if (setup_list_guard.bound)
        swofty::log("[era] era press setup mode list %s",
                    loader::install_state(core_mixins_active(SETUP_LIST_MIXIN),
                                          "armed: a game mode with more than 9 setup modes no longer crashes an era tile "
                                          "press"));
    if (virtual_lists.parse_list_bound)
        swofty::log("[era] virtual entrant lists %s",
                    loader::install_state(core_mixins_active(PARSE_LIST_MIXIN),
                                          "armed: a box whose list lies above the table's count is read into "
                                          "swofty_virtual_list when it is pressed"));
}

static loader::FunctionSet tt_guard_core_set{loader::CoreSetId::tt_guard, tt_guard_mixins, tt_guard_commit};

static std::vector<CoreMixins> era_mixins()
{
    std::vector<CoreMixins> out;
    if (picker.gate.offered() || split_off() || picker.off || !code_ready())
        return out;
    {
        std::lock_guard<std::mutex> g(picker.lock);
        if (picker.boxes.empty())
            return out;
    }
    picker.gate.offer();
    std::string why;
    if (!bind_all(why)) {
        picker.off_why = why;
        picker.off = true;
        swofty::log("[era] addon row off: %s", why.c_str());
        return out;
    }
    out.push_back({MIXIN,
                   {{"site_FeTile_EraTilesBuilt", SWOFTY_MIXIN_SITE, 100, (void *)&site_entry, nullptr},
                    {"FeTile_Press_EraTile", SWOFTY_MIXIN_WRAP, 100, (void *)&press_entry, (void **)&picker.press_orig},
                    {"Game_F1WorldActivityContainer_SelectEntrantList", SWOFTY_MIXIN_WRAP, 100, (void *)&select_entry,
                     (void **)&picker.select_orig}}});
    swofty::log("[era] %d box(es) registered; offering the era picker hooks", api_count());
    return out;
}

static void era_commit()
{
    if (picker.off || !picker.gate.settle())
        return;
    if (core_mixins_active(MIXIN)) {
        picker.live = true;
        swofty::log("[era] era picker hooks installed; addon boxes show from the next build of the picker");
        return;
    }
    picker.off_why = "the era picker hooks did not install (see the [core] lines for " + std::string(MIXIN) + ")";
    picker.off = true;
    swofty::log("[era] addon row off: %s", picker.off_why.c_str());
}

static loader::FunctionSet era_core_set{loader::CoreSetId::era, era_mixins, era_commit};

}
