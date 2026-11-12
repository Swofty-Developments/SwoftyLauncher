#include "stall_walk.hpp"

namespace stall_walk {

namespace {

int frames(CONTEXT *ctx, uintptr_t lo, uintptr_t hi, int max, Lookup lookup, Readable readable, uint64_t *rip,
           uint64_t *rsp)
{
    int n = 0;
    for (int i = 0; i < max && ctx->Rip; i++) {
        rip[n] = ctx->Rip;
        rsp[n] = ctx->Rsp;
        n++;
        if (ctx->Rsp < lo || ctx->Rsp >= hi)
            break;
        DWORD64 ib = 0, before = ctx->Rsp;
        PRUNTIME_FUNCTION f = lookup(ctx->Rip, &ib);
        if (f) {
            void *hd = nullptr;
            DWORD64 ef = 0;
            RtlVirtualUnwind(UNW_FLAG_NHANDLER, ib, ctx->Rip, f, ctx, &hd, &ef, nullptr);
        } else {
            if (!readable(ctx->Rsp, 8))
                break;
            ctx->Rip = *reinterpret_cast<DWORD64 *>(ctx->Rsp);
            ctx->Rsp += 8;
        }
        if (ctx->Rsp < before)
            break;
    }
    return n;
}

}

int walk(CONTEXT *ctx, uintptr_t lo, uintptr_t hi, int max, Lookup lookup, Readable readable, uint64_t *rip,
         uint64_t *rsp, bool *faulted)
{
    *faulted = false;
    for (int i = 0; i < max; i++)
        rip[i] = rsp[i] = 0;
    __try {
        return frames(ctx, lo, hi, max, lookup, readable, rip, rsp);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        *faulted = true;
    }
    int n = 0;
    while (n < max && rip[n])
        n++;
    return n;
}

}
