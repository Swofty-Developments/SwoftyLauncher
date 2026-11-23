#include "core/hooks/setup_layout_hooks.hpp"

#include "core/frontend/vm_string.hpp"

#include "core/apply/content/setup_layout_parse.hpp"
#include "core/registry/table.hpp"
#include "core/hooks/context.hpp"
#include "core/hooks/hooks.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <windows.h>

#include <safetyhook.hpp>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <sstream>
#include <string>
#include <vector>

// The per-scope car setup screen (docs/reference/std/setup-layout.md), as far as the hooks go.
//
// Three game functions, found by byte scan on Steam 1.0.145 (build 26566308), where the KB addresses are not valid:
//   Game_CarSetupVm_BuildSetupGroupList   (disk 0x140c154f0): rcx vm, rdx u32 slider indexes, r8d count, r9 the
//                                         group list node, a fifth stack argument the tab slot 0 to 7. The group array
//                                         of a tab is replaced with the layout's rows for that tab.
//   Game_CarSetupVm_PopulateGarageMonitorSettings (disk 0x140c18640): one entry node per slider from the global
//                                         car_setup.sliders cells. A scope's layout is entered just before it runs.
//   the min, max and steps cells (rip-relative leas inside that function): three arrays of 42 handles, value at
//                                         [[cell + 8 * slider] + 8].
// Each is looked up on its own. One that does not resolve logs a line and leaves the stock screen (a layout is then
// never entered), and the layout is left again, with its cells and strings put back, whenever the scope's facts stop
// matching (checked at every populate and at every Car_Setup_ApplyTuning, so a session in another mode restores before
// its cars are tuned). A row's name goes on the game's own text key for that slider (the lang override), the unit on the
// group's shared range key when every row of that group agrees. The layouts are the registry's setup.layout records, and SWOFTY_SETUP_LAYOUT_FILE (a
// JSON file with the `setup.layout` record, read in addition to the registry's, for development) and the probe from
// SWOFTY_SETUP_LAYOUT_LOG=1.

