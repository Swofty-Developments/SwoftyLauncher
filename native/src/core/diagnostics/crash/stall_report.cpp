#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <tlhelp32.h>

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <string>

#include "core/diagnostics/crash/blackbox.hpp"
#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_dir.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_text_logic.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/stall_frames.hpp"
#include "core/diagnostics/crash/stall_order_logic.hpp"
#include "core/diagnostics/crash/stall_report.hpp"
#include "core/diagnostics/crash/stall_walk.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"
#include "core/diagnostics/session_loader.hpp"
#include "core/runtime/loader/introspection.hpp"

namespace crash {

namespace {

/* Load-stall reports, asked for by load_stall.cpp when a load stops moving. Every thread but the asking one and the
   crash report writer is suspended in turn, only while its context is read and its stack is unwound into a table
   allocated beforehand; nothing in that window allocates, logs or takes a lock, since a suspended thread may hold
   the heap lock or the logger's. The module list, each module's function table and the thread names are read
   before the first suspension; the frames are named and the text written after the last thread runs again. */

constexpr int STALL_THREADS = 384, STALL_FRAMES = 40, KEEP_STALLS = 4;
constexpr size_t STALL_TEXT = size_t(3) << 20;

struct Sampled {
    DWORD tid, error;
    LONG priority;
    bool walked, faulted, self_thread, writer_thread;
    int frames, same_as;
    uint64_t rip[STALL_FRAMES], rsp[STALL_FRAMES];
    uint64_t regs[runtime::stall_order::kRegs];
    runtime::stall_order::Kept kept;
    char desc[80];
};

struct Text {
    char *b = nullptr;
    size_t cap = 0, n = 0;

    void put(const char *f, ...) __attribute__((format(printf, 2, 3)))
    {
        if (!b || n + 1 >= cap)
            return;
        va_list ap;
        va_start(ap, f);
        int k = vsnprintf(b + n, cap - n, f, ap);
        va_end(ap);
        if (k > 0)
            n = std::min(cap - 1, n + size_t(k));
    }
};

constinit Sampled *stall_threads{};
constinit Text stall_text{};
constinit volatile LONG stall_busy;

const char *wait_kind(const Sampled &s, char *w, size_t cap)
{
    for (int i = 0; i < s.frames && i < 4; i++) {
        uint32_t off = 0;
        const char *name = export_of(s.rip[i], off);
        if (name && runtime::stall_order::wait_export(name)) {
            snprintf(w, cap, "%s", name);
            return w;
        }
    }
    return nullptr;
}

DWORD loader_lock_owner(LONG &recursion)
{
    auto teb = reinterpret_cast<uintptr_t>(NtCurrentTeb());
    uintptr_t peb = *reinterpret_cast<uintptr_t *>(teb + 0x60);
    recursion = 0;
    if (!readable(peb + 0x110, 8))
        return 0;
    auto cs = *reinterpret_cast<const RTL_CRITICAL_SECTION *const *>(peb + 0x110);
    if (!cs || !readable(uintptr_t(cs), sizeof *cs))
        return 0;
    recursion = cs->RecursionCount;
    return DWORD(uintptr_t(cs->OwningThread));
}

void stall_regs(const CONTEXT &c, uint64_t *r)
{
    namespace so = runtime::stall_order;
    r[so::Rax] = c.Rax;
    r[so::Rbx] = c.Rbx;
    r[so::Rcx] = c.Rcx;
    r[so::Rdx] = c.Rdx;
    r[so::Rsi] = c.Rsi;
    r[so::Rdi] = c.Rdi;
    r[so::Rbp] = c.Rbp;
    r[so::Rsp] = c.Rsp;
    r[so::R8] = c.R8;
    r[so::R9] = c.R9;
    r[so::R10] = c.R10;
    r[so::R11] = c.R11;
    r[so::R12] = c.R12;
    r[so::R13] = c.R13;
    r[so::R14] = c.R14;
    r[so::R15] = c.R15;
    r[so::Rip] = c.Rip;
    r[so::EFlags] = c.EFlags;
}

int stall_sample()
{
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snap == INVALID_HANDLE_VALUE)
        return -1;
    THREADENTRY32 te{};
    te.dwSize = sizeof te;
    DWORD pid = GetCurrentProcessId(), me = GetCurrentThreadId();
    int n = 0;
    for (BOOL ok = Thread32First(snap, &te); ok && n < STALL_THREADS; ok = Thread32Next(snap, &te)) {
        if (te.th32OwnerProcessID != pid)
            continue;
        Sampled &s = stall_threads[n++];
        memset(&s, 0, sizeof s);
        s.tid = te.th32ThreadID;
        s.priority = te.tpBasePri;
        s.same_as = -1;
        s.self_thread = s.tid == me;
        s.writer_thread = report_writer.writer_tid && s.tid == report_writer.writer_tid;
    }
    CloseHandle(snap);
    for (int i = 0; i < n; i++) {
        Sampled &s = stall_threads[i];
        HANDLE h = process.thread_description ? OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, s.tid) : nullptr;
        if (!h)
            continue;
        PWSTR d = nullptr;
        if (SUCCEEDED(process.thread_description(h, &d)) && d) {
            WideCharToMultiByte(CP_UTF8, 0, d, -1, s.desc, sizeof s.desc - 1, nullptr, nullptr);
            LocalFree(d);
        }
        CloseHandle(h);
    }
    capture.stall_tid = me;
    for (int i = 0; i < n; i++) {
        Sampled &s = stall_threads[i];
        if (s.self_thread || s.writer_thread)
            continue;
        HANDLE h = OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION, FALSE, s.tid);
        if (!h) {
            s.error = GetLastError();
            continue;
        }
        if (SuspendThread(h) == DWORD(-1)) {
            s.error = GetLastError();
            CloseHandle(h);
            continue;
        }
        alignas(16) CONTEXT ctx{};
        ctx.ContextFlags = CONTEXT_FULL;
        if (GetThreadContext(h, &ctx)) {
            stall_regs(ctx, s.regs);
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
            stall_walk::Kept kept;
            kept.lo = process.exe_base;
            kept.hi = process.exe_base + process.exe_size;
            s.frames = stall_walk::walk(&ctx, lo, hi ? hi : ctx.Rsp + 1, STALL_FRAMES, stall_lookup, readable, s.rip, s.rsp,
                                        &s.faulted, process.exe_base ? &kept : nullptr);
            s.kept = kept.at;
            s.walked = true;
        } else {
            s.error = GetLastError();
        }
        ResumeThread(h);
        CloseHandle(h);
    }
    capture.stall_tid = 0;
    for (int i = 0; i < n; i++)
        for (int j = 0; j < i && stall_threads[i].same_as < 0; j++) {
            const Sampled &a = stall_threads[i], &b = stall_threads[j];
            if (a.walked && b.walked && b.same_as < 0 && a.frames > 1 && a.frames == b.frames &&
                !memcmp(a.rip, b.rip, sizeof(uint64_t) * a.frames))
                stall_threads[i].same_as = j;
        }
    return n;
}

