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
#include "core/runtime/settings.hpp"
#include "core/diagnostics/steps.hpp"

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

void drop_module(HMODULE m)
{
    runtime::events_drop_module(m);
    runtime::career_start_drop_module(m);
    swofty_core::hooks::drop_module(m);
}

}

const char *kind_name(uint32_t k)
{
    static const char *names[] = {"?", "wrap", "site", "vtable", "redirect", "patch", "on_load"};
    k = SWOFTY_MIXIN_KIND(k);
    return k < 7 ? names[k] : "?";
}

MixinManager &mixins() { return manager; }

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
            api->log("[core] addon %s: %s on %s is not declared in session.json; registration refused", current_->name.c_str(),
                     kind_name(k), m[i].target);
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
            return drop(i, bindings().note_use(m.target) ? "needs %s, which %s drops" : "needs %s, which %s does not list",
                        m.target.c_str(), bindings().file().c_str());
        m.addr = resolve(m.target.c_str());
        if (b->deferred) {
            addons_[i].waiting = true;
            continue;
        }
        if (!m.addr)
            return drop(i, "needs %s, which does not resolve on this build", m.target.c_str());
        const std::string &bk = b->kind;
        bool ok = k == SWOFTY_MIXIN_WRAP       ? bk == "func"
                  : k == SWOFTY_MIXIN_VTABLE   ? bk == "vtable"
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

void MixinManager::settle_core_sets() { loader::commit_core_sets(); }

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
            drop(i, "SwoftyAddonInit returned 0 once its code %s",
                 a.hit_deferred ? "still read encrypted at the last retry" : "decrypted");
        }
    }
}

uintptr_t MixinManager::resolve(const char *id)
{
    std::lock_guard<std::recursive_mutex> g(resolve_lock());
    const char *name = id ? id : "(null)";
    std::string key = id ? id : "";
    bool dropped = bindings().note_use(key);
    Binding *b = bindings().find(key);
    if (!b) {
        if (dropped)
            return 0;
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

Addon *current_addon() { return manager.current(); }

const std::vector<uint8_t> *original_bytes_at(uintptr_t a, uintptr_t &start) { return patches.original_bytes(a, start); }

uintptr_t resolve_for_addon(const char *id) { return manager.resolve(id); }

int register_addon_mixins(const SwoftyMixin *m, size_t n) { return manager.register_mixins(m, n); }

}
