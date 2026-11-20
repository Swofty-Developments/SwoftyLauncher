#include "core/diagnostics/crash/report_sections.hpp"

#include <tlhelp32.h>

#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/crash_dir.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_text_logic.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"

// The crash report's sections about the process: the addons and loaded modules, every thread's first frames and
// the end of loader.log.
namespace crash {

void section_modules()
{
    report_writer.out.put("\n== Addons (swofty\\settings.json)\n%s", sections.addon_text.c_str());
    report_writer.out.put("\n== Loaded modules (%d)\n", modules.nmods);
    for (int i = 0; i < modules.nmods; i++) {
        const Mod &m = modules.mods[i];
        uint32_t stamp = 0;
        auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(m.base);
        if (readable(m.base, sizeof *dos) && readable(m.base + dos->e_lfanew, sizeof(IMAGE_NT_HEADERS)))
            stamp = reinterpret_cast<const IMAGE_NT_HEADERS *>(m.base + dos->e_lfanew)->FileHeader.TimeDateStamp;
        report_writer.out.put("  %016llx %10llx  stamp %08x  %s\n", (unsigned long long)m.base, (unsigned long long)m.size, stamp,
                              m.path[0] ? m.path : m.name);
    }
}

/* Each thread is suspended only while its context is read and its first frames are named; nothing in
   that window allocates, since a suspended thread may hold the heap lock. */
void section_threads(const Request &r)
{
    report_writer.out.put("\n== Threads\n");
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snap == INVALID_HANDLE_VALUE) {
        report_writer.out.put("  the thread list is not available (error %lu)\n", GetLastError());
        return;
    }
    THREADENTRY32 te{};
    te.dwSize = sizeof te;
    DWORD pid = GetCurrentProcessId();
    int count = 0;
    static char frames[THREAD_FRAMES][320];
    for (BOOL ok = Thread32First(snap, &te); ok; ok = Thread32Next(snap, &te)) {
        if (te.th32OwnerProcessID != pid)
            continue;
        count++;
        DWORD tid = te.th32ThreadID;
        char desc[96] = "";
        HANDLE h = OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION, FALSE, tid);
        if (h && process.thread_description) {
            PWSTR d = nullptr;
            if (SUCCEEDED(process.thread_description(h, &d)) && d) {
                WideCharToMultiByte(CP_UTF8, 0, d, -1, desc, sizeof desc - 1, nullptr, nullptr);
                LocalFree(d);
            }
        }
        report_writer.out.put("  thread %lu, priority %ld%s%s%s%s\n", tid, te.tpBasePri, desc[0] ? " \"" : "", desc,
                              desc[0] ? "\"" : "",
                              tid == r.tid                        ? " (the reporting thread, see the stack above)"
                              : tid == GetCurrentThreadId()       ? " (the crash report writer)"
                              : r.has_prior && tid == r.prior.tid ? " (the thread of the earlier exception, see above)"
                                                                  : "");
        if (!h || tid == r.tid || tid == GetCurrentThreadId()) {
            if (h)
                CloseHandle(h);
            continue;
        }
        int n = 0;
        alignas(16) CONTEXT ctx{};
        ctx.ContextFlags = CONTEXT_FULL;
        if (SuspendThread(h) != DWORD(-1)) {
            report_writer.suspended = h;
            if (GetThreadContext(h, &ctx)) {
                uintptr_t lo = 0, hi = 0;
                struct {
                    LONG status;
                    void *teb;
                    uintptr_t pid, tid;
                    uintptr_t affinity;
                    LONG priority, base;
                } tbi{};
                if (process.query_thread && process.query_thread(h, 0, &tbi, sizeof tbi, nullptr) >= 0 && tbi.teb &&
                    readable(reinterpret_cast<uintptr_t>(tbi.teb), sizeof(NT_TIB))) {
                    auto tib = static_cast<NT_TIB *>(tbi.teb);
                    lo = reinterpret_cast<uintptr_t>(tib->StackLimit);
                    hi = reinterpret_cast<uintptr_t>(tib->StackBase);
                }
                n = walk(ctx, lo, hi ? hi : ctx.Rsp + 1, THREAD_FRAMES, [&](int i, const CONTEXT &c) {
                    where_into(frames[i], sizeof frames[i], c.Rip, false);
                    return true;
                });
            }
            report_writer.suspended = nullptr;
            ResumeThread(h);
        }
        CloseHandle(h);
        for (int i = 0; i < n; i++)
            report_writer.out.put("      #%d %s\n", i, frames[i]);
    }
    CloseHandle(snap);
    report_writer.out.put("  %d thread(s)\n", count);
}

void section_log()
{
    report_writer.out.put("\n== loader.log (last %d lines)\n", LOG_LINES);
    HANDLE f = CreateFileW((process.swofty_dir + L"loader.log").c_str(), GENERIC_READ,
                           FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE) {
        std::wstring other = crash_dir::fallback(data_folder());
        if (!other.empty())
            f = CreateFileW((other + L"loader-fallback.log").c_str(), GENERIC_READ,
                            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, 0, nullptr);
    }
    if (f == INVALID_HANDLE_VALUE) {
        report_writer.out.put("  loader.log could not be opened (error %lu)\n", GetLastError());
        return;
    }
    static char tail[LOG_TAIL + 1];
    LARGE_INTEGER size{}, at{};
    GetFileSizeEx(f, &size);
    at.QuadPart = size.QuadPart > LONGLONG(LOG_TAIL) ? size.QuadPart - LONGLONG(LOG_TAIL) : 0;
    SetFilePointerEx(f, at, nullptr, FILE_BEGIN);
    DWORD got = 0;
    ReadFile(f, tail, DWORD(LOG_TAIL), &got, nullptr);
    CloseHandle(f);
    char *start = report_text::log_tail(tail, got, LOG_LINES);
    report_writer.out.put("%s%s", start, got && tail[got - 1] != '\n' ? "\n" : "");
}

}