void stall_threads_text(int n)
{
    Text &t = stall_text;
    char w[512], kind[64];
    int sampled = 0, failed = 0, alike = 0;
    for (int i = 0; i < n; i++) {
        const Sampled &s = stall_threads[i];
        sampled += s.walked;
        failed += !s.walked && !s.self_thread && !s.writer_thread;
        alike += s.same_as >= 0;
    }
    t.put("\n== Threads (%d; %d unwound, %d not readable, %d with the same stack as an earlier one)\n", n, sampled, failed,
          alike);
    for (int i = 0; i < n; i++) {
        const Sampled &s = stall_threads[i];
        const char *wait = s.walked ? wait_kind(s, kind, sizeof kind) : nullptr;
        t.put("thread %lu%s%s%s, priority %ld%s%s%s%s\n", (unsigned long)s.tid, s.desc[0] ? " \"" : "", s.desc,
              s.desc[0] ? "\"" : "", s.priority, s.self_thread ? " (the load-stall watchdog, not suspended)" : "",
              s.writer_thread ? " (the crash report writer, not suspended)" : "", wait ? ", waiting in " : "", wait ? wait : "");
        if (s.self_thread || s.writer_thread)
            continue;
        if (!s.walked) {
            t.put("  not read (error %lu)\n", (unsigned long)s.error);
            continue;
        }
        if (s.same_as >= 0) {
            t.put("  same stack as thread %lu\n", (unsigned long)stall_threads[s.same_as].tid);
            continue;
        }
        for (int f = 0; f < s.frames; f++) {
            stall_frame(w, sizeof w, s.rip[f]);
            t.put("  #%-2d %016llx rsp=%016llx  %s\n", f, (unsigned long long)s.rip[f], (unsigned long long)s.rsp[f], w);
        }
        if (s.frames <= 1)
            t.put("  (the unwind stopped at the first frame)\n");
        if (s.faulted)
            t.put("  (the unwind faulted reading this stack and stopped)\n");
    }
}

