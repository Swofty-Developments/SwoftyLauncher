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

struct Probe {
    std::atomic<bool> hooked{false};
    std::atomic<uint64_t> at{0}, ms{0}, calls{0};
};

safetyhook::MidHook update_hook, unload_hook;
constinit Probe update_probe, unload_probe;
constinit std::atomic<bool> off{false};
constinit std::atomic<bool> decoded{false};
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
    if (!s.off)
        sl::capture(read_game, s);
    return sl::text(s, out, cap, name);
}

}
