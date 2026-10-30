// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cmath>
#include <cstdio>
#include <cstring>

#include "intro_video.hpp"

namespace iv = f1990::intro;

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static void put32(uint8_t *p, size_t off, uint32_t v)
{
    std::memcpy(p + off, &v, sizeof v);
}

static void header(uint8_t (&h)[iv::kBinkHeaderSize], const char *sig, uint32_t frames, uint32_t w, uint32_t hgt,
                   uint32_t num, uint32_t den)
{
    std::memset(h, 0, sizeof h);
    std::memcpy(h, sig, 4);
    put32(h, 0x04, 0x04695e5c);
    put32(h, 0x08, frames);
    put32(h, 0x0c, 0x32f8c);
    put32(h, 0x10, frames);
    put32(h, 0x14, w);
    put32(h, 0x18, hgt);
    put32(h, 0x1c, num);
    put32(h, 0x20, den);
}

static bool near(double a, double b)
{
    return std::fabs(a - b) < 1e-3;
}

int main()
{
    uint8_t h[iv::kBinkHeaderSize];

    header(h, "BIKi", 1099, 1280, 720, 2997, 100);
    auto bik = iv::parse_header(h, sizeof h);
    check("reads a Bink 1 header", bik && bik->version == 1 && bik->revision == 'i');
    check("reads the frame count and size", bik && bik->frames == 1099 && bik->width == 1280 && bik->height == 720);
    check("reads the frame rate", bik && near(iv::fps(*bik), 29.97));
    check("works out the length", bik && near(iv::seconds(*bik), 1099 / 29.97));
    check("plays a Bink 1 file named .bik", iv::check(bik, 1) == iv::Verdict::play);
    check("times out after the length plus the margin", bik && near(iv::timeout(*bik), 1099 / 29.97 + iv::kTimeoutMargin));

    const uint8_t sting[iv::kBinkHeaderSize] = {
        0x4b, 0x42, 0x32, 0x6a, 0x6c, 0x3d, 0x10, 0x01, 0x63, 0x01, 0x00, 0x00, 0xcc, 0x93, 0x03, 0x00, 0x63, 0x01,
        0x00, 0x00, 0x80, 0x07, 0x00, 0x00, 0x38, 0x04, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    };
    auto bk2 = iv::parse_header(sting, sizeof sting);
    check("reads the stock cm_f1_sting.bk2 header as Bink 2", bk2 && bk2->version == 2 && bk2->revision == 'j');
    check("cm_f1_sting.bk2 is 1920x1080, 355 frames at 30 fps",
          bk2 && bk2->width == 1920 && bk2->height == 1080 && bk2->frames == 355 && near(iv::fps(*bk2), 30.0));
    check("refuses a Bink 2 file named .bik", iv::check(bk2, 1) == iv::Verdict::wrong_version);

    header(h, "RIFF", 1099, 1280, 720, 2997, 100);
    check("refuses a file that is not Bink", iv::check(iv::parse_header(h, sizeof h), 1) == iv::Verdict::not_bink);
    header(h, "BIKi", 0, 1280, 720, 2997, 100);
    check("refuses a header without frames", !iv::parse_header(h, sizeof h));
    header(h, "BIKi", 1099, 1280, 720, 2997, 0);
    check("refuses a zero frame-rate denominator", !iv::parse_header(h, sizeof h));
    header(h, "BIKi", 1099, 1280, 720, 2997, 100);
    check("refuses a short read", !iv::parse_header(h, sizeof h - 1));
    check("refuses a null buffer", !iv::parse_header(nullptr, sizeof h));
    header(h, "BIKi", 30 * 601, 1280, 720, 30, 1);
    check("refuses a video over the cap", iv::check(iv::parse_header(h, sizeof h), 1) == iv::Verdict::too_long);

    check("stays while the video plays", iv::step(false, 10.0, 51.7) == iv::Step::stay);
    check("continues when the end callback ran", iv::step(true, 10.0, 51.7) == iv::Step::ended);
    check("continues on timeout without the callback", iv::step(false, 51.7, 51.7) == iv::Step::timed_out);
    check("the end callback wins over the timeout", iv::step(true, 99.0, 51.7) == iv::Step::ended);
    check("an end well before the length is a skip", bik && iv::ended_early(5.0, *bik));
    check("an end at the length is not a skip", bik && !iv::ended_early(36.5, *bik));

    std::printf("%s\n", failures ? "intro_video_test: FAILED" : "intro_video_test: all passed");
    return failures ? 1 : 0;
}
