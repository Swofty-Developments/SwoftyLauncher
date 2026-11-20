#pragma once

#include "core/guards/guard.hpp"

#include <safetyhook.hpp>

#include <cstdint>

namespace swofty_core::guards {

// A hook guard with one mid-function hook. install_mid_hook() logs the off line when `env` turns it off, asks `site`
// for the address to hook (`site` logs its own reason when it returns 0), hooks it into `hook` and logs the hook
// failure or armed line; every line starts with `name` (core/guards/lines.hpp). `failed_at` names the site in the hook
// failure line; `off_value` replaces switch_off for the environment value.
struct MidHookSpec {
    const char *name;
    const char *env;
    uintptr_t (*site)();
    safetyhook::MidHookFn destination;
    safetyhook::MidHook &hook;
    const char *armed;
    const char *failed_at = nullptr;
    bool (*off_value)(const char *) = nullptr;
};

void install_mid_hook(const MidHookSpec &spec);

}

#define SWOFTY_MID_HOOK_GUARD(id, what, spec)                                                                                    \
    namespace {                                                                                                                  \
    void mid_hook_setup() { ::swofty_core::guards::install_mid_hook(spec); }                                                     \
    }                                                                                                                            \
    SWOFTY_HOOK_GUARD(id, what, mid_hook_setup)
