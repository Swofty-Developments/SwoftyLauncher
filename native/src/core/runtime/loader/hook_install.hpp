#pragma once

#include "core/runtime/build.hpp"
#include "core/runtime/loader/hook_set.hpp"

#include <cstdint>
#include <string>
#include <utility>

#include <safetyhook.hpp>

namespace runtime::loader {

// One game site a core hook goes on: the signature that finds it, the tag and log channel runtime::find takes, and the
// build gate it needs (none when null).
struct HookSite {
    const runtime::Sig &sig;
    const char *tag;
    const char *channel;
    const runtime::build_proof::Gate *gate = nullptr;
};

// What one install did. A site with lines of its own reads `step`, `at` and `error`; the others log `why`:
//   not_proven  "<feature> is not proven on this build" (runtime::holds has logged its [build] line)
//   not_found   "<what> did not resolve on this build"
//   failed      "the hook on <what> at base+0x<rva> failed: <error>", or "at 0x<address>" outside the game's image
struct Installed {
    enum class Step : uint8_t { placed, not_proven, not_found, failed };
    Step step = Step::placed;
    uintptr_t at = 0;
    std::string error, why;
    explicit operator bool() const { return step == Step::placed; }
};

// Places a mid hook or an inline hook in `h` at an address the caller found, named `what` in the reason.
Installed install(safetyhook::MidHook &h, uintptr_t at, const char *what, safetyhook::MidHookFn fn);
Installed install(safetyhook::InlineHook &h, uintptr_t at, const char *what, void *detour,
                  safetyhook::InlineHook::Flags flags = safetyhook::InlineHook::Default);

// Checks the site's gate, finds its signature and places the hook there.
Installed install(safetyhook::MidHook &h, const HookSite &site, safetyhook::MidHookFn fn);
Installed install(safetyhook::InlineHook &h, const HookSite &site, void *detour,
                  safetyhook::InlineHook::Flags flags = safetyhook::InlineHook::Default);

// The same for a hook set: a refused install turns the set off with its reason.
template <class Lock, class Hook, class Where, class... Args>
bool install(HookSet<Lock> &set, Hook &h, const Where &where, Args &&...args)
{
    Installed r = install(h, where, std::forward<Args>(args)...);
    if (!r)
        set.turn_off(std::move(r.why));
    return bool(r);
}

}
