#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>

// The AUDIO_BUNDLE heap size (re/db/globals.tsv site_Ne_MemoryMapHeapSize). system/memory_map.xml gives the heap
// that holds the engine, exhaust and cabin sound bundles (audio/2025/bundles/*.bdl) a fixed 43M: a plain heap
// (virtual 0, pagesize 2M) that never grows. A stock season loads about 20 MiB of bundles and each Classic Seasons
// year adds about 14.4 MiB (four makers), so a career that keeps two or three seasons of cars loaded fills the heap
// and the game dies with "AUDIO_BUNDLE: Requested 1677300, Free 918208" at load.
//
// is_stock_audio_bundle: the heap factory's size and config are the stock AUDIO_BUNDLE ones (43M, not virtual,
// pagesize 2M); no other heap of the stock file has that size. choose: the size to give it, from
// SWOFTY_AUDIO_BUNDLE_MB (a number of MiB, 0 or off keeps the game's) or else kDefaultMb, kSmallMb on a machine with
// less than kSmallRam of memory, rounded up to the heap page. frame_name: the heap's name as the factory leaves it in
// its frame (the copy loop writes character k at rsp + kNameSlots[k]).

namespace runtime::audio_heap {

constexpr uint64_t kMiB = 1048576;
constexpr uint64_t kStock = 43 * kMiB;
constexpr uint64_t kPage = 2 * kMiB;
constexpr uint32_t kDefaultMb = 96;
constexpr uint32_t kSmallMb = 64;
constexpr uint32_t kMaxMb = 512;
constexpr uint64_t kSmallRam = 12ull * 1024 * kMiB;

constexpr size_t kConfigVirtual = 0x4;
constexpr size_t kConfigPagesize = 0x20;

constexpr size_t kNameLen = 12;
constexpr uint32_t kNameSlots[kNameLen + 1] = {0x60, 0x6f, 0x6a, 0x6e, 0x64, 0x6d, 0x68,
                                              0x6c, 0x80, 0xa7, 0x9c, 0xa6, 0x90};

enum class Source : uint8_t { off, ram, small_ram, env };

struct Choice {
    uint64_t bytes = kStock;
    Source source = Source::off;
    bool bad_env = false;
    bool raised() const { return source != Source::off && bytes > kStock; }
};

inline bool is_stock_audio_bundle(uint64_t size, uint32_t virt, uint32_t pagesize)
{
    return size == kStock && virt == 0 && pagesize == kPage;
}

inline uint64_t round_up_page(uint64_t bytes)
{
    return (bytes + kPage - 1) / kPage * kPage;
}

inline Choice choose(const char *env, uint64_t physical_bytes)
{
    Choice c;
    if (env && *env) {
        if (!std::strcmp(env, "0") || !std::strcmp(env, "off"))
            return c;
        char *end = nullptr;
        unsigned long long mb = std::strtoull(env, &end, 10);
        if (end != env && *end == '\0') {
            if (mb > kMaxMb)
                mb = kMaxMb;
            if (mb * kMiB <= kStock)
                return c;
            c.bytes = round_up_page(mb * kMiB);
            c.source = Source::env;
            return c;
        }
        c.bad_env = true;
    }
    bool small = physical_bytes != 0 && physical_bytes < kSmallRam;
    c.bytes = round_up_page((small ? kSmallMb : kDefaultMb) * kMiB);
    c.source = small ? Source::small_ram : Source::ram;
    return c;
}

inline const char *source_text(Source s)
{
    switch (s) {
    case Source::off:
        return "the game's size";
    case Source::ram:
        return "default";
    case Source::small_ram:
        return "default for a machine under 12 GiB";
    case Source::env:
        return "SWOFTY_AUDIO_BUNDLE_MB";
    }
    return "";
}

inline void frame_name(const uint8_t *rsp, char (&out)[kNameLen + 1])
{
    for (size_t k = 0; k < kNameLen; k++) {
        char ch = static_cast<char>(rsp[kNameSlots[k]]);
        out[k] = ch >= 0x20 && ch < 0x7f ? ch : '?';
    }
    out[kNameLen] = '\0';
}

}
