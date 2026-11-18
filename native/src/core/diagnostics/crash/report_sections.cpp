#include "core/diagnostics/crash/report_sections.hpp"

#include <tlhelp32.h>

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/apply/moments.hpp"
#include "core/career/save_heap_logic.hpp"
#include "core/diagnostics/crash/blackbox.hpp"
#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_dir.hpp"
#include "core/diagnostics/crash/crash_heap.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/dred.hpp"
#include "core/diagnostics/crash/dred_logic.hpp"
#include "core/diagnostics/crash/fast_fail_logic.hpp"
#include "core/diagnostics/crash/fatal_paths.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/minidump.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/runtime/json.hpp"

namespace runtime {
void placed_record_crash_text(char *out, size_t cap);
}

namespace crash {

constinit ReportSections sections{};

void hex_dump(uintptr_t from, size_t n, uintptr_t mark)
{
    Reader rd;
    for (uintptr_t a = from & ~0xfull; a < from + n; a += 16) {
        uint8_t row[16];
        bool ok = rd.read(a, row, 16);
        report_writer.out.put("    %016llx%s", (unsigned long long)a, a <= mark && mark < a + 16 ? ">" : " ");
        for (int i = 0; i < 16; i++)
            ok ? report_writer.out.put(" %02x", row[i]) : report_writer.out.put(" ??");
        report_writer.out.put("  ");
        for (int i = 0; i < 16; i++)
            report_writer.out.put("%c", ok && row[i] >= 0x20 && row[i] < 0x7f ? row[i] : '.');
        report_writer.out.put("\n");
    }
}

const char *protect_name(DWORD p)
{
    switch (p & 0xff) {
    case PAGE_NOACCESS: return "no access";
    case PAGE_READONLY: return "read";
    case PAGE_READWRITE: return "read/write";
    case PAGE_WRITECOPY: return "write-copy";
    case PAGE_EXECUTE: return "execute";
    case PAGE_EXECUTE_READ: return "execute/read";
    case PAGE_EXECUTE_READWRITE: return "execute/read/write";
    case PAGE_EXECUTE_WRITECOPY: return "execute/write-copy";
    }
    return "?";
}


void add_heap(uintptr_t obj)
{
    for (int i = 0; i < sections.nheaps; i++)
        if (sections.heaps[i].obj == obj)
            return;
    neheap::Heap h;
    if (sections.nheaps < 8 && ne_heap(obj, h))
        sections.heaps[sections.nheaps++] = h;
}

const neheap::Heap *heap_for(uintptr_t a)
{
    for (int i = 0; i < sections.nheaps; i++)
        if (sections.heaps[i].contains(a))
            return &sections.heaps[i];
    return nullptr;
}

void region(uintptr_t a, const Request &r)
{
    MEMORY_BASIC_INFORMATION m{};
    if (!VirtualQuery(reinterpret_cast<void *>(a), &m, sizeof m)) {
        report_writer.out.put("not a valid address");
        return;
    }
    auto base = reinterpret_cast<uintptr_t>(m.BaseAddress);
    if (m.State == MEM_FREE) {
        report_writer.out.put("free (unallocated) region 0x%llx +0x%llx", (unsigned long long)base, (unsigned long long)m.RegionSize);
        return;
    }
    report_writer.out.put("%s %s region 0x%llx +0x%llx (allocation 0x%llx), %s", m.State == MEM_RESERVE ? "reserved" : "committed",
            m.Type == MEM_IMAGE ? "image" : m.Type == MEM_MAPPED ? "mapped" : "private", (unsigned long long)base,
            (unsigned long long)m.RegionSize, (unsigned long long)reinterpret_cast<uintptr_t>(m.AllocationBase),
            m.State == MEM_COMMIT ? protect_name(m.Protect) : "no access");
    if (m.Protect & PAGE_GUARD)
        report_writer.out.put(", guard page");
    if (const Mod *mod = modules.at(a)) {
        char sec[9];
        section_into(mod->base, a, sec);
        report_writer.out.put("; %s+0x%llx %s", mod->name, (unsigned long long)(a - mod->base), sec);
    }
    if (a >= r.lo && a < r.hi)
        report_writer.out.put("; stack of thread %lu (rsp%+lld)", r.tid, (long long)(a - r.ctx.Rsp));
    if (const neheap::Heap *h = heap_for(a))
        report_writer.out.put("; Ne heap '%s' +0x%llx", h->name, (unsigned long long)(a - h->start));
}

void block_line(const char *label, const neheap::Block &b)
{
    auto g = [](int s) { return s < 0 ? "n/a" : s ? "intact" : "CORRUPT"; };
    report_writer.out.put("    %-9s header 0x%llx: list prev %+d next %+d (x16), block 0x%x bytes, user 0x%x bytes at 0x%llx, start "
            "guard %s, end guard %s\n",
            label, (unsigned long long)b.header, b.prev_rel, b.next_rel, b.block_size, b.user_size,
            (unsigned long long)b.user, g(b.start_guard), g(b.end_guard));
}

void heap_block(const neheap::Heap &h, uintptr_t a, const char *why, bool as_user)
{
    Reader rd;
    neheap::Block b;
    report_writer.out.put("  %s 0x%llx in heap '%s' (+0x%llx):\n", why, (unsigned long long)a, h.name,
            (unsigned long long)(a - h.start));
    bool found = false;
    if (as_user) {
        uintptr_t header = a - h.guard - 0x10;
        if (neheap::block_at(rd, h, header, b)) {
            block_line("block", b);
            found = true;
        } else if (neheap::raw_header(rd, h, header, b)) {
            report_writer.out.put("    header at 0x%llx does not match a block of this heap (block size 0x%x, user size 0x%x: a block "
                    "of that user size is 0x%x bytes):\n",
                    (unsigned long long)header, b.block_size, b.user_size, neheap::block_size_for(b.user_size, h.guard));
            hex_dump(header - 0x20, 0x60, header);
        }
        if (!found) {
            neheap::Block p;
            if (neheap::containing(rd, h, header - 1, p, 1 << 18)) {
                block_line("previous", p);
                report_writer.out.put("    the previous block's user data ends 0x%llx bytes before this header%s\n",
                        (unsigned long long)(header - (p.user + p.user_size)),
                        p.end_guard == 0 ? "; its end guard is overwritten, so a write ran past its end towards this header"
                                         : "");
                uintptr_t end = p.user + p.user_size;
                hex_dump(end >= p.user + 0x20 ? end - 0x20 : p.user, 0x60, end);
            }
            return;
        }
    } else if (neheap::containing(rd, h, a, b, 1 << 18)) {
        block_line("block", b);
        if (a < b.header + 0x10)
            report_writer.out.put("    0x%llx is in the block header\n", (unsigned long long)a);
        else if (a < b.user)
            report_writer.out.put("    0x%llx is in the start guard\n", (unsigned long long)a);
        else if (a < b.user + b.user_size)
            report_writer.out.put("    0x%llx is user data +0x%llx\n", (unsigned long long)a, (unsigned long long)(a - b.user));
        else
            report_writer.out.put("    0x%llx is past the user data (end guard or padding)\n", (unsigned long long)a);
        found = true;
    }
    if (!found) {
        report_writer.out.put("    no allocated block header found within 256 KB before it (free space or a corrupt header)\n");
        return;
    }
    neheap::Block p, n;
    if (neheap::before(rd, h, b.header, p, 1 << 18))
        block_line("previous", p);
    else
        report_writer.out.put("    previous  no allocated block ends at this header (free space, or a corrupt header before it)\n");
    if (neheap::block_at(rd, h, b.end(), n))
        block_line("next", n);
    else if (b.end() < h.start + h.size) {
        report_writer.out.put("    next      0x%llx is not an allocated block header (free space or corrupt):\n",
                (unsigned long long)b.end());
        hex_dump(b.end(), 0x20, b.end());
    }
}

void scan_message_pointers(const char *msg, uintptr_t *found, int &n, int max)
{
    for (const char *p = msg; *p && n < max; p++) {
        const char *s = p;
        if (s[0] == '0' && (s[1] == 'x' || s[1] == 'X'))
            s += 2;
        int digits = 0;
        uint64_t v = 0;
        while (digits < 17 && isxdigit((unsigned char)s[digits])) {
            char c = s[digits];
            v = v * 16 + uint64_t(c <= '9' ? c - '0' : (c | 0x20) - 'a' + 10);
            digits++;
        }
        if (digits >= 8 && digits <= 16) {
            found[n++] = uintptr_t(v);
            p = s + digits - 1;
        }
    }
}

const char *reg_names[] = {"rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
                           "r8",  "r9",  "r10", "r11", "r12", "r13", "r14", "r15"};

uint64_t reg(const CONTEXT &c, int i)
{
    return (&c.Rax)[i];
}

void section_registers(const Request &r)
{
    const CONTEXT &c = r.ctx;
    report_writer.out.put("\n== Registers\n");
    report_writer.out.put("  rax=%016llx rbx=%016llx rcx=%016llx rdx=%016llx\n", c.Rax, c.Rbx, c.Rcx, c.Rdx);
    report_writer.out.put("  rsi=%016llx rdi=%016llx rbp=%016llx rsp=%016llx\n", c.Rsi, c.Rdi, c.Rbp, c.Rsp);
    report_writer.out.put("  r8 =%016llx r9 =%016llx r10=%016llx r11=%016llx\n", c.R8, c.R9, c.R10, c.R11);
    report_writer.out.put("  r12=%016llx r13=%016llx r14=%016llx r15=%016llx\n", c.R12, c.R13, c.R14, c.R15);
    report_writer.out.put("  rip=%016llx eflags=%08lx cs=%04x ss=%04x ds=%04x es=%04x fs=%04x gs=%04x mxcsr=%08lx\n", c.Rip, c.EFlags,
            c.SegCs, c.SegSs, c.SegDs, c.SegEs, c.SegFs, c.SegGs, c.MxCsr);
    const M128A *x = &c.Xmm0;
    for (int i = 0; i < 16; i += 2)
        report_writer.out.put("  xmm%-2d=%016llx%016llx xmm%-2d=%016llx%016llx\n", i, (unsigned long long)x[i].High,
                (unsigned long long)x[i].Low, i + 1, (unsigned long long)x[i + 1].High, (unsigned long long)x[i + 1].Low);
}

bool call_before(uintptr_t v)
{
    uint8_t b[8];
    Reader rd;
    if (!rd.read(v - 8, b, 8))
        return false;
    if (b[3] == 0xE8)
        return true;
    for (int len : {2, 3, 4, 6, 7})
        if (b[8 - len] == 0xFF && ((b[8 - len + 1] >> 3) & 7) == 2)
            return true;
    return false;
}

void section_stack(const CONTEXT &ctx, uintptr_t lo, uintptr_t hi, const char *title)
{
    report_writer.out.put("\n== %s\n", title);
    char w[512];
    int frames = walk(ctx, lo, hi, REPORT_FRAMES, [&](int i, const CONTEXT &c) {
        where_into(w, sizeof w, c.Rip, true);
        report_writer.out.put("  #%-2d %016llx rsp=%016llx  %s\n", i, (unsigned long long)c.Rip, (unsigned long long)c.Rsp, w);
        return true;
    });
    if (frames <= 1)
        report_writer.out.put("  (the unwind stopped at the first frame)\n");
    report_writer.out.put("\n== Return addresses found on the stack (rsp to +0x%zx)\n", STACK_SCAN);
    uintptr_t end = std::min<uintptr_t>(hi, ctx.Rsp + STACK_SCAN);
    int shown = 0;
    Reader rd;
    for (uintptr_t p = ctx.Rsp & ~7ull; p + 8 <= end && shown < SCAN_FRAMES; p += 8) {
        uint64_t v;
        if (!rd.read(p, &v, 8) || !code_at(v) || !call_before(v))
            continue;
        where_into(w, sizeof w, v, true);
        report_writer.out.put("  rsp+0x%-5llx %016llx  %s\n", (unsigned long long)(p - ctx.Rsp), (unsigned long long)v, w);
        shown++;
    }
    if (!shown)
        report_writer.out.put("  none\n");
}

void section_memory(const Request &r)
{
    const CONTEXT &c = r.ctx;
    report_writer.out.put("\n== Code around rip\n");
    hex_dump(c.Rip - 64, 128, c.Rip);
    report_writer.out.put("\n== Memory at the registers\n");
    uintptr_t done[20];
    int ndone = 0;
    for (int i = 0; i < 16; i++) {
        uint64_t v = reg(c, i);
        if (v < 0x10000)
            continue;
        report_writer.out.put("  %s=%016llx: ", reg_names[i], (unsigned long long)v);
        region(v, r);
        report_writer.out.put("\n");
        bool dup = false;
        for (int k = 0; k < ndone; k++)
            dup |= v - done[k] + 64 < 192;
        if (dup || !readable(v, 1))
            continue;
        done[ndone++] = v;
        uintptr_t from = readable(v - 64, 64) ? v - 64 : v;
        hex_dump(from, i == 4 ? 256 : v - from + 128, v);
    }
}

void section_heap(const Request &r)
{
    if (game_paths.heap_error.set)
        add_heap(game_paths.heap_error.heap);
    for (int i = 0; i < 16; i++)
        add_heap(reg(r.ctx, i));
    Reader rd;
    for (uintptr_t p = r.ctx.Rsp & ~7ull; p < r.ctx.Rsp + 0x800 && p + 8 <= r.hi; p += 8) {
        uint64_t v;
        if (rd.read(p, &v, 8))
            add_heap(v);
    }
    if (!game_paths.heap_error.set && !sections.nheaps)
        return;
    report_writer.out.put("\n== Ne heap\n");
    if (game_paths.heap_error.set) {
        const HeapError &h = game_paths.heap_error;
        report_writer.out.put("  last heap error: 0x%x (%s) on thread %lu, %.1f s before this report: %s\n", h.code,
                h.fatal ? "fatal" : "not fatal", h.tid, double(r.ms - h.ms) / 1000, h.msg);
        if (h.stats)
            report_writer.out.put("  heap %p '%s': capacity %llu, used %llu, free %llu, largest free %llu bytes\n",
                    reinterpret_cast<void *>(h.heap), h.name, (unsigned long long)h.capacity, (unsigned long long)h.used,
                    (unsigned long long)h.free_bytes, (unsigned long long)h.largest);
        if (h.pool)
            report_writer.out.put("  %s\n", runtime::save_heap::pool_line(h.pool_info, runtime::save_heap::parse_request(h.msg)).c_str() + 8);
    }
    for (int i = 0; i < sections.nheaps; i++) {
        const neheap::Heap &h = sections.heaps[i];
        report_writer.out.put("  heap object 0x%llx '%s' (vtable %s+0x%llx): range 0x%llx +0x%llx, guard %llu bytes, allocated "
                "%llu bytes, allocated list head 0x%llx\n",
                (unsigned long long)h.obj, h.name, modules.at(process.exe_base) ? modules.at(process.exe_base)->name : "exe",
                (unsigned long long)(h.vtable - process.exe_base), (unsigned long long)h.start, (unsigned long long)h.size,
                (unsigned long long)h.guard, (unsigned long long)h.allocated, (unsigned long long)h.list);
    }
    uintptr_t from_msg[8];
    int nmsg = 0;
    if (game_paths.heap_error.set)
        scan_message_pointers(game_paths.heap_error.msg, from_msg, nmsg, 8);
    for (int i = 0; i < nmsg; i++)
        if (const neheap::Heap *h = heap_for(from_msg[i]))
            heap_block(*h, from_msg[i], "pointer from the error message", true);
    if (r.has_exception && r.rec.NumberParameters >= 2 &&
        (r.rec.ExceptionCode == EXCEPTION_ACCESS_VIOLATION || r.rec.ExceptionCode == EXCEPTION_IN_PAGE_ERROR))
        if (const neheap::Heap *h = heap_for(r.rec.ExceptionInformation[1]))
            heap_block(*h, r.rec.ExceptionInformation[1], "faulting address", false);
    for (int i = 0; i < 16; i++) {
        uint64_t v = reg(r.ctx, i);
        if (i == 4)
            continue;
        if (const neheap::Heap *h = heap_for(v)) {
            char why[32];
            snprintf(why, sizeof why, "register %s", reg_names[i]);
            heap_block(*h, v, why, false);
        }
    }
}

void section_exception(const Request &r)
{
    if (!r.has_exception)
        return;
    const EXCEPTION_RECORD &e = r.rec;
    char w[512];
    where_into(w, sizeof w, reinterpret_cast<uintptr_t>(e.ExceptionAddress), true);
    report_writer.out.put("\n== Exception\n  %s (0x%08lx), flags 0x%lx, at %s\n", code_name(e.ExceptionCode), e.ExceptionCode,
            e.ExceptionFlags, w);
    for (DWORD i = 0; i < e.NumberParameters && i < EXCEPTION_MAXIMUM_PARAMETERS; i++)
        report_writer.out.put("  parameter %lu: 0x%llx\n", i, (unsigned long long)e.ExceptionInformation[i]);
    std::string ff = fast_fail::exception_line(e.ExceptionCode, e.NumberParameters >= 1, e.ExceptionInformation[0]);
    if (!ff.empty())
        report_writer.out.put("  %s\n", ff.c_str());
    if ((e.ExceptionCode == EXCEPTION_ACCESS_VIOLATION || e.ExceptionCode == EXCEPTION_IN_PAGE_ERROR) &&
        e.NumberParameters >= 2) {
        auto op = e.ExceptionInformation[0];
        report_writer.out.put("  %s 0x%llx: ", op == 0 ? "reading" : op == 1 ? "writing" : op == 8 ? "executing (DEP)" : "accessing",
                (unsigned long long)e.ExceptionInformation[1]);
        region(e.ExceptionInformation[1], r);
        report_writer.out.put("\n");
    }
}

void section_state(const Request &r)
{
    report_writer.out.put("\n== Game state (from the black box)\n");
    blackbox::Copy c;
    for (const char *k : {"flow", "screen", "session", "race", "career", "heap", "exception"}) {
        if (recorder.events.last(k, c))
            report_writer.out.put("  last %-9s %s (%.1f s before this report)\n", k, c.text, double(r.ms - c.ms) / 1000);
        else
            report_writer.out.put("  last %-9s none recorded\n", k);
    }
    report_writer.out.put("  track and car count: core does not read them\n");
    char placed[256];
    runtime::placed_record_crash_text(placed, sizeof placed);
    report_writer.out.put("  %s\n", placed);
}

void section_blackbox(const Request &r)
{
    report_writer.out.put("\n== Black box: last events (%llu recorded, oldest first)\n", (unsigned long long)recorder.events.total());
    if (!recorder.events.each([&](const blackbox::Copy &c) {
            report_writer.out.put("  -%9.3f s [%5lu] %-9s %s\n", double(r.ms - c.ms) / 1000, (unsigned long)c.tid, c.kind, c.text);
        }))
        report_writer.out.put("  none\n");
    report_writer.out.put("\n== Black box: last game log warnings and errors (%llu recorded, oldest first)\n",
            (unsigned long long)recorder.warnings.total());
    if (!recorder.warnings.each([&](const blackbox::Copy &c) {
            report_writer.out.put("  -%9.3f s [%5lu] %s\n", double(r.ms - c.ms) / 1000, (unsigned long)c.tid, c.text);
        }))
        report_writer.out.put("  none\n");
}

void section_probes(const Request &r)
{
    report_writer.out.put("\n== Swofty probes (hook and callback activity)\n");
    static char text[48 << 10];
    text[0] = 0;
    perf::crash_text(text, sizeof text, r.probes, r.nprobes);
    for (char *line = text; *line;) {
        char *nl = strchr(line, '\n');
        size_t len = nl ? size_t(nl - line) : strlen(line);
        report_writer.out.put("  %.*s\n", int(len), line);
        line += len + (nl ? 1 : 0);
    }
}

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
        report_writer.out.put("  thread %lu, priority %ld%s%s%s%s\n", tid, te.tpBasePri, desc[0] ? " \"" : "", desc, desc[0] ? "\"" : "",
                tid == r.tid                         ? " (the reporting thread, see the stack above)"
                : tid == GetCurrentThreadId()        ? " (the crash report writer)"
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
    tail[got] = 0;
    int lines = 0;
    char *start = tail + got;
    if (start > tail && start[-1] == '\n')
        start--;
    while (start > tail && lines < LOG_LINES)
        if (*--start == '\n')
            lines++;
    if (*start == '\n')
        start++;
    char *w = start;
    for (char *p = start; *p; p++)
        if (*p != '\r')
            *w++ = *p;
    *w = 0;
    report_writer.out.put("%s%s", start, got && tail[got - 1] != '\n' ? "\n" : "");
}

void summary_line(const Request &r)
{
    bool prior = r.has_prior && !r.has_exception;
    std::string frames;
    char w[512];
    int kept = 0;
    walk(prior ? r.prior.ctx : r.ctx, prior ? r.prior.lo : r.lo, prior ? r.prior.hi : r.hi, 24,
         [&](int, const CONTEXT &c) {
             const Mod *m = modules.at(c.Rip);
             if (m && m->base == reinterpret_cast<uintptr_t>(process.self))
                 return true;
             where_into(w, sizeof w, c.Rip, false);
             frames += (frames.empty() ? "" : " < ") + std::string(w);
             return ++kept < 4;
         });
    cl("[crash] crash report %s: %s; %sstack %s; minidump %s", report_writer.report_where, r.reason,
       prior ? "earlier exception's " : "", frames.c_str(), report_writer.dump_note);
}

void step_header(Request &r)
{
    char now[32];
    local_stamp(now, sizeof now, false);
    if (r.blame[0])
        report_writer.out.put("Swofty crash report: %s\n\n", r.blame);
    else
        report_writer.out.put("Swofty crash report\n\n");
    report_writer.out.put("written      %s local time\n", now);
    report_writer.out.put("reason       %s\n", r.reason);
    report_writer.out.put("thread       %lu%s\n", r.tid, r.kind == K_EXIT ? " (the exiting thread)" : "");
    report_writer.out.put("process      pid %lu, %.1f s after the core loaded\n", GetCurrentProcessId(),
            double(r.ms - process.start_ms) / 1000);
    wchar_t path[MAX_PATH] = L"?";
    GetModuleFileNameW(nullptr, path, MAX_PATH);
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    unsigned long long size = 0;
    if (GetFileAttributesExW(path, GetFileExInfoStandard, &fa))
        size = ((unsigned long long)fa.nFileSizeHigh << 32) | fa.nFileSizeLow;
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(
        process.exe_base + reinterpret_cast<const IMAGE_DOS_HEADER *>(process.exe_base)->e_lfanew);
    report_writer.out.put("exe          %ls, %llu bytes, PE timestamp %lu, image base 0x%llx\n", path, size,
            (unsigned long)nt->FileHeader.TimeDateStamp, (unsigned long long)process.exe_base);
    report_writer.out.put("symbols      %zu KB function name(s)%s\n", symbol_table.symbols.size(),
            symbol_table.matches ? "" : " (symbols.tsv is for another build or missing)");
    report_writer.out.put("runtime      %s\n", process.wine_version ? fmt("Wine %s", process.wine_version()).c_str() : "Windows");
    report_writer.out.put("first-chance fatal-class exceptions before this report: %ld\n", capture.fatal_count);
}

void step_prior(Request &r)
{
    if (!r.has_prior)
        return;
    const Snapshot &p = r.prior;
    char w[512];
    where_into(w, sizeof w, p.ctx.Rip, true);
    report_writer.out.put("\n== Earlier fatal-class exception: %s (0x%08lx) on thread %lu, %.1f s before this report, at %s\n",
            code_name(p.rec.ExceptionCode), p.rec.ExceptionCode, p.tid, double(r.ms - p.ms) / 1000, w);
    for (DWORD i = 0; i < p.rec.NumberParameters && i < 4; i++)
        report_writer.out.put("  parameter %lu: 0x%llx\n", i, (unsigned long long)p.rec.ExceptionInformation[i]);
    static Request pr;
    pr = r;
    pr.ctx = p.ctx;
    pr.lo = p.lo;
    pr.hi = p.hi;
    pr.tid = p.tid;
    section_registers(pr);
    section_stack(p.ctx, p.lo, p.hi, "Stack of the earlier exception");
}

void step_dred(Request &r)
{
    bool removal = false;
    auto look = [&](const CONTEXT &c) {
        const DWORD64 *v = &c.Rax;
        for (int i = 0; i < 16; i++)
            removal = removal || runtime::dred::is_removal_hresult(v[i]);
    };
    look(r.ctx);
    if (r.has_prior)
        look(r.prior.ctx);
    std::string text = runtime::dred::section(removal);
    if (text.empty())
        return;
    report_writer.out.put("%s", text.c_str());
    std::string builds = d3d12hooks::rtas_recent_text(runtime::dred::build_stops());
    for (size_t at = 0; at < builds.size();) {
        size_t end = builds.find('\n', at);
        end = end == std::string::npos ? builds.size() : end + 1;
        report_writer.out.put("%.*s", int(end - at), builds.data() + at);
        size_t from = builds.find_first_not_of(' ', at), to = builds[end - 1] == '\n' ? end - 1 : end;
        if (from < to)
            logger.api->log("[gfx] rtas recent: %.*s", int(to - from), builds.data() + from);
        at = end;
    }
}

void step_close(Request &)
{
    CloseHandle(report_writer.out.file);
    report_writer.out.file = INVALID_HANDLE_VALUE;
}

const Step STEPS[NSTEPS] = {
    {"the header", step_header},
    {"the Exception section", [](Request &r) { section_exception(r); }},
    {"the Registers section", [](Request &r) { section_registers(r); }},
    {"the Stack section",
     [](Request &r) { section_stack(r.ctx, r.lo, r.hi, "Stack (unwound with the modules' exception data)"); }},
    {"the Earlier fatal-class exception section", step_prior},
    {"the Code around rip and Memory sections", [](Request &r) { section_memory(r); }},
    {"the Ne heap section", [](Request &r) { section_heap(r); }},
    {"the Game state section", [](Request &r) { section_state(r); }},
    {"the GPU device removal section", step_dred},
    {"the Black box sections", [](Request &r) { section_blackbox(r); }},
    {"the Swofty probes section", [](Request &r) { section_probes(r); }},
    {"the registry reports", [](Request &) { swofty_core::apply::moments::crash_reports(); }},
    {"the Addons and Loaded modules sections", [](Request &) { section_modules(); }},
    {"the Threads section",
     [](Request &r) {
         if (r.loader_lock)
             report_writer.out.put("\n== Threads\n  not listed: the reporting thread holds the loader lock\n");
         else
             section_threads(r);
     }},
    {"the loader.log section", [](Request &) { section_log(); }},
    {"the Minidump section",
     [](Request &r) {
         SetEvent(report_writer.text_event);
         minidump(r);
         report_writer.out.put("\n== Minidump\n  %s\n", report_writer.dump_note);
     }},
    {"closing the report", step_close},
    {"the summary line", [](Request &r) { summary_line(r); }},
};

void read_addons(const std::wstring &path)
{
    std::string text;
    if (FILE *f = _wfopen(path.c_str(), L"rb")) {
        char b[65536];
        for (size_t n; (n = fread(b, 1, sizeof b, f));)
            text.append(b, n);
        fclose(f);
    }
    json::Value doc;
    if (text.empty() || !json::parse(text, doc)) {
        sections.addon_text = text.empty() ? "  no settings.json\n" : "  settings.json is not valid JSON\n";
        return;
    }
    const json::Value &list = doc.field("addons", "mods");
    sections.addon_text = fmt("  profile '%s', %zu addon(s)\n", doc["profile"].str("?").c_str(), list.a.size());
    for (auto &a : list.a) {
        std::string dlls;
        for (auto &d : a["dlls"].a)
            dlls += (dlls.empty() ? "" : ", ") + d.str();
        sections.addon_text += fmt("  %-36s %-10s %-3s %s\n", a["id"].str("?").c_str(), a["version"].str("?").c_str(),
                          a["enabled"].type == json::Value::Bool && !a["enabled"].b ? "off" : "on",
                          dlls.empty() ? "(no dll)" : dlls.c_str());
    }
}

static_assert(sizeof STEPS / sizeof *STEPS == NSTEPS);

}
