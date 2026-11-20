#include "core/diagnostics/crash/fatal_paths.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/diagnostics/crash/blackbox.hpp"
#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/fast_fail_logic.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/prologue_cache_logic.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/runtime/loader/scanner.hpp"
#include "core/runtime/pages.hpp"

namespace runtime {
void hud_fatal();
}

namespace crash {

GameFatalPaths game_paths;

bool heap_vtable(uintptr_t vt)
{
    if (vt - process.exe_base >= process.exe_size || !readable(vt + 0x118, 8))
        return false;
    uintptr_t fn = *reinterpret_cast<uintptr_t *>(vt + 0x118);
    if (game_paths.heap_error_fn)
        return fn == game_paths.heap_error_fn;
    return fn - process.exe_base < process.exe_size && readable(fn, sizeof HEAP_ERROR_PREFIX) &&
           !memcmp(reinterpret_cast<const void *>(fn), HEAP_ERROR_PREFIX, sizeof HEAP_ERROR_PREFIX);
}

bool ne_heap(uintptr_t obj, neheap::Heap &h)
{
    if (obj < 0x10000 || (obj & 7) || !readable(obj, 0x118))
        return false;
    Reader rd;
    return heap_vtable(*reinterpret_cast<uintptr_t *>(obj)) && neheap::decode(rd, obj, h);
}

void clean(char *s)
{
    for (; *s; s++)
        if (*s == '\r' || *s == '\n' || *s == '\t')
            *s = ' ';
}

void on_heap_error(int code, const char *msg, bool is_fatal, void *heap)
{
    HeapError &h = game_paths.heap_error;
    h = {};
    h.code = code;
    h.fatal = is_fatal;
    h.tid = GetCurrentThreadId();
    h.ms = GetTickCount64();
    h.heap = reinterpret_cast<uintptr_t>(heap);
    if (msg && readable(reinterpret_cast<uintptr_t>(msg), 1)) {
        size_t n = strnlen(msg, sizeof h.msg - 1);
        memcpy(h.msg, msg, n);
        h.msg[n] = 0;
        clean(h.msg);
    }
    neheap::Heap info;
    if (ne_heap(h.heap, info)) {
        snprintf(h.name, sizeof h.name, "%s", info.name);
        auto vt = *reinterpret_cast<void ***>(heap);
        using Stat = uint64_t (*)(void *);
        using Largest = uint64_t (*)(void *, int);
        h.capacity = reinterpret_cast<Stat>(vt[0x60 / 8])(heap);
        h.free_bytes = reinterpret_cast<Stat>(vt[0x68 / 8])(heap);
        h.used = reinterpret_cast<Stat>(vt[0x70 / 8])(heap);
        h.largest = reinterpret_cast<Largest>(vt[0x80 / 8])(heap, 1);
        h.stats = true;
    } else if (heap && readable(h.heap, career::save_heap::kPoolFlags + 8) && !(h.heap & 7) &&
               *reinterpret_cast<uintptr_t *>(h.heap) ==
                   process.exe_base + (career::save_heap::kPoolVtable - career::save_heap::kImageBase)) {
        const uint8_t *p = reinterpret_cast<const uint8_t *>(h.heap);
        auto &pi = h.pool_info;
        memcpy(&pi.start, p + career::save_heap::kPoolStart, 8);
        memcpy(&pi.size, p + career::save_heap::kPoolSize, 8);
        memcpy(&pi.cursor, p + career::save_heap::kPoolCursor, 8);
        memcpy(&pi.flags, p + career::save_heap::kPoolFlags, 4);
        h.pool = true;
        snprintf(h.name, sizeof h.name, "SAVE_PROPERTY_BAG_TEMP");
        cl("%s", career::save_heap::pool_line(pi, career::save_heap::parse_request(h.msg)).c_str());
    }
    h.set = true;
    char line[blackbox::TEXT];
    snprintf(line, sizeof line, "0x%x%s in '%s': %s", code, is_fatal ? " (fatal)" : "", h.name, h.msg);
    blackbox::note("heap", line);
    cl("[crash] Ne heap error 0x%x (%s) on thread %lu: %s; heap %p '%s'%s", code, is_fatal ? "fatal" : "not fatal", h.tid, h.msg,
       heap, h.name,
       h.stats ? fmt(", capacity %llu, used %llu, free %llu, largest free %llu", (unsigned long long)h.capacity,
                     (unsigned long long)h.used, (unsigned long long)h.free_bytes, (unsigned long long)h.largest)
                     .c_str()
               : "");
    if (code == 0x58) {
        DWORD last = GetLastError();
        MEMORYSTATUSEX ms = {};
        ms.dwLength = sizeof ms;
        if (GlobalMemoryStatusEx(&ms))
            cl("[crash] Ne heap error 0x58 (useAddressSpace: the commit of a heap block failed, error 0x58 is a Win32 error the "
               "game does not map, e.g. 1455 commit limit) in '%s': last Win32 error %lu, system commit limit %llu MiB, commit "
               "left %llu MiB, physical %llu of %llu MiB free, process address space left %llu MiB; heap %s",
               h.name, last, (unsigned long long)(ms.ullTotalPageFile >> 20), (unsigned long long)(ms.ullAvailPageFile >> 20),
               (unsigned long long)(ms.ullAvailPhys >> 20), (unsigned long long)(ms.ullTotalPhys >> 20),
               (unsigned long long)(ms.ullAvailVirtual >> 20),
               h.stats ? fmt("capacity %llu, used %llu, free %llu, largest free %llu", (unsigned long long)h.capacity,
                             (unsigned long long)h.used, (unsigned long long)h.free_bytes, (unsigned long long)h.largest)
                             .c_str()
                       : "stats unavailable");
    }
    if (!is_fatal)
        return;
    alignas(16) CONTEXT ctx;
    RtlCaptureContext(&ctx);
    std::string why = fmt("fatal Ne heap error 0x%x in heap '%s': %s", code, h.name, h.msg);
    if (!request_report(K_FATAL, why.c_str(), nullptr, &ctx))
        already("fatal Ne heap error");
}

int hooked_report_memory_error(int code, const char *msg, char is_fatal, char brk, uint64_t a5, void *heap)
{
    on_heap_error(code, msg, is_fatal != 0, heap);
    return game_paths.heap_error_hook.unsafe_call<int>(code, msg, is_fatal, brk, a5, heap);
}

bool fast_fail_available() { return IsProcessorFeaturePresent(PF_FASTFAIL_AVAILABLE) != 0; }

void report_fast_fail(fast_fail::Source src, const fast_fail::Outcome &o, const CONTEXT &ctx, uintptr_t at_addr)
{
    perf::flush("fast-fail");
    if (!enter())
        return;
    std::string why = fast_fail::reason(src, o, where(at_addr));
    cl("[crash] %s", why.c_str());
    blackbox::note("fast-fail", why.c_str());
    cl("[crash]   rax=%016llx rbx=%016llx rcx=%016llx rdx=%016llx", ctx.Rax, ctx.Rbx, ctx.Rcx, ctx.Rdx);
    cl("[crash]   stack:");
    backtrace(ctx, true);
    leave();
    EXCEPTION_RECORD rec{};
    rec.ExceptionCode = o.exit_status;
    rec.ExceptionFlags = EXCEPTION_NONCONTINUABLE;
    rec.ExceptionAddress = reinterpret_cast<PVOID>(ctx.Rip);
    if (o.has_code) {
        rec.NumberParameters = 1;
        rec.ExceptionInformation[0] = o.code;
    }
    runtime::hud_fatal();
    CONTEXT copy = ctx;
    if (!request_report(K_EXCEPTION, why.c_str(), &rec, &copy))
        already("fast-fail");
}

void report_from_hook(fast_fail::Source src, safetyhook::Context &c, uintptr_t entry)
{
    alignas(16) CONTEXT ctx;
    memset(&ctx, 0, sizeof ctx);
    ctx.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER;
    ctx.Rax = c.rax, ctx.Rbx = c.rbx, ctx.Rcx = c.rcx, ctx.Rdx = c.rdx, ctx.Rsi = c.rsi, ctx.Rdi = c.rdi;
    ctx.Rbp = c.rbp, ctx.Rsp = c.rsp, ctx.R8 = c.r8, ctx.R9 = c.r9, ctx.R10 = c.r10, ctx.R11 = c.r11;
    ctx.R12 = c.r12, ctx.R13 = c.r13, ctx.R14 = c.r14, ctx.R15 = c.r15;
    ctx.Rip = entry;
    ctx.EFlags = DWORD(c.rflags);
    uintptr_t caller = readable(ctx.Rsp, 8) ? *reinterpret_cast<uintptr_t *>(ctx.Rsp) : entry;
    report_fast_fail(src, fast_fail::outcome(src, fast_fail_available(), 0), ctx, caller);
}

void on_gs_failure(safetyhook::Context &c) { report_from_hook(fast_fail::Source::GsFailure, c, game_paths.gs_failure_fn); }

void on_invalid_parameter(safetyhook::Context &c)
{
    report_from_hook(fast_fail::Source::InvalidParameter, c, game_paths.fast_fail_terminate_fn);
}

void on_abort(safetyhook::Context &c) { report_from_hook(fast_fail::Source::Abort, c, game_paths.abort_fn); }

void WINAPI hooked_raise_fail_fast(PEXCEPTION_RECORD rec, PCONTEXT given, DWORD flags)
{
    alignas(16) CONTEXT ctx;
    RtlCaptureContext(&ctx);
    if (given && readable(reinterpret_cast<uintptr_t>(given), sizeof(CONTEXT)) && given->Rip)
        memcpy(&ctx, given, sizeof ctx);
    uint32_t status = rec && readable(reinterpret_cast<uintptr_t>(rec), sizeof(DWORD)) ? rec->ExceptionCode : 0;
    report_fast_fail(fast_fail::Source::RaiseFailFast, fast_fail::outcome(fast_fail::Source::RaiseFailFast, true, status), ctx,
                     ctx.Rip);
    game_paths.fail_fast_hook.unsafe_call<void>(rec, given, flags);
}

uintptr_t function_at(uint32_t rva, const uint8_t *expect, size_t n)
{
    uintptr_t a = process.exe_base + rva;
    return rva < process.exe_size && readable(a, n) && !memcmp(reinterpret_cast<const void *>(a), expect, n) ? a : 0;
}

uintptr_t kb_function(uint32_t rva, const uint8_t *expect, size_t n)
{
    return process.kb_build ? function_at(rva, expect, n) : 0;
}

// A byte scan of the exe's executable sections, used only to find Ne_ReportMemoryError and the CRT's
// fast-fail paths on a build with no known address for them. It stays in the crash code and does not go through the
// loader's scan (whose log lines are the binding resolver's); find_prologue keeps its answers in the scan cache. It asks
// Windows about each memory region once rather than about every page.
uintptr_t scan_exe(const uint8_t *pat, size_t n, const fast_fail::Pattern *masked = nullptr)
{
    auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(process.exe_base);
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(process.exe_base + dos->e_lfanew);
    auto s = IMAGE_FIRST_SECTION(nt);
    uintptr_t found = 0;
    size_t hits = 0;
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++) {
        if (!(s->Characteristics & IMAGE_SCN_MEM_EXECUTE))
            continue;
        uintptr_t start = process.exe_base + s->VirtualAddress,
                  end = start + (s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData);
        for (uintptr_t p = start; p < end;) {
            MEMORY_BASIC_INFORMATION m;
            if (!VirtualQuery(reinterpret_cast<void *>(p), &m, sizeof m))
                break;
            uintptr_t next = std::min(reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize, end);
            if (next <= p)
                break;
            bool ok = runtime::pages::readable_region(m);
            auto *a = reinterpret_cast<const uint8_t *>(p), *hi = reinterpret_cast<const uint8_t *>(next);
            for (; ok && size_t(hi - a) >= n; a++) {
                a = static_cast<const uint8_t *>(memchr(a, pat[0], size_t(hi - a) - n + 1));
                if (!a)
                    break;
                if (masked ? fast_fail::matches(a, *masked) : !memcmp(a, pat, n)) {
                    found = reinterpret_cast<uintptr_t>(a);
                    if (++hits >= 2)
                        return 0;
                }
            }
            p = next;
        }
    }
    return hits == 1 ? found : 0;
}

