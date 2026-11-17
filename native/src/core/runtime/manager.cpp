#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <algorithm>
#include <array>
#include <cstdio>
#include <mutex>
#include <string>
#include <utility>
#include <vector>
#include "core/diagnostics/perf.hpp"
#include "core/runtime/hook_errors.hpp"
#include "core/runtime/loader/bindings.hpp"
#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/hook_pool.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "core/runtime/loader/scanner.hpp"
#include "safetyhook.hpp"
#include "swofty_api.h"

namespace runtime {
const SwoftyAssets *assets_table();
const SwoftyEvents *events_table();
const SwoftyOverlay *overlay_table();
const SwoftyTelemetry *telemetry_table();
const SwoftyDb *db_table();
const SwoftyStorage *storage_table();
const SwoftyCareer *career_table();
const SwoftyPopup *popup_table();
const SwoftyLang *lang_table();
const SwoftyCharacters *characters_table();
const SwoftyCareerWizard *wizard_table();
struct CoreMixins {
    const char *name;
    std::vector<SwoftyMixin> mixins;
};
std::vector<CoreMixins> career_mixins();
const SwoftyCareerContent *career_content_table();
std::vector<CoreMixins> career_content_mixins();
void career_content_commit();
const SwoftyEra *era_table();
const SwoftyRegistry *registry_table();
std::vector<CoreMixins> era_mixins();
void era_commit();
std::vector<CoreMixins> tt_guard_mixins();
void tt_guard_commit();
std::vector<CoreMixins> career_alias_mixins();
void career_alias_commit();
std::vector<CoreMixins> car_category_mixins();
void car_category_commit();
std::vector<CoreMixins> save_splice_mixins();
void save_splice_commit();
std::vector<CoreMixins> save_sandbox_mixins();
void save_sandbox_commit();
std::vector<CoreMixins> ers_newlap_mixins();
void ers_newlap_commit();
std::vector<CoreMixins> ers_mode_mixins();
void ers_mode_commit();
std::vector<CoreMixins> car_levers_mixins();
void car_levers_commit();
std::vector<CoreMixins> standings_guard_mixins();
void standings_guard_commit();
std::vector<CoreMixins> fe_tile_guard_mixins();
void fe_tile_guard_commit();
std::vector<CoreMixins> myteam_vehicle_guard_mixins();
void myteam_vehicle_guard_commit();
std::vector<CoreMixins> myteam_vehicle_heal_mixins();
void myteam_vehicle_heal_commit();
std::vector<CoreMixins> myteam_vehicle_keep_mixins();
void myteam_vehicle_keep_commit();
std::vector<CoreMixins> ai_corner_guard_mixins();
void ai_corner_guard_commit();
std::vector<CoreMixins> replay_grid_guard_mixins();
void replay_grid_guard_commit();
std::vector<CoreMixins> watchdog_grace_mixins();
void watchdog_grace_commit();
std::vector<CoreMixins> tyre_slots_mixins();
void tyre_slots_commit();
std::vector<CoreMixins> tyre_models_mixins();
void tyre_models_commit();
std::vector<CoreMixins> garage_crew_mixins();
std::vector<CoreMixins> garage_placement_mixins();
std::vector<CoreMixins> pit_slots_mixins();
void garage_crew_commit();
void pit_slots_commit();
std::vector<CoreMixins> pit_box_mixins();
void pit_box_commit();
std::vector<CoreMixins> garage_slots_mixins();
void garage_slots_commit();
std::vector<CoreMixins> chassis_registry_mixins();
void chassis_registry_commit();
std::vector<CoreMixins> track_acclim_mixins();
void track_acclim_commit();
std::vector<CoreMixins> audio_queue_mixins();
void audio_queue_commit();
std::vector<CoreMixins> heap_sizes_mixins();
void heap_sizes_commit();
std::vector<CoreMixins> track_select_mixins();
void track_select_commit();
std::vector<CoreMixins> team_slots_mixins();
void team_slots_commit();
std::vector<CoreMixins> leaderboard_guard_mixins();
std::vector<CoreMixins> lap_host_mixins();
void lap_host_commit();
void leaderboard_guard_commit();
std::vector<CoreMixins> thrustmaster_lcd_mixins();
void thrustmaster_lcd_commit();
std::vector<CoreMixins> estimate_laptime_mixins();
std::vector<CoreMixins> gp_continue_mixins();
void gp_continue_commit();
void estimate_laptime_commit();
std::vector<CoreMixins> red_flag_recompute_mixins();
void red_flag_recompute_commit();
std::vector<CoreMixins> box_grid_mixins();
void box_grid_commit();
std::vector<CoreMixins> quali_tyres_mixins();
void quali_tyres_commit();
std::vector<CoreMixins> track_list_guard_mixins();
void track_list_guard_commit();
std::vector<CoreMixins> garage_setup_mixins();
void garage_setup_commit();
std::vector<CoreMixins> theatre_list_mixins();
void theatre_list_commit();
std::vector<CoreMixins> tile_list_select_mixins();
void tile_list_select_commit();
std::vector<CoreMixins> session_enter_mixins();
std::vector<CoreMixins> ai_line_set_mixins();
void ai_line_set_commit();
void session_enter_commit();
std::vector<CoreMixins> characters_mixins();
std::vector<CoreMixins> wizard_mixins();
std::vector<CoreMixins> tile_route_mixins();
void characters_commit();
void wizard_commit();
void tile_route_commit();
std::string load_stage();
void career_setup();
void popup_setup();
void mfd_setup();
void ready();
void events_drop_module(void *module);
void career_start_drop_module(void *module);
const std::wstring &swofty_root();
void on_stage(void (*fn)(int));
void when_code_ready(const char *what, void (*fn)());
void stage_catch_up();
}

