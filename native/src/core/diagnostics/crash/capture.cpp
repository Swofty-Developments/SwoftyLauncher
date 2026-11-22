#include "core/diagnostics/crash/capture.hpp"

#include <cstdio>
#include <cstring>

#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/exit_logic.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"
#include "core/diagnostics/perf.hpp"

namespace runtime {
void hud_fatal();
void session_host_exit();
}

namespace render::d3d12 {
void wait_arm_before_exit();
}

namespace crash {

FaultCapture capture;

void backtrace(CONTEXT ctx, bool skip_self)
{
    auto tib = reinterpret_cast<NT_TIB *>(NtCurrentTeb());
    auto lo = reinterpret_cast<uintptr_t>(tib->StackLimit), hi = reinterpret_cast<uintptr_t>(tib->StackBase);
    for (int i = 0, shown = 0; i < MAX_FRAMES && ctx.Rip; i++) {
        HMODULE m = nullptr;
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           reinterpret_cast<LPCWSTR>(ctx.Rip), &m);
        if (!skip_self || m != process.self)
            cl("[crash]   #%-2d %s", shown++, where(ctx.Rip).c_str());
        if (ctx.Rsp < lo || ctx.Rsp >= hi)
            break;
        DWORD64 ib = 0;
        auto f = RtlLookupFunctionEntry(ctx.Rip, &ib, nullptr);
        if (f) {
            void *hd;
            DWORD64 ef;
            RtlVirtualUnwind(UNW_FLAG_NHANDLER, ib, ctx.Rip, f, &ctx, &hd, &ef, nullptr);
        } else {
            if (!readable(ctx.Rsp, 8))
                break;
            ctx.Rip = *reinterpret_cast<DWORD64 *>(ctx.Rsp);
            ctx.Rsp += 8;
        }
    }
}

bool first_time(uintptr_t ip)
{
    LONG n = capture.seen_count < MAX_SEEN ? capture.seen_count : MAX_SEEN;
    for (LONG i = 0; i < n; i++)
        if (capture.seen[i] == ip)
            return false;
    LONG i = InterlockedIncrement(&capture.seen_count) - 1;
    if (i < MAX_SEEN)
        capture.seen[i] = ip;
    else if (i == MAX_SEEN)
        cl("[crash] %ld distinct fault addresses seen; no further first-chance exceptions are logged", MAX_SEEN);
    return i < MAX_SEEN;
}

void report(const char *when, EXCEPTION_POINTERS *e)
{
    auto r = e->ExceptionRecord;
    auto c = e->ContextRecord;
    LONG n = InterlockedIncrement(&capture.reports);
    cl("[crash] #%ld %s %s (0x%08lx) on thread %lu, %lu s after load, at %s", n, when, code_name(r->ExceptionCode),
       r->ExceptionCode, GetCurrentThreadId(), (GetTickCount() - process.start_tick) / 1000, where(c->Rip).c_str());
    if ((r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION || r->ExceptionCode == EXCEPTION_IN_PAGE_ERROR) &&
        r->NumberParameters >= 2) {
        auto op = r->ExceptionInformation[0];
        cl("[crash]   %s 0x%llx",
           op == 0   ? "reading"
           : op == 1 ? "writing"
           : op == 8 ? "executing (DEP)"
                     : "accessing",
           (unsigned long long)r->ExceptionInformation[1]);
    }
    if (r->ExceptionCode == EXCEPTION_STACK_OVERFLOW)
        return;
    cl("[crash]   rax=%016llx rbx=%016llx rcx=%016llx rdx=%016llx", c->Rax, c->Rbx, c->Rcx, c->Rdx);
    cl("[crash]   rsi=%016llx rdi=%016llx rbp=%016llx rsp=%016llx", c->Rsi, c->Rdi, c->Rbp, c->Rsp);
    cl("[crash]   r8 =%016llx r9 =%016llx r10=%016llx r11=%016llx", c->R8, c->R9, c->R10, c->R11);
    cl("[crash]   r12=%016llx r13=%016llx r14=%016llx r15=%016llx", c->R12, c->R13, c->R14, c->R15);
    if (readable(c->Rip, 16)) {
        auto b = reinterpret_cast<const uint8_t *>(c->Rip);
        cl("[crash]   code: %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X %02X", b[0], b[1], b[2],
           b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]);
    }
    cl("[crash]   stack:");
    backtrace(*c, false);
}

