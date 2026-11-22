#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <utility>
#include <vector>

#include "core/apply/facts.hpp"
#include "core/boxes/era/box.hpp"

// Time Trial: a session holds the cars of the entrant list it was picked with plus three cars of its own,
// and the session system that walks them (0x142f45960, called from Game_GameModule_OnSessionEnter) keeps
// them in a table of 26 at +0xb0 with its count right behind it at +0x180. A 22-car list gives 25; a
// 24-car list gives 27 and overwrites the count. A list object (Sess_Load_AvailableEntrantLists, copied
// by 0x1424df240) holds its entrants as 0x88-byte records from +0x10 to +0x18.
//
// The guard holds for any list, whichever box or addon made the setup select it. On a Time Trial picker,
// a press on a stock box leaves swofty_f1 offering only the lists of at most 23 cars, and when a Time Trial
// session loads (StateLoadSession) the core reads the setup's resolved list (setup +0xe18, the setup being
// *(*(mode context+0x88)+0x260), its entrant setup mode +0xe10, its game setup mode +0x2b0) and, when that
// list or the entrant setup mode's first list holds more than 23 cars, selects a list of at most 23 cars
// from the same entrant setup mode instead (the box pressed, then its Time Trial list, then the first that
// fits) and holds the entrant setup mode's max_entrants (+0x268, the selector's limit, research grid24-cap
// §2 step 6) at 23 until the session ends or a box is pressed. When the fill still selects more entrants than the
// table holds, the entrant cap (table_entrants below) leaves the lists' cars past the table out before the game builds
// its participants. As the last net, a wrap of 0x142f45960 (Sess_Setup_VehicleComponents) that sees more vehicles than the
// table's 26 hands the game a vector of the first 26 for that call and logs the list, so the table is never written past
// its end; the AI job (0x14063e370) still walks every vehicle and reads a null component for the 27th (V2134), so that
// net only turns a write past the table into a fault.