namespace swofty_core::hooks {
const SwoftyHooks *table();
void window_stage();
void drop_module(void *module);
}

namespace swofty_core::services {
const SwoftyServices *table();
void report_missing();
}


using namespace runtime::loader;

namespace {

static_assert(sizeof(SwoftyContext) == sizeof(safetyhook::Context64));
static_assert(offsetof(SwoftyContext, r12) == offsetof(safetyhook::Context64, r12));
static_assert(offsetof(SwoftyContext, rip) == offsetof(safetyhook::Context64, rip));

struct Handler {
    size_t addon;
    Mixin *m;
};

struct Group {
    uint32_t kind;
    uintptr_t addr;
    size_t span;
    std::vector<Handler> handlers;
    void *tramp = nullptr;
    size_t site = SIZE_MAX;
};

struct Held {
    uint32_t kind;
    uintptr_t addr;
    size_t span;
    std::vector<Handler> handlers;
    void *tramp;
    size_t site;
};

struct Mark {
    size_t written, stubs, mids, inlines, sites;
};

constexpr size_t MAX_SITES = 256;
constexpr size_t SITE_CHAIN_ROOM = 32;

std::vector<Addon> addons;
Addon *current;
size_t settled;
std::vector<Held> held;
Mark mark;
bool closed;
json::Value session;
bool have_session;

std::vector<safetyhook::InlineHook> inline_hooks;
std::vector<std::string> inline_names;
std::vector<safetyhook::MidHook> mid_hooks;
std::vector<safetyhook::Allocation> stubs;
std::vector<std::pair<uintptr_t, std::vector<uint8_t>>> written;
struct SiteHandler {
    void (*fn)(SwoftyContext *);
    uint32_t probe;
};

std::vector<SiteHandler> site_chains[MAX_SITES];
size_t sites_used;

template <size_t I> void site_thunk(safetyhook::Context &c)
{
    for (auto &h : site_chains[I]) {
        perf::Scope p(h.probe);
        h.fn(reinterpret_cast<SwoftyContext *>(&c));
    }
}

template <size_t... I> constexpr auto make_thunks(std::index_sequence<I...>)
{
    return std::array<safetyhook::MidHookFn, sizeof...(I)>{site_thunk<I>...};
}
constexpr auto site_thunks = make_thunks(std::make_index_sequence<MAX_SITES>{});

const char *kind_name(uint32_t k)
{
    static const char *names[] = {"?", "wrap", "site", "vtable", "redirect", "patch", "on_load"};
    k = SWOFTY_MIXIN_KIND(k);
    return k < 7 ? names[k] : "?";
}

std::string utf8(const wchar_t *w)
{
    char buf[MAX_PATH * 3] = {};
    WideCharToMultiByte(CP_UTF8, 0, w, -1, buf, sizeof buf - 1, nullptr, nullptr);
    return buf;
}

const std::vector<uint8_t> *hooked_range(uintptr_t a, uintptr_t &start)
{
    for (auto &h : inline_hooks)
        if (a - h.target_address() < h.original_bytes().size())
            return start = h.target_address(), &h.original_bytes();
    for (auto &h : mid_hooks)
        if (a - h.target_address() < h.original_bytes().size())
            return start = h.target_address(), &h.original_bytes();
    for (auto &w : written)
        if (a - w.first < w.second.size())
            return start = w.first, &w.second;
    return nullptr;
}

bool same_dll(const std::string &decl, const std::string &dll)
{
    auto slash = decl.find_last_of("/\\");
    std::string a = slash == std::string::npos ? decl : decl.substr(slash + 1);
    return a.size() == dll.size() && !_stricmp(a.c_str(), dll.c_str());
}

bool declared(const Addon &addon, const SwoftyMixin &m)
{
    for (auto &e : session["native"].a) {
        if (!same_dll(e["dll"].str(), addon.name))
            continue;
        for (auto &d : e["mixins"].a) {
            bool vt = SWOFTY_MIXIN_KIND(m.kind) == SWOFTY_MIXIN_VTABLE;
            if (d["target"].str() == m.target && d["kind"].str() == kind_name(m.kind) &&
                (!vt || d["slot"].num(-1) == SWOFTY_MIXIN_ARG(m.kind)) &&
                (d["priority"].type == json::Value::Null || d["priority"].num() == m.priority) &&
                d["overwrite"].b == bool(m.kind & SWOFTY_MIXIN_OVERWRITE))
                return true;
        }
    }
    return false;
}

int api_register(const SwoftyMixin *m, size_t n)
{
    if (!current) {
        api->log("[core] register_mixins outside SwoftyAddonInit; refused");
        return 0;
    }
    std::vector<Mixin> add;
    for (size_t i = 0; i < n; i++) {
        uint32_t k = SWOFTY_MIXIN_KIND(m[i].kind);
        bool on_load = k == SWOFTY_MIXIN_ON_LOAD;
        if (k < SWOFTY_MIXIN_WRAP || k > SWOFTY_MIXIN_ON_LOAD || !m[i].handler || (!on_load && !m[i].target) ||
            (k == SWOFTY_MIXIN_PATCH && !SWOFTY_MIXIN_ARG(m[i].kind))) {
            api->log("[core] addon %s: mixin %zu is malformed (kind 0x%x); registration refused", current->name.c_str(), i,
                     m[i].kind);
            return 0;
        }
        if (!on_load && have_session && !declared(*current, m[i])) {
            api->log("[core] addon %s: %s on %s is not declared in session.json; registration refused",
                     current->name.c_str(), kind_name(k), m[i].target);
            return 0;
        }
        add.push_back({on_load ? "" : m[i].target, m[i].kind, m[i].priority, m[i].handler, m[i].orig});
    }
    current->mixins.insert(current->mixins.end(), add.begin(), add.end());
    api->log("[core] addon %s: registered %zu mixin(s)", current->name.c_str(), n);
    return 1;
}

void drop(size_t i, const char *fmt, const char *a = "", const char *b = "", const char *c = "")
{
    if (!addons[i].active)
        return;
    char why[512];
    snprintf(why, sizeof why, fmt, a, b, c);
    addons[i].active = false;
    api->log("[core] addon %s dropped: %s; none of its mixins install", addons[i].name.c_str(), why);
}

void check_targets(size_t i)
{
    addons[i].waiting = false;
    for (auto &m : addons[i].mixins) {
        uint32_t k = SWOFTY_MIXIN_KIND(m.kind), arg = SWOFTY_MIXIN_ARG(m.kind);
        if (k == SWOFTY_MIXIN_ON_LOAD)
            continue;
        Binding *b = bindings().find(m.target);
        if (!b)
            return drop(i, "needs %s, which %s does not list", m.target.c_str(), bindings().file().c_str());
        m.addr = resolve_for_addon(m.target.c_str());
        if (b->deferred) {
            addons[i].waiting = true;
            continue;
        }
        if (!m.addr)
            return drop(i, "needs %s, which does not resolve on this build", m.target.c_str());
        const std::string &bk = b->kind;
        bool ok = k == SWOFTY_MIXIN_WRAP     ? bk == "func"
                  : k == SWOFTY_MIXIN_VTABLE ? bk == "vtable"
                  : k == SWOFTY_MIXIN_REDIRECT ? bk == "call"
                                               : bk == "site" || bk == "call";
        if (!ok)
            return drop(i, "%s on %s needs another kind of binding, it is a %s", kind_name(k), m.target.c_str(), bk.c_str());
        if (k == SWOFTY_MIXIN_VTABLE && b->slots && arg >= (uint32_t)b->slots)
            return drop(i, "vtable slot out of range on %s", m.target.c_str());
        if (k == SWOFTY_MIXIN_VTABLE)
            m.addr += 8 * arg;
        if (k == SWOFTY_MIXIN_PATCH && arg > b->expect.size())
            return drop(i, "patch on %s is longer than the binding's expect bytes", m.target.c_str());
        if (k == SWOFTY_MIXIN_REDIRECT && byte_at(m.addr) != 0xE8)
            return drop(i, "redirect target %s is not a call rel32", m.target.c_str());
    }
}

std::vector<Group> plan(size_t from, bool late = false)
{
    std::vector<Group> groups;
    for (size_t i = from; i < addons.size(); i++) {
        auto &a = addons[i];
        if (!a.active || a.waiting || a.reinit || (late && !a.late))
            continue;
        for (auto &m : addons[i].mixins) {
            uint32_t k = SWOFTY_MIXIN_KIND(m.kind);
            if (k == SWOFTY_MIXIN_ON_LOAD)
                continue;
            bool chains = k == SWOFTY_MIXIN_WRAP || k == SWOFTY_MIXIN_SITE || k == SWOFTY_MIXIN_VTABLE;
            auto g = std::find_if(groups.begin(), groups.end(),
                                  [&](const Group &x) { return chains && x.kind == k && x.addr == m.addr; });
            if (g == groups.end()) {
                size_t span = k == SWOFTY_MIXIN_VTABLE     ? 8
                              : k == SWOFTY_MIXIN_REDIRECT ? 5
                              : k == SWOFTY_MIXIN_PATCH    ? SWOFTY_MIXIN_ARG(m.kind)
                                                           : insn_span(m.addr, 5);
                groups.push_back({k, m.addr, span, {}});
                g = groups.end() - 1;
            }
            g->handlers.push_back({i, &m});
        }
    }
    for (auto &g : groups)
        std::stable_sort(g.handlers.begin(), g.handlers.end(),
                         [](const Handler &a, const Handler &b) { return a.m->priority > b.m->priority; });
    std::stable_sort(groups.begin(), groups.end(), [](const Group &a, const Group &b) { return a.kind < b.kind; });
    return groups;
}

std::string owners(const Group &g)
{
    std::string s;
    for (auto &h : g.handlers)
        s += (s.empty() ? "" : " > ") + addons[h.addon].name + "(" + std::to_string(h.m->priority) +
             (h.m->kind & SWOFTY_MIXIN_OVERWRITE ? ", overwrite" : "") + ")";
    return s;
}

bool core_only(const Group &g)
{
    for (auto &h : g.handlers)
        if (!addons[h.addon].core)
            return false;
    return true;
}

bool has_overwrite(const std::vector<Handler> &hs)
{
    for (auto &h : hs)
        if (h.m->kind & SWOFTY_MIXIN_OVERWRITE)
            return true;
    return false;
}

bool chains_onto(const Group &g, const Held &h)
{
    if (!((g.kind == SWOFTY_MIXIN_WRAP || g.kind == SWOFTY_MIXIN_SITE) && g.kind == h.kind && g.addr == h.addr &&
          !has_overwrite(g.handlers) && !has_overwrite(h.handlers)))
        return false;
    return g.kind != SWOFTY_MIXIN_SITE || (h.site < MAX_SITES && h.handlers.size() + g.handlers.size() <= site_chains[h.site].capacity());
}

Held *held_for(const Group &g)
{
    for (auto &h : held)
        if (chains_onto(g, h))
            return &h;
    return nullptr;
}

bool drop_conflicts(const std::vector<Group> &groups, bool warn)
{
    std::vector<size_t> bad;
    for (size_t x = 0; x < groups.size(); x++) {
        auto &g = groups[x];
        auto &target = g.handlers[0].m->target;
        std::string over;
        for (auto &h : g.handlers)
            if (h.m->kind & SWOFTY_MIXIN_OVERWRITE)
                over += (over.empty() ? "" : ", ") + addons[h.addon].name, bad.push_back(h.addon);
        if (std::count(over.begin(), over.end(), ',')) {
            api->log("[core] conflict: two overwrites on %s: %s", target.c_str(), over.c_str());
        } else if (!over.empty()) {
            bad.pop_back();
            if (warn && !(g.handlers.back().m->kind & SWOFTY_MIXIN_OVERWRITE))
                api->log("[core] warning: an overwrite on %s hides the handlers below it: %s", target.c_str(),
                         owners(g).c_str());
        }
        for (size_t y = x + 1; y < groups.size(); y++) {
            auto &o = groups[y];
            bool data = g.kind == SWOFTY_MIXIN_VTABLE, odata = o.kind == SWOFTY_MIXIN_VTABLE;
            if (data != odata || g.addr >= o.addr + o.span || o.addr >= g.addr + g.span)
                continue;
            api->log("[core] conflict: %s %s at 0x%llx overlaps %s %s at 0x%llx: %s vs %s", kind_name(g.kind),
                     target.c_str(), (unsigned long long)g.addr, kind_name(o.kind), o.handlers[0].m->target.c_str(),
                     (unsigned long long)o.addr, owners(g).c_str(), owners(o).c_str());
            bool gc = core_only(g), oc = core_only(o);
            for (auto *z : {&g, &o})
                if (gc == oc || core_only(*z))
                    for (auto &h : z->handlers)
                        bad.push_back(h.addon);
        }
        for (auto &h : held) {
            bool data = g.kind == SWOFTY_MIXIN_VTABLE, hdata = h.kind == SWOFTY_MIXIN_VTABLE;
            if (data != hdata || g.addr >= h.addr + h.span || h.addr >= g.addr + g.span)
                continue;
            if (chains_onto(g, h))
                continue;
            api->log("[core] conflict: %s %s at 0x%llx overlaps a %s installed at the early stage: %s", kind_name(g.kind),
                     target.c_str(), (unsigned long long)g.addr, kind_name(h.kind), owners(g).c_str());
            for (auto &x : g.handlers)
                bad.push_back(x.addon);
        }
    }
    for (size_t i : bad)
        drop(i, "it conflicts with another addon (see the conflict line above)");
    return !bad.empty();
}

bool write_bytes(uintptr_t at, const void *data, size_t n)
{
    DWORD old;
    auto p = reinterpret_cast<void *>(at);
    if (!VirtualProtect(p, n, image().in_exec(at) ? PAGE_EXECUTE_READWRITE : PAGE_READWRITE, &old))
        return false;
    auto b = reinterpret_cast<const uint8_t *>(p);
    written.push_back({at, std::vector<uint8_t>(b, b + n)});
    memcpy(p, data, n);
    VirtualProtect(p, n, old, &old);
    FlushInstructionCache(GetCurrentProcess(), p, n);
    return true;
}

void uninstall_all()
{
    for (size_t i = written.size(); i-- > mark.written;) {
        auto &w = written[i];
        DWORD old;
        auto p = reinterpret_cast<void *>(w.first);
        VirtualProtect(p, w.second.size(), PAGE_EXECUTE_READWRITE, &old);
        memcpy(p, w.second.data(), w.second.size());
        VirtualProtect(p, w.second.size(), old, &old);
        FlushInstructionCache(GetCurrentProcess(), p, w.second.size());
    }
    written.resize(mark.written);
    stubs.erase(stubs.begin() + mark.stubs, stubs.end());
    mid_hooks.erase(mid_hooks.begin() + mark.mids, mid_hooks.end());
    inline_hooks.erase(inline_hooks.begin() + mark.inlines, inline_hooks.end());
    inline_names.erase(inline_names.begin() + mark.inlines, inline_names.end());
    for (size_t i = mark.sites; i < MAX_SITES; i++)
        site_chains[i].clear();
    sites_used = mark.sites;
}

void chain(const Group &g, void *last)
{
    for (size_t i = 0; i < g.handlers.size(); i++)
        if (g.handlers[i].m->orig)
            *g.handlers[i].m->orig = i + 1 < g.handlers.size() ? g.handlers[i + 1].m->handler : last;
}

} // namespace