bool enter() { return InterlockedCompareExchange(&capture.busy, (LONG)GetCurrentThreadId(), 0) == 0; }

void leave() { InterlockedExchange(&capture.busy, 0); }

void stack_bounds(uintptr_t &lo, uintptr_t &hi)
{
    auto tib = reinterpret_cast<NT_TIB *>(NtCurrentTeb());
    lo = reinterpret_cast<uintptr_t>(tib->StackLimit);
    hi = reinterpret_cast<uintptr_t>(tib->StackBase);
}

bool holds_loader_lock()
{
    auto teb = reinterpret_cast<uintptr_t>(NtCurrentTeb());
    uintptr_t peb = *reinterpret_cast<uintptr_t *>(teb + 0x60);
    if (!readable(peb + 0x110, 8))
        return false;
    uintptr_t cs = *reinterpret_cast<uintptr_t *>(peb + 0x110);
    return cs && readable(cs + 0x10, 8) && *reinterpret_cast<uintptr_t *>(cs + 0x10) == uintptr_t(GetCurrentThreadId());
}

bool deliberate_abort(const EXCEPTION_RECORD *r, const CONTEXT *c)
{
    return r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2 && r->ExceptionInformation[0] == 1 &&
           r->ExceptionInformation[1] == 1 && readable(c->Rip, sizeof FATAL_ABORT) &&
           !memcmp(reinterpret_cast<const void *>(c->Rip), FATAL_ABORT, sizeof FATAL_ABORT);
}

void keep_fatal(EXCEPTION_POINTERS *e)
{
    InterlockedIncrement(&capture.fatal_count);
    if (InterlockedCompareExchange(&capture.snapshot_lock, 1, 0))
        return;
    capture.last_fatal.rec = *e->ExceptionRecord;
    capture.last_fatal.rec.ExceptionRecord = nullptr;
    memcpy(&capture.last_fatal.ctx, e->ContextRecord, sizeof(CONTEXT));
    capture.last_fatal.tid = GetCurrentThreadId();
    capture.last_fatal.ms = GetTickCount64();
    stack_bounds(capture.last_fatal.lo, capture.last_fatal.hi);
    InterlockedExchange(&capture.snapshot_lock, 0);
}

bool request_report(Kind kind, const char *reason, const EXCEPTION_RECORD *rec, const CONTEXT *ctx)
{
    if (InterlockedCompareExchange(&report_writer.claimed, 1, 0))
        return false;
    Request &req = report_writer.req;
    req.kind = kind;
    req.tid = GetCurrentThreadId();
    req.ms = GetTickCount64();
    stack_bounds(req.lo, req.hi);
    req.blame[0] = 0;
    if (capture.blame_text[0] && capture.blame_tid == req.tid && req.ms - capture.blame_ms < BLAME_MS)
        snprintf(req.blame, sizeof req.blame, "%s", capture.blame_text);
    if (req.blame[0])
        snprintf(req.reason, sizeof req.reason, "%s; %s", req.blame, reason);
    else
        snprintf(req.reason, sizeof req.reason, "%s", reason);
    req.has_exception = rec != nullptr;
    if (rec) {
        req.rec = *rec;
        req.rec.ExceptionRecord = nullptr;
    }
    memcpy(&req.ctx, ctx, sizeof(CONTEXT));
    req.nprobes = perf::open_probes(req.probes, 32);
    req.loader_lock = holds_loader_lock();
    req.has_prior = false;
    if (kind == K_EXIT && !InterlockedCompareExchange(&capture.snapshot_lock, 1, 0)) {
        if (capture.last_fatal.ms && req.ms - capture.last_fatal.ms < PRIOR_MS) {
            req.prior = capture.last_fatal;
            req.has_prior = true;
        }
        InterlockedExchange(&capture.snapshot_lock, 0);
    }
    if (report_writer.writer_tid && GetCurrentThreadId() != report_writer.writer_tid) {
        SetEvent(report_writer.go_event);
        HANDLE text[2] = {report_writer.text_event, report_writer.done_event};
        WaitForMultipleObjects(2, text, FALSE, TEXT_WAIT_MS);
        WaitForSingleObject(report_writer.done_event, report_writer.dump_mode == DUMP_FULL ? FULL_DUMP_WAIT_MS : DUMP_WAIT_MS);
    } else if (kind != K_EXCEPTION || req.rec.ExceptionCode != EXCEPTION_STACK_OVERFLOW) {
        write_report();
    }
    return true;
}

