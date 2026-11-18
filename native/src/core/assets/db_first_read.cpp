#include "core/assets/db_first_read.hpp"

#include "core/assets/db_first_read_logic.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>
#include <utility>

namespace {

namespace fr = runtime::db_first_read;
namespace bp = runtime::build_proof;
using fr::Phase;

// Gdb_Lookup_Key 0x1445a22f0 hashes its dotted path (FNV-1a-64), probes the global map and returns the record slot or
// NULL, with no writes; sigport.py --witness finds its body 86 of 86 instructions the same on each port, with the map
// offsets +0xb8, +0xc0, +0xc8, +0xd8, +0xe8 and +0xec at the same instructions. Boot_Init_NeApplication 0x1443c0560 is
// 318 of 318 the same on each, and each step's return site (STEP_RETURNS) is the instruction after a call there on each
// port. sigverify.py matches S_BOOT_INIT and gamedb.cpp's S_LOOKUP once on each image.
constexpr uint32_t ALL_BUILDS =
    bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea) | bp::bit(bp::Build::ea_1_0_145);
constexpr bp::Proof LOOKUP_AND_STEPS{"Gdb_Lookup_Key's body and Boot_Init_NeApplication's step return sites (+0xcb to +0x3f5)",
                                     ALL_BUILDS};
constexpr bp::Gate GATE{"the database first read probe", &LOOKUP_AND_STEPS};

constexpr runtime::Sig S_BOOT_INIT{"Boot_Init_NeApplication",
                                   "56 57 48 81 EC D8 00 00 00 C5 F8 29 B4 24 C0 00 00 00 48 89 8C 24 A0 00 00 00", 0x43c0560};

using Lookup_t = void **(const char *);

// The probe's wrap of Gdb_Lookup_Key; its lock is over the listing state below.
struct FirstReadHooks : runtime::loader::HookSet<> {
    safetyhook::InlineHook lookup;
};
FirstReadHooks hooks;
std::atomic<Lookup_t *> orig{nullptr};
std::atomic<uint8_t> phase{uint8_t(Phase::idle)};
std::atomic<unsigned long> boot_thread{0};
std::atomic<uint64_t> own{0};
uintptr_t boot_init = 0;

int64_t freq = 0, t_load = 0, t_edits = 0;
uint64_t seen = 0, after = 0;
unsigned listed = 0;
fr::Sites<fr::DURING> during_sites;
fr::Sites<fr::AFTER> after_sites;

int64_t now()
{
    LARGE_INTEGER c;
    QueryPerformanceCounter(&c);
    return c.QuadPart;
}

bool env()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA(fr::ENV, v, sizeof v);
    return n && n < sizeof v && fr::env_on(v);
}

void step_of(fr::Read &r, uintptr_t frame)
{
    auto tib = reinterpret_cast<NT_TIB *>(NtCurrentTeb());
    uintptr_t lo = frame & ~uintptr_t(7), hi = reinterpret_cast<uintptr_t>(tib->StackBase);
    if (!lo || hi <= lo)
        return;
    hi = std::min(hi, lo + fr::MAX_SCAN);
    r.scanned_kib = unsigned((hi - lo) / 1024);
    r.step = fr::step_in(boot_init, reinterpret_cast<const uint64_t *>(lo), (hi - lo) / 8);
}

// Out of line, so the scan from lookup_hook's frame up never covers the frames that compute the step return sites.
__attribute__((noinline)) void note(uint64_t ret, const char *path, bool found, uintptr_t frame)
{
    unsigned long tid = GetCurrentThreadId();
    fr::Read r;
    r.boot_thread = tid == boot_thread.load(std::memory_order_relaxed);
    bool fresh = false, finished = false;
    unsigned done_sites = 0;
    uint64_t done_after = 0;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        auto p = Phase(phase.load(std::memory_order_relaxed));
        if (fr::take(p, r.boot_thread) != fr::Take::list)
            return;
        r.lookup = ++seen;
        if (p == Phase::edits) {
            fresh = during_sites.add(ret);
        } else {
            after++;
            fresh = after_sites.add(ret);
            if (fresh && after_sites.full()) {
                phase.store(uint8_t(Phase::done), std::memory_order_release);
                finished = true;
                done_sites = after_sites.n;
                done_after = after;
            }
        }
        if (fresh) {
            int64_t t = now();
            r.index = ++listed;
            r.since_load_ms = fr::ms(t_load, t, freq);
            r.since_edits_ms = p == Phase::after ? fr::ms(t_edits, t, freq) : -1;
        }
    }
    if (!fresh)
        return;
    r.thread = tid;
    r.found = found;
    r.path = fr::path_text(path);
    r.where = runtime::code_where(uintptr_t(ret));
    if (r.boot_thread)
        step_of(r, frame);
    swofty::log("%s", fr::line(r).c_str());
    if (finished)
        swofty::log("%s", fr::done_line(done_sites, done_after).c_str());
}

