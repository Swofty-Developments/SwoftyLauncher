// Host-native unit test for the core's AUDIO_BUNDLE heap size (native/src/core/audio_heap_logic.hpp): which heap
// the hook raises, the size it picks from the machine and SWOFTY_AUDIO_BUNDLE_MB, and the name read from the
// factory's frame. Built and run by native/test/run_audio_heap_test.sh.

#include "audio_heap_logic.hpp"

#include <cstdio>
#include <cstring>
#include <initializer_list>

namespace ah = runtime::audio_heap;

static int g_fail;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            g_fail++;                                                                                                  \
        }                                                                                                              \
    } while (0)

constexpr uint64_t GiB = 1024ull * ah::kMiB;

int main()
{
    CHECK(ah::kStock == 45088768);
    CHECK(ah::is_stock_audio_bundle(45088768, 0, 2 * ah::kMiB));
    CHECK(!ah::is_stock_audio_bundle(45088768, 1, 2 * ah::kMiB));
    CHECK(!ah::is_stock_audio_bundle(45088768, 0, 0));
    CHECK(!ah::is_stock_audio_bundle(30 * ah::kMiB, 0, 2 * ah::kMiB));
    CHECK(!ah::is_stock_audio_bundle(96 * ah::kMiB, 0, 2 * ah::kMiB));

    {
        ah::Choice c = ah::choose(nullptr, 32 * GiB);
        CHECK(c.raised() && c.source == ah::Source::ram && c.bytes == 96 * ah::kMiB && !c.bad_env);
        c = ah::choose("", 16 * GiB);
        CHECK(c.bytes == 96 * ah::kMiB);
        c = ah::choose(nullptr, 12 * GiB);
        CHECK(c.bytes == 96 * ah::kMiB);
        c = ah::choose(nullptr, 8 * GiB);
        CHECK(c.raised() && c.source == ah::Source::small_ram && c.bytes == 64 * ah::kMiB);
        c = ah::choose(nullptr, 0);
        CHECK(c.bytes == 96 * ah::kMiB);
    }
    {
        ah::Choice c = ah::choose("128", 8 * GiB);
        CHECK(c.raised() && c.source == ah::Source::env && c.bytes == 128 * ah::kMiB);
        c = ah::choose("101", 32 * GiB);
        CHECK(c.bytes == 102 * ah::kMiB);
        c = ah::choose("100000", 32 * GiB);
        CHECK(c.bytes == 512 * ah::kMiB);
        c = ah::choose("44", 32 * GiB);
        CHECK(c.bytes == 44 * ah::kMiB && c.raised());
        c = ah::choose("43", 32 * GiB);
        CHECK(!c.raised() && c.bytes == ah::kStock);
        c = ah::choose("20", 32 * GiB);
        CHECK(!c.raised() && c.bytes == ah::kStock);
        c = ah::choose("0", 32 * GiB);
        CHECK(!c.raised() && c.bytes == ah::kStock);
        c = ah::choose("off", 32 * GiB);
        CHECK(!c.raised() && c.bytes == ah::kStock);
        c = ah::choose("lots", 32 * GiB);
        CHECK(c.bad_env && c.raised() && c.bytes == 96 * ah::kMiB);
        c = ah::choose("96MB", 8 * GiB);
        CHECK(c.bad_env && c.bytes == 64 * ah::kMiB);
    }
    {
        for (uint64_t mb : {64u, 96u, 101u, 128u, 512u}) {
            char s[16];
            std::snprintf(s, sizeof s, "%llu", (unsigned long long)mb);
            ah::Choice c = ah::choose(s, 32 * GiB);
            CHECK(c.bytes % ah::kPage == 0);
            CHECK(c.bytes >= mb * ah::kMiB);
        }
        CHECK(ah::choose(nullptr, 32 * GiB).bytes % ah::kPage == 0);
    }
    {
        uint8_t frame[0x100];
        std::memset(frame, 0xcc, sizeof frame);
        const char *name = "AUDIO_BUNDLE";
        for (size_t k = 0; k < ah::kNameLen; k++)
            frame[ah::kNameSlots[k]] = static_cast<uint8_t>(name[k]);
        frame[ah::kNameSlots[ah::kNameLen]] = 0;
        char out[ah::kNameLen + 1];
        ah::frame_name(frame, out);
        CHECK(!std::strcmp(out, "AUDIO_BUNDLE"));
        std::memset(frame, 0, sizeof frame);
        ah::frame_name(frame, out);
        CHECK(!std::strcmp(out, "????????????"));
        bool distinct = true;
        for (size_t a = 0; a < ah::kNameLen + 1; a++)
            for (size_t b = a + 1; b < ah::kNameLen + 1; b++)
                distinct = distinct && ah::kNameSlots[a] != ah::kNameSlots[b];
        CHECK(distinct);
    }
    if (g_fail) {
        std::printf("%d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("audio heap test: ok\n");
    return 0;
}
