#include "core/runtime/loader/mixin_manager.hpp"

#include <mutex>
#include <string>

#include "core/runtime/loader/resolve_locks.hpp"
#include "core/runtime/runtime.hpp"

// What the installed mixins answer for the rest of the core: which core sets are active, who owns a piece of
// code, which overwrite sits on a binding and an addon's load index.
namespace runtime::loader {

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

}

namespace runtime {

bool core_mixins_active(const char *name) { return loader::mixins().core_set_active(name); }

bool core_mixins_closed() { return loader::mixins().closed(); }

std::string mixin_overwrite_on(const std::string &binding, uintptr_t at) { return loader::mixins().overwrite_on(binding, at); }

size_t addon_load_index(uint64_t module) { return loader::mixins().load_index(module); }

std::string mixin_occupying(uintptr_t at) { return loader::mixins().occupying(at); }

}

std::string core_code_owner(uintptr_t addr) { return runtime::loader::mixins().code_owner(addr); }