namespace {

namespace c = swofty_core::apply::content;
namespace hooks = swofty_core::hooks;

const runtime::Sig S_BUILD{"Game_CarSetupVm_BuildSetupGroupList",
                           "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 08 02 00 00 C5 F8 29 B4 24 F0 01 00 00 44 89 C3 48",
                           0xc14a30,
                           runtime::Sig::addr,
                           0,
                           0,
                           0};
const runtime::Sig S_POPULATE{"Game_CarSetupVm_PopulateGarageMonitorSettings",
                              "41 57 41 56 41 55 41 54 56 57 55 53 B8 58 77 00",
                              0xc17b80,
                              runtime::Sig::addr,
                              0,
                              0,
                              0};
const runtime::Sig S_CELLS{
    "site_Game_CarSetupVm_SliderCells",
    "48 8D 05 ?? ?? ?? ?? 48 8B 04 F0 48 8B 00 C5 F8 57 C0 C5 F0 57 C9 48 85 C0 74 05 C5 FA 10 48 08 48 8D 05 "
    "?? ?? ?? ?? 48 8B 04 F0 48 8B 00 48 85 C0 74 05 C5 FA 10 40 08 48 8D 05 ?? ?? ?? ?? 48 8B 04 F0",
    0xc19a90,
    runtime::Sig::addr,
    0,
    0,
    0};

const runtime::Sig S_VALUE_CHANGED{"Car_Setup_OnValueChanged",
                                   "41 57 41 56 56 57 55 53 48 81 EC 08 01 00 00 C5 F8 29 B4 24 F0 00 00 00 83 7A 08 02 0F 84 ?? "
                                   "?? ?? ?? 48 89 CE 48 8B 42 10 4C 8B 00 B9 E0 08 00 00 BA 02 00 00 00 B8 20 06 00 00 41",
                                   0xc1e690,
                                   runtime::Sig::addr,
                                   0,
                                   0,
                                   0};

safetyhook::InlineHook h_build, h_populate, h_changed;
std::mutex g_lock;
bool g_armed, g_probe, g_cells_ok;
uintptr_t g_max, g_min, g_steps;

using Entry = c::ParsedLayout;
std::vector<Entry> g_layouts;
int g_active = -1;
c::ScopeSwitch g_switch;
std::vector<std::string> g_keys;
std::set<std::string> g_said;

void say_once(const std::string &line)
{
    if (g_said.insert(line).second)
        swofty::log("%s", line.c_str());
}

bool cell(uintptr_t base, int slider, uintptr_t &value)
{
    uintptr_t handle = 0, node = 0, at = base + uintptr_t(slider) * 8;
    if (!swofty::mem::read(at, handle) || !handle || !swofty::mem::read(handle, node) || !node)
        return false;
    value = node + 8;
    return swofty::mem::readable(value, 4);
}

bool read_cells(int slider, float &mn, float &mx, int &steps)
{
    uintptr_t a = 0, b = 0, d = 0;
    if (!cell(g_min, slider, a) || !cell(g_max, slider, b) || !cell(g_steps, slider, d))
        return false;
    return swofty::mem::read(a, mn) && swofty::mem::read(b, mx) && swofty::mem::read(d, steps);
}

void write_cells(int slider, float mn, float mx, int steps)
{
    uintptr_t a = 0, b = 0, d = 0;
    if (!cell(g_min, slider, a) || !cell(g_max, slider, b) || !cell(g_steps, slider, d))
        return;
    std::memcpy(reinterpret_cast<void *>(a), &mn, sizeof mn);
    std::memcpy(reinterpret_cast<void *>(b), &mx, sizeof mx);
    std::memcpy(reinterpret_cast<void *>(d), &steps, sizeof steps);
}

bool add_layout(const std::string &id, const swofty::json::Value &rec, const char *from)
{
    Entry e;
    std::string why;
    if (!c::parse_setup_layout(id, rec, e, &why)) {
        say_once("[setups] layout " + id + " (" + from + ") refused: " + why);
        return false;
    }
    g_layouts.push_back(std::move(e));
    return true;
}

void load_layouts()
{
    g_layouts.clear();
    auto &store = swofty_core::registry::store();
    for (const std::string &id : store.list("setup.layout", "")) {
        auto rec = store.get("setup.layout", id);
        if (rec.status == swofty_core::registry::Status::ok)
            add_layout(id, rec.value, "registry");
    }
    size_t from_registry = g_layouts.size();
    const char *path = std::getenv("SWOFTY_SETUP_LAYOUT_FILE");
    if (path && *path) {
        std::ifstream f(path, std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        swofty::json::Error err;
        auto doc = ss.str().empty() ? swofty::json::Value() : swofty::json::parse(ss.str(), &err);
        const auto &all = doc.is_object() ? doc["setup.layout"] : doc;
        if (!all.is_object())
            swofty::log("[setups] layout file %s cannot be read or has no setup.layout record", path);
        else
            for (const auto &kv : *all.if_object())
                add_layout("file/" + kv.first, kv.second, "SWOFTY_SETUP_LAYOUT_FILE");
    }
    say_once("[setups] " + std::to_string(g_layouts.size()) + " setup layout(s): " + std::to_string(from_registry) +
             " from the registry, " + std::to_string(g_layouts.size() - from_registry) + " from SWOFTY_SETUP_LAYOUT_FILE");
}

int wanted()
{
    auto facts = hooks::core().current();
    return c::pick_layout(g_layouts, facts.get());
}

void put_strings(const c::Layout &l)
{
    auto *lang = runtime::lang_table();
    if (!lang || !lang->available || !lang->available()) {
        say_once("[setups] layout: the text override is not available; names and units stay the game's");
        return;
    }
    std::map<std::string, std::set<std::string>> units;
    for (const auto &s : l.sections)
        for (const auto &r : s.rows) {
            const char *k = c::name_key(r.slider);
            if (*k && !r.name.empty() && lang->override_set("*", k, r.name.c_str()) == 0)
                g_keys.push_back(k);
            else if (!*k)
                say_once("[setups] layout: slider " + std::to_string(r.slider) + " has no text key of its own; its name stays");
            const char *u = c::unit_key(r.slider);
            if (*u && !r.unit.empty())
                units[u].insert(r.unit);
        }
    for (const auto &[key, set] : units) {
        if (set.size() != 1) {
            say_once("[setups] layout: the rows sharing " + key + " give different units; the game's unit stays");
            continue;
        }
        if (lang->override_set("*", key.c_str(), set.begin()->c_str()) == 0)
            g_keys.push_back(key);
    }
}

void drop_strings()
{
    auto *lang = runtime::lang_table();
    if (lang && lang->override_clear)
        for (const auto &k : g_keys)
            lang->override_clear("*", k.c_str());
    g_keys.clear();
}

void probe_cells(const char *when)
{
    if (!g_probe)
        return;
    for (int i = 0; i < c::SETUP_SLIDER_COUNT; i++) {
        float mn = 0, mx = 0;
        int st = 0;
        if (!read_cells(i, mn, mx, st)) {
            swofty::log("[setup-layout] %s slider %d: cells unreadable", when, i);
            continue;
        }
        swofty::log("[setup-layout] %s slider %d key %s min %g max %g steps %d", when, i, c::name_key(i), mn, mx, st);
    }
}

void refresh_locked()
{
    if (!g_armed || !g_cells_ok)
        return;
    int want = wanted();
    if (want == g_active)
        return;
    if (g_active >= 0) {
        g_switch.leave(write_cells);
        drop_strings();
        swofty::log("[setups] layout %s left: the setup cells and texts are the game's again",
                    g_layouts[g_active].layout.id.c_str());
    }
    g_active = want;
    if (want < 0)
        return;
    const auto &l = g_layouts[want].layout;
    g_switch.enter(l, read_cells, write_cells);
    put_strings(l);
    swofty::log("[setups] layout %s entered: %zu cell swap(s), %zu text(s)", l.id.c_str(), g_switch.saved(), g_keys.size());
}

void reload_locked()
{
    if (g_active >= 0) {
        g_switch.leave(write_cells);
        drop_strings();
        g_active = -1;
    }
    load_layouts();
}

constexpr uintptr_t VM_ENTRY_FIRST = 0x360, VM_ENTRY_GROUP = 0x840, VM_ENTRY_STRIDE = 0x2c0, VM_SETUP = 0x268;
constexpr uintptr_t SETUP_STEPS = 0x70, SETUP_SLOT_SIZE = 0x550;

bool entry_parent(void *vm, int slider, void *&parent)
{
    uintptr_t at =
        uintptr_t(vm) + VM_ENTRY_FIRST + uintptr_t(slider / 3) * VM_ENTRY_GROUP + uintptr_t(slider % 3) * VM_ENTRY_STRIDE;
    uintptr_t node = 0;
    if (!swofty::mem::read(at, node) || !node)
        return false;
    parent = reinterpret_cast<void *>(node + 8);
    return true;
}

bool stored_step(void *vm, int slider, int &step)
{
    uintptr_t setup = 0;
    uint8_t slot = 0;
    if (!swofty::mem::read(uintptr_t(vm) + VM_SETUP, setup) || !setup || !swofty::mem::read(setup + 1, slot))
        return false;
    return swofty::mem::read(setup + SETUP_STEPS + uintptr_t(slider) * 4 + uintptr_t(slot) * SETUP_SLOT_SIZE, step);
}

void put_row_texts(void *vm)
{
    if (g_active < 0 || !vm)
        return;
    void *reg = *swofty::game::g_Vm_Registry;
    if (!reg)
        return;
    for (const auto &sec : g_layouts[g_active].layout.sections)
        for (const auto &r : sec.rows) {
            if (!c::controls_text(r))
                continue;
            float mn = 0, mx = 0;
            int game_steps = 0, step = 0;
            void *parent = nullptr;
            if (!read_cells(r.slider, mn, mx, game_steps) || !entry_parent(vm, r.slider, parent) ||
                !stored_step(vm, r.slider, step))
                continue;
            int steps = game_steps > 0 ? game_steps : 1;
            uint8_t no = 0;
            std::string value = c::value_text(r, step, steps), range = c::range_text_of(r, steps);
            if (!value.empty())
                frontend::vm::set_string(reg, parent, "value_str", value.c_str());
            swofty::game::Vm_Set_Bool(reg, parent, "range_localise", &no, 1);
            frontend::vm::set_string(reg, parent, "range_desc", range.c_str());
            if (g_probe)
                swofty::log("[setup-layout] slider %d step %d/%d text '%s' range '%s'", r.slider, step, steps, value.c_str(),
                            range.c_str());
        }
}

void d_changed(void *vm, void *change)
{
    h_changed.call<void>(vm, change);
    std::lock_guard<std::mutex> g(g_lock);
    put_row_texts(vm);
}

uint64_t d_build(void *vm, uint32_t *idx, uint32_t count, void *node, uint32_t tab)
{
    std::lock_guard<std::mutex> g(g_lock);
    thread_local uint32_t buf[c::LAYOUT_TAB_ROWS + 2];
    uint32_t *use = idx;
    uint32_t n = count;
    bool replaced = false;
    if (g_active >= 0 && tab < uint32_t(c::LAYOUT_TABS)) {
        auto rows = c::tab_indexes(g_layouts[g_active].layout, int(tab));
        n = uint32_t(rows.size());
        for (uint32_t i = 0; i < n; i++)
            buf[i] = rows[i];
        use = buf;
        replaced = true;
    }
    if (g_probe) {
        std::string was, now;
        for (uint32_t i = 0; i < count && idx; i++)
            was += (i ? "," : "") + std::to_string(idx[i]);
        for (uint32_t i = 0; i < n; i++)
            now += (i ? "," : "") + std::to_string(use[i]);
        swofty::log("[setup-layout] tab %u (%s) scope %s: game [%s] -> %s [%s]", tab, tab < 8 ? c::TAB_NAMES[tab] : "?",
                    g_active >= 0 ? g_layouts[g_active].layout.id.c_str() : "none", was.c_str(), replaced ? "layout" : "game",
                    now.c_str());
    }
    return h_build.call<uint64_t>(vm, use, n, node, tab);
}

uint64_t d_populate(void *vm)
{
    {
        std::lock_guard<std::mutex> g(g_lock);
        reload_locked();
        refresh_locked();
        if (g_probe) {
            auto facts = hooks::core().current();
            swofty::log("[setup-layout] setup screen opens: scope %s, %zu layout(s) known, facts %s",
                        g_active >= 0 ? g_layouts[g_active].layout.id.c_str() : "none", g_layouts.size(),
                        facts ? hooks::summary(*facts).c_str() : "none");
            probe_cells("populate");
        }
    }
    uint64_t r = h_populate.call<uint64_t>(vm);
    std::lock_guard<std::mutex> g(g_lock);
    put_row_texts(vm);
    return r;
}

}

namespace swofty_core::setup_layout_hooks {

void refresh()
{
    if (!g_armed)
        return;
    std::lock_guard<std::mutex> g(g_lock);
    refresh_locked();
}

void arm()
{
    char probe[8] = {};
    g_probe = GetEnvironmentVariableA("SWOFTY_SETUP_LAYOUT_LOG", probe, sizeof probe) && probe[0] && probe[0] != '0';
    uintptr_t cells = runtime::find(S_CELLS, "setups", "setups");
    uintptr_t build = runtime::find(S_BUILD, "setups", "setups");
    uintptr_t populate = runtime::find(S_POPULATE, "setups", "setups");
    uintptr_t changed = runtime::find(S_VALUE_CHANGED, "setups", "setups");
    if (!build || !populate || !cells) {
        swofty::log("[setups] layout: %s did not resolve on this build; the setup screen stays the game's", !build ? S_BUILD.what
                                                                                                            : !populate
                                                                                                                ? S_POPULATE.what
                                                                                                                : S_CELLS.what);
        return;
    }
    auto rel = [&](size_t off) {
        int32_t d = 0;
        if (!swofty::mem::read(cells + off + 3, d))
            return uintptr_t(0);
        return cells + off + 7 + d;
    };
    g_max = rel(0);
    g_min = rel(0x20);
    g_steps = rel(0x38);
    if (!g_max || !g_min || !g_steps || g_min != g_max + 0x150 || g_steps != g_min + 0x150) {
        swofty::log("[setups] layout: the slider cells are not three arrays 0x150 apart (max %p min %p steps %p); the setup "
                    "screen stays the game's",
                    reinterpret_cast<void *>(g_max), reinterpret_cast<void *>(g_min), reinterpret_cast<void *>(g_steps));
        return;
    }
    g_cells_ok = true;
    auto b = runtime::loader::install(h_build, build, S_BUILD.what, reinterpret_cast<void *>(&d_build));
    auto p = runtime::loader::install(h_populate, populate, S_POPULATE.what, reinterpret_cast<void *>(&d_populate));
    if (!b || !p) {
        swofty::log("[setups] layout: a hook failed (%s); the setup screen stays the game's", (!b ? b.error : p.error).c_str());
        h_build = {};
        h_populate = {};
        return;
    }
    if (changed) {
        auto ch = runtime::loader::install(h_changed, changed, S_VALUE_CHANGED.what, reinterpret_cast<void *>(&d_changed));
        if (!ch) {
            swofty::log("[setups] layout: the value-change hook failed (%s); row texts refresh only when the screen opens",
                        ch.error.c_str());
            h_changed = {};
        }
    } else
        swofty::log("[setups] layout: %s did not resolve; row texts refresh only when the screen opens", S_VALUE_CHANGED.what);
    g_armed = true;
    swofty::log("[setups] layout hooks armed: layouts are read when the setup screen opens, probe %s", g_probe ? "on" : "off");
}

}
