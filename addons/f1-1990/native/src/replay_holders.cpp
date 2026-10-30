// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - room for a 26-car grid in the replay recorder (its own DLL, so a binding it can't resolve drops only this
// hook). Setting replay_holder_fix turns it off.
//
// The replay/flashback recorder (Ne_Replay_WriteFrames) keeps one state holder per recordable: a heap
// block of 0x80 header bytes followed by a current and a previous state buffer. The holder's class is
// picked at registration from the state size a stock grid needs (20 classes, capacity 0x20 to 0xffff).
// Each frame the recordable serializes into a static scratch buffer, and the recorder copies that state
// into the current buffer and, the next frame, the current buffer into the previous one, with the full
// state size and no capacity check. At 26 cars, when the race starts, a per-car recordable's state
// outgrows its holder and the copy runs past the end of the block into the next heap block (seen
// overwriting the tyre system's wheel map table header, which later aborts the game with heap error
// 0x16 when that table is freed).
//
// After the recordable has serialized and before either copy, the recorder holds the holder at
// [rsp+0x118] and the state size is [rsp+0x98] - [rsp+0xa0]. When that size exceeds the holder's
// capacity, the holder is replaced by one of the smallest class that fits: allocated with the
// registry's allocator, header and both buffers copied, the registry slot and the recorder's cached
// pointers into the old holder moved to the new one. The old holder is left allocated, so anything
// else still pointing at it stays valid. States within capacity, which is every state on a stock
// grid, are never touched.

#include <cstring>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

SWOFTY_SETTING(bool, replay_holder_fix, true);

