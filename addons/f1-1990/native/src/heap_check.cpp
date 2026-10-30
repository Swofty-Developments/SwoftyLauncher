// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the game heap link guard (its own DLL).
//
// A game heap block carries a 0x10-byte header: +0x00 the link to the next older allocated block, +0x04 the link to
// the next newer one (16-byte units from the header), +0x08 the block size, +0x0c the requested size.
// Ne_Heap_DeallocateNonSmall unlinks a freed block through those links without checking them. At 22:13 (session
// 20261030-111101) a write past the end of a 0x460-byte block overwrote the older link of the block after it; that
// block's free wrote 4 bytes elsewhere and left its true older neighbour linked to a free block, and the neighbour's
// later free looped the free tree until the stack ran out. The wrap checks both links of every block before the game
// frees it. When the older link is broken and the block just before it in memory is allocated and links forward to
// it, the older link is set back to that block, so the unlink stays inside the list. Every broken link is logged with
// the block, the bytes before its header (the end of the block that ran over) and the first bytes of that block (its
// vtable or first fields), and the call stack, so the writer can be named. Option heap_check off passes every free
// through unchecked.

#include <windows.h>

#include <atomic>
#include <cstdint>
#include <cstdio>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "heap_check.hpp"

SWOFTY_SETTING(bool, heap_check, true);

namespace {

namespace hc = f1990::heap;

constexpr size_t kStart = 0x98, kSize = 0xa0, kGuard = 0x108;
constexpr size_t kScan = 0x10000;
constexpr int kLogEvents = 8;

std::atomic<int> g_events{0};
std::atomic<uint64_t> g_repaired{0}, g_broken{0};

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

void report(void *heap, uint8_t *ptr, uint8_t *h, const hc::Check &c, uint8_t *pred, bool repaired, uint64_t guard)
{
    g_broken.fetch_add(1);
    if (repaired)
        g_repaired.fetch_add(1);
    if (g_events.fetch_add(1) >= kLogEvents)
        return;
    char before[3 * 0x40 + 16], head[3 * 0x10 + 8], first[3 * 0x40 + 16], stack[16 * 24];
    hex(h - 0x40, 0x40, before, sizeof before);
    hex(h, 0x10, head, sizeof head);
    hex(pred ? pred + hc::kHeader + guard : nullptr, 0x40, first, sizeof first);
    frames(stack, sizeof stack);
    swofty::log("[f1-1990] heap check: heap %p block %p (requested 0x%x, size 0x%x) links older %d (%s) newer %d (%s); "
                "%s",
                heap, static_cast<void *>(ptr), hc::requested(h), hc::block_size(h), hc::link(h, 0),
                c.older_ok ? "ok" : "broken", hc::link(h, 1), c.newer_ok ? "ok" : "broken",
                repaired ? "older link set back to the block before it" : "left as it is");
    swofty::log("[f1-1990] heap check:   header %s; the 0x40 bytes before it %s", head, before);
    if (pred)
        swofty::log("[f1-1990] heap check:   block before it %p (requested 0x%x, size 0x%x, links %d %d), first bytes %s",
                    static_cast<void *>(pred + hc::kHeader + guard), hc::requested(pred), hc::block_size(pred),
                    hc::link(pred, 0), hc::link(pred, 1), first);
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
    auto readable = [](const uint8_t *p, size_t n) { return swofty::mem::readable(p, n); };
    uint8_t *pred = hc::predecessor(h, r, guard, kScan, readable);
    bool repaired = false;
    if (!c.older_ok && hc::repair_older(h, pred))
        repaired = hc::check(h, r).older_ok;
    report(heap, ptr, h, c, pred, repaired, guard);
}

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
}

SWOFTY_ADDON()
