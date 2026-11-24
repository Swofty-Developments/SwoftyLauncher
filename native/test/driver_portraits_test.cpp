#include "core/render/portraits/portrait_data.hpp"
#include "core/render/readback/pixels.hpp"

#include <cstdio>
#include <vector>

using namespace render::portraits;

namespace {

int failures = 0;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

void names_the_pool_texture()
{
    bool srgb = true;
    CHECK(data::resource("eaid://game_rtt_tex_0012", &srgb) == "eaid://game_rtt_tex_0012" && !srgb);
    CHECK(data::resource("eaid://game_rtt_tex_0012.srv", &srgb) == "eaid://game_rtt_tex_0012" && !srgb);
    CHECK(data::resource("eaid://game_rtt_tex_0999_srgb.srv", &srgb) == "eaid://game_rtt_tex_0999" && srgb);
    CHECK(data::resource("eaid://game_rtt_tex_0000.image") == "eaid://game_rtt_tex_0000");
    CHECK(data::resource("D3D eaid://game_rtt_tex_0003") == "eaid://game_rtt_tex_0003");
    CHECK(!data::resource("eaid://game_rtt_tex_12"));
    CHECK(!data::resource("eaid://game_rtt_tex_00123"));
    CHECK(!data::resource("eaid://game_rtt_tex_0012.rtv"));
    CHECK(!data::resource("eaid://game_rtt_tex_00a2"));
    CHECK(!data::resource("xeaid://game_rtt_tex_0012"));
    CHECK(!data::resource("eaid://ego_package/rtt_mainRenderTarget_600_800"));
    CHECK(!data::resource(""));
}

void builds_the_view_model_path()
{
    auto p = data::path(0, 17, 0);
    CHECK(p && std::string(p->list) == "preloaded_drivers" && p->index == 17 && std::string(p->set) == "GridRundownLeft");
    p = data::path(1, 0, 1);
    CHECK(p && std::string(p->list) == "custom_drivers" && std::string(p->set) == "GridRundownRight");
    p = data::path(2, 3, 2);
    CHECK(p && std::string(p->list) == "network_drivers" && std::string(p->set) == "Broadcast");
    CHECK(!data::path(3, 0, 0) && !data::path(0, 0x3fff, 0) && !data::path(0, 1, 3));
}

void accepts_the_game_shapes_and_formats()
{
    CHECK(data::game_shape(600, 800) && data::game_shape(400, 400) && !data::game_shape(800, 600) && !data::game_shape(64, 64));
    bool bgra = true;
    CHECK(data::format(27, bgra) && !bgra);
    CHECK(data::format(28, bgra) && !bgra && data::format(29, bgra) && !bgra);
    CHECK(data::format(87, bgra) && bgra && data::format(90, bgra) && bgra && data::format(91, bgra) && bgra);
    CHECK(!data::format(24, bgra) && !data::format(10, bgra) && !data::format(98, bgra) && !data::format(26, bgra));
}

void copies_and_scales_rows()
{
    const unsigned w = 600, h = 800, pitch = 2560;
    std::vector<uint8_t> rows(size_t(pitch) * h, 0xee);
    for (unsigned y = 0; y < h; y++)
        for (unsigned x = 0; x < w; x++) {
            uint8_t *p = rows.data() + size_t(y) * pitch + x * 4;
            p[0] = uint8_t(x);
            p[1] = uint8_t(y);
            p[2] = 200;
            p[3] = x < 300 ? 0 : 255;
        }
    unsigned ow = 0, oh = 0;
    auto full = render::readback::fit(rows.data(), w, h, pitch, false, data::MAX_SIDE, data::MAX_SIDE, ow, oh);
    CHECK(ow == 600 && oh == 800 && full.size() == size_t(600) * 800 * 4);
    CHECK(full[0] == 0 && full[3] == 0 && full[(size_t(799) * 600 + 599) * 4] == uint8_t(599) &&
          full[(size_t(799) * 600 + 599) * 4 + 1] == uint8_t(799) && full[(size_t(799) * 600 + 599) * 4 + 3] == 255);
    auto swapped = render::readback::fit(rows.data(), w, h, pitch, true, data::MAX_SIDE, data::MAX_SIDE, ow, oh);
    CHECK(swapped[0] == 200 && swapped[2] == 0 && swapped[(size_t(10) * 600 + 7) * 4 + 2] == 7);
    auto small = render::readback::fit(full.data(), ow, oh, ow * 4, false, data::MAX_SIDE, 64, ow, oh);
    CHECK(ow == 48 && oh == 64 && small.size() == size_t(48) * 64 * 4 && small.size() <= 16384);
    CHECK(small[3] == 0 && small[(47) * 4 + 3] == 255);
    CHECK(render::readback::fit(rows.data(), 1025, 4, 8192, false, data::MAX_SIDE, 64, ow, oh).empty());
    CHECK(render::readback::fit(rows.data(), 600, 800, 2399, false, data::MAX_SIDE, 64, ow, oh).empty());
}

}

int main()
{
    names_the_pool_texture();
    builds_the_view_model_path();
    accepts_the_game_shapes_and_formats();
    copies_and_scales_rows();
    if (failures)
        return 1;
    std::printf("driver portraits: ok\n");
    return 0;
}
