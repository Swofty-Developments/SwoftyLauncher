#include "core/career/hub_tab.hpp"

#include "core/career/hub_state.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"
#include "core/registry/older.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/runtime/loader/core_rank.hpp"
#include "core/runtime/loader/hook_install.hpp"

#include <safetyhook.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cctype>
#include <cstring>
#include <map>
#include <string>
#include <vector>

// Team HQ hub tabs (swofty/hub.hpp, SwoftyHub). An addon's settings.json entry carries `hub_tab`
// {name, title, options, data_group} (from its manifest); careers filed under data_group get one more
// Team HQ laptop tab after STANDINGS with one page whose content template is swofty_hub_<name>.
//
// The laptop's tabs are built natively by Career_Build_HubLaptop (0x141c3f130) when the hub loads, one
// block per primary tab. A primary tab is allocated through the laptop's allocator (laptop +0x2a8,
// vt+0x18, 0xb68 bytes), constructed by 0x1414c0860 (tab, laptop+0x28, laptop+0x268, laptop, content
// "tabbing_secondary_controller", header "tabbing_secondary_tab", lng name, icon, 1) and pushed with its
// key onto the laptop's tab vector object (laptop +0x2b0 {allocator, begin +0x2b8, end +0x2c0,
// capacity}, 0x18-byte entries {u32 key, allocator, tab}) by 0x141e5dfb0, which takes the {allocator,
// tab} pair over. A page (sub-tab) is allocated through the tab's allocator (tab +0x270, 0x660 bytes),
// constructed by the tab view model constructor 0x1414bdac0 (page, laptop+0x28, laptop+0x268, laptop,
// content template, lng name, 0, "", page type, flag) and added by 0x1414c13e0 (tab, page). The STANDINGS
// block (0x141c420dc..0x141c4261e) is the pattern. The new tab is added at 0x141c42623, where every
// branch of the STANDINGS block joins and the builder starts registering each tab's view model node,
// so the new tab gets the same registration as the stock ones.
//
// The page is a plain tab view model whose vtable is copied with slot 10 (onCommandRaised) replaced:
// swofty_next:<addon>:<option> / swofty_prev:<addon>:<option> raised by its rows step the option the
// way ADDON SETTINGS does (the state's command through settings_command), anything else goes to the
// game's handler; the launcher's page raises them on its template root, so they reach this view model.
// The page's data is published at view_model_data.hud_swofty_hub_<name>: title, count and o<k>.label /
// value (the current state's text) / description, again after every change of one of its options. The
// tab's title is the lng key lng_swofty_hub_<name>, set to the title in capitals.