namespace {

std::string install_error;

const char *failed(std::string why, const Group &g, bool allocation)
{
    install_error = std::move(why) + "; bytes at the target: " + target_bytes(g.addr);
    if (allocation)
        hook_pool().dump_free_regions_near(g.addr);
    return install_error.c_str();
}

const char *install(Group &g)
{
    void *first = g.handlers[0].m->handler;
    if (code().packed() && g.kind != SWOFTY_MIXIN_VTABLE && code().page_encrypted(g.addr))
        return "the code there still reads encrypted";
    switch (g.kind) {
    case SWOFTY_MIXIN_WRAP: {
        auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(g.addr), first, safetyhook::InlineHook::StartDisabled);
        if (!h)
            return failed(runtime::hook_error(h.error()), g, h.error().type == safetyhook::InlineHook::Error::BAD_ALLOCATION);
        g.tramp = h->original<void *>();
        chain(g, g.tramp);
        if (auto on = h->enable(); !on)
            return failed("enabling it failed: " + runtime::hook_error(on.error()), g, false);
        inline_hooks.push_back(std::move(*h));
        inline_names.push_back(g.handlers[0].m->target);
        return nullptr;
    }
    case SWOFTY_MIXIN_SITE: {
        if (sites_used == MAX_SITES)
            return "too many sites";
        size_t s = sites_used++;
        g.site = s;
        site_chains[s].reserve(SITE_CHAIN_ROOM);
        for (auto &h : g.handlers)
            site_chains[s].push_back({reinterpret_cast<void (*)(SwoftyContext *)>(h.m->handler),
                                      perf::probe(perf::owner_of(h.m->handler), "site " + h.m->target)});
        auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(g.addr), site_thunks[s]);
        if (!h) {
            auto &e = h.error();
            bool alloc = e.type == safetyhook::MidHook::Error::BAD_ALLOCATION ||
                         e.inline_hook_error.type == safetyhook::InlineHook::Error::BAD_ALLOCATION;
            return failed(runtime::hook_error(e), g, alloc);
        }
        mid_hooks.push_back(std::move(*h));
        return nullptr;
    }
    case SWOFTY_MIXIN_VTABLE:
        chain(g, *reinterpret_cast<void **>(g.addr));
        return write_bytes(g.addr, &first, sizeof first) ? nullptr : "VirtualProtect failed";
    case SWOFTY_MIXIN_REDIRECT: {
        auto stub = safetyhook::Allocator::global()->allocate_near({reinterpret_cast<uint8_t *>(g.addr)}, 14);
        if (!stub) {
            install_error = std::string("the jump stub did not allocate: ") + alloc_error(stub.error());
            hook_pool().dump_free_regions_near(g.addr);
            return install_error.c_str();
        }
        uint8_t jmp[14] = {0xFF, 0x25};
        memcpy(jmp + 6, &first, 8);
        memcpy(stub->data(), jmp, sizeof jmp);
        chain(g, reinterpret_cast<void *>(g.addr + 5 + *reinterpret_cast<const int32_t *>(g.addr + 1)));
        int32_t rel = (int32_t)(stub->address() - (g.addr + 5));
        stubs.push_back(std::move(*stub));
        return write_bytes(g.addr + 1, &rel, 4) ? nullptr : "VirtualProtect failed";
    }
    case SWOFTY_MIXIN_PATCH: {
        auto *b = bindings().find(g.handlers[0].m->target);
        if (memcmp(reinterpret_cast<const void *>(g.addr), b->expect.data(), b->expect.size()))
            return "bytes changed since resolve";
        return write_bytes(g.addr, first, g.span) ? nullptr : "VirtualProtect failed";
    }
    }
    return "unknown kind";
}

