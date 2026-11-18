#pragma once
#include "core/frontend/player_name_logic.hpp"
#include "core/frontend/ui_registry_walk.hpp"
#include "core/runtime/build_proof.hpp"

#include <atomic>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <string>

namespace frontend::player_name {

// What the bridge reads besides its byte signature: the user id the lookup is given (type +0, the id as value ^ mask at
// +8/+0x10, flag +0x39, kind +0x4c; player_name_logic.hpp) and the view model registry's vtables the pin pass walks
// (registry_layout; ui_registry_walk.hpp's node offsets). On both 1.0.145 builds the lookup and the walk's functions are
// instruction-identical to the KB build's and the four vtables moved (re/research/port-steam-26566308.md,
// re/research/ea-1-0-145.md).
inline constexpr runtime::build_proof::Proof TT_NAME_LAYOUT{
    "the name cache lookup's user id layout and the view model registry vtables",
    runtime::build_proof::bit(runtime::build_proof::Build::kb) |
        runtime::build_proof::bit(runtime::build_proof::Build::steam_1_0_145) |
        runtime::build_proof::bit(runtime::build_proof::Build::ea_1_0_145)};
inline constexpr runtime::build_proof::Gate TT_NAME{"the Time Trial player name", &TT_NAME_LAYOUT};

// The view model registry's node vtables (object, list, string leaf, string binding) as RVAs on each build.
struct RegistryVtables {
    runtime::build_proof::Build build;
    uint64_t object, list, string_leaf, string_binding;
};

inline constexpr RegistryVtables REGISTRY_VTABLES[] = {
    {runtime::build_proof::Build::kb, 0x5a70080, 0x5a6ff90, 0x565f200, 0x5660370},
    {runtime::build_proof::Build::steam_1_0_145, 0x5a72200, 0x5a72110, 0x5661200, 0x5662370},
    {runtime::build_proof::Build::ea_1_0_145, 0x5b82a40, 0x5b82950, 0x5774130, 0x57752a0},
};

// The registry layout of an image at `base` on `build`; all zero on a build with no row.
constexpr frontend::ui_registry::Layout registry_layout(runtime::build_proof::Build build, uint64_t base)
{
    for (const RegistryVtables &v : REGISTRY_VTABLES)
        if (v.build == build)
            return {base + v.object, base + v.list, base + v.string_leaf, base + v.string_binding};
    return {};
}

constexpr bool vtables_for_every_proven_build()
{
    for (const runtime::build_proof::Known &k : runtime::build_proof::KNOWN)
        if (runtime::build_proof::proven_on(TT_NAME_LAYOUT, k.build) && !registry_layout(k.build, 0).object_vt)
            return false;
    return true;
}

static_assert(vtables_for_every_proven_build());
static_assert(registry_layout(runtime::build_proof::Build::kb, 0x140000000).string_binding_vt == 0x145660370);
static_assert(registry_layout(runtime::build_proof::Build::steam_1_0_145, 0x140000000).object_vt == 0x145a72200);
static_assert(registry_layout(runtime::build_proof::Build::ea_1_0_145, 0x140000000).string_binding_vt == 0x1457752a0);
static_assert(!registry_layout(runtime::build_proof::Build::unknown, 0x140000000).object_vt &&
              !registry_layout(runtime::build_proof::Build::ea, 0).list_vt);

bool registry_read(uint64_t at, void *out, size_t n);
void probe_registry(int pass);
void scan_copies(std::string stock);
int pin_names(const std::string &name, const std::string &stock);

class SourceSwap {
public:
    void plan_swap(bool live_ours, const std::string &stock, const std::string &name, const char *source, bool visible);

private:
    struct SwapJob {
        uint64_t serial = 0;
        std::string stock, name;
        const char *source = "";
        frontend::player_name::SwapWhy why = frontend::player_name::SwapWhy::none;
    };

    void swap_worker();

    std::mutex swap_lock;
    std::condition_variable swap_cv;
    SwapJob swap_job;
    bool swap_queued = false;
    std::atomic<bool> swap_busy{false};
    frontend::player_name::SwapPlan swap_plan;
    std::once_flag swap_thread_once;
};

extern SourceSwap source_swap;

}
