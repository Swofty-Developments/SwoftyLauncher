#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>

namespace crash {

enum Kind { K_EXCEPTION, K_FATAL, K_EXIT };
enum DumpMode { DUMP_OFF, DUMP_NORMAL, DUMP_FULL };

struct Snapshot {
    alignas(16) CONTEXT ctx;
    EXCEPTION_RECORD rec;
    DWORD tid;
    ULONGLONG ms;
    uintptr_t lo, hi;
};

struct Request {
    alignas(16) CONTEXT ctx;
    EXCEPTION_RECORD rec;
    Kind kind;
    bool has_exception, has_prior, loader_lock;
    DWORD tid;
    uintptr_t lo, hi;
    ULONGLONG ms;
    char reason[640];
    char blame[320];
    uint32_t probes[32];
    int nprobes;
    Snapshot prior;
    /* Set by a thread with little stack left (hand_off): what it would have logged ("first-chance ..." or
       "unhandled"), which the writer logs from the copies before it fills the reason, the blame and loader_lock. */
    const char *handed;
    size_t stack_left;
};

}
