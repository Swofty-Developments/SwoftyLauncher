#pragma once
#include <stddef.h>
#include <stdint.h>
#include <windows.h>

/* The loader's and the core's crash handlers format and log nothing on a thread that hit a stack overflow or has
   less than this much of its stack reserve left below the handler: one crash log line takes several KB of stack
   there (the core's line, the loader's line and its log record), and a stack overflow leaves the handler a few KB.
   Such a thread hands its exception to a thread of its own stack (the core's crash report writer, the loader's log
   writer) and waits. */
#define SWOFTY_LOW_STACK ((size_t)64 << 10)

/* Bytes of the calling thread's stack reserve left below the caller's frame, or SIZE_MAX when that frame is not on
   the thread's own stack (a fiber's). */
static inline size_t swofty_stack_left(void)
{
    ULONG_PTR lo = 0, hi = 0;
    GetCurrentThreadStackLimits(&lo, &hi);
    volatile char here = 0;
    uintptr_t sp = (uintptr_t)&here;
    return sp > lo && sp < hi ? (size_t)(sp - lo) : SIZE_MAX;
}
