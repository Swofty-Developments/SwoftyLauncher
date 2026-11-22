#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>

#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/request.hpp"
#include "safetyhook.hpp"

namespace crash {

// Capture: the vectored handler and the top-level filter, the process exit hooks, the snapshot of the last
// fatal-class exception, and the hand-off of one report request to the writer. The state is fixed-size so that the
// handlers never allocate.
struct FaultCapture {
    LPTOP_LEVEL_EXCEPTION_FILTER previous;
    safetyhook::InlineHook exit_hook, terminate_hook, nt_terminate_hook;
    uintptr_t seen[MAX_SEEN];
    volatile LONG seen_count, reports, busy, exited, snapshot_lock, fatal_count;
    Snapshot last_fatal;
    /* Standard library 1.6: the hook-point guard names the before or replace handler whose fault is about
       to crash the game; a report written on that thread soon after leads with it. */
    char blame_text[320];
    DWORD blame_tid;
    ULONGLONG blame_ms;
    volatile DWORD stall_tid;
    /* When the game's window was last asked to close, and by which message: an exit soon after is that close. */
    volatile ULONGLONG close_ms;
    const char *volatile close_msg;
};

extern FaultCapture capture;

void backtrace(CONTEXT ctx, bool skip_self);
bool first_time(uintptr_t ip);
void report(const char *when, EXCEPTION_POINTERS *e);
bool enter();
void leave();
void stack_bounds(uintptr_t &lo, uintptr_t &hi);
bool holds_loader_lock();
bool deliberate_abort(const EXCEPTION_RECORD *r, const CONTEXT *c);
void keep_fatal(EXCEPTION_POINTERS *e);
bool request_report(Kind kind, const char *reason, const EXCEPTION_RECORD *rec, const CONTEXT *ctx);
void already(const char *what);
// On the writer, before a report handed over by a thread with little stack left: that thread's log lines, from the
// copies it handed over, and the report's reason, blame and loader lock.
void settle_handed(Request &r);

LONG CALLBACK veh(EXCEPTION_POINTERS *e);
LONG WINAPI unhandled(EXCEPTION_POINTERS *e);
void window_stage(int);
bool launcher_stop();
void on_exit(const char *fn, UINT code);
void WINAPI hooked_exit(UINT code);
BOOL WINAPI hooked_terminate(HANDLE process, UINT code);
LONG NTAPI hooked_nt_terminate(HANDLE process, LONG status);

}
