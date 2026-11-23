// Host-native unit test for the collision tile pool guard (native/src/core/guards/tracks/collision_pool_logic.hpp): the
// pool size it chooses from SWOFTY_COLLISION_GEOM_POOL, that it never lowers what the game read, and, on a
// transcription of the game's pool (Ne_PhysWorld_InitFromConfig) and tile loop (Ne_MoverCollision_CreateResource /
// Ne_MoverCollision_BuildMoverShapes), how many tiles of a JPAK load with the game's 2000 and with the guard's size.
// Built and run by native/test/run_tests.sh collision_pool.

#include "core/guards/tracks/collision_pool_logic.hpp"

#include <cstdio>
#include <cstring>
#include <vector>

static int failures;

#define CHECK(cond, msg)                                                                                                         \
    do {                                                                                                                         \
        if (cond) {                                                                                                              \
            printf("ok  : %s\n", msg);                                                                                           \
        } else {                                                                                                                 \
            printf("FAIL: %s\n", msg);                                                                                           \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

namespace cp = runtime::collision_pool;

namespace {

// Ne_PhysWorld_InitFromConfig's pool for world +0x40: element 0x220 rounded up to 16 plus 16 (stride 0x230), count
// objects in one block, linked into a free list through their first qword. take() is BuildMoverShapes' test: used <
// capacity and a free object, else error 0xc.
struct Pool {
    uint64_t stride = 0;
    uint32_t capacity = 0, used = 0;
    std::vector<uint8_t> block;
    uint8_t *free = nullptr;

    explicit Pool(uint32_t count)
    {
        uint64_t element = 0x220;
        uint64_t s = element + 0x10;
        stride = (element & 0xf) == 0 ? s : (s & ~uint64_t(0xf)) + 0x10;
        capacity = count;
        block.assign(stride * count, 0);
        uint8_t *prev = nullptr;
        for (uint32_t i = 0; i < count; i++) {
            uint8_t *p = block.data() + stride * i;
            if (!prev)
                free = p;
            else
                std::memcpy(prev, &p, sizeof p);
            prev = p;
        }
    }

    bool take()
    {
        if (used >= capacity || !free)
            return false;
        uint8_t *next = nullptr;
        std::memcpy(&next, free, sizeof next);
        free = next;
        used++;
        return true;
    }
};

// CreateResource: one BuildMoverShapes per JPAK entry in file order; the first failure ends the loop.
uint32_t tiles_loaded(uint32_t pool_size, uint32_t other_geoms, uint32_t tiles)
{
    Pool pool(pool_size);
    for (uint32_t i = 0; i < other_geoms; i++)
        pool.take();
    uint32_t loaded = 0;
    while (loaded < tiles && pool.take())
        loaded++;
    return loaded;
}

}

static_assert(cp::kLargeGeomsSlot == 0x108, "world-1 maxLargeGeoms is the dword at rsp+0x108 at the site");
static_assert(cp::kStock < cp::kDefault && cp::kDefault <= cp::kMax && cp::kMax < 0x261a,
              "the default and the maximum stay below the 9755-tile eager query-context path");

int main()
{
    cp::Choice c = cp::choose(nullptr);
    CHECK(c.size == cp::kDefault && !c.from_env && !c.bad_env, "unset: the default 8192");
    c = cp::choose("");
    CHECK(c.size == cp::kDefault && !c.bad_env, "empty: the default");
    c = cp::choose("4000");
    CHECK(c.size == 4000 && c.from_env && !c.clamped, "4000 from the variable");
    c = cp::choose("20000");
    CHECK(c.size == cp::kMax && c.clamped, "above the maximum: 9728");
    c = cp::choose("1500");
    CHECK(c.size == 1500 && cp::raised(cp::kStock, c.size) == cp::kStock, "below the game's 2000: the game's size stays");
    for (const char *bad : {"abc", "12x", "-5", "+5", " 5", "1e4"}) {
        c = cp::choose(bad);
        CHECK(c.bad_env && c.size == cp::kDefault, "not a number: the default, with a line");
    }
    CHECK(!cp::choice_line(cp::choose("abc"), "V").empty() && cp::choice_line(cp::choose("4000"), "V").empty(),
          "a line only for a bad or clamped value");

    CHECK(cp::raised(2000, 8192) == 8192, "2000 -> 8192");
    CHECK(cp::raised(10000, 8192) == 10000, "a settings value above the choice is kept");
    CHECK(cp::raised(8192, 8192) == 8192, "equal: unchanged");

    CHECK(Pool(1).stride == 0x230, "pool stride 0x230 (element 0x220 + 16)");
    CHECK(cp::pool_bytes(cp::kStock) == 1120000 && cp::pool_bytes(cp::kDefault) == 4587520,
          "memory: 1.07 MiB stock, 4.38 MiB at 8192");

    // About 360 other large geoms share world 1 (a 1663-tile track lost its last tiles, stock Spa's 1239 and a 1645-tile
    // track whose last tiles were off the road both looked whole).
    const uint32_t other = 360;
    CHECK(tiles_loaded(cp::kStock, other, 1239) == 1239, "stock: Spa's 1239 tiles all load");
    CHECK(tiles_loaded(cp::kStock, other, 1663) == 1640, "stock: a 1663-tile JPAK stops at tile 1640");
    CHECK(tiles_loaded(cp::kDefault, other, 1663) == 1663, "8192: the 1663 tiles all load");
    CHECK(tiles_loaded(cp::kDefault, other, 4000) == 4000, "8192: 4000 tiles all load");
    CHECK(tiles_loaded(cp::kDefault, other, 9000) == cp::kDefault - other, "8192: at most 7832 tiles beside 360 others");
    CHECK(tiles_loaded(cp::kMax, other, 10000) == cp::kMax - other, "9728: at most 9368 tiles beside 360 others");

    if (failures) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