bool in_exe_code(uintptr_t a, size_t n)
{
    auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(process.exe_base);
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(process.exe_base + dos->e_lfanew);
    auto s = IMAGE_FIRST_SECTION(nt);
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++) {
        uintptr_t start = process.exe_base + s->VirtualAddress,
                  end = start + (s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData);
        if ((s->Characteristics & IMAGE_SCN_MEM_EXECUTE) && a >= start && a < end && n <= end - a)
            return true;
    }
    return false;
}

// scan_exe's answer for a prologue, taken from the scan cache when the last boot of this exe found it and its bytes still
// match there; a scan's answer replaces the cached one. The scans cost about 0.35 s each on Windows.
uintptr_t find_prologue(const char *pattern, const uint8_t *pat, size_t n, const fast_fail::Pattern *masked, bool &cached)
{
    auto &cache = runtime::loader::scanner();
    prologue::Found f = prologue::find(
        process.exe_base, [&](uint64_t &rva) { return cache.recall(prologue::CACHE_SECTION, pattern, rva); },
        [&](uintptr_t a) {
            return in_exe_code(a, n) && readable(a, n) &&
                   (masked ? fast_fail::matches(reinterpret_cast<const uint8_t *>(a), *masked)
                           : !memcmp(reinterpret_cast<const void *>(a), pat, n));
        },
        [&] { return scan_exe(pat, n, masked); },
        [&](int hits, uint64_t rva) { cache.remember(prologue::CACHE_SECTION, pattern, hits, rva); });
    cached = f.cached;
    return f.at;
}

