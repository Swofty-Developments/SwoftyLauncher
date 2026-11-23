#include "core/boxes/picker/picker.hpp"
#include "core/hooks/hooks.hpp"

#include "core/runtime/runtime.hpp"

#include "core/apply/content/appliers.hpp"
#include "core/apply/moments.hpp"
#include "core/apply/scheduler.hpp"
#include "core/boxes/box_grid.hpp"
#include "core/boxes/box_grid_logic.hpp"
#include "core/boxes/box_options.hpp"
#include "core/boxes/tt_loaded_logic.hpp"

namespace era_picker {

namespace {

void note_list(const std::string &name)
{
    if (name.empty())
        return;
    swofty_core::hooks::update_facts([&](swofty_core::hooks::Facts &f) { f.entrant_list = name; });
}

constinit SessionLines lines;

}

std::vector<uint16_t> ids_of(uintptr_t begin, uintptr_t end, size_t field)
{
    std::vector<uint16_t> out;
    int64_t n = logic::entrant_count(begin, end);
    for (int64_t i = 0; i < n; ++i) {
        uint16_t id = logic::NO_TEAM;
        if (mem::read(begin + size_t(i) * logic::ENTRANT_RECORD + field, id))
            out.push_back(id);
    }
    return out;
}

std::vector<uint16_t> teams_of(uintptr_t begin, uintptr_t end) { return ids_of(begin, end, logic::ENTRANT_TEAM); }

logic::IdAliases id_aliases()
{
    static logic::IdAliases aliases;
    static bool read;
    if (read)
        return aliases;
    read = true;
    std::string text = runtime::read_file(runtime::swofty_root() + L"id_aliases.json", size_t(4) << 20);
    if (text.empty())
        return aliases;
    if (!logic::parse_aliases(text, aliases)) {
        swofty::log("[era] swofty\\id_aliases.json is unusable; saves made under earlier row ids are not mapped");
        return aliases;
    }
    swofty::log("[era] id aliases: %zu team id(s), %zu vehicle id(s), %zu driver id(s)", aliases.teams.size(),
                aliases.vehicles.size(), aliases.drivers.size());
    return aliases;
}

uint32_t remap_entrants(uintptr_t begin, uintptr_t end, const logic::IdAliases &aliases)
{
    uint32_t changed = 0;
    int64_t n = logic::entrant_count(begin, end);
    for (int64_t i = 0; i < n; ++i) {
        uintptr_t at = begin + size_t(i) * logic::ENTRANT_RECORD;
        const std::pair<size_t, const std::map<uint16_t, uint16_t> *> fields[] = {{logic::ENTRANT_TEAM, &aliases.teams},
                                                                                  {logic::ENTRANT_VEHICLE_ID, &aliases.vehicles},
                                                                                  {logic::ENTRANT_DRIVER, &aliases.drivers}};
        for (auto &[off, map] : fields) {
            uint16_t id = 0;
            if (!mem::read(at + off, id))
                continue;
            uint16_t now = logic::alias_of(*map, id);
            if (now == id)
                continue;
            ref<uint16_t>(reinterpret_cast<void *>(at), off) = now;
            changed++;
        }
    }
    return changed;
}

constexpr size_t VECTOR_BEGIN = 0x8;
constexpr size_t VECTOR_END = 0x10;

int entrant_vector_entry(void *vector, void *bag)
{
    int result = alias_guard.orig(vector, bag);
    const logic::IdAliases &aliases = id_aliases();
    if (result != 0 || !vector || !aliases.any())
        return result;
    uintptr_t at = reinterpret_cast<uintptr_t>(vector), begin = 0, end = 0;
    if (!mem::read(at + VECTOR_BEGIN, begin) || !mem::read(at + VECTOR_END, end) || logic::entrant_count(begin, end) <= 0)
        return result;
    if (!logic::saved_predates(aliases, teams_of(begin, end), ids_of(begin, end, logic::ENTRANT_VEHICLE_ID),
                               ids_of(begin, end, logic::ENTRANT_DRIVER)))
        return result;
    uint32_t n = remap_entrants(begin, end, aliases);
    if (n && lines.career_remap.note())
        swofty::log("[era] career load: mapped %u saved entrant id(s) to the ids the rows have now", n);
    return result;
}

[[noreturn]] void stop_session_load(const std::string &why)
{
    swofty::log("[era] %s", why.c_str());
    swofty::log("[crash] the core closes F1 25 at StateLoadSession (exit code %u): %s", logic::CONTINUE_STOP_EXIT, why.c_str());
    TerminateProcess(GetCurrentProcess(), logic::CONTINUE_STOP_EXIT);
    ExitProcess(logic::CONTINUE_STOP_EXIT);
}

std::string mode_label(uintptr_t ctx)
{
    uintptr_t holder = 0, name = 0;
    std::string label;
    if (mem::read(ctx + MODE_SETUP, holder) && holder && mem::read(holder + MODE_NAME, name) && mem::c_string(name, label, 64))
        return label;
    return {};
}

void leave_front_end(uintptr_t ctx, bool career_kept)
{
    std::string label = mode_label(ctx);
    if (label.empty())
        return;
    uintptr_t holder = 0, setup = 0, list = 0;
    std::string selected;
    if (mem::read(ctx + MODE_SETUP, holder) && holder && mem::read(holder + SETUP_MODES, setup) && setup &&
        mem::read(setup + SELECTED_LIST, list) && list)
        if (const char *n = name_of(reinterpret_cast<void *>(list), LIST_NAME))
            selected = n;
    bool box = !selected.empty() && (box_list(selected) || is_ours(selected.c_str()));
    logic::FrontEndLeave leave = logic::leaving_front_end(label, box);
    if (leave.restore && career_kept)
        restore_lists();
    else if (leave.restore)
        restore();
    if (!box && !career_kept)
        clear_fuel();
    if (leave.clear_selected)
        ref<uintptr_t>(reinterpret_cast<void *>(setup), SELECTED_LIST) = 0;
    if (leave.forget_pick)
        picker.last_pick.clear();
    if ((leave.restore || leave.clear_selected) && lines.front_end_leave.note())
        swofty::log("[era] %s session loads: %s's narrowed lists and DRS group are back%s", label.c_str(), logic::HOST,
                    leave.clear_selected ? ("; the box list " + selected + " no longer selected").c_str() : "");
}

void driver_career_load(uintptr_t ctx)
{
    uintptr_t holder = 0, setup = 0, entrant_setup = 0, list = 0, gsm = 0, gsm_holder = 0, gsm_name = 0;
    if (!mem::read(ctx + MODE_SETUP, holder) || !holder || !mem::read(holder + SETUP_MODES, setup) || !setup ||
        !mem::read(setup + SELECTED_LIST, list) || list)
        return;
    std::string setup_mode;
    if (mem::read(setup + SETUP_GAME_SETUP_MODE, gsm) && gsm && mem::read(gsm + SETUP_MODE_NAME, gsm_holder) && gsm_holder &&
        mem::read(gsm_holder + 0x10, gsm_name))
        mem::c_string(gsm_name, setup_mode, 64);
    std::vector<void *> ptrs;
    std::vector<std::string> names;
    if (mem::read(setup + SETUP_ENTRANT_SETUP, entrant_setup) && entrant_setup)
        for (const auto &l : lists_of(reinterpret_cast<const uint8_t *>(entrant_setup), ptrs))
            names.push_back(l.name);
    auto career = swofty_core::apply::scheduler().context();
    logic::CareerListPick pick = logic::career_list(names, career.data_group, career.content, setup_mode);
    if (pick.index < 0)
        stop_session_load(logic::career_stop(pick));
    ref<void *>(reinterpret_cast<void *>(setup), SELECTED_LIST) = ptrs[size_t(pick.index)];
    note_list(names[size_t(pick.index)]);
    if (lines.driver_career.note())
        swofty::log("[era] Driver Career load: %s", pick.line.c_str());
}

// The list a session of another game mode loads, and in `what` that mode's name and "session".
std::string other_mode_list(uintptr_t ctx, std::string &what)
{
    uintptr_t holder = 0, setup = 0, list = 0, name = 0;
    if (!mem::read(ctx + MODE_SETUP, holder) || !holder)
        return {};
    std::string mode;
    if (mem::read(holder + MODE_NAME, name) && name)
        mem::c_string(name, mode, 64);
    what = (mode.empty() ? std::string("unknown") : mode) + " session";
    if (!mem::read(holder + SETUP_MODES, setup) || !setup || !mem::read(setup + SELECTED_LIST, list) || !list)
        return {};
    const char *n = name_of(reinterpret_cast<void *>(list), LIST_NAME);
    return n ? std::string(n) : std::string();
}

std::string grand_prix_load(uintptr_t ctx, std::string &what)
{
    swofty_core::apply::moments::grand_prix_track_check();
    auto [mode, track] = swofty_core::apply::moments::session_mode_track();
    if (mode == "driver_career") {
        driver_career_load(ctx);
        return {};
    }
    if (mode != "grand_prix")
        return runtime::tt_loaded::follows_list(mode) ? other_mode_list(ctx, what) : std::string();
    std::string track_why = logic::track_missing(swofty_core::apply::tracks::base(), track);
    uintptr_t holder = 0, setup = 0, entrant_setup = 0, list = 0, begin = 0, end = 0, gsm = 0, gsm_holder = 0, gsm_name = 0;
    if (!mem::read(ctx + MODE_SETUP, holder) || !holder || !mem::read(holder + SETUP_MODES, setup) || !setup ||
        !mem::read(setup + SELECTED_LIST, list)) {
        if (!track_why.empty())
            stop_session_load(logic::continue_stop(track_why, {}, false));
        return {};
    }
    logic::ContinuePick pick;
    std::string chosen;
    bool empty = !list && mem::read(setup + SETUP_ENTRANT_SETUP, entrant_setup) && entrant_setup;
    if (empty) {
        std::string setup_mode;
        if (mem::read(setup + SETUP_GAME_SETUP_MODE, gsm) && gsm && mem::read(gsm + SETUP_MODE_NAME, gsm_holder) && gsm_holder &&
            mem::read(gsm_holder + 0x10, gsm_name))
            mem::c_string(gsm_name, setup_mode, 64);
        std::vector<uint16_t> saved;
        if (mem::read(setup + logic::MGR_SESSION_BEGIN, begin) && mem::read(setup + logic::MGR_SESSION_END, end))
            saved = teams_of(begin, end);
        std::vector<void *> ptrs;
        auto cars = lists_of(reinterpret_cast<const uint8_t *>(entrant_setup), ptrs);
        std::vector<logic::ListTeams> lists;
        for (size_t i = 0; i < ptrs.size(); ++i) {
            logic::ListTeams l{cars[i].name, {}};
            uintptr_t at = reinterpret_cast<uintptr_t>(ptrs[i]), lb = 0, le = 0;
            if (at && mem::read(at + logic::LIST_ENTRANTS_BEGIN, lb) && mem::read(at + logic::LIST_ENTRANTS_END, le))
                l.teams = teams_of(lb, le);
            lists.push_back(std::move(l));
        }
        logic::AliasedPick aliased = logic::continue_list_aliased(lists, saved, picker.last_press, setup_mode, id_aliases());
        pick = aliased.pick;
        if (aliased.remapped && track_why.empty()) {
            uint32_t n = remap_entrants(begin, end, id_aliases());
            swofty::log("[era] Grand Prix load: mapped %u saved entrant id(s) to the ids the rows have now", n);
        }
        if (pick.index >= 0 && track_why.empty()) {
            ref<void *>(reinterpret_cast<void *>(setup), SELECTED_LIST) = ptrs[size_t(pick.index)];
            chosen = cars[size_t(pick.index)].name;
            if (lines.grand_prix.note())
                swofty::log("[era] Grand Prix load: %s", pick.line.c_str());
            std::string grid = continue_grid.raise(reinterpret_cast<uint8_t *>(entrant_setup), cars[size_t(pick.index)].cars,
                                                   swofty_core::apply::content::seats_for_box(chosen));
            if (!grid.empty())
                swofty::log("[era] Grand Prix load: %s", grid.c_str());
        }
    }
    std::string why = logic::continue_stop(track_why, pick, empty);
    if (!why.empty()) {
        stop_session_load(why);
        return {};
    }
    if (chosen.empty() && list)
        if (const char *n = name_of(reinterpret_cast<void *>(list), LIST_NAME))
            chosen = n;
    return chosen;
}

// What a session load saw of a Time Trial: the list the session loads once the 24-car guard has chosen.
struct LoadSeen {
    bool time_trial = false;
    uintptr_t ctx = 0;
    std::string setup_mode, list, virtual_label, pressed, what = "Grand Prix";
};

void set_unlisted(const std::string &list)
{
    std::lock_guard<std::mutex> g(unlisted.lock);
    unlisted.list = list;
}

// Makes the box of the loaded list the active content when it is not, in a Time Trial, in a Grand Prix
// (follow_grand_prix: a Continue presses no box) and in another mode's session (a LAN session's box comes from the
// host): tt_loaded_logic.hpp. Runs without held_lock, like a press, before the session load's own appliers read the
// content.
void follow_grand_prix(const LoadSeen &seen)
{
    namespace tl = runtime::tt_loaded;
    if (seen.list.empty())
        return;
    std::vector<tl::BoxLists> lists;
    {
        std::lock_guard<std::mutex> g(picker.lock);
        for (const auto &box : picker.boxes)
            lists.push_back({box.entrant_list, box.time_trial});
    }
    tl::Loaded loaded = tl::loaded_box(lists, seen.list, {}, logic::HOST, VIRTUAL_HOST, seen.virtual_label);
    auto &scheduler = swofty_core::apply::scheduler();
    std::string package = loaded.kind == tl::Kind::box ? scheduler.find_package(loaded.box) : std::string();
    tl::Follow f = tl::follow_grand_prix(loaded, package, scheduler.active(), seen.what);
    if (f.action != tl::Action::follow)
        return;
    swofty::log("%s", f.line.c_str());
    swofty_core::apply::moments::box_picked(f.box.c_str(), reinterpret_cast<const void *>(seen.ctx));
}

void follow_loaded_list(const LoadSeen &seen)
{
    namespace tl = runtime::tt_loaded;
    if (!seen.time_trial) {
        set_unlisted({});
        follow_grand_prix(seen);
        return;
    }
    std::vector<tl::BoxLists> lists;
    {
        std::lock_guard<std::mutex> g(picker.lock);
        for (const auto &box : picker.boxes)
            lists.push_back({box.entrant_list, box.time_trial});
    }
    tl::Loaded loaded = tl::loaded_box(lists, seen.list, seen.setup_mode, logic::HOST, VIRTUAL_HOST, seen.virtual_label);
    auto &scheduler = swofty_core::apply::scheduler();
    std::string package =
        loaded.kind == tl::Kind::box || loaded.kind == tl::Kind::stock ? scheduler.find_package(loaded.box) : std::string();
    tl::Follow f = tl::follow(loaded, package, scheduler.active(), seen.pressed);
    set_unlisted(f.action == tl::Action::no_board ? f.box : std::string());
    if (f.action == tl::Action::keep)
        return;
    swofty::log("%s", f.line.c_str());
    if (f.action == tl::Action::follow)
        swofty_core::apply::moments::box_picked(f.box.c_str(), reinterpret_cast<const void *>(seen.ctx));
}

void session_load_held(LoadSeen &seen)
{
    static std::string last_seen;
    put_back();
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_Manager") : 0, mgr = 0, ctx = 0, setup = 0;
    if (!slot || !mem::read(slot, mgr) || !mgr || !mem::read(mgr + offsetof(swofty::game::GameManager, mode_context), ctx) ||
        !ctx)
        return;
    if (!time_trial(reinterpret_cast<void *>(ctx))) {
        CareerSession career = career_session(ctx);
        leave_front_end(ctx, career.kept);
        seen.ctx = ctx;
        seen.list = grand_prix_load(ctx, seen.what);
        note_list(seen.list);
        seen.virtual_label = filled.label;
        if (career.career)
            apply_career_rules();
        return;
    }
    seen.time_trial = true;
    seen.ctx = ctx;
    seen.pressed = picker.last_press;
    seen.virtual_label = filled.label;
    uintptr_t holder = 0, gsm = 0, gsm_holder = 0, gsm_name = 0, entrant_setup = 0, list = 0;
    if (!mem::read(ctx + MODE_SETUP, holder) || !holder || !mem::read(holder + SETUP_MODES, setup) || !setup)
        return;
    std::string setup_mode = "?";
    if (mem::read(setup + SETUP_GAME_SETUP_MODE, gsm) && gsm && mem::read(gsm + SETUP_MODE_NAME, gsm_holder) && gsm_holder &&
        mem::read(gsm_holder + 0x10, gsm_name))
        mem::c_string(gsm_name, setup_mode, 64);
    if (!mem::read(setup + SETUP_ENTRANT_SETUP, entrant_setup) || !entrant_setup || !mem::read(setup + SELECTED_LIST, list))
        return;
    std::vector<void *> ptrs;
    auto lists = lists_of(reinterpret_cast<const uint8_t *>(entrant_setup), ptrs);
    logic::ListCars selected = list ? list_cars(reinterpret_cast<void *>(list)) : logic::ListCars{};
    logic::ListCars first = lists.empty() ? logic::ListCars{} : lists[0];
    std::string line = setup_mode + "/" + selected.name + "/" + std::to_string(selected.cars);
    if (line != last_seen && lines.time_trial_list.note())
        swofty::log("[era] Time Trial session loads list %s (%lld cars) of setup mode %s (box pressed: %s)",
                    selected.name.empty() ? "?" : selected.name.c_str(), (long long)selected.cars, setup_mode.c_str(),
                    picker.last_press.empty() ? "none" : picker.last_press.c_str());
    last_seen = line;
    seen.setup_mode = setup_mode;
    seen.list = selected.name;
    note_list(selected.name);
    if (!logic::oversized(selected.cars) && !logic::oversized(first.cars)) {
        time_trial_budget(reinterpret_cast<void *>(ctx), setup);
        return;
    }
    logic::ListCars bad = logic::oversized(selected.cars) ? selected : first;
    std::vector<std::string> prefer{picker.last_press};
    {
        std::lock_guard<std::mutex> b(picker.lock);
        for (const auto &box : picker.boxes)
            if (box.entrant_list == picker.last_press)
                prefer.push_back(logic::list_for(box, true));
    }
    int64_t i = logic::time_trial_loaded(selected, first, lists, prefer);
    auto *esm = reinterpret_cast<uint8_t *>(entrant_setup);
    hold.held = {};
    hold.held.esm = esm;
    if (i >= 0) {
        void *fit = ptrs[size_t(i)];
        ref<void *>(reinterpret_cast<void *>(setup), SELECTED_LIST) = fit;
        seen.list = lists[size_t(i)].name;
        if (logic::oversized(first.cars) && i > 0)
            logic::swap_into_first(hold.held, esm, size_t(i));
    }
    int32_t limit = 0;
    if (mem::read(entrant_setup + ENTRANT_SETUP_MAX_ENTRANTS, limit) && logic::time_trial_limit(limit) != limit) {
        hold.held.limit_was = limit;
        ref<int32_t>(esm, ENTRANT_SETUP_MAX_ENTRANTS) = logic::time_trial_limit(limit);
    }
    hold.holding = hold.held.first || hold.held.limit_was;
    time_trial_budget(reinterpret_cast<void *>(ctx), setup);
    if (lines.time_trial_swap.note())
        swofty::log("[era] %s", logic::time_trial_swap(bad.name, bad.cars, setup_mode, i >= 0 ? lists[size_t(i)].name : "",
                                                       hold.held.limit_was)
                                    .c_str());
}

}

