#include "core/diagnostics/boot_route.hpp"
#include "core/diagnostics/boot_route_logic.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>

// Finds the load-stall report's front end boot route section (boot_route_logic.hpp) once at the ready stage: the
// signature of FeFlow_StateFTUE_OnUpdate gives the update, and its two rip-relative operands give the route record
// and its flag. Nothing is hooked. The report reads the record and its job with the core's page-table checked reads,
// which never raise an exception and never touch a page.

namespace {

namespace br = runtime::boot_route;
namespace bp = runtime::build_proof;

const runtime::Sig S_FTUE_UPDATE{"FeFlow_StateFTUE_OnUpdate", br::kPattern, br::kKbRva};

constexpr bp::Proof LAYOUT{"the boot route record's name (+0x20) and the job's listeners (+0x100, +0x108)",
                           bp::bit(bp::Build::kb) | bp::bit(bp::Build::ea) | bp::bit(bp::Build::steam_1_0_145) |
                               bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate GATE{"the boot route record's decoded fields", &LAYOUT};

constinit std::atomic<bool> off{true};
constinit std::atomic<bool> decoded{false};
constinit std::atomic<uint64_t> fn_at{0}, record_at{0}, flag_at{0};
char off_why[256] = "not set up";
char proof_line[320];

bool read_game(uint64_t at, void *out, size_t n)
{
    if (!at || at + n < at || !swofty::mem::readable(uintptr_t(at), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(at), n);
    return true;
}

constinit br::Snapshot snapshot{};

}

namespace runtime {

void boot_route_setup()
{
    static bool done;
    if (done)
        return;
    done = true;
    uintptr_t fn = runtime::find(S_FTUE_UPDATE, "boot route", "stall");
    br::Where w;
    if (!fn || !br::locate(read_game, fn, w)) {
        std::snprintf(off_why, sizeof off_why, "%s %s", S_FTUE_UPDATE.what,
                      fn ? "matched, but its operands are not readable" : "did not match once on this build");
        swofty::log("[stall] boot route: off, %s", off_why);
        return;
    }
    fn_at.store(w.fn);
    record_at.store(w.record);
    flag_at.store(w.flag);
    decoded.store(runtime::holds(GATE));
    if (!decoded.load())
        std::snprintf(proof_line, sizeof proof_line, "%s is proven on %s only", LAYOUT.what, bp::proven_builds(LAYOUT).c_str());
    off.store(false);
    uintptr_t base = swofty::api->module_base();
    swofty::log("[stall] boot route: %s at base+0x%llx waits on the route record at base+0x%llx (job +0x8, its status "
                "+0x48) and its flag at base+0x%llx; a load-stall report names the route, its job and their state%s",
                S_FTUE_UPDATE.what, (unsigned long long)(w.fn - base), (unsigned long long)(w.record - base),
                (unsigned long long)(w.flag - base), decoded.load() ? "" : " (name and listeners not decoded)");
}

size_t boot_route_report(char *out, size_t cap, void (*name)(char *out, size_t cap, uint64_t at))
{
    br::Snapshot &s = snapshot;
    s = br::Snapshot{};
    s.off = off.load();
    std::snprintf(s.why, sizeof s.why, "%s", off_why);
    s.base = swofty::api ? swofty::api->module_base() : 0;
    s.at.fn = fn_at.load();
    s.at.record = record_at.load();
    s.at.flag = flag_at.load();
    s.proven = decoded.load();
    std::snprintf(s.proof, sizeof s.proof, "%s", proof_line);
    if (!s.off)
        br::capture(read_game, s);
    return br::text(s, out, cap, name);
}

}
