// Host-native unit test for the core's tyre curve guard (native/src/core/guards/cars/tyre_curve_logic.hpp).
// Built and run by native/test/run_tests.sh tyre_curve.

#include "core/guards/cars/tyre_curve_logic.hpp"
#include "core/guards/skips.hpp"

#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <vector>

namespace tc = runtime::tyre_curve;

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

struct Bracket {
    int lo, hi;
};

// Car_Tyre_CurveMap_FindBracketA/B over the keys in order, with the head as -1: `ja` is an ordered greater, and the
// upper bound goes left on `jb`, which an unordered compare also takes.
static Bracket game_bracket(const std::vector<float> &keys, float x)
{
    int last = int(keys.size()) - 1;
    if (keys.size() == 1 || keys[0] > x)
        return {0, 0};
    if (x > keys[last])
        return {last, last};
    int first = 0, count = last;
    while (count > 0) {
        int half = count / 2, mid = first + half;
        if (!(x >= keys[mid])) {
            count = half;
        } else {
            first = mid + 1;
            count -= half + 1;
        }
    }
    return {first - 1, first};
}

int main()
{
    expect(tc::key_unordered(from_bits(0xffc00000u)), "the quiet NaN 0xffc00000 the crash reports carry is not a number");
    expect(tc::key_unordered(from_bits(0x7fc00000u)), "a positive quiet NaN is not a number");
    expect(tc::key_unordered(from_bits(0x7f800001u)), "a signalling NaN is not a number");
    expect(tc::key_unordered(std::numeric_limits<float>::quiet_NaN()), "quiet_NaN() is not a number");

    expect(!tc::key_unordered(0.f) && !tc::key_unordered(-0.f), "zero and minus zero are keys");
    expect(!tc::key_unordered(604.42f) && !tc::key_unordered(1.f), "a car mass and a lap fraction are keys");
    expect(!tc::key_unordered(FLT_MAX) && !tc::key_unordered(-FLT_MAX), "the largest floats are keys");
    expect(!tc::key_unordered(FLT_TRUE_MIN), "a subnormal is a key");
    expect(!tc::key_unordered(INFINITY) && !tc::key_unordered(-INFINITY), "infinities are keys");

    expect(to_bits(tc::lookup_key(from_bits(0xffc00000u))) == to_bits(-FLT_MAX),
           "a key that is not a number becomes the lowest float");
    expect(tc::lookup_key(604.42f) == 604.42f, "a key passes through unchanged");
    expect(to_bits(tc::lookup_key(-0.f)) == to_bits(-0.f), "minus zero passes through bit for bit");
    expect(tc::lookup_key(INFINITY) == INFINITY && tc::lookup_key(-INFINITY) == -INFINITY, "infinities pass through");
    expect(std::isfinite(tc::LOWEST_KEY) && tc::LOWEST_KEY < -3e38f, "the replacement key is the lowest finite float");

    const std::vector<std::vector<float>> trees = {{604.42f, 791.f, 901.f}, {0.f, 1.f}, {-5.f, 0.f, 2.f, 3.f, 10.f}, {7.f}};
    const float nan = from_bits(0xffc00000u);
    bool nan_reaches_head = true, guarded_takes_first = true, finite_stays_inside = true;
    for (auto &keys : trees) {
        Bracket b = game_bracket(keys, nan);
        if (keys.size() > 1)
            nan_reaches_head = nan_reaches_head && b.lo == -1 && b.hi == 0;
        b = game_bracket(keys, tc::lookup_key(nan));
        guarded_takes_first = guarded_takes_first && b.lo == 0 && b.hi == 0;
        for (float x : {-INFINITY, -FLT_MAX, -1e6f, -5.f, -0.f, 0.f, 0.5f, 1.f, 604.42f, 700.f, 901.f, 1e6f, FLT_MAX, INFINITY}) {
            b = game_bracket(keys, tc::lookup_key(x));
            finite_stays_inside = finite_stays_inside && b.lo >= 0 && b.hi >= b.lo && b.hi < int(keys.size());
        }
    }
    expect(nan_reaches_head, "the game's search hands back the head as the lower node for a key that is not a number");
    expect(guarded_takes_first, "with the guard's key the search returns the first node twice");
    expect(finite_stays_inside, "every number and infinity brackets inside the tree");

    swofty_core::guards::LogFirst<tc::LOG_FIRST> r{"replacements"};
    int lines = 0, last = 0;
    for (int i = 0; i < 500; i++) {
        auto line = r.note();
        if (line)
            lines++;
        if (line && *line.tail)
            last++;
    }
    expect(lines == tc::LOG_FIRST, "five hundred replacements produce exactly the capped number of lines");
    expect(last == 1, "the capped line is marked once");
    expect(r.count() == 500, "every replacement is counted");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
