#include <windows.h>

namespace crash_unwind {

namespace {
volatile DWORD guarded_tid;
}

bool unwinding_here()
{
    return guarded_tid == GetCurrentThreadId();
}

bool safe_unwind(CONTEXT *ctx, DWORD64 image_base, PRUNTIME_FUNCTION fn)
{
    bool ok = true;
    guarded_tid = GetCurrentThreadId();
    __try {
        void *handler_data;
        DWORD64 frame;
        RtlVirtualUnwind(UNW_FLAG_NHANDLER, image_base, ctx->Rip, fn, ctx, &handler_data, &frame, nullptr);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        ok = false;
    }
    guarded_tid = 0;
    return ok;
}

}
