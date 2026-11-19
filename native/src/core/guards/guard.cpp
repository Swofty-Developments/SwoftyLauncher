#include "core/guards/guard.hpp"
#include "core/guards/lines.hpp"
#include "core/guards/mid_hook.hpp"
#include "core/runtime/loader/hook_install.hpp"

#include <windows.h>

#include <array>
#include <cstring>
#include <utility>

namespace swofty_core::guards {

namespace {

std::array<const HookGuard *, HOOK_GUARD_COUNT> &hook_guards()
{
    static std::array<const HookGuard *, HOOK_GUARD_COUNT> guards{};
    return guards;
}

}

bool env_off(const char *name, char (&value)[16], bool (*off_value)(const char *))
{
    value[0] = 0;
    DWORD n = GetEnvironmentVariableA(name, value, sizeof value);
    if (!n || n >= sizeof value)
        return false;
    return off_value ? off_value(value) : switch_off(value);
}

std::vector<runtime::CoreMixins> Guard::offer()
{
    std::vector<runtime::CoreMixins> out;
    if (gate_.offered() || !runtime::code_ready() || !ready())
        return out;
    gate_.offer();
    build(out);
    for (const runtime::CoreMixins &set : out)
        offered_.push_back(set.name);
    return out;
}

bool Guard::offered_set(const char *set) const
{
    for (const char *name : offered_)
        if (!strcmp(name, set))
            return true;
    return false;
}

void Guard::commit()
{
    if (!gate_.settle())
        return;
    settle();
}

bool resolve(const std::vector<Keep> &keeps, const std::vector<const char *> &needs)
{
    bool found = true;
    for (auto &k : keeps)
        found &= (*k.address = swofty::api->resolve(k.binding)) != 0;
    for (const char *need : needs) {
        if (!found)
            break;
        bool keep = false;
        for (auto &k : keeps)
            keep |= !strcmp(k.binding, need);
        if (!keep)
            found = swofty::api->resolve(need) != 0;
    }
    if (!found)
        for (auto &k : keeps)
            *k.address = 0;
    return found;
}

SiteGuard::SiteGuard(CoreSetId id, SiteGuardSpec spec) : Guard(id), spec_(std::move(spec)) {}

void SiteGuard::build(std::vector<runtime::CoreMixins> &out)
{
    char value[16];
    if (env_off(spec_.env, value)) {
        swofty::log("%s", lines::off(spec_.name, spec_.env, value).c_str());
        return;
    }
    if (!resolve(spec_.keeps, spec_.needs)) {
        swofty::log("%s", lines::unresolved(spec_.name, spec_.needs).c_str());
    } else if (admit()) {
        runtime::CoreMixins set{spec_.set, spec_.mixins};
        extend(set);
        out.push_back(std::move(set));
    }
    for (const SitePart &part : spec_.parts) {
        if (!resolve(part.keeps, part.needs)) {
            swofty::log("%s", lines::unresolved(part.name, part.needs).c_str());
            continue;
        }
        out.push_back({part.set, part.mixins});
    }
}

namespace {

void settled(const char *set, const char *name, const char *armed)
{
    if (runtime::core_mixins_active(set))
        swofty::log("%s", lines::armed(name, armed).c_str());
    else
        swofty::log("%s", lines::not_installed(name).c_str());
}

}

void SiteGuard::settle()
{
    if (offered_set(spec_.set) || spec_.settles_when_off)
        settled(spec_.set, spec_.name, spec_.armed);
    if (spec_.on_armed && offered_set(spec_.set) && runtime::core_mixins_active(spec_.set))
        spec_.on_armed();
    for (const SitePart &part : spec_.parts)
        if (offered_set(part.set))
            settled(part.set, part.name, part.armed);
}

HookGuard::HookGuard(HookGuardId id, const char *what, void (*setup)()) : id_(id), what_(what), setup_(setup)
{
    hook_guards()[size_t(id)] = this;
}

void HookGuard::arm() const { runtime::when_code_ready(what_, setup_); }

void arm_next_hook_guard()
{
    static size_t next = 0;
    while (next < HOOK_GUARD_COUNT)
        if (const HookGuard *guard = hook_guards()[next++]) {
            guard->arm();
            return;
        }
}

void install_mid_hook(const MidHookSpec &spec)
{
    char v[16] = {};
    if (env_off(spec.env, v, spec.off_value)) {
        swofty::log("%s", lines::off(spec.name, spec.env, v).c_str());
        return;
    }
    uintptr_t at = spec.site();
    if (!at)
        return;
    ::runtime::loader::Installed r = ::runtime::loader::install(spec.hook, at, spec.name, spec.destination);
    if (!r) {
        swofty::log("%s", lines::hook_failed(spec.name, spec.failed_at, r.error).c_str());
        return;
    }
    swofty::log("%s", lines::armed(spec.name, spec.armed).c_str());
}

}
