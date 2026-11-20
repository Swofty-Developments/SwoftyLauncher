#include "core/boxes/picker/picker.hpp"

#include "core/runtime/runtime.hpp"

#include <iterator>

namespace era_picker {

// Virtual entrant lists. The game loads the rows of entrants.entrant_lists up to the table's count as list objects,
// and a list's stat id is a byte (255) while an entrant setup mode holds 32 lists. A box can instead name a list
// whose rows lie above the count: the game never loads it, so it takes no id and no slot. On a press the core reads
// those rows with the game's own Sess_Parse_EntrantList (0x142453c40: int *ordinal, row key, list, driver
// registry; the row key is a GdbTableCursor {container char[256] at +0, row char[256] at +0x100, count at +0x200}
// that it joins as container.row; it pushes 0x88 EntrantRecords into list+8 = {allocator, begin, end, capacity}) and
// swaps the result into the host list VIRTUAL_HOST, which is in swofty_f1's entrant lists like any box list. The
// next press, or a press of a stock box, swaps the host's own entrants back. Parsed vectors stay cached per row
// path for the session's life: the game may still hold records of a list it copied, so nothing is freed.

constexpr size_t KEY_ROW = 0x100, KEY_COUNT = 0x200, KEY_SIZE = 0x210;

void parse_list_entry(int *ordinal, const void *row_key, void *list, void *registry)
{
    if (registry)
        virtual_lists.boot_registry = registry;
    virtual_lists.parse_list_orig(ordinal, row_key, list, registry);
}

// The row of entrants.entrant_lists labelled `label` that the game did not load: the build's virtual-list step puts
// those rows at VIRTUAL_BASE and up (packages/core/src/games/f1_25/game_db/virtual_lists.rs, BASE).
constexpr int64_t VIRTUAL_BASE = 10000;
std::string virtual_row(const char *label)
{
    const SwoftyDb *db = runtime::db_table();
    if (!label || !db || !db->available || !db->available())
        return {};
    for (int64_t i = VIRTUAL_BASE; i < VIRTUAL_BASE + 4096; ++i) {
        std::string row = "entrants.entrant_lists.entrant_list_" + std::to_string(i);
        const char *name = nullptr;
        if (db->string_at((row + "._metadata_").c_str(), &name) != SWOFTY_ASSET_OK || !name)
            break;
        if (!strncmp(name, label, 128))
            return row;
    }
    return {};
}

void unfill_virtual()
{
    if (!filled.host)
        return;
    memcpy(static_cast<uint8_t *>(filled.host) + LIST_VECTOR, filled.own.data(), LIST_VECTOR_SIZE);
    filled = {};
}

// Parses `row` (once) and swaps its entrants into `host`; false leaves the host as it was.
bool fill_virtual(void *host, const std::string &row)
{
    if (!host || !virtual_lists.parse_list_orig)
        return false;
    auto it = virtual_lists.parsed_lists.find(row);
    if (it == virtual_lists.parsed_lists.end()) {
        uintptr_t slot = swofty::api->resolve("g_Ne_DefaultAllocator"), heap = 0;
        if (!slot || !mem::read(slot, heap) || !heap)
            return false;
        alignas(16) uint8_t list[0x40] = {};
        alignas(16) uint8_t no_registry[0x20] = {}; // read as an empty {_, begin +8, end +0x10} vector
        memcpy(list + LIST_VECTOR, &heap, sizeof heap);
        // row is "entrants.entrant_lists.entrant_list_<n>": container and row name of the key the parser joins
        alignas(16) uint8_t key[KEY_SIZE] = {};
        size_t dot = row.rfind('.');
        if (dot == std::string::npos || dot >= KEY_ROW || row.size() - dot - 1 >= KEY_ROW)
            return false;
        memcpy(key, row.data(), dot);
        memcpy(key + KEY_ROW, row.data() + dot + 1, row.size() - dot - 1);
        int32_t uncounted = -1;
        memcpy(key + KEY_COUNT, &uncounted, sizeof uncounted);
        key[KEY_COUNT + 4] = 1;
        int ordinal = 1;
        void *registry = virtual_lists.boot_registry.load();
        virtual_lists.parse_list_orig(&ordinal, key, list, registry ? registry : no_registry);
        uintptr_t begin = 0, end = 0;
        memcpy(&begin, list + LIST_VECTOR + 8, 8);
        memcpy(&end, list + LIST_VECTOR + 16, 8);
        if (!begin || end <= begin)
            return false;
        std::array<uint8_t, LIST_VECTOR_SIZE> v{};
        memcpy(v.data(), list + LIST_VECTOR, LIST_VECTOR_SIZE);
        it = virtual_lists.parsed_lists.emplace(row, v).first;
    }
    unfill_virtual();
    filled.host = host;
    filled.row = row;
    memcpy(filled.own.data(), static_cast<uint8_t *>(host) + LIST_VECTOR, LIST_VECTOR_SIZE);
    memcpy(static_cast<uint8_t *>(host) + LIST_VECTOR, it->second.data(), LIST_VECTOR_SIZE);
    return true;
}

// The list a press of `label` selects in the game: VIRTUAL_HOST for a virtual list, else the label itself.
std::string pressed_list(const char *label)
{
    if (!label)
        return {};
    if (!virtual_lists.parse_list_bound || virtual_row(label).empty())
        return label;
    return VIRTUAL_HOST;
}

void restore_lists()
{
    unfill_virtual();
    logic::put_back_cuts(picker.narrowed, picker.narrowed_count, false, put_back_failed);
}

void restore()
{
    restore_lists();
    put_back_drs();
}

void narrow(void *mode, const char *list)
{
    restore_lists();
    std::string row = virtual_lists.parse_list_bound ? virtual_row(list) : std::string();
    const char *want = row.empty() ? list : VIRTUAL_HOST;
    for_each_host_setup(mode, [&](uint8_t *setup) {
        int64_t count = at<int64_t>(setup, LIST_COUNT);
        if (count <= 1 || count > int64_t(LIST_SLOTS) || picker.narrowed_count == std::size(picker.narrowed))
            return;
        auto **lists = reinterpret_cast<void **>(setup + LISTS);
        void *hit = nullptr;
        for (int64_t i = 0; i < count && !hit; ++i) {
            const char *name = name_of(lists[i], LIST_NAME);
            if (name && !strcmp(name, want))
                hit = lists[i];
        }
        if (!hit)
            return;
        if (!row.empty() && filled.host != hit) {
            bool ok = fill_virtual(hit, row);
            if (virtual_lists.virtual_lines.note())
                swofty::log("[era] %s pressed: virtual list %s %s %s", list, row.c_str(),
                            ok ? "read into" : "could not be read into", VIRTUAL_HOST);
            if (!ok)
                return;
            filled.label = list;
        }
        Narrowed &n = picker.narrowed[picker.narrowed_count++];
        n.setup = setup;
        n.count = count;
        n.kept = 1;
        n.time_trial = false;
        n.grid = {};
        std::string grid = n.grid.raise(setup, list_cars(hit).cars);
        if (!grid.empty())
            swofty::log("[era] %s pressed: %s", list, grid.c_str());
        n.kept_lists[0] = hit;
        memcpy(n.lists, lists, size_t(count) * sizeof(void *));
        lists[0] = hit;
        ref<int64_t>(setup, LIST_COUNT) = 1;
        if (picker.lines.narrow.note())
            swofty::log("[era] %s offers only %s (of %lld lists)", logic::HOST, list, (long long)count);
    });
    apply_drs(mode, list);
    apply_warmers(mode, list);
    apply_tyre_change(mode, list);
    apply_fuel(list);
    apply_weekend(mode, list);
    // After the weekend exchange: the points swap edits the race sessions the host mode holds now.
    apply_points(mode, list);
    apply_box_options(list);
    apply_grid(list);
    apply_quali_tyres(list);
}

// Ends the Time Trial hold, putting the Time Trial cuts back before the swap (logic::end_hold); `cuts` puts them back
// even without a swap held.
logic::CutsPutBack put_back(bool cuts)
{
    continue_grid.restore();
    return logic::end_hold(picker.narrowed, picker.narrowed_count, hold.held, hold.holding, cuts, put_back_failed);
}

}
