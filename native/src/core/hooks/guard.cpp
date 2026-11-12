#include <windows.h>

#include "guard.hpp"

#include <atomic>
#include <cstring>

namespace swofty_core::hooks::guard {
namespace {

void thrown_what(const EXCEPTION_RECORD *r, char *out, size_t cap)
{
    out[0] = 0;
    if (r->ExceptionCode != HANDLER_THREW || r->NumberParameters < 1 || !r->ExceptionInformation[0])
        return;
    auto text = reinterpret_cast<const char *>(r->ExceptionInformation[0]);
    if (IsBadReadPtr(text, 1))
        return;
    size_t i = 0;
    for (; i + 1 < cap && text[i]; i++)
        out[i] = text[i];
    out[i] = 0;
}

int isolate_filter(EXCEPTION_POINTERS *e, Seen *seen)
{
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (!recoverable(code))
        return EXCEPTION_CONTINUE_SEARCH;
    seen->code = code;
    seen->at = reinterpret_cast<uintptr_t>(e->ExceptionRecord->ExceptionAddress);
    thrown_what(e->ExceptionRecord, seen->what, sizeof seen->what);
    return EXCEPTION_EXECUTE_HANDLER;
}

int blame_filter(EXCEPTION_POINTERS *e, BlameFn blame, const void *ctx)
{
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (recoverable(code) && blame) {
        char what[160];
        thrown_what(e->ExceptionRecord, what, sizeof what);
        blame(ctx, code, reinterpret_cast<uintptr_t>(e->ExceptionRecord->ExceptionAddress), what);
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

}

bool recoverable(unsigned long code)
{
    switch (code) {
    case EXCEPTION_ACCESS_VIOLATION:
    case EXCEPTION_ILLEGAL_INSTRUCTION:
    case EXCEPTION_INT_DIVIDE_BY_ZERO:
    case EXCEPTION_INT_OVERFLOW:
    case EXCEPTION_PRIV_INSTRUCTION:
    case EXCEPTION_IN_PAGE_ERROR:
    case EXCEPTION_ARRAY_BOUNDS_EXCEEDED:
    case EXCEPTION_DATATYPE_MISALIGNMENT:
    case EXCEPTION_FLT_DIVIDE_BY_ZERO:
    case EXCEPTION_FLT_INVALID_OPERATION:
    case MSVC_CXX:
    case GCC_CXX:
    case HANDLER_THREW:
        return true;
    default:
        return false;
    }
}

int isolated_call(SwoftyHookFn fn, void *user, SwoftyHookCall *call, Seen *seen)
{
    __try {
        return fn(user, call);
    } __except (isolate_filter(GetExceptionInformation(), seen)) {
        return 0;
    }
}

bool isolated_service(SwoftyServiceFn fn, void *user, const void *table, size_t size, uint32_t version, Seen *seen)
{
    __try {
        fn(user, table, size, version);
        return true;
    } __except (isolate_filter(GetExceptionInformation(), seen)) {
        return false;
    }
}

int blamed_call(SwoftyHookFn fn, void *user, SwoftyHookCall *call, BlameFn blame, const void *ctx)
{
    __try {
        return fn(user, call);
    } __except (blame_filter(GetExceptionInformation(), blame, ctx)) {
        return 0;
    }
}

namespace {

using Copy_t = void *(*)(void *, const void *, size_t);
Copy_t volatile copy_call = &memcpy;
thread_local int reading;
std::atomic<uint32_t> read_faults{0};

int read_filter(EXCEPTION_POINTERS *e)
{
    const EXCEPTION_RECORD *r = e->ExceptionRecord;
    if (r->ExceptionCode == STATUS_GUARD_PAGE_VIOLATION) {
        if (r->NumberParameters >= 2) {
            void *at = reinterpret_cast<void *>(r->ExceptionInformation[1] & ~uintptr_t(0xfff));
            MEMORY_BASIC_INFORMATION m{};
            DWORD was = 0;
            if (VirtualQuery(at, &m, sizeof m) == sizeof m && m.State == MEM_COMMIT && !(m.Protect & PAGE_GUARD))
                VirtualProtect(at, 1, m.Protect | PAGE_GUARD, &was);
        }
    } else if (r->ExceptionCode != EXCEPTION_ACCESS_VIOLATION && r->ExceptionCode != EXCEPTION_IN_PAGE_ERROR) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    read_faults.fetch_add(1, std::memory_order_relaxed);
    return EXCEPTION_EXECUTE_HANDLER;
}

bool guarded_copy(void *out, const void *from, size_t n)
{
    reading++;
    bool ok = true;
    __try {
        copy_call(out, from, n);
    } __except (read_filter(GetExceptionInformation())) {
        ok = false;
    }
    reading--;
    return ok;
}

bool guarded_touch(uintptr_t p, size_t n)
{
    reading++;
    bool ok = true;
    __try {
        uint8_t b;
        uintptr_t last = p + n - 1;
        copy_call(&b, reinterpret_cast<const void *>(p), 1);
        for (uintptr_t page = (p | 0xfff) + 1; page <= last; page += 0x1000)
            copy_call(&b, reinterpret_cast<const void *>(page), 1);
        if (last != p)
            copy_call(&b, reinterpret_cast<const void *>(last), 1);
    } __except (read_filter(GetExceptionInformation())) {
        ok = false;
    }
    reading--;
    return ok;
}

}

bool copy_memory(void *out, const void *from, size_t n)
{
    return guarded_copy(out, from, n);
}

bool reading_now()
{
    return reading != 0;
}

uint32_t read_fault_count()
{
    return read_faults.load(std::memory_order_relaxed);
}

}

namespace swofty::mem::guarded {

bool readable(uintptr_t p, size_t n)
{
    if (!p || p + (n ? n : 1) < p)
        return false;
    return swofty_core::hooks::guard::guarded_touch(p, n ? n : 1);
}

bool copy(void *out, uintptr_t from, size_t n)
{
    if (!from || from + n < from)
        return false;
    return swofty_core::hooks::guard::guarded_copy(out, reinterpret_cast<const void *>(from), n);
}

}
