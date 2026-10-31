// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the game heap link guard (its own DLL).
//
// A game heap block carries a 0x10-byte header: +0x00 the link to the next older allocated block, +0x04 the link to
// the next newer one (16-byte units from the header), +0x08 the block size, +0x0c the requested size; heap+0x100 is
// the newest block. Ne_Heap_DeallocateNonSmall unlinks a freed block through those links without checking them. At
// 22:13 (session 20261030-111101) a write past the end of a 0x460-byte block overwrote the older link of the block
// after it; at 02:01 (session 20261030-145707) both links of an allocated 0x5b0-byte block were overwritten while its
// neighbours' links to it stayed intact. The wrap checks both links of every block before the game frees it. When
// the older link is broken and the block just before it in memory is allocated and links forward to it, the older
// link is set back to that block. When a neighbour's link back to the block is broken and that neighbour's own other
// link does not hold either, the neighbour is linked back to the block (the game's unlink rewrites that field next),
// and the allocated list is walked from the newest block: each older block whose newer link is broken (it does not
// point at a block that links back) is linked to the block the walk came from, so a later free of a damaged block
// stays inside the list. Every broken link is logged with the block, the bytes before its header, the first bytes of
// the block before it in memory, the same for the neighbour whose header is damaged (the block before that header is
// the one that ran over), the walk's result and the call stack, so the writer can be named. Option heap_check off
// passes every free through unchecked.
//
// Option heap_watch (default off) names the writer directly: it arms hardware write breakpoints (DR0-DR3, 8 bytes)
// on four heap addresses in every thread of the process (the first damaged header of sessions 20261030-145707 and
// 20261031-044641 and the 0x48 bytes in front of it; F1_1990_HEAP_WATCH overrides them with up to four hex
// addresses). Threads that exist at load are armed once (suspended one by one); every thread created later arms itself from DLL_THREAD_ATTACH on its own context, with no polling. Hits that leave the watched values unchanged, and hits before the block at the first address has been allocated (its size and requested fields read 0x5c0/0x5b0), are ignored. A vectored exception handler copies each hit (thread, rip, registers, 64 stack words, the code bytes
// before rip, the watched values) into a lock-free ring and a worker thread logs it;
// writes by the heap's own code are logged in one line, any other writer with its stack. Wine does not fill DR6, so
// the slot that fired is the one whose value changed. No hit, no effect; past 50000 hits the breakpoints are cleared.

#include <windows.h>

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstdlib>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "heap_check.hpp"
#include "heap_watch_win.hpp"

SWOFTY_SETTING(bool, heap_check, true);
SWOFTY_SETTING(bool, heap_watch, false);