namespace career::hub {

namespace {

constexpr uintptr_t LAPTOP_ARG2 = 0x28, LAPTOP_ALLOC = 0x2a8, LAPTOP_ARG3 = 0x268, LAPTOP_TABS = 0x2b0, LAPTOP_TABS_END = 0x2c0,
                    TAB_ALLOC = 0x270;
constexpr size_t TAB_SIZE = 0xb68, PAGE_ALIGN = 8, VT_COPY = 64, VT_COMMAND = 10;
constexpr char ICON[] = "overrides.career.laptop.page_icons[4]";

const runtime::Sig S_RND_GATE{"laptop R&D gate", "80 38 00 0F 84 ?? ?? ?? ?? 4C 8B BE 68 02 00 00", 0x1c41bac};
const runtime::Sig S_TABS_END{
    "laptop tabs end", "48 89 DA E8 ?? ?? ?? ?? 4C 8B AE B8 02 00 00", 0x1c42623, runtime::Sig::addr, 0, 8, 0};
constexpr const char *P_STANDINGS = "E8 ?? ?? ?? ?? 48 89 5C 24 60 4C 89 6C 24 68 48 8D 94 24 B0 00 00 00 4C 8D 44 24 60 "
                                    "48 8B 8C 24 00 01 00 00 E8 ?? ?? ?? ?? 48 8B 86 C0 02 00 00 48 8B 68 F8";
const runtime::Sig S_TAB_CTOR{"laptop tab constructor", P_STANDINGS, 0x14c0860, runtime::Sig::rel32, 0, 1, 5};
const runtime::Sig S_TAB_PUSH{"laptop tab push", P_STANDINGS, 0x1e5dfb0, runtime::Sig::rel32, 0x24, 0x25, 5};
const runtime::Sig S_PAGE_CTOR{"tab view model constructor",
                               "C6 44 24 48 00 C7 44 24 40 05 00 00 00 48 C7 44 24 30 00 00 00 00 48 89 D9 4C 89 EA 4D "
                               "89 F8 49 89 F1 E8 ?? ?? ?? ?? 48 8D 05",
                               0x14bdac0,
                               runtime::Sig::rel32,
                               0x22,
                               0x23,
                               5};
const runtime::Sig S_PAGE_ADD{
    "laptop tab add page", "48 89 DA E8 ?? ?? ?? ?? 4C 8B BE 68 02 00 00", 0x14c13e0, runtime::Sig::rel32, 3, 4, 5};

const runtime::Sig S_TQ_END{"team HQ tabs end", "4C 8B A6 D0 02 00 00 4C 8B AE D8 02 00 00", 0x22d5cce};
const runtime::Sig S_TQ_RND{
    "team HQ R&D block", "E8 ?? ?? ?? ?? 4C 8B BE 68 02 00 00 4C 8B 66 28", 0x22d23e5, runtime::Sig::addr, 0, 5, 0};
const runtime::Sig S_TQ_VEHICLE{"team HQ VEHICLE block", "4C 8B BE 68 02 00 00 4C 8B 66 28 C7 84 24 E0 00 00 00 01 A7 73 B0",
                                0x22d2ce1};
constexpr const char *P_TQ_STANDINGS = "E8 ?? ?? ?? ?? 48 8D 05 ?? ?? ?? ?? 49 89 07 48 8D 05 ?? ?? ?? ?? 49 89 87 60 02 00 "
                                       "00 48 8D 05 ?? ?? ?? ?? 49 89 87 68 02 00 00 C5 F8 10 05 ?? ?? ?? ?? C4 C1 78 11 87 "
                                       "88 0B 00 00";
const runtime::Sig S_TQ_CTOR{"team HQ tab constructor", P_TQ_STANDINGS, 0x14bf8c0, runtime::Sig::rel32, 0, 1, 5};
const runtime::Sig S_TQ_VT0{"team HQ tab vtable", P_TQ_STANDINGS, 0x5787460, runtime::Sig::riprel, 5, 8, 7};
const runtime::Sig S_TQ_VT260{"team HQ tab vtable +0x260", P_TQ_STANDINGS, 0x5787540, runtime::Sig::riprel, 0xf, 0x12, 7};
const runtime::Sig S_TQ_VT268{"team HQ tab vtable +0x268", P_TQ_STANDINGS, 0x5787570, runtime::Sig::riprel, 0x1d, 0x20, 7};
const runtime::Sig S_TQ_INIT{"team HQ tab field block", P_TQ_STANDINGS, 0x577c058, runtime::Sig::riprel, 0x2b, 0x2f, 8};
const runtime::Sig S_TQ_REGISTERED{"team HQ tabs registered", "48 8B 74 24 50 48 8D BE E8 02 00 00 31 DB EB", 0x22d5d3b};
const runtime::Sig S_LAPTOP_REGISTERED{
    "laptop tabs registered", "49 83 C5 18 49 39 ED 75 C7 80 BE 98 09 00 00 00", 0x1c42689, runtime::Sig::addr, 0, 9, 0};
const runtime::Sig S_TAB_VISIBLE{"Vm_Set_TabEnabled (tab visibility)",
                                 "48 8D 9E E8 02 00 00 EB 27 0F 1F 00 31 C0 48 8D 90 68 02 00 00 48 85 C0 48 0F 44 D0 48 "
                                 "89 D9 45 31 C0 E8 ?? ?? ?? ??",
                                 0x431f470,
                                 runtime::Sig::rel32,
                                 0x22,
                                 0x23,
                                 5};
const runtime::Sig S_TQ_SHOW_ALL{"team HQ show all tabs tail",
                                 "41 B0 01 E8 ?? ?? ?? ?? 49 83 C6 18 4D 39 FE 74 37 48 8B 47 10 48 8B 4F 18",
                                 0x22d7b2b,
                                 runtime::Sig::addr,
                                 0,
                                 0x48,
                                 0};
const runtime::Sig S_TQ_PUSH{"team HQ tab push",
                             "4C 89 7C 24 60 48 8D 94 24 E0 00 00 00 4C 8D 44 24 58 48 8B 8C 24 D8 00 00 00 E8 ?? ?? ?? "
                             "?? 48 8B 86 D8 02 00 00 4C 8B 68 F8",
                             0x23ac4d0,
                             runtime::Sig::rel32,
                             0x1a,
                             0x1b,
                             5};

using Alloc_t = void *(void *allocator, size_t size, size_t align);
using AddPage_t = void(void *tab, void *page);
using Command_t = uint64_t(void *vm, swofty::game::NeStr **name, const void *data);

std::atomic<bool> live{false};
uintptr_t a_tab_ctor, a_add_page;
safetyhook::MidHook h_end, h_rnd, h_tq_end, h_tq_rnd, h_tq_registered, h_laptop_registered, h_tq_show_all;
void **page_vt, **page_vt_src;
Command_t *page_cmd;
constinit swofty_core::guards::LogFirst<48> note_lines;

bool step(const HubTab &t, const std::string &option, bool next)
{
    size_t current = 0;
    std::vector<std::string> commands;
    if (!settings_state(t.addon, option, current, nullptr, &commands, nullptr) || commands.empty())
        return false;
    size_t n = commands.size(), want = next ? (current + 1) % n : (current + n - 1) % n;
    uint32_t h = 5381;
    for (const char *s = commands[want].c_str(); *s; ++s)
        h = h * 33 + uint8_t(*s);
    return settings_command(h);
}

uint64_t page_command(void *vm, swofty::game::NeStr **name, const void *data)
{
    std::string text;
    if (name && *name)
        swofty::mem::c_string(reinterpret_cast<uintptr_t>(*name) + 9, text, 160);
    bool next = text.rfind("swofty_next:", 0) == 0, prev = text.rfind("swofty_prev:", 0) == 0;
    if (next || prev) {
        std::string rest = text.substr(12);
        size_t colon = rest.find(':');
        std::string addon = rest.substr(0, colon), option = colon == std::string::npos ? "" : rest.substr(colon + 1);
        for (auto &t : tabs) {
            if (t.addon != addon)
                continue;
            for (auto &o : t.options)
                if (o.id == option) {
                    bool ok = step(t, option, next);
                    swofty::log("[hub] %s: %s %s.%s%s", t.name.c_str(), next ? "next" : "prev", addon.c_str(), option.c_str(),
                                ok ? "" : " (no such option state)");
                    return 0;
                }
        }
    }
    return page_cmd ? page_cmd(vm, name, data) : 0;
}

}

std::vector<HubTab> tabs;
uintptr_t a_push, a_page_ctor;
uintptr_t a_tq_ctor, a_tq_vt0, a_tq_vt260, a_tq_vt268, a_tq_init, a_tq_push;

void publish(const HubTab &t)
{
    std::string base = t.data;
    if (!frontend::ui_vm_set_path(base + ".title", 0, t.title.c_str())) {
        note("[hub] %s: %s did not resolve; the page shows no data", t.name.c_str(), base.c_str());
        return;
    }
    int32_t count = int32_t(t.options.size());
    frontend::ui_vm_set_path(base + ".count", 2, &count);
    for (size_t k = 0; k < t.options.size(); k++) {
        const HubOption &o = t.options[k];
        size_t current = 0;
        std::vector<std::string> texts;
        settings_state(t.addon, o.id, current, &texts, nullptr, nullptr);
        std::string row = base + ".o" + std::to_string(k);
        frontend::ui_vm_set_path(row + ".label", 0, o.label.c_str());
        frontend::ui_vm_set_path(row + ".value", 0, current < texts.size() ? texts[current].c_str() : "");
        frontend::ui_vm_set_path(row + ".description", 0, o.description.c_str());
    }
}

bool own_vtable(void *page)
{
    void **src = *static_cast<void ***>(page);
    if (!page_vt) {
        page_vt_src = src;
        auto **copy = new void *[VT_COPY + 1];
        std::memcpy(copy, src - 1, (VT_COPY + 1) * sizeof(void *));
        page_cmd = reinterpret_cast<Command_t *>(copy[1 + VT_COMMAND]);
        copy[1 + VT_COMMAND] = reinterpret_cast<void *>(&page_command);
        page_vt = copy + 1;
    }
    if (src != page_vt_src)
        return false;
    *static_cast<void ***>(page) = page_vt;
    return true;
}

void *alloc_through(uintptr_t holder)
{
    void *allocator = at<void *>(holder);
    if (!allocator || !swofty::mem::object_like(allocator))
        return nullptr;
    auto fn = at<Alloc_t *>(at<uintptr_t>(reinterpret_cast<uintptr_t>(allocator)) + 0x18);
    return swofty::mem::in_image(reinterpret_cast<uintptr_t>(fn)) ? allocator : nullptr;
}

void *allocate(void *allocator, size_t size, size_t align)
{
    auto fn = at<Alloc_t *>(at<uintptr_t>(reinterpret_cast<uintptr_t>(allocator)) + 0x18);
    return fn(allocator, size, align);
}

namespace {

void add_tab(uintptr_t laptop, HubTab &t)
{
    void *allocator = alloc_through(laptop + LAPTOP_ALLOC);
    if (!allocator) {
        note("[hub] %s: the laptop has no allocator; tab not added", t.name.c_str());
        return;
    }
    void *tab = allocate(allocator, TAB_SIZE, TAB_ALIGN);
    if (!tab)
        return;
    void *a2 = at<void *>(laptop + LAPTOP_ARG2), *a3 = at<void *>(laptop + LAPTOP_ARG3);
    reinterpret_cast<TabCtor_t *>(a_tab_ctor)(tab, a2, a3, reinterpret_cast<void *>(laptop), "tabbing_secondary_controller",
                                              "tabbing_secondary_tab", t.lng.c_str(), ICON, 1);
    struct {
        void *allocator, *obj;
    } owned{allocator, tab};
    uint32_t key = t.key;
    reinterpret_cast<Push_t *>(a_push)(reinterpret_cast<void *>(laptop + LAPTOP_TABS), &key, &owned);
    uintptr_t end = at<uintptr_t>(laptop + LAPTOP_TABS_END);
    if (!end || at<void *>(end - 8) != tab) {
        note("[hub] %s: the tab is not the laptop's last tab after the push; no page added", t.name.c_str());
        return;
    }
    void *page_alloc = alloc_through(reinterpret_cast<uintptr_t>(tab) + TAB_ALLOC);
    void *page = page_alloc ? allocate(page_alloc, PAGE_SIZE, PAGE_ALIGN) : nullptr;
    if (!page) {
        note("[hub] %s: no page allocated", t.name.c_str());
        return;
    }
    reinterpret_cast<PageCtor_t *>(a_page_ctor)(page, a2, a3, reinterpret_cast<void *>(laptop), t.content.c_str(), t.lng.c_str(),
                                                nullptr, "", PAGE_TYPE, 0);
    bool mine = own_vtable(page);
    reinterpret_cast<AddPage_t *>(a_add_page)(tab, page);
    publish(t);
    swofty::log("[hub] %s: tab '%s' added after STANDINGS (key 0x%08x, page %s%s, data %s)", t.name.c_str(), t.title.c_str(),
                t.key, t.content.c_str(), mine ? "" : ", its commands stay the game's", t.data.c_str());
}

void tabs_end(safetyhook::Context &c)
{
    uintptr_t laptop = c.rsi;
    if (!laptop || !swofty::mem::object_like(laptop))
        return;
    char group[64] = {};
    if (runtime::career_content_table()->data_group(nullptr, group, sizeof group) != SWOFTY_ASSET_OK)
        return;
    for (auto &t : tabs)
        if (t.group == group)
            add_tab(laptop, t);
}

std::string upper(std::string s)
{
    for (auto &ch : s)
        ch = char(std::toupper(uint8_t(ch)));
    return s;
}

void load()
{
    std::string text = runtime::read_file(runtime::under_swofty("settings.json"), size_t(16) << 20);
    json::Value doc;
    if (text.empty() || !json::parse(text, doc))
        return;
    namespace older = swofty_core::registry::older;
    auto &st = runtime::older_store();
    std::map<std::string, const json::Value *> manifests;
    for (auto &m : doc.field("addons", "mods").a) {
        const json::Value &h = m["hub_tab"];
        if (h.type != json::Value::Obj || (m["enabled"].type == json::Value::Bool && !m["enabled"].b))
            continue;
        manifests[m["id"].str()] = &m;
        older::HubTab t;
        t.addon = m["id"].str();
        t.name = h["name"].str();
        t.title = h["title"].str(m["name"].str(t.addon.c_str()).c_str());
        t.group = h["data_group"].str();
        if (t.name.empty() || t.group.empty())
            continue;
        for (auto &id : h["options"].a)
            t.options.push_back(id.str());
        older::put(st, t.addon, older::HUB_TAB, t.name, older::hub_record(t), "hub_tab manifest field");
    }
    for (auto &m : doc.field("addons", "mods").a)
        if (!manifests.count(m["id"].str()) && m["options"].type == json::Value::Arr)
            manifests[m["id"].str()] = &m;
    for (auto &id : older::in_load_order(st, older::HUB_TAB)) {
        older::HubTab r = older::hub_from(id, older::effective(st, older::HUB_TAB, id));
        if (r.name.empty() || r.group.empty())
            continue;
        HubTab t;
        t.addon = r.addon;
        t.name = r.name;
        t.title = r.title;
        t.group = r.group;
        t.lng = "lng_swofty_hub_" + t.name;
        t.content = "swofty_hub_" + t.name;
        t.data = "view_model_data.hud_swofty_hub_" + t.name;
        t.key = runtime::fnv32(t.content.c_str());
        auto at = manifests.find(t.addon);
        for (auto &oid : r.options)
            if (at != manifests.end())
                for (auto &o : (*at->second)["options"].a)
                    if (o["id"].str() == oid)
                        t.options.push_back({oid, o["label"].str(oid.c_str()), o["description"].str()});
        tabs.push_back(std::move(t));
    }
}

void setup()
{
    using runtime::loader::install;
    load();
    if (tabs.empty())
        return;
    a_tab_ctor = runtime::find(S_TAB_CTOR, "hub tabs", "hub");
    a_push = runtime::find(S_TAB_PUSH, "hub tabs", "hub");
    a_page_ctor = runtime::find(S_PAGE_CTOR, "hub tabs", "hub");
    a_add_page = runtime::find(S_PAGE_ADD, "hub tabs", "hub");
    uintptr_t end = runtime::find(S_TABS_END, "hub tabs", "hub"), gate = runtime::find(S_RND_GATE, "hub tabs", "hub");
    a_tq_ctor = runtime::find(S_TQ_CTOR, "hub tabs", "hub");
    a_tq_vt0 = runtime::find(S_TQ_VT0, "hub tabs", "hub");
    a_tq_vt260 = runtime::find(S_TQ_VT260, "hub tabs", "hub");
    a_tq_vt268 = runtime::find(S_TQ_VT268, "hub tabs", "hub");
    a_tq_init = runtime::find(S_TQ_INIT, "hub tabs", "hub");
    a_tq_push = runtime::find(S_TQ_PUSH, "hub tabs", "hub");
    a_tq_vehicle = runtime::find(S_TQ_VEHICLE, "hub tabs", "hub");
    uintptr_t tq_end = runtime::find(S_TQ_END, "hub tabs", "hub"), tq_rnd = runtime::find(S_TQ_RND, "hub tabs", "hub");
    a_set_visible = runtime::find(S_TAB_VISIBLE, "hub tabs", "hub");
    uintptr_t tq_registered = runtime::find(S_TQ_REGISTERED, "hub tabs", "hub"),
              laptop_reg = runtime::find(S_LAPTOP_REGISTERED, "hub tabs", "hub"),
              tq_show_all = runtime::find(S_TQ_SHOW_ALL, "hub tabs", "hub");
    bool laptop = a_tab_ctor && a_push && a_page_ctor && a_add_page && end && gate;
    bool team_hq = a_push && a_page_ctor && a_tq_ctor && a_tq_vt0 && a_tq_vt260 && a_tq_vt268 && a_tq_init && a_tq_push &&
                   a_tq_vehicle && tq_end && tq_rnd;
    if (laptop && !(install(h_end, end, S_TABS_END.what, tabs_end) && install(h_rnd, gate, S_RND_GATE.what, rnd_gate))) {
        h_end = {};
        laptop = false;
    }
    if (team_hq &&
        !(install(h_tq_end, tq_end, S_TQ_END.what, team_hq_end) && install(h_tq_rnd, tq_rnd, S_TQ_RND.what, team_hq_rnd))) {
        h_tq_end = {};
        team_hq = false;
    }
    if (laptop && a_set_visible && laptop_reg &&
        install(h_laptop_registered, laptop_reg, S_LAPTOP_REGISTERED.what, laptop_registered))
        laptop_flag = true;
    if (team_hq && a_set_visible && tq_registered &&
        install(h_tq_registered, tq_registered, S_TQ_REGISTERED.what, team_hq_registered))
        team_hq_flag = true;
    if (team_hq_flag && tq_show_all)
        install(h_tq_show_all, tq_show_all, S_TQ_SHOW_ALL.what, team_hq_show_all);
    swofty::log("[hub] builders: Driver Career Team HQ (team_hq_container) %s, laptop (laptop_container) %s",
                team_hq ? "hooked" : "NOT hooked", laptop ? "hooked" : "NOT hooked");
    swofty::log("[hub] R&D tab: Team HQ %s, laptop %s", team_hq_flag ? "follows hide_rnd live" : "left out at the next entry",
                laptop_flag ? "follows hide_rnd live" : "left out at the next entry");
    if (!laptop && !team_hq)
        return;
    if (const SwoftyLang *l = runtime::lang_table())
        for (auto &t : tabs)
            l->override_set("*", t.lng.c_str(), upper(t.title).c_str());
    live = true;
    for (auto &t : tabs)
        swofty::log("[hub] %s: tab '%s' for careers of %s, %zu option row(s) from %s", t.name.c_str(), t.title.c_str(),
                    t.group.c_str(), t.options.size(), t.addon.c_str());
}

int api_available() { return live.load() ? 1 : 0; }

int api_set_option(const char *addon, const char *option, const char *value)
{
    if (!addon || !option || !value)
        return SWOFTY_ASSET_INVALID;
    size_t current = 0;
    std::vector<std::string> commands, values;
    if (!settings_state(addon, option, current, nullptr, &commands, &values))
        return SWOFTY_ASSET_INVALID;
    for (size_t i = 0; i < values.size(); i++)
        if (values[i] == value) {
            if (i == current)
                return SWOFTY_ASSET_OK;
            uint32_t h = 5381;
            for (const char *s = commands[i].c_str(); *s; ++s)
                h = h * 33 + uint8_t(*s);
            return settings_command(h) ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
        }
    return SWOFTY_ASSET_INVALID;
}

const SwoftyHub hub_table = {SWOFTY_HUB_VERSION, api_available, api_hide_rnd, api_set_option};

}

void note(const char *fmt, const char *a, const char *b, const char *c)
{
    if (note_lines.note())
        swofty::log(fmt, a, b, c);
}

}

void hub_option_changed(const std::string &addon, const std::string &option)
{
    if (!career::hub::live.load())
        return;
    for (auto &t : career::hub::tabs) {
        if (t.addon != addon)
            continue;
        for (auto &o : t.options)
            if (o.id == option) {
                career::hub::publish(t);
                break;
            }
    }
}

extern "C" __declspec(dllexport) const SwoftyHub *SwoftyCoreHub(void) { return &career::hub::hub_table; }

SWOFTY_CORE_RANK(hub_tab)

SWOFTY_ON_LOAD { runtime::when_code_ready("hub tabs: laptop builder hooks", career::hub::setup); }
