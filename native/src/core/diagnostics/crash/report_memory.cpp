#include "core/diagnostics/crash/report_sections.hpp"

#include <cstdio>

#include "core/career/save_heap_logic.hpp"
#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_heap.hpp"
#include "core/diagnostics/crash/fast_fail_logic.hpp"
#include "core/diagnostics/crash/fatal_paths.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_text_logic.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"

// The crash report's sections about memory: the Exception section and the faulting address, the code around rip,
// the memory at the registers and the Ne heap blocks around them.
namespace crash {

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
    case PAGE_NOACCESS:
        return "no access";
    case PAGE_READONLY:
        return "read";
    case PAGE_READWRITE:
        return "read/write";
    case PAGE_WRITECOPY:
        return "write-copy";
    case PAGE_EXECUTE:
        return "execute";
    case PAGE_EXECUTE_READ:
        return "execute/read";
    case PAGE_EXECUTE_READWRITE:
        return "execute/read/write";
    case PAGE_EXECUTE_WRITECOPY:
        return "execute/write-copy";
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
        report_writer.out.put("free (unallocated) region 0x%llx +0x%llx", (unsigned long long)base,
                              (unsigned long long)m.RegionSize);
        return;
    }
    report_writer.out.put("%s %s region 0x%llx +0x%llx (allocation 0x%llx), %s",
                          m.State == MEM_RESERVE ? "reserved" : "committed",
                          m.Type == MEM_IMAGE    ? "image"
                          : m.Type == MEM_MAPPED ? "mapped"
                                                 : "private",
                          (unsigned long long)base, (unsigned long long)m.RegionSize,
                          (unsigned long long)reinterpret_cast<uintptr_t>(m.AllocationBase),
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
    report_writer.out.put(
        "    %-9s header 0x%llx: list prev %+d next %+d (x16), block 0x%x bytes, user 0x%x bytes at 0x%llx, start "
        "guard %s, end guard %s\n",
        label, (unsigned long long)b.header, b.prev_rel, b.next_rel, b.block_size, b.user_size, (unsigned long long)b.user,
        g(b.start_guard), g(b.end_guard));
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
            report_writer.out.put(
                "    header at 0x%llx does not match a block of this heap (block size 0x%x, user size 0x%x: a block "
                "of that user size is 0x%x bytes):\n",
                (unsigned long long)header, b.block_size, b.user_size, neheap::block_size_for(b.user_size, h.guard));
            hex_dump(header - 0x20, 0x60, header);
        }
        if (!found) {
            neheap::Block p;
            if (neheap::containing(rd, h, header - 1, p, 1 << 18)) {
                block_line("previous", p);
                report_writer.out.put(
                    "    the previous block's user data ends 0x%llx bytes before this header%s\n",
                    (unsigned long long)(header - (p.user + p.user_size)),
                    p.end_guard == 0 ? "; its end guard is overwritten, so a write ran past its end towards this header" : "");
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
        report_writer.out.put(
            "    previous  no allocated block ends at this header (free space, or a corrupt header before it)\n");
    if (neheap::block_at(rd, h, b.end(), n))
        block_line("next", n);
    else if (b.end() < h.start + h.size) {
        report_writer.out.put("    next      0x%llx is not an allocated block header (free space or corrupt):\n",
                              (unsigned long long)b.end());
        hex_dump(b.end(), 0x20, b.end());
    }
}

const char *reg_names[] = {"rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
                           "r8",  "r9",  "r10", "r11", "r12", "r13", "r14", "r15"};

uint64_t reg(const CONTEXT &c, int i) { return (&c.Rax)[i]; }

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
                                  reinterpret_cast<void *>(h.heap), h.name, (unsigned long long)h.capacity,
                                  (unsigned long long)h.used, (unsigned long long)h.free_bytes, (unsigned long long)h.largest);
        if (h.pool)
            report_writer.out.put("  %s\n",
                                  career::save_heap::pool_line(h.pool_info, career::save_heap::parse_request(h.msg)).c_str() + 8);
    }
    for (int i = 0; i < sections.nheaps; i++) {
        const neheap::Heap &h = sections.heaps[i];
        report_writer.out.put(
            "  heap object 0x%llx '%s' (vtable %s+0x%llx): range 0x%llx +0x%llx, guard %llu bytes, allocated "
            "%llu bytes, allocated list head 0x%llx\n",
            (unsigned long long)h.obj, h.name, modules.at(process.exe_base) ? modules.at(process.exe_base)->name : "exe",
            (unsigned long long)(h.vtable - process.exe_base), (unsigned long long)h.start, (unsigned long long)h.size,
            (unsigned long long)h.guard, (unsigned long long)h.allocated, (unsigned long long)h.list);
    }
    uintptr_t from_msg[8];
    int nmsg = 0;
    if (game_paths.heap_error.set)
        report_text::scan_message_pointers(game_paths.heap_error.msg, from_msg, nmsg, 8);
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
        report_writer.out.put("  %s 0x%llx: ",
                              op == 0   ? "reading"
                              : op == 1 ? "writing"
                              : op == 8 ? "executing (DEP)"
                                        : "accessing",
                              (unsigned long long)e.ExceptionInformation[1]);
        region(e.ExceptionInformation[1], r);
        report_writer.out.put("\n");
    }
}

}
