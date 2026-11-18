#include "core/guards/guard.hpp"
#include "core/guards/lines.hpp"
#include "core/guards/mid_hook.hpp"
#include "core/runtime/hook_errors.hpp"

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
    return out;
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
        wanted_ = true;
        runtime::CoreMixins set{spec_.set, spec_.mixins};
        extend(set);
        out.push_back(std::move(set));
    }
    parts_offered_.assign(spec_.parts.size(), false);
    for (size_t i = 0; i < spec_.parts.size(); i++) {
        const SitePart &part = spec_.parts[i];
        if (!resolve(part.keeps, part.needs)) {
            swofty::log("%s", lines::unresolved(part.name, part.needs).c_str());
            continue;
        }
        parts_offered_[i] = true;
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
    if (wanted_ || spec_.settles_when_off)
        settled(spec_.set, spec_.name, spec_.armed);
    for (size_t i = 0; i < parts_offered_.size(); i++)
        if (parts_offered_[i])
            settled(spec_.parts[i].set, spec_.parts[i].name, spec_.parts[i].armed);
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
    auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), spec.destination);
    if (!h) {
        swofty::log("%s", lines::hook_failed(spec.name, spec.failed_at, runtime::hook_error(h.error())).c_str());
        return;
    }
    spec.hook = std::move(*h);
    swofty::log("%s", lines::armed(spec.name, spec.armed).c_str());
}

}