bool install_all(std::vector<Group> &groups)
{
    for (auto &g : groups) {
        if (held_for(g))
            continue;
        const char *err = install(g);
        auto &target = g.handlers[0].m->target;
        if (err) {
            api->log("[core] %s on %s at 0x%llx: %s", kind_name(g.kind), target.c_str(), (unsigned long long)g.addr, err);
            uninstall_all();
            for (auto &h : g.handlers)
                drop(h.addon, "installing its %s on %s failed", kind_name(g.kind), target.c_str());
            return false;
        }
        api->log("[core] %s %s at 0x%llx (base+0x%llx) installed: %s", kind_name(g.kind), target.c_str(),
                 (unsigned long long)g.addr, image().rva(g.addr), owners(g).c_str());
    }
    return true;
}

void add_core_sets(std::vector<runtime::CoreMixins> sets)
{
    for (auto &c : sets) {
        Addon own{c.name, {}, true, true};
        for (auto &m : c.mixins)
            own.mixins.push_back({m.target, m.kind, m.priority, m.handler, m.orig});
        api->log("[core] core mixin set '%s': %zu mixin(s), installed or dropped on its own", c.name, own.mixins.size());
        addons.push_back(std::move(own));
    }
}

size_t offer_core_sets()
{
    size_t before = addons.size();
    for (auto *sets : {runtime::save_sandbox_mixins, runtime::career_content_mixins, runtime::characters_mixins, runtime::wizard_mixins,
                       runtime::tile_route_mixins, runtime::era_mixins, runtime::tt_guard_mixins, runtime::career_alias_mixins, runtime::car_category_mixins,
                       runtime::save_splice_mixins, runtime::ers_newlap_mixins, runtime::ers_mode_mixins, runtime::car_levers_mixins,
                       runtime::garage_crew_mixins, runtime::garage_placement_mixins, runtime::pit_slots_mixins, runtime::pit_box_mixins, runtime::garage_slots_mixins, runtime::chassis_registry_mixins, runtime::standings_guard_mixins, runtime::ai_corner_guard_mixins, runtime::replay_grid_guard_mixins, runtime::fe_tile_guard_mixins, runtime::myteam_vehicle_guard_mixins, runtime::myteam_vehicle_heal_mixins, runtime::myteam_vehicle_keep_mixins, runtime::tyre_slots_mixins, runtime::tyre_models_mixins, runtime::track_acclim_mixins,
                       runtime::audio_queue_mixins, runtime::heap_sizes_mixins, runtime::team_slots_mixins, runtime::track_select_mixins,
                       runtime::leaderboard_guard_mixins, runtime::lap_host_mixins, runtime::thrustmaster_lcd_mixins, runtime::estimate_laptime_mixins, runtime::red_flag_recompute_mixins, runtime::quali_tyres_mixins, runtime::box_grid_mixins, runtime::gp_continue_mixins, runtime::track_list_guard_mixins, runtime::watchdog_grace_mixins, runtime::garage_setup_mixins, runtime::theatre_list_mixins, runtime::tile_list_select_mixins, runtime::session_enter_mixins, runtime::ai_line_set_mixins})
        add_core_sets(sets());
    return addons.size() - before;
}

