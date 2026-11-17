#pragma once

#include <vector>

namespace runtime {
struct CoreMixins;
}

// The commit pass's entry points into the core's own mixin sets (core/guards/registry.cpp).
namespace swofty_core::guards {

// Hands every registered core set's offer to `add`, in CoreSetId order.
void offer_core_sets(void (*add)(std::vector<runtime::CoreMixins>));
// Settles every registered core set, in COMMIT_ORDER.
void commit_core_sets();

}
