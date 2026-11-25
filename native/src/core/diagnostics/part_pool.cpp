#include "core/diagnostics/part_pool_logic.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/hook_errors.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>

// Logs how full the game's render model part pool is. A trace-only mid hook on the call of Ne_BitAllocator_AllocBit in
// the render model slot loop (found by signature, Steam 0x3d3d845) keeps the allocator object the call gets in
// rcx, one relaxed store per part. The pool is read with the core's page-table checked reads, which never raise an
// exception and never touch a page: at every StateLoadSession Entering and about 5 s after StateSession starts running
// it logs the set bits, the capacity and the high-water index (part_pool_logic.hpp). It reads without the allocator's
// lock, so a count taken while a model loads can be a few slots behind. Steam 1.0.145 only: the bit layout was read
// there. SWOFTY_PART_POOL_LOG=0 turns it off.

namespace {

namespace bp = runtime::build_proof;
namespace pp = runtime::part_pool;

const runtime::Sig SITE{"the bit allocator call in the render model slot loop",
                        "49 8B 4C 24 20 E8 ?? ?? ?? ?? 49 8B 4C 24 58 89 04 B9 48 FF C7 0F B7 46 48 48 39 C7 72",
                        0x3d3d845,
                        runtime::Sig::addr,
                        0,
                        5};
constexpr DWORD SETTLE_MS = 5000;

constexpr bp::Proof LAYOUT{"the model part pool's allocator layout (+0x8, +0x10, +0x20, +0x24, +0x59)",
                           bp::bit(bp::Build::steam_1_0_145)};
constexpr bp::Gate GATE{"the model part pool log", &LAYOUT};

safetyhook::MidHook hook;
constinit std::atomic<uint64_t> allocator{0};
constinit std::atomic<bool> armed{false};
constinit std::atomic<uint32_t> running_serial{0};

void on_alloc(safetyhook::Context &c) { allocator.store(c.rcx, std::memory_order_relaxed); }

bool read_game(uint64_t at, void *out, size_t n)
{
    if (!at || at + n < at || !swofty::mem::readable(uintptr_t(at), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(at), n);
    return true;
}

void report(const char *when)
{
    pp::Reading r = pp::capture(read_game, allocator.load(std::memory_order_relaxed));
    char text[256];
    pp::line(r, when, text, sizeof text);
    swofty::log("%s", text);
}

DWORD WINAPI settle(LPVOID arg)
{
    uint32_t serial = uint32_t(reinterpret_cast<uintptr_t>(arg));
    Sleep(SETTLE_MS);
    if (running_serial.load() == serial)
        report("session ready + 5 s");
    return 0;
}

}

namespace runtime {

void part_pool_setup()
{
    static bool done;
    if (done)
        return;
    done = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_PART_POOL_LOG", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[render] model part pool log off (SWOFTY_PART_POOL_LOG=%s)", v);
        return;
    }
    if (!runtime::holds(GATE))
        return;
    uintptr_t at = runtime::find(SITE, "setup", "render");
    if (!at) {
        swofty::log("[render] model part pool log off: %s did not resolve on this build", SITE.what);
        return;
    }
    auto made = safetyhook::MidHook::create(reinterpret_cast<void *>(at), on_alloc);
    if (!made) {
        swofty::log("[render] model part pool log: the hook failed: %s", runtime::hook_error(made.error()).c_str());
        return;
    }
    hook = std::move(*made);
    armed.store(true);
    swofty::log("[render] model part pool log hooked at 0x%llx (%s): it logs the part pool's set bits, capacity and "
                "high-water index at StateLoadSession and 5 s after the session is ready",
                (unsigned long long)at, SITE.what);
}

void part_pool_flow(uint32_t to, const char *name)
{
    if (!armed.load(std::memory_order_relaxed) || !name)
        return;
    if (to == 1 && !std::strcmp(name, "StateLoadSession")) {
        running_serial.fetch_add(1);
        report("StateLoadSession");
    } else if (to == 2 && !std::strcmp(name, "StateSession")) {
        uint32_t serial = running_serial.fetch_add(1) + 1;
        if (HANDLE t = CreateThread(nullptr, 64 << 10, settle, reinterpret_cast<LPVOID>(uintptr_t(serial)), 0, nullptr))
            CloseHandle(t);
    }
}

}
