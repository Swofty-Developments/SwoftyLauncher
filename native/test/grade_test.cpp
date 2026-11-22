// Host-native unit test for the colour maths of shader looks (native/src/core/render/grade_math.hpp).
// The D3D12 hooks that read back and replace the game's tonemap LUT only run in game; this covers
// the packed LUT reader, the sRGB round trip, the trilinear lookup and the per-texel grade.
// Built and run by native/test/run_tests.sh grade.

#include "core/render/grade_math.hpp"

#include <cmath>
#include <cstdio>
#include <string>

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

static bool near(float a, float b, float eps = 1e-4f) { return std::fabs(a - b) <= eps; }

template <class F> static std::string packed(uint32_t size, F colour)
{
    std::string out("SLUT", 4);
    auto put = [&](const void *p) { out.append(static_cast<const char *>(p), 4); };
    uint32_t version = 1;
    float lo = 0, hi = 1;
    put(&version);
    put(&size);
    for (int i = 0; i < 3; i++)
        put(&lo);
    for (int i = 0; i < 3; i++)
        put(&hi);
    for (uint32_t b = 0; b < size; b++)
        for (uint32_t g = 0; g < size; g++)
            for (uint32_t r = 0; r < size; r++) {
                float in[3] = {float(r) / (size - 1), float(g) / (size - 1), float(b) / (size - 1)}, rgb[3];
                colour(in, rgb);
                for (float v : rgb)
                    put(&v);
            }
    return out;
}

static void identity(const float in[3], float out[3])
{
    for (int c = 0; c < 3; c++)
        out[c] = in[c];
}

static void grey(const float in[3], float out[3])
{
    float l = 0.2126f * in[0] + 0.7152f * in[1] + 0.0722f * in[2];
    out[0] = out[1] = out[2] = l;
}

static void test_unpack()
{
    render::grade::Lut l;
    std::string why;
    CHECK(render::grade::unpack(packed(3, identity), l, why) && l.size == 3 && l.table.size() == 81, "reads a packed 3^3 LUT");
    CHECK(near(l.table[(1 * 9 + 2 * 3 + 0) * 3 + 1], 1.0f), "entries run red fastest, then green, then blue");
    std::string bad = packed(3, identity);
    bad.pop_back();
    CHECK(!render::grade::unpack(bad, l, why) && why.find("bytes") != std::string::npos, "a short file says its size");
    CHECK(!render::grade::unpack("NOPE", l, why) && why == "it does not start with SLUT", "a wrong magic is refused");
    std::string flipped = packed(2, identity);
    float one = 1;
    memcpy(&flipped[12], &one, 4);
    CHECK(!render::grade::unpack(flipped, l, why), "a domain whose minimum is not below its maximum is refused");
}

static void test_srgb()
{
    bool round = true;
    for (float x = 0; x <= 1.0f; x += 0.01f)
        round &= near(render::grade::srgb_decode(render::grade::srgb_encode(x)), x);
    CHECK(round, "sRGB encode and decode round-trip");
    CHECK(near(render::grade::srgb_encode(0.5f), 0.7354f), "linear 0.5 encodes to sRGB 0.735");
}

static void test_sample_and_apply()
{
    render::grade::Lut id, mono;
    std::string why;
    render::grade::unpack(packed(17, identity), id, why);
    render::grade::unpack(packed(17, grey), mono, why);
    float in[3] = {0.3f, 0.6f, 0.9f}, out[3];
    render::grade::sample(id, in, out);
    CHECK(near(out[0], 0.3f) && near(out[1], 0.6f) && near(out[2], 0.9f), "the identity LUT samples back its input");
    bool same = true;
    for (float v : {0.0f, 0.02f, 0.18f, 0.5f, 1.0f, 1.132f}) {
        float rgb[3] = {v, v * 0.5f, v * 0.25f};
        render::grade::apply(id, 1.0f, rgb);
        same &= near(rgb[0], v, 2e-4f) && near(rgb[1], v * 0.5f, 2e-4f) && near(rgb[2], v * 0.25f, 2e-4f);
    }
    CHECK(same, "an identity look leaves every colour as it was, light above 1 included");
    float red[3] = {0.8f, 0.1f, 0.1f};
    render::grade::apply(mono, 1.0f, red);
    CHECK(near(red[0], red[1], 1e-3f) && near(red[1], red[2], 1e-3f), "a monochrome look greys a red");
    float half[3] = {0.8f, 0.1f, 0.1f};
    render::grade::apply(mono, 0.5f, half);
    CHECK(half[0] > red[0] && half[0] < 0.8f && half[1] < red[1], "strength 0.5 sits between the original and the look");
    float none[3] = {0.8f, 0.1f, 0.1f};
    render::grade::apply(mono, 0.0f, none);
    CHECK(near(none[0], 0.8f) && near(none[1], 0.1f), "strength 0 keeps the original");
}

static void test_half()
{
    bool round = true;
    for (float v : {0.0f, 1e-5f, 0.001f, 0.18f, 0.5f, 1.0f, 1.132f, 3.75f})
        round &= near(render::grade::unpack_half(render::grade::pack_half(v)), v, v * 1e-3f + 1e-7f);
    CHECK(round, "half floats round-trip display colours");
    CHECK(render::grade::pack_half(-1.0f) == 0 && render::grade::pack_half(1e9f) == 0x7bff,
          "negatives pack as 0, overflow as the largest half");
    CHECK(render::grade::unpack_half(0x3c00) == 1.0f && render::grade::unpack_half(0xbc00) == -1.0f,
          "1.0 and -1.0 unpack exactly");
}

int main()
{
    test_unpack();
    test_srgb();
    test_sample_and_apply();
    test_half();
    if (failures)
        printf("\n%d check(s) FAILED\n", failures);
    else
        printf("\nall shader look checks passed\n");
    return failures ? 1 : 0;
}