void settle_core_sets()
{
    runtime::save_sandbox_commit();
    runtime::tile_route_commit();
    runtime::career_content_commit();
    runtime::characters_commit();
    runtime::wizard_commit();
    runtime::era_commit();
    runtime::tt_guard_commit();
    runtime::career_alias_commit();
    runtime::car_category_commit();
    runtime::save_splice_commit();
    runtime::ers_newlap_commit();
    runtime::lap_host_commit();
    runtime::ers_mode_commit();
    runtime::car_levers_commit();
    runtime::standings_guard_commit();
    runtime::fe_tile_guard_commit();
    runtime::myteam_vehicle_guard_commit();
    runtime::myteam_vehicle_heal_commit();
    runtime::myteam_vehicle_keep_commit();
    runtime::ai_corner_guard_commit();
    runtime::replay_grid_guard_commit();
    runtime::watchdog_grace_commit();
    runtime::tyre_slots_commit();
    runtime::tyre_models_commit();
    runtime::garage_crew_commit();
    runtime::pit_slots_commit();
    runtime::pit_box_commit();
    runtime::garage_slots_commit();
    runtime::chassis_registry_commit();
    runtime::track_acclim_commit();
    runtime::audio_queue_commit();
    runtime::heap_sizes_commit();
    runtime::team_slots_commit();
    runtime::track_select_commit();
    runtime::leaderboard_guard_commit();
    runtime::thrustmaster_lcd_commit();
    runtime::estimate_laptime_commit();
    runtime::red_flag_recompute_commit();
    runtime::box_grid_commit();
    runtime::quali_tyres_commit();
    runtime::gp_continue_commit();
    runtime::track_list_guard_commit();
    runtime::garage_setup_commit();
    runtime::theatre_list_commit();
    runtime::tile_list_select_commit();
    runtime::session_enter_commit();
    runtime::ai_line_set_commit();
}

