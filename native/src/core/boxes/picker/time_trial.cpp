#include "core/boxes/picker/picker.hpp"

#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"
#include "core/guards/session/replay_grid_logic.hpp"

#include <safetyhook.hpp>

#include <algorithm>
#include <iterator>

namespace era_picker {

constexpr size_t MODULE_VEHICLES_BEGIN = 0x28;
constexpr size_t MODULE_VEHICLES_END = 0x30;
constexpr size_t MODULE_MODE_CONTEXT = 0x80;

namespace {

constinit BudgetLines lines;

namespace bp = runtime::build_proof;
// The fill (0x142936760) is the same 2816 instructions on Steam 26566308, the EA App KB build and EA 1.0.145, with the
// entrants vector (+0x2d8..+0x2f8), the records' +0x5c and +0x82 and the participants (+0xad0..+0xae0) at the same
// instructions (sigport.py --witness); the site pattern matches once on each. On EA 1.0.145 the human records' +0x48
// stores and the record push 0x140af2ab0 (138 instructions, its 0x88 stride and +0x48 reads) are the same too.
constexpr bp::Proof FILL_ENTRANTS{"the Time Trial fill's entrants at its participant loop (holder +0x2e0, +0x2e8, "
                                  "0x88-byte records, human flag +0x4b bit 0x10)",
                                  bp::proofs::VERIFIED_LAYOUT.builds | bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate ENTRANT_CAP{"the Time Trial entrant cap", &FILL_ENTRANTS};
const runtime::Sig SIG_FILL_PARTICIPANTS{"session.time_trial_fill_participants", "48 8B B7 E0 02 00 00 48 89 7C 24 60",
                                         0x2939dc2};
const runtime::loader::HookSite FILL_PARTICIPANTS{SIG_FILL_PARTICIPANTS, "Time Trial entrant cap", "era", &ENTRANT_CAP};

// The Time Trial entrant cap: a mid hook at the fill's participant loop.
struct EntrantCapHooks : runtime::loader::HookSet<> {
    safetyhook::MidHook fill_participants;
};
EntrantCapHooks entrant_cap;
Cap<LOG_FIRST> cap_lines;

void entrant_cap_mid(safetyhook::Context &c)
{
    uintptr_t holder = c.rdi, begin = 0, end = 0;
    if (!holder || !mem::read(holder + logic::MGR_SESSION_BEGIN, begin) || !mem::read(holder + logic::MGR_SESSION_END, end))
        return;
    int64_t n = logic::entrant_count(begin, end);
    if (n < 0 || size_t(n) <= logic::TT_TABLE)
        return;
    std::vector<bool> human(static_cast<size_t>(n));
    size_t humans = 0;
    for (size_t i = 0; i < human.size(); ++i) {
        uint8_t flags = 0;
        human[i] =
            mem::read(begin + i * logic::ENTRANT_RECORD + logic::ENTRANT_HUMAN_BYTE, flags) && (flags & logic::ENTRANT_HUMAN);
        humans += human[i];
    }
    std::vector<bool> fixed = human;
    fixed.back() = true;
    std::vector<size_t> keep = logic::table_entrants(fixed);
    ref<uintptr_t>(reinterpret_cast<uint8_t *>(holder), logic::MGR_SESSION_END) =
        reinterpret_cast<uintptr_t>(logic::keep_entrants(reinterpret_cast<uint8_t *>(begin), keep));
    if (cap_lines.note())
        swofty::log("[era] %s", logic::entrant_cap_line(human.size(), keep.size(), humans).c_str());
}

}

bool install_entrant_cap()
{
    if (entrant_cap.live())
        return true;
    auto r = runtime::loader::install(entrant_cap.fill_participants, FILL_PARTICIPANTS, entrant_cap_mid);
    if (r.step == runtime::loader::Installed::Step::failed) {
        swofty::log("[era] Time Trial entrant cap off: the hook on the fill's participant loop failed: %s", r.error.c_str());
    } else if (!r) {
        swofty::log("[era] Time Trial entrant cap off: the fill's participant loop is not proven or did not resolve on this "
                    "build");
    } else {
        entrant_cap.set_live();
        return true;
    }
    entrant_cap.turn_off(std::move(r.why));
    return false;
}

std::string session_list(uintptr_t ctx)
{
    uintptr_t holder = 0, setup = 0, list = 0, gsm = 0, gsm_holder = 0, gsm_name = 0;
    std::string mode = "?";
    if (!ctx || !mem::read(ctx + MODE_SETUP, holder) || !holder || !mem::read(holder + SETUP_MODES, setup) || !setup)
        return "an unread list";
    if (mem::read(setup + SETUP_GAME_SETUP_MODE, gsm) && gsm && mem::read(gsm + SETUP_MODE_NAME, gsm_holder) && gsm_holder &&
        mem::read(gsm_holder + 0x10, gsm_name))
        mem::c_string(gsm_name, mode, 64);
    logic::ListCars cars =
        mem::read(setup + SELECTED_LIST, list) && list ? list_cars(reinterpret_cast<void *>(list)) : logic::ListCars{};
    return "list " + (cars.name.empty() ? std::string("?") : cars.name) + " (" + std::to_string(cars.cars) +
           " cars) of setup mode " + mode;
}

int64_t descriptor_participants(uintptr_t holder, size_t off)
{
    uintptr_t desc = 0, begin = 0, end = 0;
    if (!holder || !mem::read(holder + off, desc) || !desc || !mem::read(desc + logic::DESC_PARTICIPANTS_BEGIN, begin) ||
        !mem::read(desc + logic::DESC_PARTICIPANTS_END, end))
        return -1;
    return logic::participant_count(begin, end);
}

void session_trace(const std::string &where, int64_t vehicles)
{
    if (time_trial_guard.trace_lines >= logic::TRACE_LINES)
        return;
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_Manager") : 0, gm = 0, ctx = 0, holder = 0, setup = 0;
    if (!slot || !mem::read(slot, gm) || !gm)
        return;
    logic::SessionTrace t;
    t.where = where;
    t.vehicles = vehicles;
    uintptr_t name = 0, module = 0;
    if (mem::read(gm + offsetof(swofty::game::GameManager, mode_context), ctx) && ctx && mem::read(ctx + MODE_SETUP, holder) &&
        holder) {
        if (mem::read(holder + MODE_NAME, name) && name)
            mem::c_string(name, t.mode, 64);
        if (mem::read(holder + SETUP_MODES, setup) && setup) {
            uintptr_t begin = 0, end = 0, list = 0, gsm = 0, gsm_holder = 0, gsm_name = 0;
            if (mem::read(setup + logic::MGR_SESSION_BEGIN, begin) && mem::read(setup + logic::MGR_SESSION_END, end)) {
                t.entrants = logic::entrant_count(begin, end);
                for (int64_t i = 0; i < t.entrants && i < int64_t(logic::MAX_PARTICIPANTS); ++i) {
                    uint16_t team = 0;
                    mem::read(begin + size_t(i) * logic::ENTRANT_RECORD + logic::ENTRANT_TEAM, team);
                    t.teams.push_back(team);
                }
            }
            if (mem::read(setup + SELECTED_LIST, list) && list) {
                logic::ListCars cars = list_cars(reinterpret_cast<void *>(list));
                t.list = cars.name;
                t.list_cars = cars.cars;
            }
            if (mem::read(setup + SETUP_GAME_SETUP_MODE, gsm) && gsm && mem::read(gsm + SETUP_MODE_NAME, gsm_holder) &&
                gsm_holder && mem::read(gsm_holder + 0x10, gsm_name) && gsm_name)
                mem::c_string(gsm_name, t.setup_mode, 64);
        }
    }
    t.budget = time_trial_guard.budget.load();
    static int64_t last_participants = -1;
    t.previous = last_participants;
    t.gm_session = descriptor_participants(gm, logic::GM_SESSION);
    if (mem::read(gm + logic::GM_MODULE, module) && module) {
        t.module_session = descriptor_participants(module, logic::MODULE_SESSION);
        t.module_next = descriptor_participants(module, logic::MODULE_SESSION_NEXT);
    }
    time_trial_guard.trace_lines++;
    swofty::log("[era] %s", logic::session_trace_line(t).c_str());
    last_participants = t.gm_session;
    std::string mismatch = logic::budget_mismatch(t);
    if (!mismatch.empty())
        swofty::log("[era] %s", mismatch.c_str());
}

void vehicles_entry(void *module)
{
    auto *m = static_cast<uint8_t *>(module);
    uintptr_t begin = at<uintptr_t>(m, MODULE_VEHICLES_BEGIN), end = at<uintptr_t>(m, MODULE_VEHICLES_END);
    session_trace("Sess_Setup_VehicleComponents", int64_t(logic::session_vehicles(begin, end)));
    runtime::replay_grid::session_vehicles.store(int64_t(logic::session_vehicles(begin, end)));
    uintptr_t within = logic::vehicles_end_within_table(begin, end);
    if (within == end)
        return vehicle_guard.orig(module);
    if (vehicle_guard.lines.note())
        swofty::log("[era] the session holds %zu vehicles and its vehicle table %zu; setting up the first %zu (the "
                    "session's %s)",
                    logic::session_vehicles(begin, end), logic::TT_TABLE, logic::TT_TABLE,
                    session_list(at<uintptr_t>(m, MODULE_MODE_CONTEXT)).c_str());
    ref<uintptr_t>(m, MODULE_VEHICLES_END) = within;
    vehicle_guard.orig(module);
    if (at<uintptr_t>(m, MODULE_VEHICLES_END) == within)
        ref<uintptr_t>(m, MODULE_VEHICLES_END) = end;
}

bool keep_time_trial_lists(void *mode)
{
    bool any = false;
    for_each_host_setup(mode, [&](uint8_t *setup) {
        std::vector<void *> ptrs;
        auto info = lists_of(setup, ptrs);
        auto keep = logic::time_trial_keep(info);
        if (!logic::time_trial_narrows(info, keep) || picker.narrowed_count == std::size(picker.narrowed))
            return;
        Narrowed &n = picker.narrowed[picker.narrowed_count++];
        n.setup = setup;
        n.count = int64_t(info.size());
        n.kept = int64_t(keep.size());
        n.time_trial = true;
        n.grid = {};
        memcpy(n.lists, ptrs.data(), info.size() * sizeof(void *));
        for (size_t i = 0; i < keep.size(); ++i)
            n.kept_lists[i] = ptrs[keep[i]];
        memcpy(setup + LISTS, n.kept_lists, keep.size() * sizeof(void *));
        ref<int64_t>(setup, LIST_COUNT) = n.kept;
        any = true;
        for (size_t i = 0; i < info.size(); ++i) {
            if (!logic::oversized(info[i].cars) || std::find(time_trial_guard.kept_out.begin(), time_trial_guard.kept_out.end(),
                                                             info[i].name) != time_trial_guard.kept_out.end())
                continue;
            time_trial_guard.kept_out.push_back(info[i].name);
            if (time_trial_guard.kept_lines.note())
                swofty::log("[era] %s keeps %s out of Time Trial while a stock box is picked: it holds %lld cars; Time "
                            "Trial holds at most %zu (the game adds %zu cars of its own to a table of %zu)%s",
                            logic::HOST, info[i].name.c_str(), (long long)info[i].cars, logic::TT_MAX_CARS, logic::TT_EXTRA_CARS,
                            logic::TT_TABLE, keep.empty() ? "; no list of it fits, so it offers none until the next press" : "");
        }
    });
    return any;
}

struct ListSlot {
    uint8_t *setup;
    size_t index;
};

struct LeaderboardList {
    void *list;
    std::vector<ListSlot> slots;
};

bool leaderboard_of(const void *list, uint8_t &id)
{
    uint8_t set = 0;
    auto at_list = reinterpret_cast<uintptr_t>(list);
    return list && mem::read(at_list + logic::LIST_LEADERBOARD_SET, set) && set &&
           mem::read(at_list + logic::LIST_LEADERBOARD, id);
}

std::vector<uint32_t> vehicle_refs(const void *list)
{
    std::vector<uint32_t> refs;
    uintptr_t begin = 0, end = 0;
    auto at_list = reinterpret_cast<uintptr_t>(list);
    if (!mem::read(at_list + logic::LIST_ENTRANTS_BEGIN, begin) || !mem::read(at_list + logic::LIST_ENTRANTS_END, end))
        return refs;
    int64_t n = logic::entrant_count(begin, end);
    for (int64_t i = 0; i < n; ++i) {
        uint32_t ref = 0;
        if (mem::read(begin + size_t(i) * logic::ENTRANT_RECORD + logic::ENTRANT_VEHICLE, ref))
            refs.push_back(ref);
    }
    return refs;
}

bool box_list(const std::string &name)
{
    std::lock_guard<std::mutex> g(picker.lock);
    for (const auto &box : picker.boxes)
        if (box.entrant_list == name || box.time_trial == name)
            return true;
    return false;
}

uint8_t *resolved_esm(uintptr_t setup, uint8_t *esm)
{
    uint8_t id = 0;
    uintptr_t slots = 0, meta = 0, mask = 0;
    if (!esm || !mem::read(reinterpret_cast<uintptr_t>(esm), id) || !mem::read(setup + logic::MGR_OVERRIDE_MASK, mask) || !mask ||
        !mem::read(setup + logic::MGR_OVERRIDES, slots) || !mem::read(setup + logic::MGR_OVERRIDE_META, meta) || !slots ||
        meta < slots || (meta - slots) % logic::OVERRIDE_SLOT)
        return esm;
    size_t count = (meta - slots) / logic::OVERRIDE_SLOT;
    if (count > logic::MAX_OVERRIDE_SLOTS || !mem::readable(reinterpret_cast<void *>(slots), count * logic::OVERRIDE_SLOT) ||
        !mem::readable(reinterpret_cast<void *>(meta), count))
        return esm;
    auto o = logic::override_for(id, reinterpret_cast<const uint8_t *>(slots), reinterpret_cast<const uint8_t *>(meta), count);
    if (o.found && !o.to && lines.empty_override.note())
        swofty::log("[era] the Time Trial's override row for entrant setup mode %u is empty: the game leaves that mode "
                    "out, so the budget does too",
                    unsigned(id));
    return o.found ? static_cast<uint8_t *>(const_cast<void *>(o.to)) : esm;
}

std::vector<uint32_t> pool_refs(uint32_t pools)
{
    std::vector<uint32_t> refs;
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_Manager") : 0, gm = 0, es = 0;
    if (!slot || !mem::read(slot, gm) || !gm || !mem::read(gm + runtime::manager_entrants_system(), es) || !es)
        return refs;
    for (auto [bit, off] : {std::pair<uint32_t, size_t>{logic::TAKES_POOL_A, logic::ES_POOL_A},
                            std::pair<uint32_t, size_t>{logic::TAKES_POOL_B, logic::ES_POOL_B}}) {
        uintptr_t pool = 0, begin = 0, end = 0;
        if (!(pools & bit) || !mem::read(es + off, pool) || !pool || !mem::read(pool + 8, begin) || !mem::read(pool + 0x10, end))
            continue;
        auto got = logic::pool_refs_from(begin, end, [](uintptr_t a, auto &v) { return mem::read(a, v); });
        if (got.capped && lines.pool_capped.note())
            swofty::log("[era] entrants system pool +0x%zx holds more than %zu vehicle slots; the Time Trial budget counts "
                        "the first %zu",
                        off, logic::MAX_POOL, logic::MAX_POOL);
        refs.insert(refs.end(), got.refs.begin(), got.refs.end());
    }
    return refs;
}

void time_trial_budget(void *mode, uintptr_t setup)
{
    uintptr_t selected = 0;
    uint8_t board = 0;
    if (!mem::read(setup + SELECTED_LIST, selected) || !selected || !leaderboard_of(reinterpret_cast<void *>(selected), board))
        return;
    SetupModes modes = setup_modes(mode);
    if (!modes)
        return;
    std::vector<LeaderboardList> found{{reinterpret_cast<void *>(selected), {}}};
    std::vector<bool> addon{false};
    uint32_t pools = 0;
    for (auto **p = modes.begin; p != modes.end; ++p) {
        const char *gsm = name_of(*p, SETUP_MODE_NAME);
        bool host = gsm && !strcmp(gsm, logic::HOST);
        int64_t n = at<int64_t>(*p, ENTRANT_SETUP_COUNT);
        auto **esms = reinterpret_cast<uint8_t **>(*p + ENTRANT_SETUPS);
        for (int64_t k = 0; k < n && k < ENTRANT_SETUP_SLOTS; ++k) {
            uint8_t *esm = resolved_esm(setup, esms[k]);
            int64_t count = 0;
            uint32_t player = 0, second = 0;
            if (!esm || !mem::read(reinterpret_cast<uintptr_t>(esm) + LIST_COUNT, count) || count <= 0 ||
                count > int64_t(LIST_SLOTS))
                continue;
            mem::read(reinterpret_cast<uintptr_t>(esm) + logic::ESM_VEHICLE_FLAGS, player);
            mem::read(reinterpret_cast<uintptr_t>(esm) + logic::ESM_VEHICLE_FLAGS + 4, second);
            uint32_t takes = logic::vehicle_flags(player, second);
            auto **lists = reinterpret_cast<void **>(esm + LISTS);
            for (int64_t i = 0; i < count; ++i) {
                uint8_t id = 0;
                if (!leaderboard_of(lists[i], id) || id != board)
                    continue;
                pools |= takes & (logic::TAKES_POOL_A | logic::TAKES_POOL_B);
                if (!(takes & logic::TAKES_LIST) && lists[i] != reinterpret_cast<void *>(selected))
                    continue;
                size_t at_found = found.size();
                for (size_t f = 0; f < found.size(); ++f)
                    if (found[f].list == lists[i])
                        at_found = f;
                if (at_found == found.size()) {
                    found.push_back({lists[i], {}});
                    addon.push_back(false);
                }
                found[at_found].slots.push_back({esm, size_t(i)});
                addon[at_found] = addon[at_found] || host;
            }
        }
    }
    std::vector<logic::TtList> lists;
    for (size_t f = 0; f < found.size(); ++f) {
        const char *name = name_of(found[f].list, LIST_NAME);
        logic::TtList l;
        l.name = name ? name : "?";
        l.selected = f == 0;
        l.addon = !l.selected && (addon[f] || box_list(l.name));
        l.refs = vehicle_refs(found[f].list);
        lists.push_back(std::move(l));
    }
    if (pools) {
        logic::TtList pool;
        pool.name = "entrants system pools";
        pool.fixed = true;
        pool.refs = pool_refs(pools);
        lists.push_back(std::move(pool));
    }
    auto budget = logic::time_trial_budget(lists);
    size_t unrecorded = 0;
    for (size_t d : budget.dropped)
        for (const auto &slot : found[d].slots)
            if (!logic::cut_list(picker.narrowed, picker.narrowed_count, std::size(picker.narrowed), slot.setup, found[d].list))
                unrecorded++;
    if (unrecorded)
        swofty::log("[era] Time Trial cut skipped in %zu entrant setup mode(s): the core's restore stack of %zu is full, "
                    "so those lists stay offered",
                    unrecorded, std::size(picker.narrowed));
    time_trial_guard.budget = int64_t(budget.cars + logic::TT_OWN_ENTRIES);
    if (lines.budget.note())
        swofty::log("[era] %s", logic::time_trial_budget_line(lists, budget, board).c_str());
}

}
