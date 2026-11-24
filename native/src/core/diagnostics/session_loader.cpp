#include "core/diagnostics/session_loader.hpp"
#include "core/diagnostics/session_loader_logic.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

// Entry probes for the load-stall report's session loader section (session_loader_logic.hpp). Trace-only mid hooks at
// the first instruction of Game_StreamingAssetLoadTask_Update, which the loader's tick runs every frame while the
// loader is in state 7, and of Sess_Loader_UnloadSession, which every session unload calls once, keep the loader
// they were called on (rcx), when and how often. A call costs two relaxed stores and one atomic add; nothing reads
// game memory there. The report reads the loader from the newest of the two with the core's page-table checked reads,
// which never raise an exception and never touch a page. SWOFTY_SESSION_LOADER_PROBE=0 turns both off.

namespace {

namespace sl = runtime::session_loader;
namespace bp = runtime::build_proof;

constexpr const char *UPDATE = "Game_StreamingAssetLoadTask_Update";
constexpr const char *UNLOAD = "Sess_Loader_UnloadSession";

constexpr bp::Proof LAYOUT{"the session loader's fields (+0x8, +0x10, +0x1c, +0xc0, +0x158, +0x188, +0x190, +0x220, "
                           "+0x228) and its id lookup",
                           bp::bit(bp::Build::kb) | bp::bit(bp::Build::ea) | bp::bit(bp::Build::steam_1_0_145) |
                               bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate GATE{"the session loader's decoded fields", &LAYOUT};

constexpr bp::Proof UNLOAD_LAYOUT{"the session loader's unload tick gates (+0xc, +0x10, +0x14, +0x1c, +0x24, +0x110), "
                                  "sub-task lists (+0x1d8, +0x200, +0x1f0) and the vehicle spawning sub-task",
                                  bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate UNLOAD_GATE{"the session loader's unload gates", &UNLOAD_LAYOUT};

constexpr bp::Proof WORLD_LAYOUT{"the world task fields (+0x60, +0x88, +0xa0, +0xc8, +0xcc), its module's world lists "
                                 "(+0xe8, +0xf0, +0xf8) and a world's name, state (+0x200) and stop-pending byte (+0x218)",
                                 bp::bit(bp::Build::steam_1_0_145)};
constexpr bp::Gate WORLD_GATE{"the session loader's world tasks", &WORLD_LAYOUT};

struct Probe {
    std::atomic<bool> hooked{false};
    std::atomic<uint64_t> at{0}, ms{0}, calls{0};
};

safetyhook::MidHook update_hook, unload_hook;
constinit Probe update_probe, unload_probe;
constinit std::atomic<bool> off{false};
constinit std::atomic<bool> decoded{false};
constinit std::atomic<bool> unload_decoded{false};
constinit std::atomic<bool> world_decoded{false};
constinit std::atomic<uint64_t> entering_ms{0};
char world_proof_line[320];
constinit sl::WorldSet entering_set{};
char unload_proof_line[320];
char off_why[256];
char proof_line[320];

void seen(Probe &p, uint64_t rcx)
{
    p.at.store(rcx, std::memory_order_relaxed);
    p.ms.store(GetTickCount64(), std::memory_order_relaxed);
    p.calls.fetch_add(1, std::memory_order_relaxed);
}

void on_update(safetyhook::Context &c) { seen(update_probe, c.rcx); }

void on_unload(safetyhook::Context &c) { seen(unload_probe, c.rcx); }

bool place(Probe &p, safetyhook::MidHook &hook, const char *name, safetyhook::MidHookFn fn, std::string &why)
{
    uintptr_t at = swofty::api->resolve(name);
    if (!at) {
        why += std::string(why.empty() ? "" : "; ") + name + " does not resolve on this build";
        return false;
    }
    if (auto r = runtime::loader::install(hook, at, name, fn); !r) {
        why += std::string(why.empty() ? "" : "; ") + "the hook on " + name + " failed: " + r.error;
        return false;
    }
    p.hooked.store(true);
    return true;
}

bool read_game(uint64_t at, void *out, size_t n)
{
    if (!at || at + n < at || !swofty::mem::readable(uintptr_t(at), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(at), n);
    return true;
}

sl::Seen seen_of(const Probe &p)
{
    sl::Seen s;
    s.hooked = p.hooked.load(std::memory_order_relaxed);
    s.at = p.at.load(std::memory_order_relaxed);
    s.ms = p.ms.load(std::memory_order_relaxed);
    s.calls = p.calls.load(std::memory_order_relaxed);
    return s;
}

constinit sl::Snapshot snapshot{};

}

namespace runtime {

void session_loader_setup()
{
    static bool done;
    if (done)
        return;
    done = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_SESSION_LOADER_PROBE", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        std::snprintf(off_why, sizeof off_why, "SWOFTY_SESSION_LOADER_PROBE=%s", v);
        off.store(true);
        swofty::log("[stall] session loader probes off (SWOFTY_SESSION_LOADER_PROBE=%s)", v);
        return;
    }
    std::string why;
    bool a = place(update_probe, update_hook, UPDATE, on_update, why);
    bool b = place(unload_probe, unload_hook, UNLOAD, on_unload, why);
    if (!a && !b) {
        std::snprintf(off_why, sizeof off_why, "%s", why.c_str());
        off.store(true);
        swofty::log("[stall] session loader probes off: %s", why.c_str());
        return;
    }
    decoded.store(runtime::holds(GATE));
    if (!decoded.load())
        std::snprintf(proof_line, sizeof proof_line, "%s is proven on %s only", LAYOUT.what, bp::proven_builds(LAYOUT).c_str());
    unload_decoded.store(runtime::holds(UNLOAD_GATE));
    if (!unload_decoded.load())
        std::snprintf(unload_proof_line, sizeof unload_proof_line, "%s is proven on %s only", UNLOAD_LAYOUT.what,
                      bp::proven_builds(UNLOAD_LAYOUT).c_str());
    world_decoded.store(runtime::holds(WORLD_GATE));
    if (!world_decoded.load())
        std::snprintf(world_proof_line, sizeof world_proof_line, "%s is proven on %s only", WORLD_LAYOUT.what,
                      bp::proven_builds(WORLD_LAYOUT).c_str());
    swofty::log("[stall] session loader probes hooked (%s%s%s): a load-stall report names the loader they last ran on, "
                "its state, asset load, completion and ids with each id's state%s%s",
                a ? UPDATE : "", a && b ? ", " : "", b ? UNLOAD : "", why.empty() ? "" : "; ", why.c_str());
}

size_t session_loader_report(char *out, size_t cap, void (*name)(char *out, size_t cap, uint64_t at))
{
    sl::Snapshot &s = snapshot;
    s.off = off.load();
    std::snprintf(s.why, sizeof s.why, "%s", s.off ? (off_why[0] ? off_why : "not set up") : "");
    s.update = seen_of(update_probe);
    s.unload = seen_of(unload_probe);
    s.now = GetTickCount64();
    s.proven = decoded.load();
    std::snprintf(s.proof, sizeof s.proof, "%s", proof_line);
    s.unload_proven = unload_decoded.load();
    std::snprintf(s.unload_proof, sizeof s.unload_proof, "%s", unload_proof_line);
    HMODULE image = GetModuleHandleA(nullptr);
    uint64_t vtable_rva = sl::spawn_vtable_rva(runtime::running_build());
    s.spawn_vtable = s.unload_proven && image && vtable_rva ? uint64_t(reinterpret_cast<uintptr_t>(image)) + vtable_rva : 0;
    s.world_proven = world_decoded.load();
    std::snprintf(s.world_proof, sizeof s.world_proof, "%s", world_proof_line);
    s.image_base = image ? uint64_t(reinterpret_cast<uintptr_t>(image)) : 0;
    s.world_proven = s.world_proven && s.image_base && sl::world_task_vtables(runtime::running_build(), s.world_vtable);
    if (s.world_proven)
        for (uint64_t &v : s.world_vtable)
            v += s.image_base;
    if (!s.off)
        sl::capture(read_game, s);
    return sl::text(s, out, cap, name);
}

void session_loader_flow(uint32_t to, const char *name)
{
    if (to != 1 || !name || std::strcmp(name, "StateUnloadSession") || off.load() || !world_decoded.load())
        return;
    uint64_t now = GetTickCount64(), last = entering_ms.load();
    if (last && now - last < 1000)
        return;
    entering_ms.store(now);
    HMODULE image = GetModuleHandleA(nullptr);
    uint64_t base = image ? uint64_t(reinterpret_cast<uintptr_t>(image)) : 0;
    uint64_t vt[2];
    if (!base || !sl::world_task_vtables(runtime::running_build(), vt)) {
        swofty::log("[stall] unload entering: world tasks not decoded on this build");
        return;
    }
    vt[0] += base;
    vt[1] += base;
    sl::Seen a = seen_of(update_probe), b = seen_of(unload_probe);
    uint64_t other = 0, loader = sl::pick(a, b, other);
    if (!loader) {
        swofty::log("[stall] unload entering: no session loader seen yet");
        return;
    }
    sl::capture_worlds(read_game, loader, base, vt, entering_set);
    if (!entering_set.read) {
        swofty::log("[stall] unload entering: loader 0x%llx +0x200 module list not readable", (unsigned long long)loader);
        return;
    }
    swofty::log("[stall] unload entering: loader 0x%llx has %zu module(s) in the +0x200 list", (unsigned long long)loader,
                entering_set.total);
    char line[640];
    for (size_t i = 0; i < entering_set.got; i++) {
        sl::entering_line(entering_set, i, line, sizeof line);
        swofty::log("[stall] %s", line);
    }
    if (entering_set.got < entering_set.total)
        swofty::log("[stall] unload entering: %zu more module(s) not shown", entering_set.total - entering_set.got);
}

}
