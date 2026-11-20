// Host-native unit test for the core's anim clip time guard (native/src/core/guards/engine/anim_clip_time_logic.hpp).
// Built and run by native/test/run_tests.sh anim_clip_time.

#include "core/guards/engine/anim_clip_time_logic.hpp"
#include "core/guards/skips.hpp"

#include <cfloat>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <vector>

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

// vcvttss2si r32: truncation, and 0x80000000 for a value that is not a number or does not fit.
static int32_t cvtt(float x)
{
    if (std::isnan(x) || x >= 2147483648.f || x < -2147483648.f)
        return INT32_MIN;
    return int32_t(x);
}

struct Track {
    float first_key, rate;
    int buckets;
};

enum class Path { first_key, bucket, last_key };

struct Lookup {
    Path path;
    int32_t bucket;
};

// The keyframe samplers' index path (0x143a3d470 on Steam 26566308): `vucomiss first_key, time; jae` takes the first
// key, an unordered compare included in the not-taken side; the bucket index is cvtt((time - first_key) * rate), and the
// signed `cmp buckets-1, index; jle` sends only indexes at or past the last bucket to the last-key path.
static Lookup game_lookup(const Track &t, float time)
{
    if (!std::isnan(time) && t.first_key >= time)
        return {Path::first_key, 0};
    int32_t index = cvtt((time - t.first_key) * t.rate);
    if (t.buckets - 1 > index)
        return {Path::bucket, index};
    return {Path::last_key, index};
}

static bool reads_inside(const Track &t, const Lookup &l)
{
    return l.path != Path::bucket || (l.bucket >= 0 && l.bucket < t.buckets);
}

int main()
{
    expect(act::time_unordered(from_bits(0x7fc00000u)), "the quiet NaN 0x7fc00000 the crash reports carry is not a number");
    expect(act::time_unordered(from_bits(0xffc00000u)), "a negative quiet NaN is not a number");
    expect(act::time_unordered(from_bits(0x7f800001u)), "a signalling NaN is not a number");
    expect(act::time_unordered(std::numeric_limits<float>::quiet_NaN()), "quiet_NaN() is not a number");

    expect(!act::time_unordered(0.f) && !act::time_unordered(-0.f), "zero and minus zero are times");
    expect(!act::time_unordered(0.5f) && !act::time_unordered(2.25f), "clip times are times");
    expect(!act::time_unordered(FLT_MAX) && !act::time_unordered(-FLT_MAX), "the largest floats are times");
    expect(!act::time_unordered(INFINITY) && !act::time_unordered(-INFINITY), "infinities are times");

    expect(to_bits(act::sample_time(from_bits(0x7fc00000u))) == to_bits(-FLT_MAX),
           "a time that is not a number becomes the lowest float");
    expect(act::sample_time(1.5f) == 1.5f, "a time passes through unchanged");
    expect(to_bits(act::sample_time(-0.f)) == to_bits(-0.f), "minus zero passes through bit for bit");
    expect(act::sample_time(INFINITY) == INFINITY && act::sample_time(-INFINITY) == -INFINITY, "infinities pass through");
    expect(std::isfinite(act::EARLIEST_TIME) && act::EARLIEST_TIME < -3e38f, "the replacement time is the lowest finite float");

    const std::vector<Track> tracks = {{0.f, 30.f, 4}, {0.f, 1.f, 4}, {-1.f, 60.f, 128}, {2.f, 0.5f, 2}, {0.f, 0.f, 3}};
    const float nan = from_bits(0x7fc00000u);
    bool nan_reads_outside = true, guarded_takes_first = true, clip_times_stay_inside = true;
    for (auto &t : tracks) {
        Lookup l = game_lookup(t, nan);
        nan_reads_outside = nan_reads_outside && l.path == Path::bucket && l.bucket == INT32_MIN;
        l = game_lookup(t, act::sample_time(nan));
        guarded_takes_first = guarded_takes_first && l.path == Path::first_key;
        for (float x : {-INFINITY, -FLT_MAX, -5.f, -0.f, 0.f, 0.01f, 0.5f, 1.f, 2.f, 3.f, 10.f, 100.f}) {
            l = game_lookup(t, act::sample_time(x));
            clip_times_stay_inside = clip_times_stay_inside && reads_inside(t, l);
        }
    }
    expect(nan_reads_outside, "the game's samplers index the bucket table at 0x80000000 for a time that is not a number");
    expect(guarded_takes_first, "with the guard's time the samplers take their first-key exit");
    expect(clip_times_stay_inside, "every clip time and minus infinity indexes inside the bucket table");

    swofty_core::guards::LogFirst<act::LOG_FIRST> r{"replacements"};
    int lines = 0, last = 0;
    for (int i = 0; i < 500; i++) {
        auto line = r.note();
        if (line)
            lines++;
        if (line && *line.tail)
            last++;
    }
    expect(lines == act::LOG_FIRST, "five hundred replacements produce exactly the capped number of lines");
    expect(last == 1, "the capped line is marked once");
    expect(r.count() == 500, "every replacement is counted");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