void **lookup_hook(const char *path)
{
    auto p = Phase(phase.load(std::memory_order_acquire));
    Lookup_t *call = orig.load(std::memory_order_acquire);
    if (p == Phase::idle || p == Phase::done)
        return call(path);
    fr::Take t = fr::take(p, GetCurrentThreadId() == boot_thread.load(std::memory_order_relaxed));
    if (t == fr::Take::own)
        own.fetch_add(1, std::memory_order_relaxed);
    void **slot = call(path);
    if (t == fr::Take::list)
        note(reinterpret_cast<uintptr_t>(__builtin_return_address(0)), path, slot != nullptr,
             reinterpret_cast<uintptr_t>(__builtin_frame_address(0)));
    return slot;
}

}

void runtime::db_first_read::cannot(const std::string &why)
{
    if (env())
        swofty::log("%s", off_line(why).c_str());
}

void runtime::db_first_read::arm(uintptr_t lookup)
{
    if (!env())
        return;
    if (!runtime::holds(GATE)) {
        swofty::log("%s", off_line("it is not proven on this build").c_str());
        hooks.turn_off(std::string(GATE.feature) + " is not proven on this build");
        return;
    }
    boot_init = runtime::find(S_BOOT_INIT, "gamedb");
    if (!lookup || !boot_init) {
        std::string why = lookup ? "the Boot_Init_NeApplication signature is missing on this build"
                                 : "the Gdb_Lookup_Key signature is missing on this build";
        swofty::log("%s", off_line(why).c_str());
        hooks.turn_off(std::move(why));
        return;
    }
    auto r = runtime::loader::install(hooks.lookup, lookup, "Gdb_Lookup_Key", reinterpret_cast<void *>(&lookup_hook),
                                      safetyhook::InlineHook::StartDisabled);
    if (!r) {
        swofty::log("%s", off_line("hooking Gdb_Lookup_Key failed").c_str());
        hooks.turn_off(std::move(r.why));
        return;
    }
    orig.store(hooks.lookup.original<Lookup_t *>(), std::memory_order_release);
    if (!hooks.lookup.enable()) {
        swofty::log("%s", off_line("the Gdb_Lookup_Key hook could not be enabled").c_str());
        hooks.turn_off("the Gdb_Lookup_Key hook could not be enabled");
        return;
    }
    LARGE_INTEGER f;
    QueryPerformanceFrequency(&f);
    freq = f.QuadPart;
    hooks.set_live();
    uintptr_t base = swofty::api->module_base();
    swofty::log(
        "%s",
        armed_line(bp::exe_name(runtime::running_build(), runtime::running_exe()), lookup - base, boot_init - base).c_str());
}

void runtime::db_first_read::loaded()
{
    if (!hooks.live())
        return;
    std::lock_guard<std::mutex> g(hooks.lock);
    t_load = now();
    boot_thread.store(GetCurrentThreadId(), std::memory_order_relaxed);
    phase.store(uint8_t(Phase::edits), std::memory_order_release);
}

void runtime::db_first_read::edited()
{
    if (!hooks.live())
        return;
    double ran = 0;
    uint64_t others = 0;
    unsigned sites = 0;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        t_edits = now();
        ran = fr::ms(t_load, t_edits, freq);
        others = seen;
        sites = during_sites.n;
        phase.store(uint8_t(Phase::after), std::memory_order_release);
    }
    swofty::log(
        "%s",
        edits_line(ran, boot_thread.load(std::memory_order_relaxed), own.load(std::memory_order_relaxed), others, sites).c_str());
}