namespace {

namespace hc = f1990::heap;

constexpr size_t kStart = 0x98, kSize = 0xa0, kNewest = 0x100, kGuard = 0x108;
constexpr size_t kScan = 0x10000;
constexpr size_t kWalkBlocks = size_t(1) << 22;
constexpr int kWalkFixes = 16;
constexpr int kLogEvents = 8;

std::atomic<int> g_events{0};
std::atomic<uint64_t> g_repaired{0}, g_broken{0};

bool readable(const uint8_t *p, size_t n)
{
    return swofty::mem::readable(p, n);
}

uint64_t u64(const void *p, size_t off)
{
    uint64_t v;
    std::memcpy(&v, static_cast<const uint8_t *>(p) + off, sizeof v);
    return v;
}

void hex(const uint8_t *p, size_t n, char *out, size_t cap)
{
    size_t w = 0;
    out[0] = 0;
    if (!p || !swofty::mem::readable(p, n)) {
        std::snprintf(out, cap, "(unreadable)");
        return;
    }
    for (size_t i = 0; i < n && w + 3 < cap; i++)
        w += size_t(std::snprintf(out + w, cap - w, "%02x%s", p[i], (i % 8 == 7 && i + 1 < n) ? " " : ""));
}

void frames(char *out, size_t cap)
{
    void *f[16] = {};
    USHORT n = RtlCaptureStackBackTrace(1, 16, f, nullptr);
    auto img = swofty::mem::detail::image();
    size_t w = 0;
    out[0] = 0;
    for (USHORT i = 0; i < n && w + 32 < cap; i++) {
        auto a = reinterpret_cast<uintptr_t>(f[i]);
        if (a >= img.lo && a < img.hi)
            w += size_t(std::snprintf(out + w, cap - w, "%sexe+0x%llx", w ? " " : "",
                                      static_cast<unsigned long long>(a - img.lo)));
        else
            w += size_t(std::snprintf(out + w, cap - w, "%s%p", w ? " " : "", f[i]));
    }
}

void exe_pointers(const uint8_t *p, size_t n, char *out, size_t cap)
{
    auto img = swofty::mem::detail::image();
    size_t w = 0;
    out[0] = 0;
    if (!p || !swofty::mem::readable(p, n))
        return;
    for (size_t off = 0; off + 8 <= n && w + 40 < cap; off += 8) {
        uint64_t v = u64(p, off);
        if (v >= img.lo && v < img.hi)
            w += size_t(std::snprintf(out + w, cap - w, " +0x%zx=exe+0x%llx", off,
                                      static_cast<unsigned long long>(v - img.lo)));
    }
}

void describe(const char *what, uint8_t *h, const uint8_t *header, const hc::Range &r, uint64_t guard)
{
    char before[3 * 0x40 + 16], head[3 * 0x10 + 8], first[3 * 0x40 + 16], ptrs[8 * 40];
    hex(h - 0x40, 0x40, before, sizeof before);
    hex(header, 0x10, head, sizeof head);
    swofty::log("[f1-1990] heap check:   %s %p (requested 0x%x, size 0x%x) header %s; the 0x40 bytes before it %s", what,
                static_cast<void *>(h + hc::kHeader + guard), hc::requested(h), hc::block_size(h), head, before);
    uint8_t *pred = hc::predecessor(h, r, guard, kScan, readable);
    if (!pred) {
        swofty::log("[f1-1990] heap check:   no allocated block ends at its header");
        return;
    }
    const uint8_t *user = pred + hc::kHeader + guard;
    hex(user, 0x40, first, sizeof first);
    exe_pointers(user, 0x40, ptrs, sizeof ptrs);
    swofty::log("[f1-1990] heap check:   block before it %p (requested 0x%x, size 0x%x, links %d %d), first bytes %s; "
                "exe pointers:%s",
                static_cast<const void *>(user), hc::requested(pred), hc::block_size(pred), hc::link(pred, 0),
                hc::link(pred, 1), first, ptrs[0] ? ptrs : " none");
}

void report(void *heap, uint8_t *ptr, uint8_t *h, const uint8_t *h0, const hc::Check &c, uint8_t *damaged,
            const uint8_t *d0, bool repaired, const hc::Neighbours &nb, const hc::Walk &walk, const hc::Range &r,
            uint64_t guard)
{
    g_broken.fetch_add(1);
    if (repaired || nb.older || nb.newer || walk.fixed)
        g_repaired.fetch_add(1);
    if (g_events.fetch_add(1) >= kLogEvents)
        return;
    char stack[16 * 24];
    frames(stack, sizeof stack);
    swofty::log("[f1-1990] heap check: heap %p block %p (requested 0x%x, size 0x%x) links older %d (%s) newer %d (%s); "
                "%s%s%s",
                heap, static_cast<void *>(ptr), hc::requested(h), hc::block_size(h), hc::link(h0, 0),
                c.older_ok ? "ok" : "broken", hc::link(h0, 1), c.newer_ok ? "ok" : "broken",
                repaired ? "older link set back to the block before it" : "own links left as they are",
                nb.older ? "; older neighbour linked back to it" : "", nb.newer ? "; newer neighbour linked back to it" : "");
    describe("block", h, h0, r, guard);
    if (damaged != h)
        describe("damaged neighbour", damaged, d0, r, guard);
    char fixed[64] = "", end[64] = "reached the oldest block";
    if (walk.first_fixed)
        std::snprintf(fixed, sizeof fixed, " (the first at header %p)", static_cast<void *>(walk.first_fixed));
    if (!walk.complete)
        std::snprintf(end, sizeof end, "stopped at header %p", static_cast<void *>(walk.stopped_at));
    swofty::log("[f1-1990] heap check:   list walk from the newest block: %zu block(s), %d newer link(s) set back%s; %s",
                walk.blocks, walk.fixed, fixed, end);
    swofty::log("[f1-1990] heap check:   stack %s", stack);
}

void inspect(void *heap, uint8_t *ptr)
{
    uint64_t lo = u64(heap, kStart), size = u64(heap, kSize), guard = u64(heap, kGuard);
    if (guard > 0x1000 || !lo || size > (uint64_t(1) << 40))
        return;
    hc::Range r{uintptr_t(lo), uintptr_t(lo + size)};
    uint8_t *h = ptr - guard - hc::kHeader;
    if (!r.holds(h, hc::kHeader) || !hc::allocated(h, guard))
        return;
    hc::Check c = hc::check(h, r);
    if (c.ok())
        return;
    uint8_t *damaged = h;
    if (!c.newer_ok && hc::sane(c.newer, r, guard, readable) && !hc::link_holds(c.newer, 0, r, guard, readable))
        damaged = c.newer;
    else if (!c.older_ok && hc::sane(c.older, r, guard, readable) && !hc::link_holds(c.older, 1, r, guard, readable))
        damaged = c.older;
    uint8_t h0[hc::kHeader], d0[hc::kHeader];
    std::memcpy(h0, h, sizeof h0);
    std::memcpy(d0, damaged, sizeof d0);
    uint8_t *pred = hc::predecessor(h, r, guard, kScan, readable);
    bool repaired = false;
    if (!c.older_ok && damaged == h && hc::repair_older(h, pred))
        repaired = hc::check(h, r).older_ok;
    hc::Neighbours nb = hc::repair_neighbours(h, c, r, guard, readable);
    auto *newest = reinterpret_cast<uint8_t *>(u64(heap, kNewest));
    hc::Walk walk = hc::repair_list(newest, r, guard, kWalkBlocks, kWalkFixes, readable);
    report(heap, ptr, h, h0, c, damaged, d0, repaired, nb, walk, r, guard);
}

namespace hw = f1990::watch;

}

SWOFTY_WRAP(Ne_Heap_DeallocateNonSmall, uint64_t, void *heap, uint8_t *ptr)
{
    if (heap_check && heap && ptr)
        inspect(heap, ptr);
    return orig(heap, ptr);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] heap check: %s", heap_check ? "on (links of every large game heap free are checked)"
                                                       : "off (heap_check)");
    if (heap_watch)
        hw::start_watch();
}

extern "C" BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID)
{
    if (reason == DLL_THREAD_ATTACH)
        hw::on_thread_attach();
    return TRUE;
}

SWOFTY_ADDON()
