#include "core/diagnostics/crash/minidump.hpp"

#include <cstdio>

#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/fatal_paths.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/report_writer.hpp"

namespace crash {

struct DumpBudget {
    DWORD tid;
    uint64_t stacks, cap;
    int dropped;
};

BOOL CALLBACK dump_callback(PVOID param, PMINIDUMP_CALLBACK_INPUT in, PMINIDUMP_CALLBACK_OUTPUT o)
{
    auto b = static_cast<DumpBudget *>(param);
    if (!in || !o)
        return TRUE;
    if (in->CallbackType == ThreadCallback && b->cap) {
        uint64_t size = in->Thread.StackEnd > in->Thread.StackBase ? in->Thread.StackEnd - in->Thread.StackBase : 0;
        if (in->Thread.ThreadId != b->tid && b->stacks + size > b->cap) {
            o->ThreadWriteFlags &= ~ULONG(ThreadWriteStack);
            b->dropped++;
        } else {
            b->stacks += size;
        }
    }
    return TRUE;
}

void minidump(Request &r)
{
    if (report_writer.dump_mode == DUMP_OFF) {
        snprintf(report_writer.dump_note, sizeof report_writer.dump_note, "not written (SWOFTY_CRASH_DUMP=off)");
        return;
    }
    if (!report_writer.write_dump) {
        snprintf(report_writer.dump_note, sizeof report_writer.dump_note, "not written: dbghelp.dll has no MiniDumpWriteDump");
        return;
    }
    if (r.loader_lock) {
        snprintf(report_writer.dump_note, sizeof report_writer.dump_note,
                 "not written: the reporting thread holds the loader lock, which the dump writer's thread needs");
        return;
    }
    HANDLE f = CreateFileW(report_writer.dump_path, GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_ALWAYS,
                           FILE_ATTRIBUTE_NORMAL, nullptr);
    report_writer.dump_file = f;
    if (f == INVALID_HANDLE_VALUE) {
        snprintf(report_writer.dump_note, sizeof report_writer.dump_note, "not written: cannot create the file (error %lu)",
                 GetLastError());
        return;
    }
    EXCEPTION_RECORD rec{};
    alignas(16) CONTEXT ctx = r.ctx;
    DWORD tid = r.tid;
    if (r.has_exception) {
        rec = r.rec;
    } else if (r.has_prior) {
        rec = r.prior.rec;
        ctx = r.prior.ctx;
        tid = r.prior.tid;
    } else {
        rec.ExceptionCode = r.kind == K_EXIT ? CODE_EXIT : CODE_GAME_FATAL;
        rec.ExceptionAddress = reinterpret_cast<void *>(ctx.Rip);
        rec.NumberParameters = 1;
        rec.ExceptionInformation[0] = r.kind == K_FATAL && game_paths.heap_error.set ? ULONG_PTR(game_paths.heap_error.code) : 0;
    }
    EXCEPTION_POINTERS ptrs{&rec, &ctx};
    MINIDUMP_EXCEPTION_INFORMATION ex{tid, &ptrs, FALSE};
    MINIDUMP_USER_STREAM comment{CommentStreamA, ULONG(report_writer.out.kept ? report_writer.out.n : 0), report_writer.out.b};
    MINIDUMP_USER_STREAM_INFORMATION streams{comment.BufferSize ? 1ul : 0ul, &comment};
    DumpBudget budget{tid, 0, report_writer.dump_mode == DUMP_FULL ? 0 : DUMP_STACKS, 0};
    MINIDUMP_CALLBACK_INFORMATION cb{dump_callback, &budget};
    auto type = MINIDUMP_TYPE(MiniDumpWithThreadInfo | MiniDumpWithProcessThreadData | MiniDumpWithUnloadedModules |
                              MiniDumpWithFullMemoryInfo |
                              (report_writer.dump_mode == DUMP_FULL ? MiniDumpWithFullMemory | MiniDumpWithHandleData
                                                                    : MiniDumpWithIndirectlyReferencedMemory));
    DWORD t0 = GetTickCount();
    BOOL ok = report_writer.write_dump(GetCurrentProcess(), GetCurrentProcessId(), f, type, &ex, &streams, &cb);
    DWORD err = ok ? 0 : GetLastError();
    LARGE_INTEGER size{};
    GetFileSizeEx(f, &size);
    CloseHandle(f);
    report_writer.dump_file = INVALID_HANDLE_VALUE;
    if (!ok) {
        DeleteFileW(report_writer.dump_path);
        snprintf(report_writer.dump_note, sizeof report_writer.dump_note, "MiniDumpWriteDump failed (error %lu)", err);
        return;
    }
    char name[64];
    WideCharToMultiByte(CP_UTF8, 0, wcsrchr(report_writer.dump_path, L'\\') + 1, -1, name, sizeof name, nullptr, nullptr);
    snprintf(report_writer.dump_note, sizeof report_writer.dump_note, "%s, %.1f MB, %s, written in %.1f s%s", name,
             double(size.QuadPart) / (1 << 20),
             report_writer.dump_mode == DUMP_FULL ? "all memory" : "thread stacks and registers",
             double(GetTickCount() - t0) / 1000,
             budget.dropped ? fmt("; %d thread stack(s) left out past the %llu MB budget", budget.dropped,
                                  (unsigned long long)(DUMP_STACKS >> 20))
                                  .c_str()
                            : "");
}

}
