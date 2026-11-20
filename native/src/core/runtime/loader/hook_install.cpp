#include "core/runtime/loader/hook_install.hpp"

#include "core/runtime/hook_errors.hpp"

#include <cstdio>

#include <swofty/mem.hpp>

namespace runtime::loader {

namespace {

Installed refused(Installed::Step step, uintptr_t at, const char *what, std::string error = {})
{
    Installed r;
    r.step = step;
    r.at = at;
    if (step == Installed::Step::not_found) {
        r.why = std::string(what) + " did not resolve on this build";
        return r;
    }
    char where[48];
    if (swofty::mem::in_image(at))
        snprintf(where, sizeof where, "base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
    else
        snprintf(where, sizeof where, "0x%llx", (unsigned long long)at);
    r.why = "the hook on " + std::string(what) + " at " + where + " failed: " + error;
    r.error = std::move(error);
    return r;
}

Installed placed(uintptr_t at)
{
    Installed r;
    r.at = at;
    return r;
}

Installed not_proven(const runtime::build_proof::Gate &gate)
{
    Installed r;
    r.step = Installed::Step::not_proven;
    r.why = std::string(gate.feature) + " is not proven on this build";
    return r;
}

}

Installed install(safetyhook::MidHook &h, uintptr_t at, const char *what, safetyhook::MidHookFn fn)
{
    if (!at)
        return refused(Installed::Step::not_found, 0, what);
    auto made = safetyhook::MidHook::create(reinterpret_cast<void *>(at), fn);
    if (!made)
        return refused(Installed::Step::failed, at, what, runtime::hook_error(made.error()));
    h = std::move(*made);
    return placed(at);
}

Installed install(safetyhook::InlineHook &h, uintptr_t at, const char *what, void *detour, safetyhook::InlineHook::Flags flags)
{
    if (!at)
        return refused(Installed::Step::not_found, 0, what);
    auto made = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), detour, flags);
    if (!made)
        return refused(Installed::Step::failed, at, what, runtime::hook_error(made.error()));
    h = std::move(*made);
    return placed(at);
}

Installed install(safetyhook::MidHook &h, const HookSite &site, safetyhook::MidHookFn fn)
{
    if (site.gate && !runtime::holds(*site.gate))
        return not_proven(*site.gate);
    return install(h, runtime::find(site.sig, site.tag, site.channel), site.sig.what, fn);
}

Installed install(safetyhook::InlineHook &h, const HookSite &site, void *detour, safetyhook::InlineHook::Flags flags)
{
    if (site.gate && !runtime::holds(*site.gate))
        return not_proven(*site.gate);
    return install(h, runtime::find(site.sig, site.tag, site.channel), site.sig.what, detour, flags);
}

}
