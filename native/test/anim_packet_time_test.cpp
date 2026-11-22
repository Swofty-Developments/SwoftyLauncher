// Host-native unit test for the core's anim packet time guard (native/src/core/guards/engine/anim_packet_time_logic.hpp).
// Built and run by native/test/run_tests.sh anim_packet_time.
//
// The samplers below follow the keyframe packets' track sampling instruction by instruction: the B1 packet's inline
// sampler in its slot +0x18 (Steam 26566308 0x143a3bd80, KB 0x143a397b0), which B4 and L1 share, and the per-track
// samplers of L3, L4 and S4 (Steam 26566308 0x143a3d470, KB 0x143a3aea0). The UI animation player (Steam 26566308
// 0x1441fb320) calls slot +0x18 of each packet with its own time, past the anim clip time guard's thunk.

#include "core/guards/engine/anim_packet_time_logic.hpp"
#include "core/guards/skips.hpp"

#include <cfloat>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <string>
#include <vector>

namespace apt = runtime::anim_packet_time;
namespace act = runtime::anim_clip_time;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static float from_bits(uint32_t bits)
{
    float f;
    std::memcpy(&f, &bits, sizeof f);
    return f;
}

static uint32_t to_bits(float f)
{
    uint32_t bits;
    std::memcpy(&bits, &f, sizeof bits);
    return bits;
}

static int32_t cvtt(float x)
{
    if (std::isnan(x) || x >= 2147483648.f || x < -2147483648.f)
        return INT32_MIN;
    return int32_t(x);
}

static float maxss(float a, float b) { return a > b ? a : b; }

struct Track {
    int keys;
    float first_key, rate;
    int buckets;
};

enum class Path { single_key, two_keys, first_key, bucket, last_key };

struct Sampled {
    Path path;
    int32_t bucket;
    float weight;
};

constexpr uint64_t BUCKETS = 0x2a7c51e40;

static Sampled game_sample(const Track &t, float time)
{
    if (t.keys == 1)
        return {Path::single_key, 0, 0.f};
    if (t.keys < 3) {
        float x = (time - t.first_key) * t.rate;
        float clamped = maxss(0.f, x);
        return {Path::two_keys, 0, 1.f < x ? 1.f : clamped};
    }
    if (!std::isnan(time) && t.first_key >= time)
        return {Path::first_key, 0, 0.f};
    int32_t index = cvtt((time - t.first_key) * t.rate);
    if (t.buckets - 1 > index)
        return {Path::bucket, index, 0.f};
    return {Path::last_key, index, 0.f};
}

static uint64_t bucket_read(int32_t index) { return BUCKETS + uint64_t(int64_t(index) * 2); }

static bool reads_inside(const Track &t, const Sampled &s)
{
    return s.path != Path::bucket || (s.bucket >= 0 && s.bucket < t.buckets);
}

static bool shows_first_key(const Sampled &s)
{
    return s.path == Path::single_key || s.path == Path::first_key || (s.path == Path::two_keys && s.weight == 0.f);
}

static bool same(const Sampled &a, const Sampled &b)
{
    return a.path == b.path && a.bucket == b.bucket && to_bits(a.weight) == to_bits(b.weight);
}

struct Packet {
    bool keyframes;
    std::vector<Track> tracks;
};

enum class Guard { none, clip_thunk, packets };

static float packet_time(float time, Guard guard) { return guard == Guard::packets ? apt::sampler_time(time).time : time; }

static float clip_thunk_time(float time, Guard guard) { return guard == Guard::none ? time : act::sample_time(time); }

static std::vector<Sampled> sample_packets(const std::vector<Packet> &packets, float time, Guard guard)
{
    std::vector<Sampled> out;
    for (auto &p : packets) {
        if (!p.keyframes)
            continue;
        for (auto &t : p.tracks)
            out.push_back(game_sample(t, packet_time(time, guard)));
    }
    return out;
}

static std::vector<Sampled> ui_player(const std::vector<Packet> &packets, float player_time, Guard guard)
{
    return sample_packets(packets, player_time, guard);
}

static std::vector<Sampled> clip_player(const std::vector<Packet> &packets, float clip_time, Guard guard)
{
    return sample_packets(packets, clip_thunk_time(clip_time, guard), guard);
}

static bool all_inside(const std::vector<Packet> &packets, const std::vector<Sampled> &s)
{
    size_t i = 0;
    for (auto &p : packets) {
        if (!p.keyframes)
            continue;
        for (auto &t : p.tracks)
            if (!reads_inside(t, s[i++]))
                return false;
    }
    return true;
}

static bool all_first_key(const std::vector<Sampled> &s)
{
    for (auto &x : s)
        if (!shows_first_key(x))
            return false;
    return true;
}