constexpr const char *FOUND_BY_PROLOGUE = "found by its prologue";
constexpr const char *CACHED_PROLOGUE = "found by its prologue on an earlier boot of this exe";

void hook_heap_errors()
{
    game_paths.heap_error_fn = kb_function(RVA_HEAP_ERROR, HEAP_ERROR_PREFIX, sizeof HEAP_ERROR_PREFIX);
    uint8_t expect[64];
    size_t n = 0;
    for (const char *p = PATTERN_REPORT_MEMORY_ERROR; *p && n < sizeof expect;) {
        char *end;
        expect[n++] = uint8_t(strtoul(p, &end, 16));
        p = *end ? end + 1 : end;
    }
    game_paths.report_memory_error = kb_function(RVA_REPORT_MEMORY_ERROR, expect, n);
    const char *how = "KB address";
    if (!game_paths.report_memory_error && process.exe_stamp == EA_STAMP) {
        game_paths.report_memory_error = function_at(EA_RVA_REPORT_MEMORY_ERROR, expect, n);
        how = "EA app build address";
    }
    if (!game_paths.report_memory_error) {
        bool cached = false;
        game_paths.report_memory_error = find_prologue(PATTERN_REPORT_MEMORY_ERROR, expect, n, nullptr, cached);
        how = cached ? CACHED_PROLOGUE : FOUND_BY_PROLOGUE;
    }
    if (!game_paths.report_memory_error) {
        cl("[crash] Ne_ReportMemoryError: its prologue matched 0 or several times; a fatal heap error is reported at "
           "the abort only");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(game_paths.report_memory_error),
                                            reinterpret_cast<void *>(&hooked_report_memory_error));
    if (h)
        game_paths.heap_error_hook = std::move(*h);
    cl("[crash] Ne_ReportMemoryError at base+0x%llx %s (%s); heap objects are recognised by %s",
       (unsigned long long)(game_paths.report_memory_error - process.exe_base), h ? "hooked" : "NOT hooked", how,
       game_paths.heap_error_fn ? "their error method at the KB address" : "the error method's prologue");
}