void extend_held(Held &h, const Group &g)
{
    auto rest = std::vector<Handler>(h.handlers.begin() + 1, h.handlers.end());
    rest.insert(rest.end(), g.handlers.begin(), g.handlers.end());
    std::stable_sort(rest.begin(), rest.end(), [](const Handler &a, const Handler &b) { return a.m->priority > b.m->priority; });
    size_t old = h.handlers.size();
    h.handlers.resize(1);
    h.handlers.insert(h.handlers.end(), rest.begin(), rest.end());
    if (h.kind == SWOFTY_MIXIN_WRAP) {
        Group all{h.kind, h.addr, h.span, h.handlers};
        for (size_t i = all.handlers.size(); i-- > 0;)
            if (all.handlers[i].m->orig)
                *all.handlers[i].m->orig = i + 1 < all.handlers.size() ? all.handlers[i + 1].m->handler : h.tramp;
    } else {
        for (auto &x : g.handlers)
            site_chains[h.site].push_back({reinterpret_cast<void (*)(SwoftyContext *)>(x.m->handler),
                                           perf::probe(perf::owner_of(x.m->handler), "site " + x.m->target)});
    }
    Group all{h.kind, h.addr, h.span, h.handlers};
    api->log("[core] %s %s at 0x%llx (base+0x%llx) chained onto the one installed earlier (%zu handler(s) before): %s",
             kind_name(h.kind), g.handlers[0].m->target.c_str(), (unsigned long long)h.addr, image().rva(h.addr), old,
             owners(all).c_str());
}

