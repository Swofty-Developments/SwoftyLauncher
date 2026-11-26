#include "core/guards/skips.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/hook_set.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <set>
#include <string>
#include <vector>

#include "core/boxes/car_category_logic.hpp"
#include "core/boxes/options_store.hpp"
#include "core/frontend/chosen_tiles.hpp"
#include "core/frontend/tile_records_read.hpp"
#include "core/hooks/ui_points.hpp"

// LAN Car Category addon boxes. The host's Car Category screen after the LAN Game dialog is the F1 World
// container's tab session_create_dlc_select_tab (CreateSessionDLCSelectTabViewModel). Its tile build
// (Game_CreateSessionDlcSelectTab_BuildTiles) fills two fixed tiles in a tile set at vm+0x630: the VM list
// buttons, then F1 2025 (vm+0x650) and F1 2026 (vm+0x6d0), each a child appended with Vm_List_AppendChild
// and filled with Game_CreateSessionDlcSelectTab_FillTile. Its command handler takes button_1 (F1 2025:
// disc content, route open_create_custom_25), button_2 (F1 2026) and back_pressed. The lobby reads the
// car category from the options store's category option in setupFromUnranked
// (Game_Network_CategoryOptionUsing), whose value names are the multiplayer category values addons add
// (Classic Seasons adds cs_f1_2016 and the like, with a scenario of the same name).
//
// Build (wrapped): after the stock build, every era box (swofty/era.hpp) whose entrant list is a category
// value, up to the launcher UI build's slots (buttons.entry_2 to entry_5, shown by the addon_select_N buttons
// it adds to the template, which raise button_3 to button_6), gets a child appended to buttons and filled
// into core-owned tile storage, which holds four VM
// handles for the view model's life. Before the next build those handles are unlinked and released the way
// the game's tile set clear releases its own (a handle the game already cleared is skipped). The log
// records the buttons child count before and after and every added box, which with a look at the screen
// says whether it spawns one tile per child or shows its two fixed slots.
//
// Press (wrapped): button_<3 + i> is added box i. It remembers that box's category value and runs the stock
// F1 2025 path (button_1). Other commands go to the stock handler; one it does not know is logged.
//
// setupFromUnranked (wrapped): the original resets the session (scenario 0), forces the category option to
// the index the content domain gives (3, f1, on the F1 2025 path), reads it and plays the scenario of that
// name, so an index set before it does not last. With a remembered value, on a game that decides the
// scenario (session role offline or host), after the original the category option's index is set to it
// through the option's vtable (+0x10 get, +0x18 set) and Game_Network_SetScenario plays the scenario of the
// value's name (Classic Seasons' cs_f1_2016 is stat id 15, setup mode swofty_f1). That call sets the setup
// mode and entrants, marks the session dirty and, on the host, sends the scenario to the players. The
// previous index comes back at the next build of the screen or a press on a stock box, if the option still
// holds ours.
//
// Every hook falls back to the stock behaviour when a shape is not what it expects.

