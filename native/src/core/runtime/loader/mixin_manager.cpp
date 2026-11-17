#include "core/runtime/loader/mixin_manager.hpp"

#include <algorithm>
#include <cstdio>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

#include "core/hooks/hooks.hpp"
#include "core/runtime/loader/bindings.hpp"
#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/introspection.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "core/runtime/loader/scanner.hpp"
#include "core/runtime/runtime.hpp"
#include "core/diagnostics/steps.hpp"

std::string settings_addon(const void *from, std::string &dll);

namespace runtime::loader {

namespace {

constinit MixinManager manager;

int timed_init(SwoftyAddonInitFn init, std::string dll, const char *pass)
{
    perf::Stopwatch watch;
    int r = init(api);
    double ms = watch.total_ms();
    api->log(perf::STEP_LINE, ms, ("addon init " + dll + pass).c_str());
    if (ms > perf::SLOW_ADDON_INIT_MS) {
        std::string module, id = settings_addon(reinterpret_cast<const void *>(init), module);
        api->log(perf::SLOW_ADDON_INIT_LINE, ms, (id.empty() ? dll : id).c_str());
    }
    return r;
}

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

bool same_dll(const std::string &decl, const std::string &dll)
{
    auto slash = decl.find_last_of("/\\");
    std::string a = slash == std::string::npos ? decl : decl.substr(slash + 1);
    return a.size() == dll.size() && !_stricmp(a.c_str(), dll.c_str());
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
    return g.kind != SWOFTY_MIXIN_SITE || patches.chain_fits(h.site, h.handlers.size() + g.handlers.size());
}

void drop_module(HMODULE m)
{
    runtime::events_drop_module(m);
    runtime::career_start_drop_module(m);
    swofty_core::hooks::drop_module(m);
}

}

MixinManager &mixins()
{
    return manager;
}

bool MixinManager::declared(const Addon &addon, const SwoftyMixin &m) const
{
    for (auto &e : session_["native"].a) {
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

int MixinManager::register_mixins(const SwoftyMixin *m, size_t n)
{
    if (!current_) {
        api->log("[core] register_mixins outside SwoftyAddonInit; refused");
        return 0;
    }
    std::vector<Mixin> add;
    for (size_t i = 0; i < n; i++) {
        uint32_t k = SWOFTY_MIXIN_KIND(m[i].kind);
        bool on_load = k == SWOFTY_MIXIN_ON_LOAD;
        if (k < SWOFTY_MIXIN_WRAP || k > SWOFTY_MIXIN_ON_LOAD || !m[i].handler || (!on_load && !m[i].target) ||
            (k == SWOFTY_MIXIN_PATCH && !SWOFTY_MIXIN_ARG(m[i].kind))) {
            api->log("[core] addon %s: mixin %zu is malformed (kind 0x%x); registration refused", current_->name.c_str(), i,
                     m[i].kind);
            return 0;
        }
        if (!on_load && have_session_ && !declared(*current_, m[i])) {
            api->log("[core] addon %s: %s on %s is not declared in session.json; registration refused",
                     current_->name.c_str(), kind_name(k), m[i].target);
            return 0;
        }
        add.push_back({on_load ? "" : m[i].target, m[i].kind, m[i].priority, m[i].handler, m[i].orig});
    }
    current_->mixins.insert(current_->mixins.end(), add.begin(), add.end());
    api->log("[core] addon %s: registered %zu mixin(s)", current_->name.c_str(), n);
    return 1;
}

void MixinManager::drop(size_t i, const char *fmt, const char *a, const char *b, const char *c)
{
    if (!addons_[i].active)
        return;
    char why[512];
    snprintf(why, sizeof why, fmt, a, b, c);
    addons_[i].active = false;
    api->log("[core] addon %s dropped: %s; none of its mixins install", addons_[i].name.c_str(), why);
}

void MixinManager::check_targets(size_t i)
{
    addons_[i].waiting = false;
    for (auto &m : addons_[i].mixins) {
        uint32_t k = SWOFTY_MIXIN_KIND(m.kind), arg = SWOFTY_MIXIN_ARG(m.kind);
        if (k == SWOFTY_MIXIN_ON_LOAD)
            continue;
        Binding *b = bindings().find(m.target);
        if (!b)
            return drop(i, "needs %s, which %s does not list", m.target.c_str(), bindings().file().c_str());
        m.addr = resolve(m.target.c_str());
        if (b->deferred) {
            addons_[i].waiting = true;
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

std::vector<Group> MixinManager::plan(size_t from, bool late)
{
    std::vector<Group> groups;
    for (size_t i = from; i < addons_.size(); i++) {
        auto &a = addons_[i];
        if (!a.active || a.waiting || a.reinit || (late && !a.late))
            continue;
        for (auto &m : addons_[i].mixins) {
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

std::string MixinManager::owners(const Group &g) const
{
    std::string s;
    for (auto &h : g.handlers)
        s += (s.empty() ? "" : " > ") + addons_[h.addon].name + "(" + std::to_string(h.m->priority) +
             (h.m->kind & SWOFTY_MIXIN_OVERWRITE ? ", overwrite" : "") + ")";
    return s;
}

bool MixinManager::core_only(const Group &g) const
{
    for (auto &h : g.handlers)
        if (!addons_[h.addon].core)
            return false;
    return true;
}

Held *MixinManager::held_for(const Group &g)
{
    for (auto &h : held_)
        if (chains_onto(g, h))
            return &h;
    return nullptr;
}

bool MixinManager::drop_conflicts(const std::vector<Group> &groups, bool warn)
{
    std::vector<size_t> bad;
    for (size_t x = 0; x < groups.size(); x++) {
        auto &g = groups[x];
        auto &target = g.handlers[0].m->target;
        std::string over;
        for (auto &h : g.handlers)
            if (h.m->kind & SWOFTY_MIXIN_OVERWRITE)
                over += (over.empty() ? "" : ", ") + addons_[h.addon].name, bad.push_back(h.addon);
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
        for (auto &h : held_) {
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

bool MixinManager::install_all(std::vector<Group> &groups)
{
    for (auto &g : groups) {
        if (held_for(g))
            continue;
        const char *err = patches.install(g);
        auto &target = g.handlers[0].m->target;
        if (err) {
            api->log("[core] %s on %s at 0x%llx: %s", kind_name(g.kind), target.c_str(), (unsigned long long)g.addr, err);
            patches.rollback();
            for (auto &h : g.handlers)
                drop(h.addon, "installing its %s on %s failed", kind_name(g.kind), target.c_str());
            return false;
        }
        api->log("[core] %s %s at 0x%llx (base+0x%llx) installed: %s", kind_name(g.kind), target.c_str(),
                 (unsigned long long)g.addr, image().rva(g.addr), owners(g).c_str());
    }
    return true;
}

void MixinManager::add_core_sets(std::vector<runtime::CoreMixins> sets)
{
    for (auto &c : sets) {
        Addon own{c.name, {}, true, true};
        for (auto &m : c.mixins)
            own.mixins.push_back({m.target, m.kind, m.priority, m.handler, m.orig});
        api->log("[core] core mixin set '%s': %zu mixin(s), installed or dropped on its own", c.name, own.mixins.size());
        addons_.push_back(std::move(own));
    }
}

size_t MixinManager::offer_core_sets()
{
    size_t before = addons_.size();
    loader::offer_core_sets([](std::vector<runtime::CoreMixins> sets) { manager.add_core_sets(std::move(sets)); });
    return addons_.size() - before;
}

void MixinManager::settle_core_sets()
{
    loader::commit_core_sets();
}

void MixinManager::extend_held(Held &h, const Group &g)
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
        patches.extend_site(h.site, g.handlers);
    }
    Group all{h.kind, h.addr, h.span, h.handlers};
    api->log("[core] %s %s at 0x%llx (base+0x%llx) chained onto the one installed earlier (%zu handler(s) before): %s",
             kind_name(h.kind), g.handlers[0].m->target.c_str(), (unsigned long long)h.addr, image().rva(h.addr), old,
             owners(all).c_str());
}

void MixinManager::install_groups(size_t from, bool late, size_t *count)
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
            held_.push_back({g.kind, g.addr, g.span, g.handlers, g.tramp, g.site});
    }
    patches.keep();
}

void MixinManager::note_waiting(size_t i) const
{
    if (addons_[i].active && addons_[i].waiting)
        api->log("[core] addon %s waits: a mixin target sits in code that still reads encrypted; it installs once that "
                 "code decrypts",
                 addons_[i].name.c_str());
}

void MixinManager::install(size_t from, size_t *count)
{
    settled_ = addons_.size();
    for (size_t i = from; i < settled_; i++)
        if (addons_[i].active && !addons_[i].reinit)
            check_targets(i), note_waiting(i);
    install_groups(from, false, count);
}

void MixinManager::run_on_load(size_t i)
{
    if (!addons_[i].active || addons_[i].waiting || addons_[i].reinit)
        return;
    for (auto &x : addons_[i].mixins)
        if (SWOFTY_MIXIN_KIND(x.kind) == SWOFTY_MIXIN_ON_LOAD)
            reinterpret_cast<void (*)()>(x.handler)();
}

void MixinManager::run_on_load_from(size_t from)
{
    for (size_t i = from; i < settled_; i++)
        run_on_load(i);
}

bool MixinManager::waiting(bool core) const
{
    for (auto &a : addons_)
        if (a.active && (a.waiting || a.reinit) && (!core || a.core))
            return true;
    return false;
}

/* An SDK addon's init fails when a binding it wants is deferred, so it is called again once code decrypts;
   it registered nothing on the failed call. */
void MixinManager::reinit_deferred(bool final)
{
    for (size_t i = 0; i < addons_.size(); i++) {
        if (!addons_[i].reinit || !addons_[i].active)
            continue;
        addons_[i].hit_deferred = false;
        addons_[i].mixins.clear();
        current_ = &addons_[i];
        int r = timed_init(addons_[i].reinit, addons_[i].name, " (again, its code decrypted)");
        current_ = nullptr;
        auto &a = addons_[i];
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

void MixinManager::install_late(size_t *count)
{
    size_t from = settled_;
    settled_ = addons_.size();
    bool any = false;
    for (size_t i = 0; i < settled_; i++) {
        auto &a = addons_[i];
        if (!a.active || a.reinit || !(a.waiting || i >= from))
            continue;
        check_targets(i);
        if (a.active && !a.waiting)
            a.late = any = true;
    }
    if (any)
        install_groups(0, true, count);
    for (size_t i = 0; i < addons_.size(); i++)
        if (addons_[i].late)
            run_on_load(i), addons_[i].late = false;
}

size_t MixinManager::active_from(size_t from, size_t *total) const
{
    size_t active = 0;
    for (size_t i = from; i < addons_.size(); i++)
        active += addons_[i].active;
    *total = addons_.size() - from;
    return active;
}

bool MixinManager::core_set_active(const char *name) const
{
    for (auto &a : addons_)
        if (a.name == name)
            return a.active && !a.waiting && !a.reinit && !a.mixins.empty();
    return false;
}

std::string MixinManager::code_owner(uintptr_t addr) const
{
    for (auto &m : addons_)
        for (auto &x : m.mixins)
            if (reinterpret_cast<uintptr_t>(x.handler) == addr && SWOFTY_MIXIN_KIND(x.kind) != SWOFTY_MIXIN_PATCH)
                return m.name + " " + kind_name(x.kind) + (x.target.empty() ? "" : " " + x.target) + " handler";
    return patches.trampoline_owner(addr);
}

std::string MixinManager::overwrite_on(const std::string &binding, uintptr_t at) const
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    for (auto &a : addons_)
        if (a.active && !a.core)
            for (auto &m : a.mixins)
                if ((m.kind & SWOFTY_MIXIN_OVERWRITE) && (m.target == binding || (at && m.addr == at)))
                    return a.name;
    return {};
}

size_t MixinManager::load_index(uint64_t module) const
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    for (size_t i = 0; i < addons_.size(); i++)
        if (module && reinterpret_cast<uint64_t>(addons_[i].module) == module)
            return i + 1;
    return 0;
}

std::string MixinManager::occupying(uintptr_t at) const
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    if (std::string s = patches.occupying(at); !s.empty())
        return s;
    for (auto &h : held_)
        if (h.addr < at + 5 && at < h.addr + h.span)
            return "a mixin installed at the early stage";
    return {};
}

uintptr_t MixinManager::resolve(const char *id)
{
    std::lock_guard<std::recursive_mutex> g(resolve_lock());
    const char *name = id ? id : "(null)";
    Binding *b = bindings().find(id ? id : "");
    if (!b) {
        if (current_)
            api->log("[core] addon %s needs %s, which %s does not list: this loader is older than the addon, or %s is "
                     "not an exported binding",
                     current_->name.c_str(), name, bindings().file().c_str(), name);
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
        if (current_) {
            current_->hit_deferred = true;
            api->log("[core] addon %s needs %s, which waits for its code to decrypt (%s)", current_->name.c_str(), name,
                     b->tr.reason.c_str());
        }
        return 0;
    }
    if (!b->addr && current_)
        api->log("[core] addon %s needs %s, which does not resolve on this build", current_->name.c_str(), name);
    return b->addr;
}

void MixinManager::read_manifest(const std::wstring &dir)
{
    std::wstring session_path = dir + L"..\\..\\session.json";
    std::string text = read_whole_file(session_path);
    have_session_ = !text.empty();
    if (have_session_ && !json::parse(text, session_)) {
        api->log("[core] session.json unreadable; every mixin registration is refused");
        session_ = {};
    }
    api->log(have_session_ ? "[core] session.json: mixins not declared there are refused"
                           : "[core] no session.json; mixins are not checked against a manifest");
}

int MixinManager::init_addon(const wchar_t *name, SwoftyAddonInitFn init)
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    addons_.push_back({utf8(name), {}, true});
    current_ = &addons_.back();
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(init), &current_->module);
    int r = timed_init(init, current_->name, "");
    if (!r) {
        current_->mixins.clear();
        drop_module(current_->module);
        if (code().packed() && current_->hit_deferred) {
            current_->reinit = init;
            api->log("[core] addon %s: its init needs code that still reads encrypted (a binding or a scan); "
                     "SwoftyAddonInit runs again once that code decrypts",
                     current_->name.c_str());
            r = 1;
        } else {
            current_->active = false;
        }
    }
    current_ = nullptr;
    return r;
}

Addon *current_addon()
{
    return manager.current();
}

const std::vector<uint8_t> *original_bytes_at(uintptr_t a, uintptr_t &start)
{
    return patches.original_bytes(a, start);
}

uintptr_t resolve_for_addon(const char *id)
{
    return manager.resolve(id);
}

int register_addon_mixins(const SwoftyMixin *m, size_t n)
{
    return manager.register_mixins(m, n);
}

}

namespace runtime {

bool core_mixins_active(const char *name)
{
    return loader::mixins().core_set_active(name);
}

bool core_mixins_closed()
{
    return loader::mixins().closed();
}

std::string mixin_overwrite_on(const std::string &binding, uintptr_t at)
{
    return loader::mixins().overwrite_on(binding, at);
}

size_t addon_load_index(uint64_t module)
{
    return loader::mixins().load_index(module);
}

std::string mixin_occupying(uintptr_t at)
{
    return loader::mixins().occupying(at);
}

}

std::string core_code_owner(uintptr_t addr)
{
    return runtime::loader::mixins().code_owner(addr);
}
