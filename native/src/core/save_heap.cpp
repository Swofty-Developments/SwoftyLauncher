#include "save_heap_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

namespace {

namespace sh = runtime::save_heap;

constexpr const char *MIXIN = "swofty_core.dll (save temp heap)";
constexpr const char *SITE = "site_Ne_MemoryMapInternalHeapSize";

sh::Choice choice;
bool offered = false, settled = false;
std::atomic<bool> applied{false};

void heap_size(SwoftyContext *c)
{
    const auto *config = reinterpret_cast<const uint8_t *>(c->rdi);
    uint32_t virt = 0, expand = 0;
    std::memcpy(&virt, config + sh::kConfigVirtual, sizeof virt);
    std::memcpy(&expand, config + sh::kConfigExpand, sizeof expand);
    if (!sh::is_stock_save_temp_alloc(c->r15, virt, expand))
        return;
    if (applied.exchange(true))
        return;
    swofty::log("[saves] temp pool: heap SAVE_TEMP_ALLOC (virtual %u, expand %u%%): size %llu -> %llu bytes (%llu MiB, %s)",
                virt, expand, (unsigned long long)c->r15, (unsigned long long)choice.bytes,
                (unsigned long long)(choice.bytes / sh::kMiB), sh::source_text(choice.source));
    c->r15 = choice.bytes;
}

}

namespace runtime {

std::vector<CoreMixins> save_heap_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_SAVE_TEMP_MB", v, sizeof v);
    const char *env = n && n < sizeof v ? v : nullptr;
    choice = sh::choose(env);
    if (n >= sizeof v || choice.bad_env)
        swofty::log("[saves] temp pool: SWOFTY_SAVE_TEMP_MB is not a number of MiB, 0 or off; using the default");
    if (!choice.raised()) {
        swofty::log("[saves] temp pool: SAVE_TEMP_ALLOC kept at the game's %llu MiB (SWOFTY_SAVE_TEMP_MB=%s)",
                    (unsigned long long)(sh::kStock / sh::kMiB), env ? env : "");
        return out;
    }
    if (load_stage() != "early") {
        swofty::log("[saves] temp pool: SAVE_TEMP_ALLOC not raised: the core loaded at the %s stage, after "
                    "system/memory_map.xml made the heaps (the early stage loads it before)",
                    load_stage().c_str());
        return out;
    }
    if (!swofty::api->resolve(SITE)) {
        swofty::log("[saves] temp pool: SAVE_TEMP_ALLOC not raised: %s does not resolve on this build", SITE);
        return out;
    }
    out.push_back({MIXIN, {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&heap_size, nullptr}}});
    return out;
}

void save_heap_commit()
{
    if (!offered || settled)
        return;
    settled = true;
    if (!core_mixins_active(MIXIN))
        return;
    swofty::log("[saves] temp pool: SAVE_TEMP_ALLOC armed: %llu MiB instead of the game's %llu MiB when "
                "system/memory_map.xml is read",
                (unsigned long long)(choice.bytes / sh::kMiB), (unsigned long long)(sh::kStock / sh::kMiB));
}

bool save_heap_raised(uint64_t &bytes)
{
    bytes = applied.load() ? choice.bytes : sh::kStock;
    return applied.load();
}

}