void already(const char *what)
{
    if (report_writer.claimed && report_writer.report_name[0])
        cl("[crash] %s; %s already has this process's crash report", what, report_writer.report_where);
}

LONG CALLBACK veh(EXCEPTION_POINTERS *e)
{
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (capture.stall_tid && capture.stall_tid == GetCurrentThreadId())
        return EXCEPTION_CONTINUE_SEARCH;
    if (writer_fault(e, false))
        return EXCEPTION_CONTINUE_EXECUTION;
    if (!fatal(code))
        return EXCEPTION_CONTINUE_SEARCH;
    keep_fatal(e);
    bool abort = deliberate_abort(e->ExceptionRecord, e->ContextRecord);
    bool certain = abort || code == 0xC0000374 || code == 0xC0000409 || code == EXCEPTION_STACK_OVERFLOW;
    if (!enter())
        return EXCEPTION_CONTINUE_SEARCH;
    uintptr_t ip = e->ContextRecord->Rip;
    std::string at;
    if (first_time(ip)) {
        at = where(ip);
        if (capture.reports < MAX_REPORTS)
            report("first-chance (the game may still handle it)", e);
        else
            cl("[crash] first-chance %s (0x%08lx) at %s; report cap reached, no details", code_name(code), code, at.c_str());
        blackbox::note("exception", fmt("first-chance %s at ", code_name(code)).c_str(), at.c_str());
    }
    leave();
    if (certain) {
        if (code != EXCEPTION_STACK_OVERFLOW)
            runtime::hud_fatal();
        if (at.empty() && code != EXCEPTION_STACK_OVERFLOW)
            at = where(ip);
        std::string why = abort ? "the game aborted itself (Ne_FatalAbort writes 0xDEADDEAD to address 1) at " + at
                                : fmt("%s (0x%08lx) at ", code_name(code), code) + at;
        if (!request_report(abort ? K_FATAL : K_EXCEPTION, why.c_str(), e->ExceptionRecord, e->ContextRecord))
            already(abort ? "Ne_FatalAbort" : code_name(code));
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

LONG WINAPI unhandled(EXCEPTION_POINTERS *e)
{
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (enter()) {
        cl("[crash] unhandled %s (0x%08lx) at %s reached the top-level filter; %s", code_name(code), code,
           where(e->ContextRecord->Rip).c_str(), capture.previous ? "passing it to the game's filter" : "the process ends here");
        if (!fatal(code) || first_time(e->ContextRecord->Rip))
            report("unhandled", e);
        leave();
    }
    std::string why = fmt("unhandled %s (0x%08lx)", code_name(code), code);
    auto r = e->ExceptionRecord;
    if ((code == EXCEPTION_ACCESS_VIOLATION || code == EXCEPTION_IN_PAGE_ERROR) && r->NumberParameters >= 2)
        why += fmt(" %s 0x%llx",
                   r->ExceptionInformation[0] == 1   ? "writing"
                   : r->ExceptionInformation[0] == 8 ? "executing"
                                                     : "reading",
                   (unsigned long long)r->ExceptionInformation[1]);
    why += " at " + where(e->ContextRecord->Rip);
    if (!request_report(K_EXCEPTION, why.c_str(), r, e->ContextRecord))
        already("unhandled exception");
    return capture.previous ? capture.previous(e) : EXCEPTION_CONTINUE_SEARCH;
}

void window_stage(int) { InterlockedExchange(&process.window_reached, 1); }

// Whether the launcher asked this game's session to stop: the data folder's sessions\latest.txt names the session
// folder, and its stop_requested was written after this process started.
bool launcher_stop()
{
    std::wstring data = data_folder();
    if (data.empty())
        return false;
    HANDLE f = CreateFileW((data + L"\\sessions\\latest.txt").c_str(), GENERIC_READ,
                           FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
                           nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return false;
    char text[MAX_PATH * 4];
    DWORD n = 0;
    BOOL read = ReadFile(f, text, sizeof text, &n, nullptr);
    CloseHandle(f);
    std::string file = read ? exit_logic::stop_file(std::string(text, n)) : std::string();
    wchar_t path[MAX_PATH * 2];
    if (file.empty() || !MultiByteToWideChar(CP_UTF8, 0, file.c_str(), -1, path, MAX_PATH * 2))
        return false;
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    FILETIME created{}, exited{}, kernel{}, user{};
    if (!GetFileAttributesExW(path, GetFileExInfoStandard, &fa) ||
        !GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user))
        return false;
    auto count = [](FILETIME t) { return uint64_t(t.dwHighDateTime) << 32 | t.dwLowDateTime; };
    return exit_logic::stop_is_ours(count(fa.ftLastWriteTime), count(created));
}

void on_exit(const char *fn, UINT code)
{
    perf::flush(fn);
    perf::stop(EXIT_WRITER_WAIT_MS);
    runtime::session_host_exit();
    if (InterlockedExchange(&capture.exited, 1) || !enter())
        return;
    bool early = !process.window_reached, stack = code || early;
    cl("[crash] %s(%u) on thread %lu, %lu s after load%s%s", fn, code, GetCurrentThreadId(),
       (GetTickCount() - process.start_tick) / 1000, early ? ", before the game's window" : "", stack ? "; caller stack:" : "");
    blackbox::note("exit", fmt("%s(%u)", fn, code).c_str());
    alignas(16) CONTEXT ctx;
    RtlCaptureContext(&ctx);
    if (stack)
        backtrace(ctx, true);
    leave();
    ULONGLONG now = GetTickCount64(), since = capture.last_fatal.ms ? now - capture.last_fatal.ms : ~0ull;
    exit_logic::Exit x{code, since < EXIT_AFTER_FATAL_MS, now, capture.close_ms, code && launcher_stop()};
    switch (exit_logic::verdict(x)) {
    case exit_logic::Verdict::Clean:
        return;
    case exit_logic::Verdict::Close:
        cl("[crash] %s(%u) ends a close the game was asked for (%s); no crash report", fn, code,
           x.stop_requested ? "the launcher asked the session to stop"
                            : fmt("%s %.1f s earlier", capture.close_msg, double(now - x.close_ms) / 1000).c_str());
        return;
    case exit_logic::Verdict::Crash:
        break;
    }
    std::string why = fmt("%s(%u = 0x%x) on thread %lu", fn, code, code, GetCurrentThreadId());
    if (!code)
        why += fmt(", %.1f s after a fatal-class exception", double(since) / 1000);
    if (!request_report(K_EXIT, why.c_str(), nullptr, &ctx))
        already(fn);
}

void WINAPI hooked_exit(UINT code)
{
    on_exit("ExitProcess", code);
    render::d3d12::wait_arm_before_exit();
    capture.exit_hook.unsafe_call<void>(code);
}

BOOL WINAPI hooked_terminate(HANDLE process, UINT code)
{
    if (GetProcessId(process) == GetCurrentProcessId())
        on_exit("TerminateProcess", code);
    return capture.terminate_hook.unsafe_call<BOOL>(process, code);
}

LONG NTAPI hooked_nt_terminate(HANDLE process, LONG status)
{
    if (!capture.exited && (!process || GetProcessId(process) == GetCurrentProcessId()))
        on_exit(process ? "NtTerminateProcess" : "NtTerminateProcess(other threads)", UINT(status));
    return capture.nt_terminate_hook.unsafe_call<LONG>(process, status);
}

}
