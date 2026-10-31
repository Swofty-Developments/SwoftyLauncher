#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace f1990::watch {

constexpr int kSlots = 4;
constexpr size_t kStackWords = 64;
constexpr uint32_t kRing = 512;
constexpr uintptr_t kAllocLo = 0x3960000, kAllocHi = 0x396c000;

constexpr uint64_t kDefault[kSlots] = {0x2FD8487A0, 0x2FD806CF0, 0x2FD80CE18, 0x2FD7CB368};

constexpr uint64_t dr7(int n)
{
    uint64_t v = n > 0 ? (uint64_t(1) << 8) : 0;
    for (int i = 0; i < n && i < kSlots; i++)
        v |= (uint64_t(1) << (i * 2)) | (uint64_t(1) << (16 + i * 4)) | (uint64_t(2) << (18 + i * 4));
    return v;
}

constexpr bool in_allocator(uint64_t rip, uint64_t image_lo)
{
    return rip >= image_lo + kAllocLo && rip < image_lo + kAllocHi;
}

inline int parse(const char *s, uint64_t *out, int cap)
{
    int n = 0;
    while (s && *s && n < cap) {
        while (*s == ' ' || *s == ',' || *s == ';')
            s++;
        if (!*s)
            break;
        char *end = nullptr;
        unsigned long long v = std::strtoull(s, &end, 16);
        if (end == s)
            break;
        if (v & ~uint64_t(7))
            out[n++] = v & ~uint64_t(7);
        s = end;
    }
    return n;
}

struct Hit {
    std::atomic<uint32_t> ready{0};
    uint32_t tid = 0;
    uint32_t changed = 0;
    uint32_t armed = 0;
    uint64_t tick = 0;
    uint64_t rip = 0, rsp = 0, dr6 = 0;
    uint64_t value[kSlots]{};
    uint64_t reg[8]{};
    uint64_t stack[kStackWords]{};
    uint32_t stack_words = 0;
    uint8_t code[16]{};
};

struct Ring {
    Hit slot[kRing];
    std::atomic<uint32_t> head{0};
    uint32_t tail = 0;
    std::atomic<uint32_t> dropped{0};

    Hit *begin()
    {
        uint32_t h = head.load(std::memory_order_relaxed);
        for (;;) {
            if (h - __atomic_load_n(&tail, __ATOMIC_ACQUIRE) >= kRing) {
                dropped.fetch_add(1);
                return nullptr;
            }
            if (head.compare_exchange_weak(h, h + 1))
                break;
        }
        Hit *s = &slot[h % kRing];
        s->ready.store(0, std::memory_order_relaxed);
        return s;
    }

    static void publish(Hit *s) { s->ready.store(1, std::memory_order_release); }

    Hit *next()
    {
        if (tail == head.load(std::memory_order_acquire))
            return nullptr;
        Hit *s = &slot[tail % kRing];
        if (!s->ready.load(std::memory_order_acquire))
            return nullptr;
        return s;
    }

    void pop() { __atomic_store_n(&tail, tail + 1, __ATOMIC_RELEASE); }
};

inline uint32_t changed_mask(const uint64_t *shadow, const uint64_t *now, int n)
{
    uint32_t m = 0;
    for (int i = 0; i < n; i++)
        if (shadow[i] != now[i])
            m |= 1u << i;
    return m;
}

inline void exe_or_raw(char *out, size_t cap, uint64_t v, uint64_t lo, uint64_t hi)
{
    if (v >= lo && v < hi)
        std::snprintf(out, cap, "exe+0x%llx", static_cast<unsigned long long>(v - lo));
    else
        std::snprintf(out, cap, "0x%llx", static_cast<unsigned long long>(v));
}

inline size_t exe_stack(const Hit &h, uint64_t lo, uint64_t hi, char *out, size_t cap, int max_frames)
{
    size_t w = 0;
    int n = 0;
    out[0] = 0;
    for (uint32_t i = 0; i < h.stack_words && n < max_frames && w + 24 < cap; i++) {
        uint64_t v = h.stack[i];
        if (v >= lo + 0x1000 && v < hi) {
            w += size_t(std::snprintf(out + w, cap - w, "%sexe+0x%llx", w ? " " : "",
                                      static_cast<unsigned long long>(v - lo)));
            n++;
        }
    }
    return w;
}

}
