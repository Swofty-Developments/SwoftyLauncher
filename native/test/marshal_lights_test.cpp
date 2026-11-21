// Host-native unit test for the core's marshal lights guard (native/src/core/guards/session/marshal_lights_logic.hpp).
// Built and run by native/test/run_tests.sh marshal_lights.
//
// The walk below follows Sess_MarshalLights_AssignToZones instruction by instruction (Steam 26566308 0x142c79330,
// KB 0x142c76da0): back to front over the light list, first zone in list order, erase of a light in no zone, the step
// at site_Sess_MarshalLights_PrevLightLoad, the exit at site_Sess_MarshalLights_WalkDone and its sort by zone id.

#include "core/guards/session/marshal_lights_logic.hpp"
#include "core/guards/skips.hpp"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <map>
#include <vector>

namespace ml = runtime::marshal_lights;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Memory {
    std::map<uint64_t, uint64_t> words;
};

static bool read_word(uint64_t address, void *out, size_t n, void *ctx)
{
    auto *m = static_cast<Memory *>(ctx);
    auto it = m->words.find(address);
    if (n != sizeof(uint64_t) || it == m->words.end())
        return false;
    std::memcpy(out, &it->second, n);
    return true;
}

struct Light {
    int id = -1;
    float fraction = 0;
    float stored = 0;
};

struct Zone {
    float start;
    float end;
    int id;
};

enum class Walk { done, read_before_list };

constexpr uint64_t SYSTEM = 0x788d1e0;
constexpr uint64_t BEGIN = 0x27735a20;

static const Zone *zone_of(float f, const std::vector<Zone> &zones)
{
    for (auto &z : zones) {
        bool wrapped = z.start > z.end;
        if (wrapped ? (z.end >= f || f >= z.start) : (z.end >= f && f >= z.start))
            return &z;
    }
    return nullptr;
}

static Walk assign_to_zones(std::vector<Light *> &list, const std::vector<Zone> &zones, bool guarded, int &ended)
{
    if (list.empty())
        return Walk::done;
    Memory memory;
    memory.words[SYSTEM + ml::LIST_BEGIN] = BEGIN;
    size_t r13 = list.size();
    while (true) {
        Light *light = list[r13 - 1];
        float f = light->fraction;
        size_t rbp;
        if (const Zone *z = zone_of(f, zones)) {
            light->id = z->id;
            rbp = r13;
        } else {
            rbp = r13 - 1;
            list.erase(list.begin() + long(rbp));
        }
        if (guarded && ml::walk_ends(SYSTEM, BEGIN + rbp * 8, read_word, &memory)) {
            r13 = rbp;
            ended++;
            break;
        }
        if (rbp == 0)
            return Walk::read_before_list;
        list[rbp - 1]->stored = f;
        r13 = rbp - 1;
        if (r13 == 0)
            break;
    }
    for (size_t pass = list.size(); pass > 1; pass--)
        for (size_t i = 0; i + 1 < pass; i++)
            if (list[i]->id > list[i + 1]->id)
                std::swap(list[i], list[i + 1]);
    return Walk::done;
}

static size_t capacity_after(size_t pushes)
{
    size_t capacity = 0;
    for (size_t size = 0; size < pushes; size++)
        if (size == capacity)
            capacity = std::max(capacity + capacity / 2, capacity + 1);
    return capacity;
}

static std::vector<Light> lights_around_the_lap(size_t n)
{
    std::vector<Light> lights(n);
    for (size_t i = 0; i < n; i++)
        lights[i].fraction = 0.026f + 0.966f * float(i) / float(n - 1);
    return lights;
}

static std::vector<Light *> list_of(std::vector<Light> &lights)
{
    std::vector<Light *> list;
    for (auto &l : lights)
        list.push_back(&l);
    return list;
}

static void decision()
{
    Memory memory;
    memory.words[SYSTEM + ml::LIST_BEGIN] = BEGIN;
    expect(ml::walk_ends(SYSTEM, BEGIN, read_word, &memory),
           "the crash's state (rbp = list begin 0x27735a20, [rsi+0x68] = 0x27735a20) ends the walk");
    expect(!ml::walk_ends(SYSTEM, BEGIN + 8, read_word, &memory),
           "the step after the first light got a zone (rbp = begin + 8) runs the game's code");
    expect(!ml::walk_ends(SYSTEM, BEGIN + 7 * 8, read_word, &memory), "a step in the middle of the list runs the game's code");
    expect(!ml::walk_ends(SYSTEM, BEGIN - 8, read_word, &memory),
           "a slot below the list, which the game's walk never has at its step, runs the game's code");

    Memory ashton;
    ashton.words[0x78311e0 + ml::LIST_BEGIN] = 0x25ca0000;
    expect(ml::walk_ends(0x78311e0, 0x25ca0000, read_word, &ashton),
           "the 18 Nov crash's state (rbp = list begin 0x25ca0000) ends the walk");

    Memory empty;
    expect(!ml::walk_ends(SYSTEM, BEGIN, read_word, &empty), "a system whose list start does not read runs the game's code");
    expect(!ml::walk_ends(0, BEGIN, read_word, &memory), "a null system runs the game's code");
    Memory null_begin;
    null_begin.words[SYSTEM + ml::LIST_BEGIN] = 0;
    expect(!ml::walk_ends(SYSTEM, 0, read_word, &null_begin), "a list with no block runs the game's code");
    expect(ml::STORE_FROM_SITE == 0x2c794d2 - 0x2c794c7, "the faulting store is 0xb bytes past the step site");
}