void stall_running_text(int n)
{
    namespace so = runtime::stall_order;
    static so::Thread order[STALL_THREADS];
    static int pick[STALL_THREADS];
    static char block[2048];
    char w[512], kind[64];
    int game = 0;
    for (int i = 0; i < n; i++) {
        const Sampled &s = stall_threads[i];
        so::Thread &o = order[i];
        o.read = s.walked && !s.self_thread && !s.writer_thread;
        o.waiting = o.read && wait_kind(s, kind, sizeof kind);
        o.game = o.read && s.kept.frame >= 0;
        game += o.read && !o.waiting && o.game;
    }
    int k = so::running_first(order, n, pick);
    Text &t = stall_text;
    so::heading(k, game, block, sizeof block);
    t.put("%s", block);
    for (int j = 0; j < k; j++) {
        const Sampled &s = stall_threads[pick[j]];
        t.put("thread %lu%s%s%s, priority %ld, not in a wait%s\n", (unsigned long)s.tid, s.desc[0] ? " \"" : "", s.desc,
              s.desc[0] ? "\"" : "", s.priority, s.kept.frame >= 0 ? ", in game code" : "");
        t.put("  registers as suspended:\n");
        so::regs_text(s.regs, block, sizeof block);
        t.put("%s", block);
        so::kept_text(s.kept, block, sizeof block);
        t.put("%s", block);
        if (s.kept.frame >= 0) {
            uint64_t q[so::kFrameQwords] = {};
            int got = 0;
            while (got < so::kFrameQwords && readable(uintptr_t(s.kept.rsp) + got * 8u, 8)) {
                q[got] = *reinterpret_cast<const uint64_t *>(uintptr_t(s.kept.rsp) + got * 8u);
                got++;
            }
            so::stack_text(s.kept.rsp, q, got, block, sizeof block);
            t.put("%s", block);
        }
        int top = std::min(s.frames, so::kTopFrames);
        t.put("  top %d of %d frame(s):\n", top, s.frames);
        for (int f = 0; f < top; f++) {
            stall_frame(w, sizeof w, s.rip[f]);
            t.put("  #%-2d %016llx rsp=%016llx  %s\n", f, (unsigned long long)s.rip[f], (unsigned long long)s.rsp[f], w);
        }
        if (s.faulted)
            t.put("  (the unwind faulted reading this stack and stopped)\n");
    }
}

void stall_core_text(int n)
{
    Text &t = stall_text;
    char w[512];
    t.put("\n== Threads in swofty code (core and addons; a core lock's waiter shows a swofty frame under a wait)\n");
    int shown = 0;
    for (int i = 0; i < n; i++) {
        const Sampled &s = stall_threads[i];
        const Sampled &src = s.same_as >= 0 ? stall_threads[s.same_as] : s;
        if (!s.walked)
            continue;
        for (int f = 0; f < src.frames; f++) {
            const Mod *m = modules.at(src.rip[f]);
            bool core = m && process.self && m->base == reinterpret_cast<uintptr_t>(process.self);
            std::string owner = !m ? core_code_owner(src.rip[f]) : std::string();
            if (!core && owner.empty())
                continue;
            stall_frame(w, sizeof w, src.rip[f]);
            t.put("  thread %lu frame #%d: %s\n", (unsigned long)s.tid, f, w);
            shown++;
            break;
        }
    }
    if (!shown)
        t.put("  none\n");
    LONG recursion = 0;
    DWORD owner = loader_lock_owner(recursion);
    if (owner)
        t.put("  the loader lock is held by thread %lu (recursion %ld)\n", (unsigned long)owner, recursion);
    else
        t.put("  the loader lock is free\n");
    t.put("  core std::mutex owners are not tracked\n");
}

bool create_stall(const std::wstring &dir, const char *stamp, wchar_t *path, HANDLE &f)
{
    for (int i = 0; i < 100; i++) {
        wchar_t name[64];
        swprintf(name, 64, i ? L"stall-%hs-%d.txt" : L"stall-%hs.txt", stamp, i + 1);
        swprintf(path, MAX_PATH, L"%ls%ls", dir.c_str(), name);
        f = CreateFileW(path, GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (f != INVALID_HANDLE_VALUE)
            return true;
        if (GetLastError() != ERROR_FILE_EXISTS)
            return false;
    }
    return false;
}

}

}

