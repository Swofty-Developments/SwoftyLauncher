#include <cstdint>
#include <cstdio>

#include "../src/points_math.hpp"

static int failed = 0;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL line %d: %s\n", __LINE__, #c);                                                           \
            ++failed;                                                                                                  \
        }                                                                                                              \
    } while (0)

static uint32_t game_total(int base, int extra) { return static_cast<uint32_t>(base + extra); }

int main()
{
    CHECK(ce::clamp_extra(-8, 0) == 0);
    CHECK(ce::clamp_extra(-8, 3) == -3);
    CHECK(ce::clamp_extra(-8, 20) == -8);
    CHECK(ce::clamp_extra(5, 0) == 5);
    CHECK(ce::clamp_extra(-8, -4) == 0);

    {
        int base = 0;
        int extra = ce::merge_extra(0, -8, base);
        CHECK(game_total(base, extra) == 0u);
        CHECK(game_total(base, -8) == 4294967288u);
        CHECK(ce::shown_points(-8) == 0);
    }

    {
        int base = 10;
        int with = ce::merge_extra(0, -8, base);
        CHECK(game_total(base, with) == 2u);
        int removed = ce::merge_extra(0, 0, base);
        CHECK(game_total(base, removed) == 10u);
    }

    {
        int base = 4;
        int own = 2;
        CHECK(game_total(base, ce::merge_extra(own, -8, base)) == 0u);
        CHECK(game_total(base, ce::merge_extra(own, 0, base)) == 6u);
        CHECK(game_total(base, ce::merge_extra(own, 3, base)) == 9u);
    }

    if (failed) {
        std::printf("%d failure(s)\n", failed);
        return 1;
    }
    std::printf("points_math_test: ok\n");
    return 0;
}