static void no_zones_unguarded()
{
    auto lights = lights_around_the_lap(30);
    auto list = list_of(lights);
    int ended = 0;
    expect(capacity_after(30) == 42, "30 lights registered one by one leave the list with capacity 42, as in both dumps");
    expect(assign_to_zones(list, {}, false, ended) == Walk::done, "the first session on a track with no zones runs through");
    expect(list.size() == 15, "the first session erases every other light and keeps 15");
    expect(list.front() == &lights[0], "the first light is skipped in the first session and stays first");
    expect(assign_to_zones(list, {}, false, ended) == Walk::read_before_list,
           "the second session erases the first light and loads the 8 bytes before the list");
    expect(list.size() == 7, "7 lights are left when it does, as in both dumps");
}

static void no_zones_guarded()
{
    auto lights = lights_around_the_lap(30);
    auto list = list_of(lights);
    int ended = 0;
    std::vector<size_t> sizes;
    bool clean = true;
    for (int session = 0; session < 8; session++) {
        clean &= assign_to_zones(list, {}, true, ended) == Walk::done;
        sizes.push_back(list.size());
    }
    expect(clean, "with the guard every session on a track with no zones runs through");
    expect(sizes == std::vector<size_t>({15, 7, 3, 1, 0, 0, 0, 0}),
           "the list shrinks 15, 7, 3, 1, 0 as the game intends and then stays empty");
    expect(ended == 4, "the guard ends the walk in the four sessions that erase the first light");
}

static void covered_lap()
{
    std::vector<Zone> zones;
    for (int i = 0; i < 13; i++)
        zones.push_back({0.106f + 0.0660f * float(i), 0.106f + 0.0660f * float(i + 1), i + 1});
    zones.push_back({0.964f, 0.106f, 14});
    auto lights = lights_around_the_lap(30);
    auto list = list_of(lights);
    int ended = 0;
    expect(assign_to_zones(list, zones, true, ended) == Walk::done, "a lap the zones cover runs through");
    expect(list.size() == 30 && ended == 0, "no light is erased and the guard never acts");
    bool sorted = std::is_sorted(list.begin(), list.end(), [](const Light *a, const Light *b) { return a->id < b->id; });
    expect(sorted, "the lights end sorted by zone id");
    bool placed = std::all_of(list.begin(), list.end(), [](const Light *l) { return l->id >= 1 && l->id <= 14; });
    expect(placed, "every light has a zone");
    std::vector<Light *> before = list;
    expect(assign_to_zones(list, zones, true, ended) == Walk::done && list == before && ended == 0,
           "the next session leaves the list as it was");
}

static void first_light_outside()
{
    std::vector<Zone> zones = {{0.2f, 0.95f, 1}};
    std::vector<Light> lights(5);
    float fractions[] = {0.1f, 0.3f, 0.5f, 0.7f, 0.9f};
    for (int i = 0; i < 5; i++)
        lights[i].fraction = fractions[i];
    auto stock = list_of(lights);
    int ended = 0;
    expect(assign_to_zones(stock, zones, false, ended) == Walk::read_before_list,
           "an unguarded first session faults when the first light is the only one outside the zones");
    for (auto &l : lights)
        l = Light{-1, l.fraction, 0};
    auto guarded = list_of(lights);
    expect(assign_to_zones(guarded, zones, true, ended) == Walk::done && ended == 1,
           "with the guard it erases that light and ends the walk");
    expect(guarded.size() == 4 && guarded.front() == &lights[1], "the four lights inside the zone are kept");
}

static void log_cap()
{
    swofty_core::guards::LogFirst<ml::LOG_FIRST> s{"skips"};
    int lines = 0, last = 0;
    for (int i = 0; i < 100; i++) {
        auto line = s.note();
        if (line)
            lines++;
        if (line && *line.tail)
            last++;
    }
    expect(lines == ml::LOG_FIRST && last == 1, "a hundred skips log the capped number of lines, the last one marked");
}

int main()
{
    decision();
    no_zones_unguarded();
    no_zones_guarded();
    covered_lap();
    first_light_outside();
    log_cap();
    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