namespace {

namespace mem = swofty::mem;
namespace logic = runtime::car_category;
namespace store = runtime::options_store;
using store::text_at;
using store::vfunc;
using swofty::game::Game_CreateSessionDlcSelectTab_BuildTiles_t;
using swofty::game::Game_CreateSessionDlcSelectTab_FillTile_t;
using swofty::game::Game_CreateSessionDlcSelectTab_HandleCommand_t;
using swofty::game::Game_Network_CategoryOptionUsing_t;
using swofty::game::Game_Network_SetScenario_t;
using swofty::game::Vm_List_AppendChild_t;
using swofty::game::Vm_Release_Node_t;

constexpr const char *MIXIN = "swofty_core.dll (car category)";
constexpr uint32_t LOG_FIRST = 8;

constexpr size_t VM_TILE_SET = 0x630;
constexpr size_t VM_LIST_NODE = 0x638;
constexpr size_t VM_TILE_A = 0x650;
constexpr size_t VM_TILE_B = 0x6d0;
constexpr size_t VM_SPAN = 0x7a8;
constexpr size_t TILE_BYTES = 0x80;
constexpr size_t TILE_HANDLES = 4;
constexpr size_t HANDLE_WRAPPER = 0x20;
constexpr size_t VM_ROUTER = 0x620;
constexpr size_t ROUTER_FIRE_SLOT = 16;
constexpr uint32_t ROUTE_ARG_KIND = 4;
constexpr const char *STOCK_IMAGE = "#overrides.home_screen.tiles_multiplayer_6x12[create_dlc]";

constexpr size_t SELF_NETWORK = 0x10;
constexpr size_t NETWORK_SESSION = 0x418;
constexpr size_t SESSION_ROLE = 8;
constexpr size_t SESSION_SPEC = 0x18;
constexpr size_t SESSION_SPEC_SET = 0x1c;
constexpr size_t SESSION_SCENARIO = 0x410;
constexpr size_t SESSION_SPAN = 0x770;
constexpr size_t SCENARIOS_BEGIN = 0x68;
constexpr size_t SCENARIOS_END = 0x70;
constexpr size_t SCENARIO_BYTES = 0x50;
constexpr size_t SCENARIO_NAME_HOLDER = 8;
constexpr size_t HOLDER_NAME = 0x10;
constexpr size_t MAX_SCENARIOS = 512;
constexpr uint32_t DEFAULT_SPEC = 0x3fff0000;
constexpr size_t MAX_CHILDREN = 4096;

struct Bound {
    uintptr_t make_str, build, press, fill, append, release, setup, find, store, key, registry, set_scenario, scenarios;
} b;

struct Added {
    std::string list, value, route;
    uint8_t *node;
};

struct Forced {
    void *option = nullptr;
    int prior = -1, ours = -1;
};

// The Car Category set: the tile build, the button press and the category setup from the unranked lobby.
struct CategoryHooks : runtime::loader::HookSet<std::recursive_mutex> {
    runtime::loader::Original<Game_CreateSessionDlcSelectTab_BuildTiles_t> build_tiles;
    runtime::loader::Original<Game_CreateSessionDlcSelectTab_HandleCommand_t> handle_command;
    runtime::loader::Original<Game_Network_CategoryOptionUsing_t> option_using;
};

CategoryHooks hooks;
alignas(16) uint8_t tiles[logic::MAX_ADDED][TILE_BYTES];
size_t tiles_used;
std::vector<Added> added;
uint8_t *built_vm;
std::string chosen;
Forced forced;
constinit swofty_core::guards::LogFirst<LOG_FIRST> build_lines, value_lines, refused_lines, setup_lines, scenario_lines;
constinit swofty_core::guards::LogFirst<16> unknown_lines;
constinit swofty_core::guards::LogFirst<LOG_FIRST> record_lines, image_lines, route_lines;
std::set<std::string> told_slices;
thread_local bool applying;

template <class T> T at(const void *base, size_t off)
{
    T v{};
    memcpy(&v, static_cast<const uint8_t *>(base) + off, sizeof v);
    return v;
}

void *registry() { return *reinterpret_cast<void **>(b.registry); }

size_t children(uint8_t *list)
{
    if (!mem::readable(list, 0x88))
        return SIZE_MAX;
    if (auto *vector_list = vfunc<char(void *)>(list, 2); vector_list && vector_list(list)) {
        auto *begin = at<uint8_t *>(list, 0x40), *end = at<uint8_t *>(list, 0x48);
        return end >= begin && size_t(end - begin) / 8 <= MAX_CHILDREN ? size_t(end - begin) / 8 : SIZE_MAX;
    }
    if (auto *counted = vfunc<char(void *)>(list, 5); counted && counted(list)) {
        auto n = at<uint64_t>(list, 0x80);
        return n <= MAX_CHILDREN ? size_t(n) : SIZE_MAX;
    }
    return 0;
}

void *category_option(std::string &why)
{
    void *option = nullptr;
    switch (store::find(b.find, *reinterpret_cast<void **>(b.store), "category", option)) {
    case store::Found::store_unreadable:
        why = "the options store is not readable";
        break;
    case store::Found::no_option:
        why = "the options store has no category option";
        break;
    case store::Found::option_unreadable:
        why = "the category option is not readable";
        break;
    case store::Found::ok:
        break;
    }
    return option;
}

std::vector<logic::Value> values_of(void *option) { return store::values(option, reinterpret_cast<const uint8_t *>(b.key)); }

void restore_forced(const char *why)
{
    if (!forced.option)
        return;
    int now = -1;
    std::string err;
    void *option = category_option(err);
    if (option == forced.option && store::get_index(option, now) && now == forced.ours &&
        store::set_index(option, forced.prior)) {
        int back = -1;
        store::get_index(option, back);
        swofty::log("[car-category] category option index %d -> %d (restored, %s)", now, back, why);
    } else {
        swofty::log("[car-category] category option left as it is (%s): %s", why,
                    option != forced.option ? "the option object changed" : "it no longer holds the forced index");
    }
    forced = {};
}

void release_tiles()
{
    size_t released = 0;
    for (size_t t = 0; t < tiles_used; ++t)
        for (size_t h = 0; h < TILE_HANDLES; ++h) {
            auto &handle = *reinterpret_cast<runtime::era::Handle *>(tiles[t] + h * HANDLE_WRAPPER + 8);
            released +=
                logic::unlink(handle, [](uint8_t *node) { reinterpret_cast<Vm_Release_Node_t *>(b.release)(registry(), node); });
        }
    if (tiles_used)
        swofty::log("[car-category] released %zu handle(s) of %zu added tile(s) before the rebuild", released, tiles_used);
    tiles_used = 0;
    added.clear();
    built_vm = nullptr;
}

std::string hex_hash(uint32_t h)
{
    char s[16];
    snprintf(s, sizeof s, "0x%08x", h);
    return s;
}

void add_record_tiles(uint8_t *v, Vm_List_AppendChild_t *append, Game_CreateSessionDlcSelectTab_FillTile_t *fill)
{
    namespace tr = frontend::tile_records;
    tr::Options opt;
    opt.need_route = true;
    opt.cap = logic::records_room(tiles_used);
    auto found = tr::read(tr::Place::car_category, opt);
    for (auto &line : found.lines)
        if (auto n = record_lines.note())
            swofty::log("[ui.tile] %s%s", line.c_str(), n.tail ? n.tail : "");
    {
        frontend::chosen_tiles::Chosen chosen{found.tiles, opt.cap, tr::Place::car_category};
        swofty_core::hooks::ui_points::car_category_tiles(&chosen);
        std::vector<std::string> told;
        size_t dropped = frontend::chosen_tiles::settle(found.tiles, opt.cap, tr::store_route_exists(), &told);
        for (auto &line : told)
            if (auto n = record_lines.note())
                swofty::log("[ui.tile] %s%s", line.c_str(), n.tail ? n.tail : "");
        if (dropped)
            swofty::log(
                "[ui.tile] car_category: %zu tile(s) a ui.car_category.tiles handler left over the %zu slot(s) or repeated "
                "are dropped",
                dropped, opt.cap);
    }
    size_t images = 0, shown = 0;
    for (auto &t : found.tiles) {
        void **node = append(v + VM_TILE_SET);
        if (!node || !mem::readable(node, 8)) {
            swofty::log("[ui.tile] car_category: appending a child to buttons gave %p; %s and the tiles after it are not shown",
                        static_cast<void *>(node), t.id.c_str());
            break;
        }
        auto image = tr::resolve_image(t, tr::store_slices());
        if (std::string warn = tr::missing_slice_warning(t, image, tr::store_provided(), told_slices); !warn.empty())
            swofty::log("[ui.tile] car_category: %s", warn.c_str());
        std::string path = STOCK_IMAGE;
        if (image.kind == tr::Image::Kind::slice)
            path = logic::slice_path(image.array, image.name), images++;
        else if (image.kind == tr::Image::Kind::unresolved)
            if (auto n = image_lines.note())
                swofty::log("[ui.tile] car_category: %s: %s; the tile shows the F1 2026 tile's image%s", t.id.c_str(),
                            image.why.c_str(), n.tail ? n.tail : "");
        auto title = tr::title_text(t);
        std::string command = "goto_" + t.id;
        memset(tiles[tiles_used], 0, TILE_BYTES);
        fill(tiles[tiles_used], node, title.text, command.c_str(), path.c_str());
        added.push_back({t.id, {}, t.route, at<uint8_t *>(tiles[tiles_used], 8)});
        tiles_used++;
        shown++;
        swofty::log("[ui.tile] car_category: tile %s added (press command button_%zu = %s, route %s, image %s)", t.id.c_str(),
                    logic::STOCK_BOXES + added.size(), hex_hash(logic::button_hash(logic::STOCK_BOXES + added.size())).c_str(),
                    t.route.c_str(), path.c_str());
    }
    if (found.tiles.size() || found.capped || found.refused)
        swofty::log("[ui.tile] car_category: %zu of %zu tile record(s) added after F1 2026 and the era boxes (%zu gated out, "
                    "%zu refused, %zu over the %zu slot(s) left of %zu)",
                    shown, found.tiles.size(), found.gated_out, found.refused, found.capped, opt.cap, logic::MAX_ADDED);
    if (shown && shown > images)
        if (auto n = route_lines.note())
            swofty::log("[ui.tile] car_category: %zu tile(s) have no usable image and show the F1 2026 tile's image%s",
                        shown - images, n.tail ? n.tail : "");
}

void open_route(void *vm, const std::string &route)
{
    auto *router = mem::readable(vm, VM_ROUTER + 8) ? at<void *>(vm, VM_ROUTER) : nullptr;
    auto *fire = router && mem::readable(router, 8) ? vfunc<void(void *, void *, void *)>(router, ROUTER_FIRE_SLOT) : nullptr;
    if (!fire) {
        swofty::log("[ui.tile] car_category: route %s not opened: the screen's router is not readable", route.c_str());
        return;
    }
    swofty::game::NeStr *name = nullptr;
    reinterpret_cast<swofty::game::Ne_Make_StrFromCStr_t *>(b.make_str)(&name, route.c_str(), 0, 0);
    struct {
        uint32_t kind = ROUTE_ARG_KIND;
        uint32_t pad = 0;
        uint64_t holder = 0;
    } arg;
    fire(router, &name, &arg);
}

void build_entry(void *vm)
{
    auto *v = static_cast<uint8_t *>(vm);
    {
        std::lock_guard<std::recursive_mutex> g(hooks.lock);
        release_tiles();
        restore_forced("the Car Category screen was built again");
        chosen.clear();
    }
    hooks.build_tiles(vm);
    if (!hooks.live() || hooks.off() || !mem::readable(v, VM_SPAN))
        return;
    std::lock_guard<std::recursive_mutex> g(hooks.lock);
    uint8_t *list = at<uint8_t *>(v, VM_LIST_NODE);
    size_t before = children(list);
    if (before == SIZE_MAX) {
        swofty::log("[car-category] the buttons list at vm+0x638 (%p) has a shape this core does not know; no boxes added", list);
        return;
    }
    std::string why;
    void *option = category_option(why);
    std::vector<logic::Value> values = option ? values_of(option) : std::vector<logic::Value>{};
    if (value_lines.note()) {
        std::string listed;
        for (size_t i = 0; i < values.size(); ++i)
            listed += (i ? ", " : "") + std::to_string(i) + "=" + values[i].name + " (" + values[i].display + ")";
        int index = -1;
        bool got = option && store::get_index(option, index);
        swofty::log("[car-category] category option %p: %zu value(s): %s; current index %s%s", option, values.size(),
                    listed.empty() ? "none" : listed.c_str(), got ? std::to_string(index).c_str() : "unknown",
                    option ? "" : (" (" + why + ")").c_str());
    }
    auto *append = reinterpret_cast<Vm_List_AppendChild_t *>(b.append);
    auto *fill = reinterpret_cast<Game_CreateSessionDlcSelectTab_FillTile_t *>(b.fill);
    for (const auto &box : runtime::era_boxes()) {
        int value = logic::value_of(values, box.entrant_list);
        if (value < 0) {
            swofty::log("[car-category] box %s from %s skipped: no category value has that game name", box.entrant_list.c_str(),
                        box.owner.c_str());
            continue;
        }
        if (tiles_used >= logic::MAX_ADDED) {
            swofty::log("[car-category] box %s from %s skipped: the screen has %zu addon box slots", box.entrant_list.c_str(),
                        box.owner.c_str(), logic::MAX_ADDED);
            continue;
        }
        void **node = append(v + VM_TILE_SET);
        if (!node || !mem::readable(node, 8)) {
            swofty::log("[car-category] appending a child to buttons gave %p; no more boxes", static_cast<void *>(node));
            break;
        }
        std::string command = "goto_" + box.entrant_list, image = runtime::era::image_of(box);
        memset(tiles[tiles_used], 0, TILE_BYTES);
        fill(tiles[tiles_used], node, box.title.c_str(), command.c_str(), image.c_str());
        auto *child = at<uint8_t *>(tiles[tiles_used], 8);
        added.push_back({box.entrant_list, values[size_t(value)].name, {}, child});
        tiles_used++;
        swofty::log("[car-category] added box %zu (press command button_%zu = %s): %s from %s, value %d, title %s, "
                    "image %s, child node %p",
                    added.size() - 1, logic::STOCK_BOXES + added.size(),
                    hex_hash(logic::button_hash(logic::STOCK_BOXES + added.size())).c_str(), box.entrant_list.c_str(),
                    box.owner.c_str(), value, box.title.c_str(), image.c_str(), child);
    }
    add_record_tiles(v, append, fill);
    built_vm = added.empty() ? nullptr : v;
    size_t after = children(list);
    if (build_lines.note())
        swofty::log("[car-category] Car Category built: view model %p, buttons list %p, %zu child(ren) before and %zu "
                    "after; stock tiles on nodes %p (F1 2025) and %p (F1 2026); %zu addon box(es) added",
                    vm, list, before, after, at<void *>(v, VM_TILE_A + 8), at<void *>(v, VM_TILE_B + 8), added.size());
}

char *press_entry(void *vm, void *command)
{
    auto *cmd = command ? *static_cast<const uint32_t **>(command) : nullptr;
    if (!cmd || !mem::readable(cmd, 4))
        return hooks.handle_command(vm, command);
    uint32_t hash = *cmd;
    std::unique_lock<std::recursive_mutex> g(hooks.lock);
    if (hash == logic::BUTTON_1 || hash == logic::BUTTON_2) {
        if (!chosen.empty() || forced.option)
            swofty::log("[car-category] stock box %s pressed; the addon category is dropped",
                        hash == logic::BUTTON_1 ? "F1 2025" : "F1 2026");
        chosen.clear();
        restore_forced("a stock box was pressed");
        g.unlock();
        return hooks.handle_command(vm, command);
    }
    int box = logic::added_box_of(hash, added.size());
    if (box < 0) {
        if (hash != logic::BACK && unknown_lines.note())
            swofty::log("[car-category] command %s on the Car Category screen is not a stock or addon box (%zu addon box(es))",
                        hex_hash(hash).c_str(), added.size());
        g.unlock();
        return hooks.handle_command(vm, command);
    }
    if (vm != built_vm) {
        if (refused_lines.note())
            swofty::log("[car-category] addon box press refused: the screen that raised it is not the one the boxes were "
                        "built for");
        g.unlock();
        return hooks.handle_command(vm, command);
    }
    if (!added[size_t(box)].route.empty()) {
        std::string route = added[size_t(box)].route;
        chosen.clear();
        restore_forced("a tile record was pressed");
        g.unlock();
        swofty::log("[ui.tile] car_category: tile %s pressed: opening route %s", added[size_t(box)].list.c_str(), route.c_str());
        open_route(vm, route);
        return nullptr;
    }
    chosen = added[size_t(box)].value;
    swofty::log("[car-category] addon box %d pressed: category %s; running the F1 2025 path (button_1)", box, chosen.c_str());
    g.unlock();
    uint32_t stock = logic::BUTTON_1;
    const void *fake[4] = {&stock, nullptr, nullptr, nullptr};
    return hooks.handle_command(vm, fake);
}

std::vector<logic::Scenario> scenarios()
{
    std::vector<logic::Scenario> out;
    void *registry = *reinterpret_cast<void **>(b.scenarios);
    if (!mem::readable(registry, SCENARIOS_END + 8))
        return out;
    auto *e = at<uint8_t *>(registry, SCENARIOS_BEGIN), *end = at<uint8_t *>(registry, SCENARIOS_END);
    if (!e || end < e || size_t(end - e) % SCENARIO_BYTES || size_t(end - e) / SCENARIO_BYTES > MAX_SCENARIOS ||
        !mem::readable(e, size_t(end - e)))
        return out;
    for (; e < end; e += SCENARIO_BYTES) {
        auto *holder = at<uint8_t *>(e, SCENARIO_NAME_HOLDER);
        std::string name = mem::readable(holder, HOLDER_NAME + 8) ? text_at(at<const char *>(holder, HOLDER_NAME)) : "";
        out.push_back({e[0], name});
    }
    return out;
}

// Puts the category option on the remembered value after setupFromUnranked forced its own. True when the
// option holds it.
bool hold_category(const std::string &want)
{
    std::string why;
    void *option = category_option(why);
    std::vector<logic::Value> values = option ? values_of(option) : std::vector<logic::Value>{};
    int target = logic::value_of(values, want), before = -1, after = -1;
    if (!option) {
        swofty::log("[car-category] setupFromUnranked: category %s not set: %s", want.c_str(), why.c_str());
        return false;
    }
    if (target < 0) {
        swofty::log("[car-category] setupFromUnranked: category %s not set: no value has that name (%zu value(s))", want.c_str(),
                    values.size());
        return false;
    }
    if (!store::get_index(option, before) || before < 0 || size_t(before) >= values.size()) {
        swofty::log("[car-category] setupFromUnranked: category %s not set: the option's current index is %d of %zu",
                    want.c_str(), before, values.size());
        return false;
    }
    if (before == target)
        return true;
    if (!store::set_index(option, target) || !store::get_index(option, after) || after != target) {
        swofty::log("[car-category] setupFromUnranked: setting the category option to %d (%s) read back %d; putting %d "
                    "back",
                    target, want.c_str(), after, before);
        store::set_index(option, before);
        return false;
    }
    if (!forced.option)
        forced = {option, before, target};
    swofty::log("[car-category] setupFromUnranked: category option index %d (%s) -> %d (%s)", before,
                values[size_t(before)].name.c_str(), after, want.c_str());
    return true;
}

// Plays the scenario of the remembered value's name on the session setupFromUnranked just set up.
void play_scenario(void *self, const std::string &want)
{
    auto *network = mem::readable(self, SELF_NETWORK + 8) ? at<uint8_t *>(self, SELF_NETWORK) : nullptr;
    auto *session = mem::readable(network, NETWORK_SESSION + 8) ? at<uint8_t *>(network, NETWORK_SESSION) : nullptr;
    if (!mem::readable(session, SESSION_SPAN)) {
        swofty::log("[car-category] setupFromUnranked: scenario for %s not played: the session is not readable", want.c_str());
        return;
    }
    int role = at<int>(session, SESSION_ROLE);
    if (!logic::decides_scenario(role)) {
        if (scenario_lines.note())
            swofty::log("[car-category] setupFromUnranked: session role %d takes the host's scenario", role);
        return;
    }
    std::vector<logic::Scenario> all = scenarios();
    int stat = logic::scenario_of(all, want);
    if (stat < 0) {
        swofty::log("[car-category] setupFromUnranked: scenario for %s not played: no scenario has that name (%zu "
                    "scenario(s))",
                    want.c_str(), all.size());
        return;
    }
    uint8_t now = at<uint8_t>(session, SESSION_SCENARIO);
    if (now == uint8_t(stat))
        return;
    uint32_t spec = at<uint8_t>(session, SESSION_SPEC_SET) ? at<uint32_t>(session, SESSION_SPEC) : DEFAULT_SPEC;
    reinterpret_cast<Game_Network_SetScenario_t *>(b.set_scenario)(session, uint8_t(stat), &spec);
    swofty::log("[car-category] setupFromUnranked: scenario %u -> %d (%s), now %u, session role %d%s", now, stat, want.c_str(),
                at<uint8_t>(session, SESSION_SCENARIO), role, role == 1 ? "; the host sends it to the players" : "");
}

void setup_entry(void *self)
{
    if (applying) {
        hooks.option_using(self);
        return;
    }
    hooks.option_using(self);
    std::unique_lock<std::recursive_mutex> g(hooks.lock);
    std::string want = chosen;
    if (want.empty()) {
        if (forced.option && setup_lines.note())
            swofty::log("[car-category] setupFromUnranked with no addon category chosen; the forced index stays until "
                        "the next build or stock press");
        return;
    }
    applying = true;
    if (hold_category(want))
        play_scenario(self, want);
    applying = false;
}

bool bind_all(std::string &why)
{
    return CategoryHooks::bind({{"Game_CreateSessionDlcSelectTab_BuildTiles", &b.build},
                                {"Game_CreateSessionDlcSelectTab_HandleCommand", &b.press},
                                {"Game_CreateSessionDlcSelectTab_FillTile", &b.fill},
                                {"Vm_List_AppendChild", &b.append},
                                {"Ne_Make_StrFromCStr", &b.make_str},
                                {"Vm_Release_Node", &b.release},
                                {"Game_Network_CategoryOptionUsing", &b.setup},
                                {"Game_OptionsStore_FindOption", &b.find},
                                {"g_Game_OptionsStore", &b.store},
                                {"k_Gdb_StringXorKey", &b.key},
                                {"g_Vm_Registry", &b.registry},
                                {"Game_Network_SetScenario", &b.set_scenario},
                                {"g_Game_Scenarios", &b.scenarios}},
                               why);
}

}