using namespace era_picker;

namespace runtime {

void era_session_load()
{
    LoadSeen seen;
    {
        std::lock_guard<std::mutex> g(hold.lock);
        session_load_held(seen);
    }
    follow_loaded_list(seen);
}

bool era_session_unlisted(std::string &list)
{
    std::lock_guard<std::mutex> g(unlisted.lock);
    list = unlisted.list;
    return !list.empty();
}

}

namespace era_picker {

void drop_setup_pointers()
{
    logic::SetupPointers p;
    {
        std::lock_guard<std::mutex> g(hold.lock);
        p.held = hold.holding;
        hold.holding = false;
        hold.held = {};
        p.narrowed = picker.narrowed_count;
        picker.narrowed_count = 0;
    }
    p.drs = box_rules.drs.forget();
    box_rules.warmers.forget();
    box_rules.rev_lights.forget();
    box_rules.tyre_change.forget();
    box_rules.time_limits.forget();
    box_rules.weekends.forget();
    box_rules.points.forget();
    box_rules.drs_log.forget();
    box_rules.warmers_log.forget();
    box_rules.rev_lights_log.forget();
    box_rules.points_log.forget();
    box_rules.tyre_change_log.forget();
    box_rules.weekend_log.forget();
    runtime::box_options_forget();
    box_rules.box_options_log.forget();
    box_rules.grid_log.forget();
    box_rules.career_log.forget();
    forget_career_rules();
    std::string line = logic::availability_drop_line(p);
    if (!line.empty() && !availability_guard.logged_dropped.exchange(true))
        swofty::log("[era] %s", line.c_str());
}

void availability_entry(void *self, void *availability)
{
    drop_setup_pointers();
    availability_guard.orig(self, availability);
}

bool on_this_stack(const void *p)
{
    auto tib = reinterpret_cast<NT_TIB *>(NtCurrentTeb());
    auto a = reinterpret_cast<uintptr_t>(p);
    return a >= reinterpret_cast<uintptr_t>(tib->StackLimit) && a < reinterpret_cast<uintptr_t>(tib->StackBase);
}

void *setup_list_entry(void *out, void *allocator)
{
    uintptr_t slot = swofty::api->resolve("g_Ne_DefaultAllocator"), heap = 0;
    if (allocator && on_this_stack(allocator) && slot && mem::read(slot, heap) && heap) {
        if (setup_list_guard.swap_lines.note())
            swofty::log("[era] era press: the game mode's setup mode list grows on the heap, not in the 256-byte "
                        "stack arena that holds 9");
        return setup_list_guard.orig(out, reinterpret_cast<void *>(heap));
    }
    return setup_list_guard.orig(out, allocator);
}

}

namespace runtime {

void era_session_trace(const char *state, int to)
{
    static constexpr const char *STEP[4] = {"None", "Entering", "Updating", "Exiting"};
    if (!state || !logic::trace_flow(state, to))
        return;
    session_trace(std::string(state) + " " + STEP[to & 3], -1);
}

void era_session_unloaded(const char *state, int to)
{
    if (!state || !logic::time_trial_cut_off(state, to))
        return;
    time_trial_guard.budget = -1;
    std::lock_guard<std::mutex> g(hold.lock);
    logic::CutsPutBack r = put_back(true);
    if ((r.put || r.failed) && lines.unload_put_back.note())
        swofty::log("[era] StateUnloadSession Exiting: the lists the Time Trial guards left out are back in %zu entrant "
                    "setup mode(s)%s",
                    r.put, r.failed ? ("; " + std::to_string(r.failed) + " could not be put back").c_str() : "");
}

}