uintptr_t find_fast_fail_path(const char *name, uint32_t rva, const char *pattern, const char *&how)
{
    fast_fail::Pattern p = fast_fail::parse_pattern(pattern);
    uintptr_t a = process.exe_base + rva;
    how = "KB address";
    if (process.kb_build && rva < process.exe_size && readable(a, p.bytes.size()) &&
        fast_fail::matches(reinterpret_cast<const uint8_t *>(a), p))
        return a;
    bool cached = false;
    a = find_prologue(pattern, p.bytes.data(), p.bytes.size(), &p, cached);
    how = cached ? CACHED_PROLOGUE : FOUND_BY_PROLOGUE;
    if (!a)
        cl("[crash] %s: its prologue matched 0 or several times; a fast-fail from it gets no report", name);
    return a;
}

void hook_fast_fail_path(const char *name, uint32_t rva, const char *pattern, uintptr_t &fn, safetyhook::MidHook &hook,
                         void (*callback)(safetyhook::Context &))
{
    const char *how = "";
    fn = find_fast_fail_path(name, rva, pattern, how);
    if (!fn)
        return;
    auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(fn), callback);
    if (h)
        hook = std::move(*h);
    cl("[crash] %s at base+0x%llx %s (%s)", name, (unsigned long long)(fn - process.exe_base), h ? "hooked" : "NOT hooked", how);
}

void hook_fast_fails()
{
    hook_fast_fail_path("__report_gsfailure", RVA_REPORT_GS_FAILURE, PATTERN_REPORT_GS_FAILURE, game_paths.gs_failure_fn,
                        game_paths.gs_hook, on_gs_failure);
    hook_fast_fail_path("the CRT invalid-parameter terminate path", RVA_FAST_FAIL_TERMINATE, PATTERN_FAST_FAIL_TERMINATE,
                        game_paths.fast_fail_terminate_fn, game_paths.invalid_parameter_hook, on_invalid_parameter);
    hook_fast_fail_path("the CRT abort path", RVA_INVALID_PARAMETER_ABORT, PATTERN_INVALID_PARAMETER_ABORT, game_paths.abort_fn,
                        game_paths.abort_hook, on_abort);
}

}
