#include "core/runtime/loader/hook_pool.hpp"

#include "core/runtime/hook_errors.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"

namespace runtime::loader {

namespace {

std::string where(const uint8_t *ip)
{
    char b[48];
    uintptr_t a = reinterpret_cast<uintptr_t>(ip);
    if (a >= image().base())
        snprintf(b, sizeof b, "base+0x%llx", image().rva(a));
    else
        snprintf(b, sizeof b, "0x%llx", (unsigned long long)a);
    return b;
}

}

HookPool &hook_pool()
{
    static HookPool instance;
    return instance;
}

void HookPool::adopt(void (*loader_fn)())
{
    allocator_ = safetyhook::Allocator::global();
    HMODULE loader = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(loader_fn), &loader);
    auto take = loader ? reinterpret_cast<int (*)(uintptr_t *, size_t *)>(
                             reinterpret_cast<void *>(GetProcAddress(loader, "SwoftyLoaderHookPool")))
                       : nullptr;
    uintptr_t at = 0;
    size_t size = 0;
    if (!take || !take(&at, &size)) {
        api->log("[core] hook pool: %s; trampolines come from safetyhook's own search near each hook",
                 take ? "the loader reserved none at attach" : "this loader reserves none");
        return;
    }
    if (!VirtualAlloc(reinterpret_cast<void *>(at), size, MEM_COMMIT, PAGE_EXECUTE_READWRITE)) {
        api->log("[core] hook pool: committing %zu KB at 0x%llx failed, error %lu; trampolines come from "
                 "safetyhook's own search near each hook",
                 size >> 10, (unsigned long long)at, GetLastError());
        VirtualFree(reinterpret_cast<void *>(at), 0, MEM_RELEASE);
        return;
    }
    allocator_->adopt(reinterpret_cast<uint8_t *>(at), size);
    pool_ = at;
    long long off = (long long)(at - image().base());
    api->log("[core] hook pool: %zu KB at 0x%llx (base%s0x%llx), reserved at loader attach; hook trampolines and "
             "stubs come from it",
             size >> 10, (unsigned long long)at, off < 0 ? "-" : "+", (unsigned long long)(off < 0 ? -off : off));
}

void HookPool::dump_free_regions_near(uintptr_t target)
{
    if (dumped_)
        return;
    dumped_ = true;
    SYSTEM_INFO si;
    GetSystemInfo(&si);
    uintptr_t lo = target > 0x80000000ULL ? target - 0x80000000ULL : (uintptr_t)si.lpMinimumApplicationAddress;
    uintptr_t hi = target + 0x80000000ULL;
    int regions = 0, slots = 0, shown = 0;
    unsigned long long biggest = 0;
    for (uintptr_t p = lo & ~(uintptr_t)0xFFF; p < hi;) {
        MEMORY_BASIC_INFORMATION m;
        if (!VirtualQuery(reinterpret_cast<void *>(p), &m, sizeof m)) {
            api->log("[core]   VirtualQuery failed at 0x%llx, error %lu", (unsigned long long)p, GetLastError());
            break;
        }
        uintptr_t next = (uintptr_t)m.BaseAddress + m.RegionSize;
        if (m.State == MEM_FREE) {
            uintptr_t first = (p + 0xFFFF) & ~(uintptr_t)0xFFFF;
            int n = first < next ? (int)((next - first) >> 16) : 0;
            regions++;
            slots += n;
            biggest = m.RegionSize > biggest ? m.RegionSize : biggest;
            if (shown < 16) {
                api->log("[core]   free 0x%llx..0x%llx (%llu KB, %d whole 64 KB slot(s))", (unsigned long long)p,
                         (unsigned long long)next, (unsigned long long)(m.RegionSize >> 10), n);
                shown++;
            }
        }
        p = next > p ? next : p + si.dwPageSize;
    }
    api->log("[core]   within 2 GB of 0x%llx: %d free region(s), %d whole 64 KB slot(s), largest %llu KB; hook pool %s",
             (unsigned long long)target, regions, slots, biggest >> 10, pool_ ? "adopted" : "none");
}

const char *alloc_error(safetyhook::Allocator::Error e)
{
    return e == safetyhook::Allocator::Error::NO_MEMORY_IN_RANGE ? "no free memory within 2 GB of the target"
                                                                 : "VirtualAlloc failed";
}

std::string target_bytes(uintptr_t at)
{
    std::string s;
    char b[4];
    for (int i = 0; i < 16; i++) {
        snprintf(b, sizeof b, i ? " %02X" : "%02X", reinterpret_cast<const uint8_t *>(at)[i]);
        s += b;
    }
    return s;
}

}

namespace runtime {

std::string hook_error(const safetyhook::InlineHook::Error &e)
{
    using loader::alloc_error;
    using E = safetyhook::InlineHook::Error;
    switch (e.type) {
    case E::BAD_ALLOCATION:
        return std::string("the trampoline did not allocate: ") + alloc_error(e.allocator_error);
    case E::FAILED_TO_DECODE_INSTRUCTION:
        return "the instruction at " + loader::where(e.ip) + " does not decode";
    case E::SHORT_JUMP_IN_TRAMPOLINE:
        return "a short jump at " + loader::where(e.ip) + " sits in the relocated bytes";
    case E::IP_RELATIVE_INSTRUCTION_OUT_OF_RANGE:
        return "the ip-relative instruction at " + loader::where(e.ip) + " cannot be relocated that far";
    case E::UNSUPPORTED_INSTRUCTION_IN_TRAMPOLINE:
        return "the instruction at " + loader::where(e.ip) + " cannot be relocated";
    case E::FAILED_TO_UNPROTECT:
        return "the page at " + loader::where(e.ip) + " did not unprotect";
    case E::NOT_ENOUGH_SPACE:
        return "not enough room for the jump at " + loader::where(e.ip);
    }
    return "unknown InlineHook error " + std::to_string((int)e.type);
}

std::string hook_error(const safetyhook::MidHook::Error &e)
{
    if (e.type == safetyhook::MidHook::Error::BAD_ALLOCATION)
        return std::string("the stub did not allocate: ") + loader::alloc_error(e.allocator_error);
    return hook_error(e.inline_hook_error);
}

}
