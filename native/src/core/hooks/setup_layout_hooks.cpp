#include "core/hooks/setup_layout_hooks.hpp"

#include "core/apply/content/setup_layout.hpp"
#include "core/hooks/context.hpp"
#include "core/hooks/hooks.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"

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
// group's shared range key when every row of that group agrees. The layout comes from SWOFTY_SETUP_LAYOUT_FILE (a
// JSON file with the `setup.layout` record, the registry declaration is open) and the probe from
// SWOFTY_SETUP_LAYOUT_LOG=1.

namespace {

namespace c = swofty_core::apply::content;
namespace hooks = swofty_core::hooks;

const runtime::Sig S_BUILD{"Game_CarSetupVm_BuildSetupGroupList",
                           "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 08 02 00 00 C5 F8 29 B4 24 F0 01 00 00 44 89 C3 48",
                           0xc14a30, runtime::Sig::addr, 0, 0, 0};
const runtime::Sig S_POPULATE{"Game_CarSetupVm_PopulateGarageMonitorSettings", "41 57 41 56 41 55 41 54 56 57 55 53 B8 58 77 00",
                              0xc17b80, runtime::Sig::addr, 0, 0, 0};
const runtime::Sig S_CELLS{"site_Game_CarSetupVm_SliderCells",
                           "48 8D 05 ?? ?? ?? ?? 48 8B 04 F0 48 8B 00 C5 F8 57 C0 C5 F0 57 C9 48 85 C0 74 05 C5 FA 10 48 08 48 8D 05 "
                           "?? ?? ?? ?? 48 8B 04 F0 48 8B 00 48 85 C0 74 05 C5 FA 10 40 08 48 8D 05 ?? ?? ?? ?? 48 8B 04 F0",
                           0xc19a90, runtime::Sig::addr, 0, 0, 0};

safetyhook::InlineHook h_build, h_populate;
std::mutex g_lock;
bool g_armed, g_probe, g_cells_ok;
uintptr_t g_max, g_min, g_steps;

struct Entry {
    c::Layout layout;
    std::shared_ptr<hooks::Selectors> within;
};
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

const swofty::json::Value &member(const swofty::json::Value &v, const char *k)
{
    static const swofty::json::Value none;
    return v.is_object() ? v[k] : none;
}

bool parse_layout(const std::string &id, const swofty::json::Value &rec, Entry &out)
{
    out.layout.id = id;
    const auto &sections = member(rec, "sections");
    if (!sections.is_array())
        return false;
    for (size_t i = 0; i < sections.size(); i++) {
        const auto &s = sections[i];
        c::LayoutSection sec;
        const auto &tab = member(s, "tab");
        if (tab.is_string())
            sec.tab = c::tab_index(*tab.if_string());
        else if (tab.is_number())
            sec.tab = int(tab.as_int(-1));
        if (const auto *t = member(s, "title").if_string())
            sec.title = *t;
        const auto &rows = member(s, "rows");
        for (size_t j = 0; rows.is_array() && j < rows.size(); j++) {
            const auto &r = rows[j];
            c::LayoutRow row;
            const auto &slider = member(r, "slider");
            row.slider = slider.is_string() ? c::slider_index(*slider.if_string()) : int(slider.as_int(-1));
            if (const auto *n = member(r, "name").if_string())
                row.name = *n;
            if (const auto *n = member(r, "desc").if_string())
                row.desc = *n;
            if (const auto *n = member(r, "unit").if_string())
                row.unit = *n;
            if (member(r, "min").is_number() && member(r, "max").is_number()) {
                row.min = float(member(r, "min").as_double());
                row.max = float(member(r, "max").as_double());
            }
            if (member(r, "steps").is_number())
                row.steps = int(member(r, "steps").as_int());
            sec.rows.push_back(std::move(row));
        }
        out.layout.sections.push_back(std::move(sec));
    }
    auto v = c::check_layout(out.layout);
    if (!v.ok) {
        swofty::log("[setups] layout %s refused: %s", id.c_str(), v.why.c_str());
        return false;
    }
    const auto &within = member(rec, "within");
    if (!within.is_null()) {
        swofty::json::Value list = within.is_array() ? within : swofty::json::Value::array();
        if (!within.is_array())
            list.push_back(within);
        auto sel = std::make_shared<hooks::Selectors>();
        std::string why;
        if (!hooks::Selectors::compile(swofty::json::dump(list), *sel, &why)) {
            swofty::log("[setups] layout %s refused: its within does not compile (%s)", id.c_str(), why.c_str());
            return false;
        }
        if (!sel->empty())
            out.within = std::move(sel);
    }
    return true;
}

void load_layouts()
{
    const char *path = std::getenv("SWOFTY_SETUP_LAYOUT_FILE");
    if (!path || !*path)
        return;
    std::ifstream f(path, std::ios::binary);
    std::stringstream ss;
    ss << f.rdbuf();
    if (!f || ss.str().empty()) {
        swofty::log("[setups] layout file %s cannot be read", path);
        return;
    }
    swofty::json::Error err;
    auto doc = swofty::json::parse(ss.str(), &err);
    const auto &all = member(doc, "setup.layout");
    if (!all.is_object()) {
        swofty::log("[setups] layout file %s has no setup.layout record", path);
        return;
    }
    for (const auto &kv : *all.if_object()) {
        Entry e;
        if (parse_layout(kv.first, kv.second, e))
            g_layouts.push_back(std::move(e));
    }
    swofty::log("[setups] %zu setup layout(s) from %s", g_layouts.size(), path);
}

int wanted()
{
    auto facts = hooks::core().current();
    if (!facts)
        return -1;
    for (int i = int(g_layouts.size()) - 1; i >= 0; i--)
        if (!g_layouts[i].within || g_layouts[i].within->match(*facts))
            return i;
    return -1;
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
        swofty::log("[setups] layout %s left: the setup cells and texts are the game's again", g_layouts[g_active].layout.id.c_str());
    }
    g_active = want;
    if (want < 0)
        return;
    const auto &l = g_layouts[want].layout;
    g_switch.enter(l, read_cells, write_cells);
    put_strings(l);
    swofty::log("[setups] layout %s entered: %zu cell swap(s), %zu text(s)", l.id.c_str(), g_switch.saved(), g_keys.size());
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
        refresh_locked();
        if (g_probe) {
            auto facts = hooks::core().current();
            swofty::log("[setup-layout] setup screen opens: scope %s, %zu layout(s) known, facts %s",
                        g_active >= 0 ? g_layouts[g_active].layout.id.c_str() : "none", g_layouts.size(),
                        facts ? hooks::summary(*facts).c_str() : "none");
            probe_cells("populate");
        }
    }
    return h_populate.call<uint64_t>(vm);
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
    load_layouts();
    if (g_layouts.empty() && !g_probe)
        return;
    uintptr_t cells = runtime::find(S_CELLS, "setups", "setups");
    uintptr_t build = runtime::find(S_BUILD, "setups", "setups");
    uintptr_t populate = runtime::find(S_POPULATE, "setups", "setups");
    if (!build || !populate || !cells) {
        swofty::log("[setups] layout: %s did not resolve on this build; the setup screen stays the game's",
                    !build ? S_BUILD.what : !populate ? S_POPULATE.what : S_CELLS.what);
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
    g_armed = true;
    swofty::log("[setups] layout hooks armed: %zu layout(s), probe %s", g_layouts.size(), g_probe ? "on" : "off");
}

}