namespace runtime::era {

constexpr size_t TT_TABLE = 26;
// A raised grid stops at the stock 22 cars, the grid slots of the f1 and f1_26 worlds, for a list the core's boot
// lowering did not make. A box whose content.package seats more goes up to its seats, at most the 24 cars of My Team's
// stock grid: a package of 12 teams loads a My Team world (24 grid slots, 12 garages), its own season's or the box
// world the boot lowering gives it (box_world.hpp); the replay recorder guard keeps every copy within its holder's
// block (replay_grid_guard.cpp); and 24 cars with the safety car fit the 26-slot physics body registry
// (Car_Chassis_Build) that 27 overrun.
constexpr size_t GRID_RAISE_MAX = 22;
constexpr size_t PACKAGE_GRID_MAX = 24;
constexpr size_t TT_EXTRA_CARS = 3;
constexpr size_t TT_MAX_CARS = TT_TABLE - TT_EXTRA_CARS;
constexpr size_t LIST_ENTRANTS_BEGIN = 0x10;
constexpr size_t LIST_ENTRANTS_END = 0x18;
constexpr size_t ENTRANT_RECORD = 0x88;
constexpr size_t MAX_LIST_ENTRANTS = 64;

inline size_t grid_raise_cap(size_t package_seats)
{
    return package_seats > GRID_RAISE_MAX ? std::min(package_seats, PACKAGE_GRID_MAX) : GRID_RAISE_MAX;
}

inline int64_t entrant_count(uintptr_t begin, uintptr_t end)
{
    if (!begin || end < begin || (end - begin) % ENTRANT_RECORD || (end - begin) / ENTRANT_RECORD > MAX_LIST_ENTRANTS)
        return -1;
    return int64_t((end - begin) / ENTRANT_RECORD);
}

inline std::string time_trial_refusal(const Box &box, int64_t cars)
{
    if (cars < 0 || size_t(cars) <= TT_MAX_CARS)
        return {};
    std::string list = list_for(box, true);
    std::string why = "its Time Trial list " + list + " holds " + std::to_string(cars) + " cars; Time Trial holds at most " +
                      std::to_string(TT_MAX_CARS) + " (the game adds " + std::to_string(TT_EXTRA_CARS) +
                      " cars of its own to a table of " + std::to_string(TT_TABLE) + ")";
    if (box.time_trial.empty())
        why += ". Give the box a Time Trial entrant list of at most " + std::to_string(TT_MAX_CARS) + " cars";
    return why;
}

// The Time Trial guard. A list whose car count was read and is past TT_MAX_CARS is oversized; a count that
// could not be read (-1) is kept, so the guard never drops what it cannot see.
struct ListCars {
    std::string name;
    int64_t cars = -1;
};

inline bool oversized(int64_t cars) { return cars >= 0 && size_t(cars) > TT_MAX_CARS; }

inline std::vector<size_t> time_trial_keep(const std::vector<ListCars> &lists)
{
    std::vector<size_t> keep;
    for (size_t i = 0; i < lists.size(); ++i)
        if (!oversized(lists[i].cars))
            keep.push_back(i);
    return keep;
}

// A Time Trial session's cars come from more than its own list: the Time Trial mode's descriptor fill
// (0x142936760, vtable 0x1457b13d0 slot 0x80) walks every game setup mode of the mode (EntrantsManager slot
// 0x1d0), each one's entrant setup modes (slot 0x1d8) and their selectable lists (+0x130, count +0x230), takes
// every list whose leaderboard (the row's leaderboard descriptor stat id, optional byte +0xe0, engaged +0xf0)
// matches the selected list's, and pushes one entrant per distinct vehicle ref (EntrantRecord +0x5c) of them,
// plus 4 entries of its own. A stock F1 2025 Time Trial draws 11 cars (f1_custom_team, leaderboard f1) and
// spawns 15 vehicles. A list an addon copies from f1_2025 keeps leaderboard f1, so F1 2012 Season's
// bdx_s5_2012 (12 vehicles) made it 27, past the 26-slot vehicle table. A package's list has a descriptor of its
// own since 1.7.20 (content.package leaderboard), so only the lists that share one count together. A stock press in Time Trial narrows swofty_f1 whenever one of
// its lists is oversized, to no list at all when none fits; at each Time Trial load the core also counts the distinct cars of every list of the
// selected list's leaderboard and drops whole lists, addon lists first, until they fit 22 (26 less the 4).
inline bool time_trial_narrows(const std::vector<ListCars> &lists, const std::vector<size_t> &keep)
{
    return !lists.empty() && keep.size() != lists.size();
}

constexpr size_t LIST_LEADERBOARD = 0xe0;
constexpr size_t LIST_LEADERBOARD_SET = 0xf0;
constexpr size_t ENTRANT_VEHICLE = 0x5c;
constexpr size_t TT_OWN_ENTRIES = 4;
constexpr size_t TT_ROOM = TT_TABLE - TT_OWN_ENTRIES;

inline bool vehicle_ref_set(uint32_t ref) { return (ref >> 16) != 0x3fff; }

struct TtList {
    std::string name;
    bool selected = false, addon = false;
    std::vector<uint32_t> refs;
    bool fixed = false;
};

// The fill dedupes vehicle refs by their byte +0 and u16 +2 (0x1424a1c20), so byte +1 is not part of the key.
inline uint32_t ref_key(uint32_t ref) { return ref & 0xffff00ffu; }

// Entrant setup mode overrides (the game mode's EntrantSetupModeOverride rows): the fill gets each game setup mode's
// entrant setup modes through EntrantsManager slot 0x1d8 (body 0x1485b5280), which copies gsm +0x10 (count +0x70)
// and swaps every mode whose stat id byte +0 has an entry in the flat map at manager +0xfd8: slots of 0x10 bytes
// from +0xfe0 to the metadata bytes at +0xfe8 (key byte +0, override at +8), mask +0xff8 (0 = empty map). A slot is
// in use when its metadata byte is not 0; keys are unique, so a scan finds what the game's probe finds.
constexpr size_t MGR_OVERRIDES = 0xfe0;
constexpr size_t MGR_OVERRIDE_META = 0xfe8;
constexpr size_t MGR_OVERRIDE_MASK = 0xff8;
constexpr size_t OVERRIDE_SLOT = 0x10;
constexpr size_t MAX_OVERRIDE_SLOTS = 1024;

struct Override {
    bool found = false;
    const void *to = nullptr;
};

// The game installs a row's value even when it is null (0x1485b5280 writes it unchecked), so a found row with a
// null value leaves that entrant setup mode slot empty.
inline Override override_for(uint8_t id, const uint8_t *slots, const uint8_t *meta, size_t count)
{
    if (!slots || !meta || count > MAX_OVERRIDE_SLOTS)
        return {};
    for (size_t i = 0; i < count; ++i)
        if (meta[i] && slots[i * OVERRIDE_SLOT] == id)
            return {true, *reinterpret_cast<void *const *>(slots + i * OVERRIDE_SLOT + 8)};
    return {};
}

// 0x1424a1c20 takes a list's own records when the mode's flag word +0x258 + idx * 4 has bit 0 (idx 0 or 1 for the
// fill's human record), every ref of the entrants system pool +0x338 with bit 1 and of +0x348 with bit 2.
constexpr size_t ESM_VEHICLE_FLAGS = 0x258;
constexpr uint32_t TAKES_LIST = 1, TAKES_POOL_A = 2, TAKES_POOL_B = 4;
constexpr size_t ES_POOL_A = 0x338;
constexpr size_t ES_POOL_B = 0x348;
constexpr size_t POOL_SLOT = 0x10;
constexpr size_t MAX_POOL = 8192;

// An entrants system pool is indexed by vehicle stat id, so its length is the highest id + 1: 0x10-byte slots from
// begin to end, a pointer to the vehicle ref at +8 or 0 when the id is free. read(address, value) is a guarded read.
struct PoolRefs {
    std::vector<uint32_t> refs;
    bool capped = false;
};

template <class Read> PoolRefs pool_refs_from(uintptr_t begin, uintptr_t end, Read read)
{
    PoolRefs out;
    if (!begin || end < begin || (end - begin) % POOL_SLOT)
        return out;
    size_t slots = (end - begin) / POOL_SLOT;
    if (slots > MAX_POOL) {
        out.capped = true;
        slots = MAX_POOL;
    }
    for (size_t i = 0; i < slots; ++i) {
        uintptr_t at_ref = 0;
        uint32_t ref = 0;
        if (read(begin + i * POOL_SLOT + 8, at_ref) && at_ref && read(at_ref, ref))
            out.refs.push_back(ref);
    }
    return out;
}

inline uint32_t vehicle_flags(uint32_t player, uint32_t second_player) { return player | second_player; }

// A cut is made only where the restore stack can record it, or where an entry for that mode already does.
inline bool cut_recordable(size_t used, size_t capacity, bool known) { return known || used < capacity; }

struct TtBudget {
    std::vector<size_t> dropped;
    size_t cars = 0;
};

// The selected list always stays; the other lists of its leaderboard join in the order stock lists, then addon
// lists, each whole while the distinct vehicle refs of everything kept stay within room.
inline TtBudget time_trial_budget(const std::vector<TtList> &lists, size_t room = TT_ROOM)
{
    std::vector<size_t> order(lists.size());
    for (size_t i = 0; i < order.size(); ++i)
        order[i] = i;
    auto rank = [&](size_t i) { return lists[i].selected || lists[i].fixed ? 0 : lists[i].addon ? 2 : 1; };
    std::stable_sort(order.begin(), order.end(), [&](size_t a, size_t b) { return rank(a) < rank(b); });
    std::vector<uint32_t> have;
    TtBudget out;
    for (size_t i : order) {
        std::vector<uint32_t> next = have;
        for (uint32_t r : lists[i].refs)
            if (vehicle_ref_set(r) && std::find(next.begin(), next.end(), ref_key(r)) == next.end())
                next.push_back(ref_key(r));
        if (lists[i].selected || lists[i].fixed || next.size() <= room)
            have = std::move(next);
        else
            out.dropped.push_back(i);
    }
    std::sort(out.dropped.begin(), out.dropped.end());
    out.cars = have.size();
    return out;
}

inline size_t distinct_cars(const std::vector<uint32_t> &refs)
{
    std::vector<uint32_t> d;
    for (uint32_t r : refs)
        if (vehicle_ref_set(r) && std::find(d.begin(), d.end(), ref_key(r)) == d.end())
            d.push_back(ref_key(r));
    return d.size();
}

// Leaderboard descriptor stat ids of the stock leaderboards.leaderboard_descriptor rows, for the log only; an id past
// the stock rows is a descriptor an addon's package or database operations added.
inline std::string leaderboard_text(unsigned id)
{
    static constexpr const char *STOCK[] = {"f1", "classic", "f1_26", "f2", nullptr, "f2_26"};
    std::string out = std::to_string(id);
    if (id < std::size(STOCK) && STOCK[id])
        out += std::string(", ") + STOCK[id];
    else if (id >= std::size(STOCK))
        out += ", added";
    return out;
}

inline std::string time_trial_budget_line(const std::vector<TtList> &lists, const TtBudget &b, unsigned leaderboard)
{
    std::string selected = "?";
    size_t kept = 0, pool = 0;
    for (size_t i = 0; i < lists.size(); ++i) {
        if (lists[i].selected)
            selected = lists[i].name;
        if (lists[i].fixed)
            pool += distinct_cars(lists[i].refs);
        else if (std::find(b.dropped.begin(), b.dropped.end(), i) == b.dropped.end())
            kept++;
    }
    std::string line = "the Time Trial session's list " + selected + " (leaderboard " + leaderboard_text(leaderboard) +
                       ") draws " + std::to_string(b.cars) + " cars from " + std::to_string(kept) +
                       " list(s) of its leaderboard" +
                       (pool ? " and the entrants system pools (" + std::to_string(pool) + ")" : std::string()) +
                       "; with the game's " + std::to_string(TT_OWN_ENTRIES) + " entries " +
                       std::to_string(b.cars + TT_OWN_ENTRIES) + " of the " + std::to_string(TT_TABLE) + "-slot vehicle table";
    if (!b.dropped.empty()) {
        line += "; left out until the session unloads:";
        for (size_t k = 0; k < b.dropped.size(); ++k) {
            const auto &l = lists[b.dropped[k]];
            line += std::string(k ? "," : "") + " " + l.name + " (" + std::to_string(distinct_cars(l.refs)) + " cars" +
                    (l.addon ? "" : ", stock") + ")";
        }
    }
    return line;
}

inline int64_t time_trial_fallback(const std::vector<ListCars> &lists, const std::vector<std::string> &prefer)
{
    auto fits = [&](size_t i) { return lists[i].cars >= 0 && !oversized(lists[i].cars); };
    for (const auto &want : prefer)
        for (size_t i = 0; i < lists.size(); ++i)
            if (!want.empty() && lists[i].name == want && fits(i))
                return int64_t(i);
    for (size_t i = 0; i < lists.size(); ++i)
        if (fits(i))
            return int64_t(i);
    return -1;
}

// The index in lists of the list a Time Trial session loads after the 24-car guard, -1 while the session's own
// selected list stays. The leaderboard budget is computed for this list, not for the one selected before the swap.
inline int64_t time_trial_loaded(const ListCars &selected, const ListCars &first, const std::vector<ListCars> &lists,
                                 const std::vector<std::string> &prefer)
{
    if (!oversized(selected.cars) && !oversized(first.cars))
        return -1;
    return time_trial_fallback(lists, prefer);
}

constexpr size_t ENTRANT_SETUP_MAX_ENTRANTS = 0x268;

struct GridLimit {
    uint8_t *esm = nullptr;
    int32_t was = 0, set = 0;