namespace runtime {

// Names the category value the next setupFromUnranked applies, for a host that skips the Car Category screen
// (the session host's car_category): what a press on an addon box remembers. An empty value drops any earlier choice, so a
// stock host starts from the stock index. False, with the reason, when the value cannot be applied.
bool car_category_choose(const std::string &value, std::string &why)
{
    std::lock_guard<std::recursive_mutex> g(hooks.lock);
    if (value.empty()) {
        chosen.clear();
        restore_forced("a stock category was chosen without the screen");
        return true;
    }
    if (!hooks.live() || hooks.off()) {
        why = "the addon category " + value + " needs the Car Category hooks, which are " +
              (hooks.off() ? "off: " + hooks.why() : std::string("not installed"));
        return false;
    }
    std::string err;
    void *option = category_option(err);
    if (!option) {
        why = "the addon category " + value + " cannot be checked: " + err;
        return false;
    }
    if (logic::value_of(values_of(option), value) < 0) {
        why = "no category value is named " + value;
        return false;
    }
    chosen = value;
    swofty::log("[car-category] category %s chosen without the Car Category screen", value.c_str());
    return true;
}

static std::vector<CoreMixins> car_category_mixins()
{
    std::vector<CoreMixins> out;
    if (hooks.gate.offered() || hooks.off() || !code_ready() ||
        (era_boxes().empty() && !frontend::tile_records::has_place(frontend::tile_records::Place::car_category)))
        return out;
    hooks.gate.offer();
    std::string why;
    if (!bind_all(why)) {
        hooks.turn_off(why);
        swofty::log("[car-category] addon boxes off: %s", why.c_str());
        return out;
    }
    out.push_back(
        {MIXIN,
         {{"Game_CreateSessionDlcSelectTab_BuildTiles", SWOFTY_MIXIN_WRAP, 100, (void *)&build_entry, hooks.build_tiles.slot()},
          {"Game_CreateSessionDlcSelectTab_HandleCommand", SWOFTY_MIXIN_WRAP, 100, (void *)&press_entry,
           hooks.handle_command.slot()},
          {"Game_Network_CategoryOptionUsing", SWOFTY_MIXIN_WRAP, 100, (void *)&setup_entry, hooks.option_using.slot()}}});
    swofty::log("[car-category] %zu era box(es) registered; offering the Car Category hooks", era_boxes().size());
    return out;
}

static void car_category_commit()
{
    if (hooks.off() || !hooks.gate.settle())
        return;
    if (core_mixins_active(MIXIN)) {
        hooks.set_live();
        swofty::log("[car-category] Car Category hooks installed; addon boxes show from the next build of the screen");
        return;
    }
    hooks.turn_off("the Car Category hooks did not install (see the [core] lines for " + std::string(MIXIN) + ")");
    swofty::log("[car-category] addon boxes off: %s", hooks.why().c_str());
}

static loader::FunctionSet car_category_core_set{loader::CoreSetId::car_category, car_category_mixins, car_category_commit};

}