void install_groups(size_t from, bool late, size_t *count)
{
    std::vector<Group> groups;
    do
        groups = plan(from, late);
    while (drop_conflicts(groups, false) || !install_all(groups));
    drop_conflicts(groups, true);
    for (auto &g : groups) {
        count[SWOFTY_MIXIN_KIND(g.kind)]++;
        if (Held *h = held_for(g))
            extend_held(*h, g);
        else
            held.push_back({g.kind, g.addr, g.span, g.handlers, g.tramp, g.site});
    }
    mark = {written.size(), stubs.size(), mid_hooks.size(), inline_hooks.size(), sites_used};
}

void note_waiting(size_t i)
{
    if (addons[i].active && addons[i].waiting)
        api->log("[core] addon %s waits: a mixin target sits in code that still reads encrypted; it installs once that "
                 "code decrypts",
                 addons[i].name.c_str());
}

void install_pass(size_t from, size_t *count)
{
    settled = addons.size();
    for (size_t i = from; i < settled; i++)
        if (addons[i].active && !addons[i].reinit)
            check_targets(i), note_waiting(i);
    install_groups(from, false, count);
}

void run_on_load(size_t i)
{
    if (!addons[i].active || addons[i].waiting || addons[i].reinit)
        return;
    for (auto &x : addons[i].mixins)
        if (SWOFTY_MIXIN_KIND(x.kind) == SWOFTY_MIXIN_ON_LOAD)
            reinterpret_cast<void (*)()>(x.handler)();
}

bool waits(bool core)
{
    for (auto &a : addons)
        if (a.active && (a.waiting || a.reinit) && (!core || a.core))
            return true;
    return false;
}

void drop_module(HMODULE m)
{
    runtime::events_drop_module(m);
    runtime::career_start_drop_module(m);
    swofty_core::hooks::drop_module(m);
}

/* An SDK addon's init fails when a binding it wants is deferred, so it is called again once code decrypts;
   it registered nothing on the failed call. */
void reinit_pass(bool final)
{
    for (size_t i = 0; i < addons.size(); i++) {
        if (!addons[i].reinit || !addons[i].active)
            continue;
        addons[i].hit_deferred = false;
        addons[i].mixins.clear();
        current = &addons[i];
        int r = addons[i].reinit(api);
        current = nullptr;
        auto &a = addons[i];
        if (r) {
            a.reinit = nullptr, a.waiting = true;
            api->log("[core] addon %s: SwoftyAddonInit ran again now that its code decrypted and returned 1", a.name.c_str());
            continue;
        }
        a.mixins.clear();
        drop_module(a.module);
        if (!a.hit_deferred || final) {
            a.reinit = nullptr;
            drop(i, "SwoftyAddonInit returned 0 once its code %s", a.hit_deferred ? "still read encrypted at the last retry" : "decrypted");
        }
    }
}

void late_install(size_t *count)
{
    size_t from = settled;
    settled = addons.size();
    bool any = false;
    for (size_t i = 0; i < settled; i++) {
        auto &a = addons[i];
        if (!a.active || a.reinit || !(a.waiting || i >= from))
            continue;
        check_targets(i);
        if (a.active && !a.waiting)
            a.late = any = true;
    }
    if (any)
        install_groups(0, true, count);
    for (size_t i = 0; i < addons.size(); i++)
        if (addons[i].late)
            run_on_load(i), addons[i].late = false;
}

}

namespace runtime {

bool core_mixins_active(const char *name)
{
    for (auto &a : addons)
        if (a.name == name)
            return a.active && !a.waiting && !a.reinit && !a.mixins.empty();
    return false;
}

bool core_mixins_closed()
{
    return closed;
}

}

std::string core_code_owner(uintptr_t addr)
{
    for (auto &m : addons)
        for (auto &x : m.mixins)
            if (reinterpret_cast<uintptr_t>(x.handler) == addr && SWOFTY_MIXIN_KIND(x.kind) != SWOFTY_MIXIN_PATCH)
                return m.name + " " + kind_name(x.kind) + (x.target.empty() ? "" : " " + x.target) + " handler";
    for (size_t i = 0; i < inline_hooks.size(); i++)
        if (addr - inline_hooks[i].trampoline().address() < inline_hooks[i].trampoline().size())
            return "trampoline of the wrap on " + inline_names[i];
    return {};
}