    std::string raise(uint8_t *setup_mode, int64_t cars, size_t package_seats = 0)
    {
        restore();
        if (!setup_mode || cars <= 0)
            return "";
        int32_t limit = *reinterpret_cast<int32_t *>(setup_mode + ENTRANT_SETUP_MAX_ENTRANTS);
        if (limit <= 0 || cars <= limit)
            return "";
        size_t cap = grid_raise_cap(package_seats);
        bool clamped = size_t(cars) > cap;
        int32_t target = clamped ? int32_t(cap) : int32_t(cars);
        if (target <= limit)
            return "";
        esm = setup_mode;
        was = limit;
        set = target;
        *reinterpret_cast<int32_t *>(esm + ENTRANT_SETUP_MAX_ENTRANTS) = set;
        std::string line = "max_entrants " + std::to_string(was) + " -> " + std::to_string(set) + " for a list of " +
                           std::to_string(cars) + " cars";
        if (clamped)
            line += "; clamped to " + std::to_string(cap) + " cars, " +
                    (cap > GRID_RAISE_MAX ? "the stock My Team grid" : "the stock grid");
        return line;
    }

    void restore()
    {
        if (set && esm && *reinterpret_cast<int32_t *>(esm + ENTRANT_SETUP_MAX_ENTRANTS) == set)
            *reinterpret_cast<int32_t *>(esm + ENTRANT_SETUP_MAX_ENTRANTS) = was;
        esm = nullptr;
        was = set = 0;
    }
};

inline int32_t time_trial_limit(int32_t max_entrants)
{
    return max_entrants < 1 || size_t(max_entrants) > TT_MAX_CARS ? int32_t(TT_MAX_CARS) : max_entrants;
}

inline std::string time_trial_swap(const std::string &list, int64_t cars, const std::string &setup_mode,
                                   const std::string &instead, int32_t limit_was)
{
    std::string why = "the Time Trial session's list " + list + " (setup mode " + setup_mode + ") holds " + std::to_string(cars) +
                      " cars; Time Trial holds at most " + std::to_string(TT_MAX_CARS) + " (the game adds " +
                      std::to_string(TT_EXTRA_CARS) + " cars of its own to a table of " + std::to_string(TT_TABLE) + ")";
    if (!instead.empty())
        why += "; loading " + instead + " instead";
    else
        why += "; its entrant setup mode offers no list of at most " + std::to_string(TT_MAX_CARS) + " cars";
    if (limit_was)
        why += "; the session draws at most " + std::to_string(TT_MAX_CARS) + " cars (max_entrants " + std::to_string(limit_was) +
               " -> " + std::to_string(TT_MAX_CARS) + " until the session ends)";
    return why;
}

// The core's restore stack of entrant setup mode list arrays (the lists at +0x130, their count at +0x230). An addon
// box press narrows swofty_f1's modes to one list, a stock box press on a Time Trial picker keeps the lists past 23 cars
// out, and a Time Trial load leaves whole lists of its leaderboard out (time_trial set on the last two). Each entry
// holds the array before (lists, count) and after (kept_lists, kept); it is put back only while the mode still holds
// what the core left, so a list array changed since is not overwritten.
constexpr size_t ESM_LISTS = 0x130;
constexpr size_t ESM_LIST_COUNT = 0x230;
constexpr size_t ESM_LIST_SLOTS = 32;

inline void **esm_lists(uint8_t *esm) { return reinterpret_cast<void **>(esm + ESM_LISTS); }

inline int64_t &esm_list_count(uint8_t *esm) { return *reinterpret_cast<int64_t *>(esm + ESM_LIST_COUNT); }

struct ListCut {
    uint8_t *setup = nullptr;
    int64_t count = 0, kept = 0;
    void *lists[ESM_LIST_SLOTS] = {};
    void *kept_lists[ESM_LIST_SLOTS] = {};
    bool time_trial = false;
    GridLimit grid;
};

inline bool put_back_cut(ListCut &n)
{
    n.grid.restore();
    if (!n.setup || n.kept < 0 || n.count < 0 || size_t(n.kept) > ESM_LIST_SLOTS || size_t(n.count) > ESM_LIST_SLOTS ||
        esm_list_count(n.setup) != n.kept || memcmp(esm_lists(n.setup), n.kept_lists, size_t(n.kept) * sizeof(void *)))
        return false;
    memcpy(esm_lists(n.setup), n.lists, size_t(n.count) * sizeof(void *));
    esm_list_count(n.setup) = n.count;
    return true;
}

// The lists an entry left out of its mode, in the mode's order before the cut.
inline std::vector<void *> left_out(const ListCut &n)
{
    std::vector<void *> out;
    int64_t kept = std::clamp<int64_t>(n.kept, 0, int64_t(ESM_LIST_SLOTS));
    for (int64_t i = 0; i < n.count && size_t(i) < ESM_LIST_SLOTS; ++i)
        if (std::find(n.kept_lists, n.kept_lists + kept, n.lists[i]) == n.kept_lists + kept)
            out.push_back(n.lists[i]);
    return out;
}

struct CutsPutBack {
    size_t put = 0, failed = 0;
};

// Puts entries back from the top of the stack (only the Time Trial ones when time_trial_only) and drops them. An entry
// whose mode no longer holds what it left is handed to failed and is not counted as put back.
template <class Failed> CutsPutBack put_back_cuts(ListCut *stack, size_t &used, bool time_trial_only, Failed failed)
{
    CutsPutBack r;
    for (size_t i = used; i-- > 0;) {
        if (time_trial_only && !stack[i].time_trial)
            continue;
        if (put_back_cut(stack[i])) {
            r.put++;
        } else {
            r.failed++;
            failed(stack[i]);
        }
    }
    size_t keep = 0;
    if (time_trial_only)
        for (size_t i = 0; i < used; ++i)
            if (!stack[i].time_trial)
                stack[keep++] = stack[i];
    used = keep;
    return r;
}

// A Time Trial load's leaderboard cut of `list` out of `esm`. The first cut of a mode pushes an entry when the stack has
// room; a later one extends the Time Trial entry that already holds the mode. False when the stack is full: the list stays.
inline bool cut_list(ListCut *stack, size_t &used, size_t capacity, uint8_t *esm, void *list)
{
    int64_t count = esm_list_count(esm);
    if (count < 0 || size_t(count) > ESM_LIST_SLOTS)
        return false;
    bool known = false;
    for (size_t i = 0; i < used; ++i)
        known = known || (stack[i].time_trial && stack[i].setup == esm && stack[i].kept == count);
    if (!cut_recordable(used, capacity, known))
        return false;
    void **arr = esm_lists(esm);
    if (!known) {
        ListCut &n = stack[used++];
        n = {};
        n.setup = esm;
        n.count = count;
        memcpy(n.lists, arr, size_t(count) * sizeof(void *));
        n.time_trial = true;
        n.kept = count;
        memcpy(n.kept_lists, n.lists, sizeof(n.lists));
    }
    int64_t w = 0;
    for (int64_t i = 0; i < count; ++i)
        if (arr[i] != list)
            arr[w++] = arr[i];
    esm_list_count(esm) = w;
    for (size_t i = used; i-- > 0;)
        if (stack[i].time_trial && stack[i].setup == esm) {
            stack[i].kept = w;
            memcpy(stack[i].kept_lists, arr, size_t(w) * sizeof(void *));
            break;
        }
    return true;
}

// The 24-car guard's hold on a Time Trial session's entrant setup mode: the list of at most 23 cars it moved into slot 0
// (fit, from `slot`, where the first list went) and the max_entrants it lowered (limit_was, 0 when untouched).
struct TtHold {
    uint8_t *esm = nullptr;
    int32_t limit_was = 0;
    size_t slot = 0;
    void *first = nullptr;
    void *fit = nullptr;
};

inline void swap_into_first(TtHold &h, uint8_t *esm, size_t slot)
{
    void **arr = esm_lists(esm);
    h.esm = esm;
    h.slot = slot;
    h.first = arr[0];
    h.fit = arr[slot];
    arr[slot] = h.first;
    arr[0] = h.fit;
}

inline void put_back_hold(const TtHold &h)
{
    if (!h.esm)
        return;
    auto &limit = *reinterpret_cast<int32_t *>(h.esm + ENTRANT_SETUP_MAX_ENTRANTS);
    if (h.limit_was && limit == time_trial_limit(h.limit_was))
        limit = h.limit_was;
    void **arr = esm_lists(h.esm);
    if (h.first && h.slot < ESM_LIST_SLOTS && arr[0] == h.fit && arr[h.slot] == h.first) {
        arr[h.slot] = arr[0];
        arr[0] = h.first;
    }
}

// Ends the hold. The leaderboard cut of the same load was made on the swapped array and is recorded against it, so the
// Time Trial entries are put back first: undoing the swap first changes the array, and the cut lists would stay out.
// With `cuts` (the session unload) they are put back whether or not a swap is held; otherwise only while one is.
template <class Failed> CutsPutBack end_hold(ListCut *stack, size_t &used, TtHold &held, bool &holding, bool cuts, Failed failed)
{
    CutsPutBack r;
    if (cuts || (holding && held.first))
        r = put_back_cuts(stack, used, true, failed);
    if (holding) {
        holding = false;
        put_back_hold(held);
    }
    return r;
}

inline std::string put_back_failed_line(const std::vector<std::string> &names, int64_t now, int64_t kept, bool time_trial)
{
    std::string list;
    for (const auto &name : names)
        list += (list.empty() ? "" : ", ") + name;
    return std::string("the lists the ") + (time_trial ? "Time Trial guard" : "box narrowing") +
           " left out of an entrant setup mode could not be put back: its lists changed since (" + std::to_string(now) +
           " now, " + std::to_string(kept) + " left), so " + (list.empty() ? "none" : list) + " stay out of it";
}

constexpr size_t VEHICLE_SLOT = 8;

inline size_t session_vehicles(uintptr_t begin, uintptr_t end)
{
    return end > begin ? size_t(((end - begin) / VEHICLE_SLOT) & 0xff) : 0;
}

inline uintptr_t vehicles_end_within_table(uintptr_t begin, uintptr_t end)
{
    return session_vehicles(begin, end) > TT_TABLE ? begin + TT_TABLE * VEHICLE_SLOT : end;
}

// The Time Trial entrant cap. The fill (0x142936760) pushes its records through 0x140af2ab0: two sites before
// its setup mode walks (its human records carry +0x48 = 0x10000000, stored at 0x1429378f9 and 0x142938303), the lists'
// cars in the walk (0x142938e2b), and one record of its own after the walks (0x142939be8, straight-line code that every
// path to the loop passes, so always the last). At 0x142939dc2, inside its entrants-changing messages
// (0x1424e2b90 .. 0x1424e2f00), builds one participant per entrant whose vehicle ref (+0x5c) resolves. The session's
// vehicles are those participants, so entrants held to the table's 26 there keep every later reader (the vehicle setup's
// table, the AI update's cache 0x1405ee110 and its job 0x14063e370) within what the game holds. Records are erased the
// way the game's own selector erases them (V2126): later records copied down, then the end moved. The fill itself clears
// the vector by setting its end to its begin (0x1429367ed), so a record owns nothing that a copy or a drop would break.
constexpr size_t ENTRANT_HUMAN_BYTE = 0x4b;
constexpr uint8_t ENTRANT_HUMAN = 0x10;

// The entrants kept, ascending: every fixed one (a human record, or the fill's own last record), and the others in their
// order while the table has room.
inline std::vector<size_t> table_entrants(const std::vector<bool> &fixed, size_t table = TT_TABLE)
{
    size_t always = size_t(std::count(fixed.begin(), fixed.end(), true));
    size_t room = table > always ? table - always : 0;
    std::vector<size_t> keep;
    for (size_t i = 0; i < fixed.size(); ++i) {
        if (fixed[i]) {
            keep.push_back(i);
        } else if (room) {
            keep.push_back(i);
            --room;
        }
    }
    return keep;
}

// Moves the kept records to the front in their order; returns the new end.
inline uint8_t *keep_entrants(uint8_t *begin, const std::vector<size_t> &keep)
{
    for (size_t k = 0; k < keep.size(); ++k)
        if (keep[k] != k)
            std::memmove(begin + k * ENTRANT_RECORD, begin + keep[k] * ENTRANT_RECORD, ENTRANT_RECORD);
    return begin + keep.size() * ENTRANT_RECORD;
}

inline std::string entrant_cap_line(size_t entrants, size_t kept, size_t humans)
{
    return "the Time Trial session selected " + std::to_string(entrants) + " entrants for a vehicle table of " +
           std::to_string(TT_TABLE) + "; it keeps " + std::to_string(kept) + " (" + std::to_string(humans) +
           " human) and leaves " + std::to_string(entrants - kept) +
           " of the lists' cars out before the game builds its participants";
}
}