namespace runtime {

bool stall_report(int index, int max, const char *why, const char *state)
{
    using namespace crash;
    if (!why)
        why = "";
    if (report_writer.claimed || report_writer.writing || InterlockedCompareExchange(&stall_busy, 1, 0))
        return false;
    if (!stall_threads)
        stall_threads = static_cast<Sampled *>(
            VirtualAlloc(nullptr, sizeof(Sampled) * STALL_THREADS, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
    if (!stall_text.b) {
        stall_text.b = static_cast<char *>(VirtualAlloc(nullptr, STALL_TEXT, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
        stall_text.cap = stall_text.b ? STALL_TEXT : 0;
    }
    if (!stall_threads || !stall_text.b) {
        cl("[stall] no memory for a load-stall report");
        InterlockedExchange(&stall_busy, 0);
        return false;
    }
    modules.snapshot();
    stall_tables();
    ULONGLONG t0 = GetTickCount64();
    int n = stall_sample();
    ULONGLONG took = GetTickCount64() - t0;
    Text &t = stall_text;
    t.n = 0;
    t.b[0] = 0;
    char now[32], stamp[32];
    local_stamp(now, sizeof now, false);
    local_stamp(stamp, sizeof stamp, true);
    t.put("== Load stall report %d of at most %d (%s)\n", index, max, now);
    t.put("%s\n", why);
    t.put("%s", state);
    t.put("game: F1_25.exe stamp %08x%s, %llu s since the core loaded%s%s\n", process.exe_stamp,
          process.kb_build   ? " (the known build)"
          : process.ea_build ? " (EA diagnostics on)"
                             : "",
          (unsigned long long)((GetTickCount64() - process.start_ms) / 1000), process.wine_version ? ", Wine " : "",
          process.wine_version ? process.wine_version() : "");
    if (t.n + 1 < t.cap)
        t.n += runtime::session_loader_report(t.b + t.n, t.cap - t.n, stall_frame);
    if (n < 0)
        t.put("\n== Threads\n  the thread list is not available (error %lu)\n", GetLastError());
    else {
        t.put("threads read in %llu ms (each suspended only while its context was read and its stack unwound)\n",
              (unsigned long long)took);
        stall_running_text(n);
        stall_threads_text(n);
        stall_core_text(n);
    }
    t.put("\n== Black box: last events (%llu recorded, oldest first)\n", (unsigned long long)recorder.events.total());
    ULONGLONG ms = GetTickCount64();
    if (!recorder.events.each([&](const blackbox::Copy &c) {
            t.put("  -%9.3f s [%5lu] %-9s %s\n", double(ms - c.ms) / 1000, (unsigned long)c.tid, c.kind, c.text);
        }))
        t.put("  none\n");
    t.put("\n== Loaded modules (%d)\n", modules.nmods);
    for (int i = 0; i < modules.nmods; i++)
        t.put("  %016llx %10llx  %s\n", (unsigned long long)modules.mods[i].base, (unsigned long long)modules.mods[i].size,
              modules.mods[i].path[0] ? modules.mods[i].path : modules.mods[i].name);
    wchar_t path[MAX_PATH] = {};
    HANDLE f = INVALID_HANDLE_VALUE;
    std::wstring dir = process.swofty_dir;
    if (!create_stall(dir, stamp, path, f)) {
        std::wstring other = crash_dir::fallback(data_folder());
        if (!other.empty()) {
            CreateDirectoryW(other.c_str(), nullptr);
            if (create_stall(other, stamp, path, f))
                dir = other;
        }
    }
    std::string where = "not written";
    if (f != INVALID_HANDLE_VALUE) {
        DWORD wrote = 0;
        WriteFile(f, t.b, DWORD(t.n), &wrote, nullptr);
        CloseHandle(f);
        std::string name = narrow(wcsrchr(path, L'\\') ? wcsrchr(path, L'\\') + 1 : path);
        where = crash_dir::logged(narrow(dir), narrow(process.swofty_dir), name);
        if (index == 1)
            prune(dir, L"stall-*.txt", KEEP_STALLS);
    }
    cl("[stall] load-stall report %d of at most %d: %s; %d thread(s) read in %llu ms; written to %s", index, max, why, n,
       (unsigned long long)took, where.c_str());
    report_text::each_line(t.b, [](const char *line, size_t len) { cl("[stall] %.*s", int(std::min<size_t>(len, 1000)), line); });
    cl("[stall] end of load-stall report %d", index);
    InterlockedExchange(&stall_busy, 0);
    return true;
}

}
