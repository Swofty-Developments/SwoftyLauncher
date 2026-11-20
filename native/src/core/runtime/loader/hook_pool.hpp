#pragma once

#include <cstdint>
#include <memory>
#include <string>

#include "safetyhook.hpp"

namespace runtime::loader {

/* Hooks reach their trampolines with a 5-byte jump, so the trampolines must sit within 2 GB of the code.
   The loader reserves a pool next to the image at attach (SwoftyLoaderHookPool); the core commits it and
   hands it to safetyhook's global allocator, which serves every later trampoline and stub from it. On a
   build whose core loads at the window stage the game has used the address space around the image by
   then, and every site and short wrap failed with no memory in range. */
class HookPool {
public:
    void adopt(void (*loader_fn)());
    // Logs the free address space around target once per run, for the first hook that found no memory in range.
    void dump_free_regions_near(uintptr_t target);

private:
    std::shared_ptr<safetyhook::Allocator> allocator_;
    uintptr_t pool_ = 0;
    bool dumped_ = false;
};

HookPool &hook_pool();

const char *alloc_error(safetyhook::Allocator::Error e);
std::string target_bytes(uintptr_t at);

}
