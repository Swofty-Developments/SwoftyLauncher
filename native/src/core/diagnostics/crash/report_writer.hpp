#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <dbghelp.h>

#include <algorithm>
#include <cstdarg>
#include <cstdio>
#include <string>

#include "core/diagnostics/crash/request.hpp"

namespace crash {

// The report text, written through a fixed buffer that is flushed to the file as it fills.
struct Out {
    HANDLE file = INVALID_HANDLE_VALUE;
    char *b = nullptr;
    size_t cap = 0, n = 0, written = 0;
    bool kept = true;

    void put(const char *f, ...) __attribute__((format(printf, 2, 3)))
    {
        if (!b)
            return;
        if (cap - n < 8192)
            flush(true);
        va_list ap;
        va_start(ap, f);
        int k = vsnprintf(b + n, cap - n, f, ap);
        va_end(ap);
        if (k > 0)
            n = std::min(cap - 1, n + size_t(k));
    }

    void flush(bool reset = false)
    {
        if (file != INVALID_HANDLE_VALUE && n > written) {
            DWORD w;
            WriteFile(file, b + written, DWORD(n - written), &w, nullptr);
        }
        written = n;
        if (reset)
            n = written = 0, kept = false;
    }
};

using DumpFn = BOOL(WINAPI *)(HANDLE, DWORD, HANDLE, MINIDUMP_TYPE, PMINIDUMP_EXCEPTION_INFORMATION,
                              PMINIDUMP_USER_STREAM_INFORMATION, PMINIDUMP_CALLBACK_INFORMATION);

// Reporting: one crash report per process (swofty\crash-<time>.txt and its minidump), written by a thread started at
// attach while the reporting thread waits, step by step so that a fault in one step costs that step only.
struct ReportWriter {
    Request req;
    Out out;
    HANDLE go_event, text_event, done_event;
    DWORD writer_tid;
    uintptr_t writer_stack_top;
    HANDLE volatile suspended, dump_file = INVALID_HANDLE_VALUE;
    volatile LONG claimed, writing, step_at, writer_faults;
    DWORD fault_code;
    uintptr_t fault_ip;
    wchar_t report_path[MAX_PATH], dump_path[MAX_PATH];
    char report_name[64], report_where[MAX_PATH * 3], dump_note[256];
    std::wstring crash_dir;
    DumpMode dump_mode = DUMP_NORMAL;
    DumpFn write_dump;
};

extern ReportWriter report_writer;

void local_stamp(char *b, size_t n, bool file_name);
bool create_report(const std::wstring &dir, const char *stamp);
std::string narrow(const std::wstring &w);
void report_names();
void run_steps();
void write_report();
[[noreturn]] void writer_loop();
[[noreturn]] void writer_resume();
bool writer_fault(EXCEPTION_POINTERS *e, bool breakpoint);
LONG CALLBACK last_chance(EXCEPTION_POINTERS *e);
DWORD WINAPI writer_main(void *);
void prune(const std::wstring &dir, const wchar_t *pattern, int keep);

}
