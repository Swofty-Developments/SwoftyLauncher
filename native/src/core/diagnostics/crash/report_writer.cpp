#include "core/diagnostics/crash/report_writer.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_dir.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_sections.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"

namespace crash {

ReportWriter report_writer;

void local_stamp(char *b, size_t n, bool file_name)
{
    SYSTEMTIME t;
    GetLocalTime(&t);
    snprintf(b, n, file_name ? "%04u%02u%02u-%02u%02u%02u" : "%04u-%02u-%02u %02u:%02u:%02u.%03u", t.wYear, t.wMonth, t.wDay,
             t.wHour, t.wMinute, t.wSecond, t.wMilliseconds);
}

bool create_report(const std::wstring &dir, const char *stamp)
{
    for (int i = 0; i < 100; i++) {
        snprintf(report_writer.report_name, sizeof report_writer.report_name - 4, i ? "crash-%s-%d" : "crash-%s", stamp, i + 1);
        wchar_t wname[64];
        MultiByteToWideChar(CP_UTF8, 0, report_writer.report_name, -1, wname, 64);
        swprintf(report_writer.report_path, MAX_PATH, L"%ls%ls.txt", dir.c_str(), wname);
        swprintf(report_writer.dump_path, MAX_PATH, L"%ls%ls.dmp", dir.c_str(), wname);
        report_writer.out.file = CreateFileW(report_writer.report_path, GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_NEW,
                                             FILE_ATTRIBUTE_NORMAL, nullptr);
        if (report_writer.out.file != INVALID_HANDLE_VALUE)
            return true;
        if (GetLastError() != ERROR_FILE_EXISTS)
            return false;
    }
    return false;
}

std::string narrow(const std::wstring &w)
{
    char b[MAX_PATH * 3];
    int n = WideCharToMultiByte(CP_UTF8, 0, w.c_str(), -1, b, sizeof b, nullptr, nullptr);
    return n > 0 ? std::string(b, n - 1) : std::string();
}

void report_names()
{
    char stamp[32];
    local_stamp(stamp, sizeof stamp, true);
    DWORD refused = 0;
    report_writer.crash_dir = process.swofty_dir;
    if (!create_report(report_writer.crash_dir, stamp)) {
        refused = GetLastError();
        std::wstring other = crash_dir::fallback(data_folder());
        if (!other.empty()) {
            CreateDirectoryW(other.c_str(), nullptr);
            if (create_report(other, stamp))
                report_writer.crash_dir = other;
        }
    }
    strcat(report_writer.report_name, ".txt");
    snprintf(report_writer.report_where, sizeof report_writer.report_where, "%s",
             crash_dir::logged(narrow(report_writer.crash_dir), narrow(process.swofty_dir), report_writer.report_name).c_str());
    if (report_writer.crash_dir != process.swofty_dir)
        cl("[crash] the game folder refused the crash report (error %lu); it goes to the data folder", refused);
}

void run_steps()
{
    while (report_writer.step_at < NSTEPS) {
        STEPS[report_writer.step_at].run(report_writer.req);
        report_writer.out.flush();
        report_writer.step_at = report_writer.step_at + 1;
    }
}

void write_report()
{
    Request &r = report_writer.req;
    if (r.handed)
        settle_handed(r);
    InterlockedExchange(&report_writer.writing, 1);
    cl("[crash] writing a crash report: %s", r.reason);
    modules.snapshot();
    report_names();
    report_writer.out.n = report_writer.out.written = 0;
    report_writer.out.kept = true;
    report_writer.step_at = 0;
    run_steps();
    InterlockedExchange(&report_writer.writing, 0);
}

[[noreturn]] void writer_loop()
{
    for (;;) {
        InterlockedExchange(&report_writer.writing, 0);
        SetEvent(report_writer.text_event);
        SetEvent(report_writer.done_event);
        WaitForSingleObject(report_writer.go_event, INFINITE);
        write_report();
    }
}

/* Entered in place of the faulting instruction, on a clean part of the writer's own stack. A thread
   the Threads section had suspended is resumed and a half-written minidump is dropped; the report
   notes what faulted where and goes on with the next step. */
[[noreturn]] void writer_resume()
{
    if (HANDLE h = report_writer.suspended) {
        report_writer.suspended = nullptr;
        ResumeThread(h);
        CloseHandle(h);
    }
    LONG step = report_writer.step_at, n = InterlockedIncrement(&report_writer.writer_faults);
    if (step == CLOSE_STEP - 1 && report_writer.dump_file != INVALID_HANDLE_VALUE) {
        CloseHandle(report_writer.dump_file);
        report_writer.dump_file = INVALID_HANDLE_VALUE;
        DeleteFileW(report_writer.dump_path);
        report_writer.out.put("\n== Minidump\n  not written: the writer faulted while writing it\n");
    }
    char w[512];
    where_into(w, sizeof w, report_writer.fault_ip, false);
    report_writer.out.put("\n[%s is cut short: the writer hit %s (0x%08lx) at %s]\n", STEPS[step].name,
                          code_name(report_writer.fault_code), report_writer.fault_code, w);
    report_writer.out.flush();
    cl("[crash] the crash report writer hit %s (0x%08lx) at %s in %s of %s; %s", code_name(report_writer.fault_code),
       report_writer.fault_code, w, STEPS[step].name, report_writer.report_where,
       n >= MAX_WRITER_FAULTS ? "too many faults, the report ends here" : "it goes on with the next step");
    report_writer.step_at = n >= MAX_WRITER_FAULTS ? std::max(step + 1, CLOSE_STEP) : step + 1;
    run_steps();
    writer_loop();
}

/* A fatal-class fault on the writer thread while it writes moves it to writer_resume instead of
   ending the process. veh checks it first, before any other handler; a breakpoint is checked last
   (last_chance), since other code plants int3 hooks in APIs the writer calls (the offline hook does in
   GetModuleFileNameW) and its own handler takes those. */
bool writer_fault(EXCEPTION_POINTERS *e, bool breakpoint)
{
    if (!report_writer.writing || GetCurrentThreadId() != report_writer.writer_tid || !report_writer.writer_stack_top)
        return false;
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (breakpoint ? code != EXCEPTION_BREAKPOINT : !fatal(code))
        return false;
    report_writer.fault_code = code;
    report_writer.fault_ip = reinterpret_cast<uintptr_t>(e->ExceptionRecord->ExceptionAddress);
    e->ContextRecord->Rip = reinterpret_cast<DWORD64>(&writer_resume);
    e->ContextRecord->Rsp = ((report_writer.writer_stack_top - 0x4000) & ~0xfull) - 8;
    return true;
}

LONG CALLBACK last_chance(EXCEPTION_POINTERS *e)
{
    return writer_fault(e, true) ? EXCEPTION_CONTINUE_EXECUTION : EXCEPTION_CONTINUE_SEARCH;
}

DWORD WINAPI writer_main(void *)
{
    uintptr_t lo;
    stack_bounds(lo, report_writer.writer_stack_top);
    WaitForSingleObject(report_writer.go_event, INFINITE);
    write_report();
    writer_loop();
}

void prune(const std::wstring &dir, const wchar_t *pattern, int keep)
{
    std::vector<std::wstring> names;
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((dir + pattern).c_str(), &fd);
    if (h == INVALID_HANDLE_VALUE)
        return;
    do
        names.push_back(fd.cFileName);
    while (FindNextFileW(h, &fd));
    FindClose(h);
    std::sort(names.begin(), names.end());
    for (size_t i = 0; i + keep < names.size(); i++)
        DeleteFileW((dir + names[i]).c_str());
}

}
