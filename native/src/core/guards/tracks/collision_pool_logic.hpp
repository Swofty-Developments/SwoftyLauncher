#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>

// The size of physics world 1's large-geom pool, the pool every VCQTC collision tile of a track's TRN~ JPAK comes from
// (collision_pool_guard.cpp). No Windows or game code, so the host unit test includes it.
namespace runtime::collision_pool {

// Game_Read_PhysicsWorldConfig keeps the world-1 PhysicsWorldConfig at rsp+0x40; maxLargeGeoms is its dword at +0xc8.
constexpr size_t kConfigOnStack = 0x40;
constexpr size_t kLargeGeoms = 0xc8;
constexpr size_t kLargeGeomsSlot = kConfigOnStack + kLargeGeoms;

// The game's default for PhysicsWorld.maxLargeGeoms (mov r9d, 0x7d0 before the settings read).
constexpr uint32_t kStock = 2000;
// The pool size the guard sets when the variable is unset.
constexpr uint32_t kDefault = 8192;
// The largest pool the guard sets. Ne_MoverCollision_CreateResource builds the tile query context eagerly, at load,
// for a JPAK of more than 0x261a (9754) tiles, a path no stock track takes; a pool of at most 9728 objects keeps every
// track on the lazy path the stock tracks use.
constexpr uint32_t kMax = 9728;
// One pool object: element 0x220 bytes, rounded up by the pool to 16 plus a 16-byte header (Ne_PhysWorld_InitFromConfig).
constexpr uint64_t kObjectBytes = 0x230;

struct Choice {
    uint32_t size = kDefault;
    bool from_env = false;
    bool bad_env = false;
    bool clamped = false;
};

// env: the variable's value, or null when unset. 0 and off are handled by the guard before this (the guard is off).
inline Choice choose(const char *env)
{
    Choice c;
    if (!env || !*env)
        return c;
    char *end = nullptr;
    unsigned long long n = env[0] >= '0' && env[0] <= '9' ? std::strtoull(env, &end, 10) : 0;
    if (!end || *end != '\0') {
        c.bad_env = true;
        return c;
    }
    c.from_env = true;
    if (n > kMax) {
        n = kMax;
        c.clamped = true;
    }
    c.size = static_cast<uint32_t>(n);
    return c;
}

// The pool size world 1 gets: never below what the game read (the settings value or the 2000 default).
inline uint32_t raised(uint32_t game, uint32_t chosen) { return chosen > game ? chosen : game; }

inline uint64_t pool_bytes(uint32_t objects) { return uint64_t(objects) * kObjectBytes; }

inline std::string choice_line(const Choice &c, const char *env_name)
{
    char b[400];
    if (c.bad_env)
        std::snprintf(b, sizeof b, "[collision] %s is not a number of pool objects, 0 or off; using the default %u", env_name,
                      kDefault);
    else if (c.clamped)
        std::snprintf(b, sizeof b, "[collision] %s is above %u; using %u", env_name, kMax, kMax);
    else
        b[0] = 0;
    return b;
}

inline std::string raise_line(uint32_t game, uint32_t now, bool from_env)
{
    char b[400];
    if (now == game)
        std::snprintf(b, sizeof b,
                      "[collision] tile pool: the game's large-geom pool already has %u objects (asked for no more); left as "
                      "it is",
                      game);
    else
        std::snprintf(b, sizeof b,
                      "[collision] tile pool: world-1 large-geom pool %u -> %u objects (%s), %.1f MiB instead of %.1f MiB; "
                      "track collision tiles beyond the game's ~1640 no longer go missing",
                      game, now, from_env ? "SWOFTY_COLLISION_GEOM_POOL" : "default", double(pool_bytes(now)) / 1048576.0,
                      double(pool_bytes(game)) / 1048576.0);
    return b;
}

}