int main()
{
    const float crash_time = from_bits(0xffc00000u);
    const Track crash_track{12, 0.f, 30.f, 6};
    Sampled crash = game_sample(crash_track, crash_time);
    expect(crash.path == Path::bucket && uint32_t(crash.bucket) == 0x80000000u,
           "the crash's time 0xffc00000 passes the first-key compare and converts to r8d = 0x80000000");
    expect(bucket_read(crash.bucket) == BUCKETS - 0x100000000ull,
           "movzx eax, word [rax+rcx*2] with rcx = movsxd r8d reads 4 GB below the bucket table");
    Sampled guarded_crash = game_sample(crash_track, apt::sampler_time(crash_time).time);
    expect(guarded_crash.path == Path::first_key, "with the guard's time the crash's track takes its first-key exit");

    expect(apt::TYPE_COUNT == 6, "the guard wraps the samplers of six packet types");
    expect(std::string(apt::TYPES[0]) == "B1", "the faulting B1 packet is the first type the guard names");

    const float nan = from_bits(0x7fc00000u);
    const std::vector<float> not_finite = {nan, crash_time, from_bits(0x7f800001u), INFINITY, -INFINITY};
    bool not_finite_replaced = true;
    for (float x : not_finite) {
        apt::Time t = apt::sampler_time(x);
        not_finite_replaced = not_finite_replaced && t.replaced && to_bits(t.time) == to_bits(-FLT_MAX);
    }
    expect(not_finite_replaced, "every time that is not a finite number is replaced by the lowest float");
    bool finite_untouched = true;
    for (float x : {-FLT_MAX, -5.f, -0.f, 0.f, FLT_TRUE_MIN, 0.01f, 0.5f, 1.f, 2.f, 10.f, 100.f, 3e9f, FLT_MAX}) {
        apt::Time t = apt::sampler_time(x);
        finite_untouched = finite_untouched && !t.replaced && to_bits(t.time) == to_bits(x);
    }
    expect(finite_untouched, "every finite time reaches the sampler bit for bit and is not counted");

    const std::vector<Track> tracks = {{3, 0.f, 30.f, 4}, {12, 0.f, 1.f, 4},  {64, -1.f, 60.f, 128},
                                       {4, 2.f, 0.5f, 2}, {3, 0.f, 0.f, 3},   {2, 0.f, 30.f, 0},
                                       {2, 1.f, 0.f, 0},  {2, -2.f, 0.5f, 0}, {1, 0.f, 30.f, 0}};
    bool nan_reads_outside = true, inf_reads_outside = true, two_keys_nan_weight = true;
    bool guarded_first = true, guarded_inside = true, finite_same = true;
    for (auto &t : tracks) {
        Sampled s = game_sample(t, nan);
        if (t.keys >= 3)
            nan_reads_outside = nan_reads_outside && s.path == Path::bucket && s.bucket == INT32_MIN;
        if (t.keys == 2 && t.rate != 0.f)
            two_keys_nan_weight = two_keys_nan_weight && std::isnan(s.weight);
        s = game_sample(t, INFINITY);
        if (t.keys >= 3)
            inf_reads_outside = inf_reads_outside && s.path == Path::bucket && s.bucket == INT32_MIN;
        for (float x : not_finite) {
            Sampled g = game_sample(t, apt::sampler_time(x).time);
            guarded_first = guarded_first && shows_first_key(g);
            guarded_inside = guarded_inside && reads_inside(t, g);
        }
        for (float x : {-FLT_MAX, -5.f, -0.f, 0.f, 0.01f, 0.5f, 1.f, 2.f, 3.f, 10.f, 100.f}) {
            Sampled g = game_sample(t, apt::sampler_time(x).time), game = game_sample(t, x);
            finite_same = finite_same && same(g, game);
            guarded_inside = guarded_inside && reads_inside(t, g);
        }
    }
    expect(nan_reads_outside, "a track of 3 or more keys indexes its bucket table at 0x80000000 for a time that is not a number");
    expect(inf_reads_outside, "a track of 3 or more keys indexes its bucket table at 0x80000000 for plus infinity");
    expect(two_keys_nan_weight, "a 2-key track blends at a weight that is not a number for a time that is not a number");
    expect(guarded_first, "with the guard's time every track shows its first key for every non-finite time");
    expect(guarded_inside, "with the guard's time every track reads inside its bucket table");
    expect(finite_same, "every finite time takes the same path, bucket and weight as without the guard");

    const std::vector<Packet> ui = {{true, {{12, 0.f, 30.f, 6}, {2, 0.f, 30.f, 0}}}, {false, {}}, {true, {{3, 0.f, 1.f, 3}}}};
    expect(!all_inside(ui, ui_player(ui, crash_time, Guard::none)),
           "the UI animation player faults sampling its packets at a time that is not a number without a guard");
    expect(!all_inside(ui, ui_player(ui, crash_time, Guard::clip_thunk)),
           "the clip thunk guard does not cover the UI animation player, which calls the packets itself");
    std::vector<Sampled> guarded_ui = ui_player(ui, crash_time, Guard::packets);
    expect(all_inside(ui, guarded_ui) && all_first_key(guarded_ui),
           "with the packet guard the UI animation player's packets show their first key");
    expect(all_inside(ui, clip_player(ui, crash_time, Guard::clip_thunk)), "the clip thunk guard still covers the clip player");
    std::vector<Sampled> both = clip_player(ui, crash_time, Guard::packets);
    expect(all_inside(ui, both) && all_first_key(both), "both guards together leave the clip player at its first key");
    bool ui_finite_same = true;
    for (float x : {0.f, 0.25f, 1.f, 2.5f}) {
        std::vector<Sampled> a = ui_player(ui, x, Guard::packets), b = ui_player(ui, x, Guard::none);
        for (size_t i = 0; i < a.size(); i++)
            ui_finite_same = ui_finite_same && same(a[i], b[i]);
    }
    expect(ui_finite_same, "the UI animation player samples every finite time as the game does");

    expect(apt::offset_in(0x1441fb7f6, 0x140000000) == 0x41fb7f6, "a caller in the game is logged as its offset");
    expect(apt::offset_in(0x1000, 0x140000000) == 0x1000, "a caller below the game's base is logged as it is");

    swofty_core::guards::LogFirst<apt::LOG_FIRST> r{"replacements"};
    int lines = 0, last = 0;
    for (int i = 0; i < 500; i++) {
        auto line = r.note();
        if (line)
            lines++;
        if (line && *line.tail)
            last++;
    }
    expect(lines == apt::LOG_FIRST, "five hundred replacements produce exactly the capped number of lines");
    expect(last == 1, "the capped line is marked once");
    expect(r.count() == 500, "every replacement is counted");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
