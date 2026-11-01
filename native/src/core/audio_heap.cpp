#include "audio_heap_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// AUDIO_BUNDLE heap size (re/db/globals.tsv site_Ne_MemoryMapHeapSize, audio_heap_logic.hpp).
//
// system/memory_map.xml makes the AUDIO_BUNDLE heap 43M and the heap never grows, so a career that keeps two or
// three seasons of car audio loaded (Progressive from 2021, Classic Seasons lists in Time Trial) ends in
// "Ne heap error 0xc in heap 'AUDIO_BUNDLE': Allocation error". Ne_MemoryMap_CreateHeap (the factory of the plain
// <heap> element) passes the size in r9 to Ne_MemoryMap_AllocHeapBlock and keeps it at [rsp+0x58] for the heap
// object's capacity. site_Ne_MemoryMapHeapSize is that spill: the hook replaces r9 with the new size, so the block
// and the heap object agree.
//
// The heaps are made while the game reads system/memory_map.xml, after the CreateMutexW trigger and before the
// window, so the hook works when the core loads at the early stage (every profile whose addons edit the database or
// register records); at the window stage the heap exists already and nothing is changed.
//
// SWOFTY_AUDIO_BUNDLE_MB sets the size in MiB; 0 or off keeps the game's 43M.

namespace {

namespace ah = runtime::audio_heap;

constexpr const char *MIXIN = "swofty_core.dll (audio bundle heap)";
constexpr const char *SITE = "site_Ne_MemoryMapHeapSize";

ah::Choice choice;
bool offered = false, settled = false;
std::atomic<bool> applied{false};

void heap_size(SwoftyContext *c)
{
    const auto *config = reinterpret_cast<const uint8_t *>(c->rdi);
    uint32_t virt = 0, pagesize = 0;
    std::memcpy(&virt, config + ah::kConfigVirtual, sizeof virt);
    std::memcpy(&pagesize, config + ah::kConfigPagesize, sizeof pagesize);
    if (!ah::is_stock_audio_bundle(c->r9, virt, pagesize))
        return;
    if (applied.exchange(true))
        return;
    char name[ah::kNameLen + 1];
    ah::frame_name(reinterpret_cast<const uint8_t *>(c->rsp), name);
    swofty::log("[audio] heap '%s' (virtual %u, pagesize %u): size %llu -> %llu bytes (%llu MiB, %s)", name, virt,
                pagesize, (unsigned long long)c->r9, (unsigned long long)choice.bytes,
                (unsigned long long)(choice.bytes / ah::kMiB), ah::source_text(choice.source));
    c->r9 = choice.bytes;
}

}

namespace runtime {

std::vector<CoreMixins> audio_heap_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_AUDIO_BUNDLE_MB", v, sizeof v);
    const char *env = n && n < sizeof v ? v : nullptr;
    MEMORYSTATUSEX mem = {};
    mem.dwLength = sizeof mem;
    uint64_t physical = GlobalMemoryStatusEx(&mem) ? mem.ullTotalPhys : 0;
    choice = ah::choose(env, physical);
    if (n >= sizeof v || choice.bad_env)
        swofty::log("[audio] SWOFTY_AUDIO_BUNDLE_MB is not a number of MiB, 0 or off; using the default");
    if (!choice.raised()) {
        swofty::log("[audio] AUDIO_BUNDLE heap kept at the game's %llu MiB (SWOFTY_AUDIO_BUNDLE_MB=%s)",
                    (unsigned long long)(ah::kStock / ah::kMiB), env ? env : "");
        return out;
    }
    if (load_stage() != "early") {
        swofty::log("[audio] AUDIO_BUNDLE heap not raised: the core loaded at the %s stage, after system/memory_map.xml "
                    "made the heaps (the early stage loads it before)",
                    load_stage().c_str());
        return out;
    }
    if (!swofty::api->resolve(SITE)) {
        swofty::log("[audio] AUDIO_BUNDLE heap not raised: %s does not resolve on this build", SITE);
        return out;
    }
    out.push_back({MIXIN, {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&heap_size, nullptr}}});
    return out;
}

void audio_heap_commit()
{
    if (!offered || settled)
        return;
    settled = true;
    if (!core_mixins_active(MIXIN))
        return;
    swofty::log("[audio] AUDIO_BUNDLE heap armed: %llu MiB instead of the game's %llu MiB when system/memory_map.xml "
                "is read",
                (unsigned long long)(choice.bytes / ah::kMiB), (unsigned long long)(ah::kStock / ah::kMiB));
}

}