namespace {

struct HolderClass {
    uint32_t vtable_rva;
    uint32_t cap;
    uint32_t buf;
};

constexpr HolderClass kClasses[] = {
    {0x5660400, 0x20, 0x80},     {0x56cfa10, 0x39, 0xc0},     {0x56588e0, 0x40, 0xc0},     {0x5660460, 0x80, 0x100},
    {0x5658970, 0x100, 0x180},   {0x5660430, 0x400, 0x480},   {0x568a270, 0x7bc, 0x840},   {0x565b180, 0x800, 0x880},
    {0x565b1b0, 0xc00, 0xc80},   {0x5660490, 0x1000, 0x1080}, {0x5659790, 0x1800, 0x1880}, {0x5635390, 0x1c00, 0x1c80},
    {0x5659c30, 0x2000, 0x2080}, {0x568b3f0, 0x2008, 0x2080}, {0x57b6ec0, 0x2400, 0x2480}, {0x56589d0, 0x2800, 0x2880},
    {0x56589a0, 0x4000, 0x4080}, {0x56360a0, 0x8000, 0x8080}, {0x5658940, 0xa800, 0xa880}, {0x5658910, 0xffff, 0x10080},
};
constexpr int kCount = sizeof kClasses / sizeof kClasses[0];
constexpr uintptr_t kHeader = 0x80;

constexpr uint32_t kStreamPos = 0x98, kStreamBase = 0xa0, kHolder = 0x118, kEntryBase = 0x198, kEntryIndex = 0x190;
constexpr uint32_t kCapSlots[] = {0xc8, 0x108};
constexpr uint32_t kHeaderSlots[] = {0xb8, 0xd0, 0xd8, 0xe0, 0xf0, 0xf8, 0x100};
constexpr uint32_t kCurSlot = 0xc0, kPrevSlot = 0xe8;

bool g_checked, g_ok;
int g_upgrades;

uintptr_t vt_of(int i)
{
    return ::swofty::mem::detail::image().lo + kClasses[i].vtable_rva;
}

bool fn_is(uintptr_t fn, const uint8_t *want, size_t n)
{
    return ::swofty::mem::readable(fn, n) && !std::memcmp(reinterpret_cast<const void *>(fn), want, n);
}

bool class_matches(int i)
{
    const HolderClass &k = kClasses[i];
    uintptr_t slots[5];
    if (!::swofty::mem::read(vt_of(i), slots))
        return false;
    uint8_t cap[6] = {0xb8, 0, 0, 0, 0, 0xc3};
    std::memcpy(cap + 1, &k.cap, 4);
    uint8_t cur[8] = {0x48, 0x8d, 0x81, 0x80, 0, 0, 0, 0xc3};
    uint8_t prev[8] = {0x48, 0x8d, 0x81, 0, 0, 0, 0, 0xc3};
    uint32_t prev_off = kHeader + k.buf;
    std::memcpy(prev + 3, &prev_off, 4);
    return fn_is(slots[2], cap, 6) && fn_is(slots[3], cur, 8) && fn_is(slots[4], prev, 8);
}

bool check_classes()
{
    if (!g_checked) {
        g_checked = true;
        g_ok = true;
        for (int i = 0; i < kCount; i++)
            if (!class_matches(i)) {
                swofty::log("[f1-1990] replay holders: class %d (vtable rva 0x%x) does not match this build; fix off", i,
                            kClasses[i].vtable_rva);
                g_ok = false;
                break;
            }
    }
    return g_ok;
}

int class_of(uintptr_t vt)
{
    for (int i = 0; i < kCount; i++)
        if (vt_of(i) == vt)
            return i;
    return -1;
}

uintptr_t &slot(swofty::Context &c, uint32_t off)
{
    return *reinterpret_cast<uintptr_t *>(c.rsp + off);
}

void upgrade(swofty::Context &c)
{
    uintptr_t pos = slot(c, kStreamPos), base = slot(c, kStreamBase), old = slot(c, kHolder);
    if (pos <= base || pos - base <= slot(c, kCapSlots[1]) || !replay_holder_fix || !::swofty::mem::object_like(old))
        return;
    uintptr_t size = pos - base;
    int from = class_of(*reinterpret_cast<uintptr_t *>(old));
    if (from < 0 || size <= kClasses[from].cap || !check_classes())
        return;
    int to = from;
    while (to < kCount && (kClasses[to].cap < size || kClasses[to].buf <= kClasses[from].buf))
        to++;
    if (to >= kCount) {
        swofty::log("[f1-1990] replay holders: a state of %zu bytes is larger than any holder; left as is", size);
        return;
    }
    uintptr_t entry_slot = slot(c, kEntryBase) + slot(c, kEntryIndex) + 0x20;
    uintptr_t alloc = 0;
    if (!::swofty::mem::readable(entry_slot - 8, 16) || *reinterpret_cast<uintptr_t *>(entry_slot) != old ||
        !::swofty::mem::read(entry_slot - 8, alloc) || !::swofty::mem::object_like(alloc)) {
        swofty::log("[f1-1990] replay holders: registry slot for holder %p not found; left as is",
                    reinterpret_cast<void *>(old));
        return;
    }
    const HolderClass &a = kClasses[from], &b = kClasses[to];
    using Alloc = void *(*)(uintptr_t, uint64_t, uint64_t);
    auto alloc_fn = reinterpret_cast<Alloc>((*reinterpret_cast<uintptr_t **>(alloc))[3]);
    auto *fresh = static_cast<uint8_t *>(alloc_fn(alloc, kHeader + 2 * uint64_t(b.buf), 0x10));
    if (!fresh) {
        swofty::log("[f1-1990] replay holders: allocation of %u bytes failed; left as is", kHeader + 2 * b.buf);
        return;
    }
    auto *src = reinterpret_cast<const uint8_t *>(old);
    std::memset(fresh, 0, kHeader + 2 * size_t(b.buf));
    std::memcpy(fresh, src, kHeader);
    *reinterpret_cast<uintptr_t *>(fresh) = vt_of(to);
    std::memcpy(fresh + kHeader, src + kHeader, a.buf);
    std::memcpy(fresh + kHeader + b.buf, src + kHeader + a.buf, a.buf);
    uintptr_t now = reinterpret_cast<uintptr_t>(fresh);
    *reinterpret_cast<uintptr_t *>(entry_slot) = now;
    slot(c, kHolder) = now;
    for (uint32_t off : kHeaderSlots)
        if (slot(c, off) >= old && slot(c, off) < old + kHeader)
            slot(c, off) += now - old;
    slot(c, kCurSlot) = now + kHeader;
    slot(c, kPrevSlot) = now + kHeader + b.buf;
    for (uint32_t off : kCapSlots)
        if (slot(c, off) == a.cap)
            slot(c, off) = b.cap;
    c.r12 = now;
    if (g_upgrades++ < 64) {
        std::string name;
        ::swofty::mem::c_string(c.r13, name, 96);
        swofty::log("[f1-1990] replay holders: %s state %zu bytes > capacity 0x%x; holder %p -> %p (capacity 0x%x)",
                    name.empty() ? "?" : name.c_str(), size, a.cap, reinterpret_cast<void *>(old), fresh, b.cap);
    }
}

}

SWOFTY_SITE(site_Ne_ReplayAfterRecord, swofty::Context &c)
{
    upgrade(c);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] replay holders loaded: holder growth %s", replay_holder_fix ? "on" : "off (replay_holder_fix)");
}

SWOFTY_ADDON()