namespace runtime {

std::string mixin_overwrite_on(const std::string &binding, uintptr_t at)
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    for (auto &a : addons)
        if (a.active && !a.core)
            for (auto &m : a.mixins)
                if ((m.kind & SWOFTY_MIXIN_OVERWRITE) && (m.target == binding || (at && m.addr == at)))
                    return a.name;
    return {};
}

size_t addon_load_index(uint64_t module)
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    for (size_t i = 0; i < addons.size(); i++)
        if (module && reinterpret_cast<uint64_t>(addons[i].module) == module)
            return i + 1;
    return 0;
}

std::string mixin_occupying(uintptr_t at)
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    for (size_t i = 0; i < inline_hooks.size(); i++)
        if (inline_hooks[i].target_address() == at)
            return "the wrap on " + inline_names[i];
    for (auto &m : mid_hooks)
        if (m.target_address() >= at && m.target_address() < at + 5)
            return "a mid-hook";
    for (auto &w : written)
        if (w.first < at + 5 && at < w.first + w.second.size())
            return "patched bytes";
    for (auto &h : held)
        if (h.addr < at + 5 && at < h.addr + h.span)
            return "a mixin installed at the early stage";
    return {};
}

}

namespace runtime::loader {

Addon *current_addon()
{
    return current;
}

const std::vector<uint8_t> *original_bytes_at(uintptr_t a, uintptr_t &start)
{
    return hooked_range(a, start);
}

uintptr_t resolve_for_addon(const char *id)
{
    std::lock_guard<std::recursive_mutex> g(resolve_lock());
    const char *name = id ? id : "(null)";
    Binding *b = bindings().find(id ? id : "");
    if (!b) {
        if (current)
            api->log("[core] addon %s needs %s, which %s does not list: this loader is older than the addon, or %s is "
                     "not an exported binding",
                     current->name.c_str(), name, bindings().file().c_str(), name);
        else
            api->log("[core] no binding %s in %s", name, bindings().file().c_str());
        return 0;
    }
    if (!b->resolved) {
        bool was = b->deferred;
        bindings().attempt(*b, false);
        if (was && !b->deferred)
            bindings().note_reported_change();
    }
    if (b->deferred) {
        if (current) {
            current->hit_deferred = true;
            api->log("[core] addon %s needs %s, which waits for its code to decrypt (%s)", current->name.c_str(), name,
                     b->tr.reason.c_str());
        }
        return 0;
    }
    if (!b->addr && current)
        api->log("[core] addon %s needs %s, which does not resolve on this build", current->name.c_str(), name);
    return b->addr;
}

int register_addon_mixins(const SwoftyMixin *m, size_t n)
{
    return api_register(m, n);
}

void read_mixin_manifest(const std::wstring &dir)
{
    std::wstring session_path = dir + L"..\\..\\session.json";
    std::string text = read_whole_file(session_path);
    have_session = !text.empty();
    if (have_session && !json::parse(text, session)) {
        api->log("[core] session.json unreadable; every mixin registration is refused");
        session = {};
    }
    api->log(have_session ? "[core] session.json: mixins not declared there are refused"
                          : "[core] no session.json; mixins are not checked against a manifest");
}

int init_addon(const wchar_t *name, SwoftyAddonInitFn init)
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    addons.push_back({utf8(name), {}, true});
    current = &addons.back();
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(init), &current->module);
    int r = init(api);
    if (!r) {
        current->mixins.clear();
        drop_module(current->module);
        if (code().packed() && current->hit_deferred) {
            current->reinit = init;
            api->log("[core] addon %s: its init needs code that still reads encrypted (a binding or a scan); "
                     "SwoftyAddonInit runs again once that code decrypts",
                     current->name.c_str());
            r = 1;
        } else {
            current->active = false;
        }
    }
    current = nullptr;
    return r;
}

void add_core_mixin_sets(std::vector<runtime::CoreMixins> sets)
{
    add_core_sets(std::move(sets));
}

size_t offer_core_mixin_sets()
{
    return offer_core_sets();
}

void settle_core_mixin_sets()
{
    settle_core_sets();
}

size_t settled_addons()
{
    return settled;
}

size_t active_addons_from(size_t from, size_t *total)
{
    size_t active = 0;
    for (size_t i = from; i < addons.size(); i++)
        active += addons[i].active;
    *total = addons.size() - from;
    return active;
}

void install_addons(size_t from, size_t *count)
{
    install_pass(from, count);
}

void run_addons_on_load(size_t from)
{
    for (size_t i = from; i < settled; i++)
        run_on_load(i);
}

bool addons_wait(bool core_only)
{
    return waits(core_only);
}

void reinit_deferred_addons(bool final)
{
    reinit_pass(final);
}

void install_late_addons(size_t *count)
{
    late_install(count);
}

void close_mixins(bool done)
{
    closed = done;
}

}
