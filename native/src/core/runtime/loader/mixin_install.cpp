#include "core/runtime/loader/mixin_manager.hpp"

#include <algorithm>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "core/runtime/runtime.hpp"

// Planning and installing the registered mixins: the groups by target, the conflicts dropped, each group
// installed or chained onto the hook a group before it holds, and the late installs.
namespace runtime::loader {

namespace {

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
                api->log("[core] warning: an overwrite on %s hides the handlers below it: %s", target.c_str(), owners(g).c_str());
        }
        for (size_t y = x + 1; y < groups.size(); y++) {
            auto &o = groups[y];
            bool data = g.kind == SWOFTY_MIXIN_VTABLE, odata = o.kind == SWOFTY_MIXIN_VTABLE;
            if (data != odata || g.addr >= o.addr + o.span || o.addr >= g.addr + g.span)
                continue;
            api->log("[core] conflict: %s %s at 0x%llx overlaps %s %s at 0x%llx: %s vs %s", kind_name(g.kind), target.c_str(),
                     (unsigned long long)g.addr, kind_name(o.kind), o.handlers[0].m->target.c_str(), (unsigned long long)o.addr,
                     owners(g).c_str(), owners(o).c_str());
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

}
