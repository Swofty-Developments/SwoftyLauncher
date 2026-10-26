// Host-native unit test for the portable overlay draw-list core (native/src/core/hud_drawlist.hpp).
// The D3D12 present hook and GPU renderer can only run in game; this covers everything that does
// not: the draw-list building, the per-addon paint registration and token lifecycle, text
// measurement and the image id registry. Built and run by native/test/run_hud_test.sh.

#include "hud_drawlist.hpp"
#include "hud_guard.hpp"
#include "hud_png.hpp"

#include <cstddef>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

static int failures;

#define CHECK(cond, msg)                                                                                                \
    do {                                                                                                                \
        if (cond) {                                                                                                     \
            printf("ok  : %s\n", msg);                                                                                  \
        } else {                                                                                                        \
            printf("FAIL: %s\n", msg);                                                                                  \
            failures++;                                                                                                 \
        }                                                                                                               \
    } while (0)

static void paint_box(SwoftyPaint *paint, void *user)
{
    auto &f = *reinterpret_cast<hud::Frame *>(paint);
    int tag = int(reinterpret_cast<intptr_t>(user));
    hud::rect(f, 10, 20, 30, 40, 0x112233ff);
    hud::text(f, 1, 2, 16, 0xffffffff, tag == 7 ? "seven" : "x");
}

static void paint_nothing(SwoftyPaint *paint, void *user)
{
    (void)paint;
    (void)user;
}

static void test_measure()
{
    CHECK(hud::glyphs("abc") == 3, "glyphs counts ascii");
    CHECK(hud::glyphs("") == 0, "glyphs of empty is zero");
    // "café" is 5 bytes (é = 2 bytes) but 4 codepoints.
    CHECK(hud::glyphs("caf\xc3\xa9") == 4, "glyphs counts utf-8 codepoints, not bytes");
    float w = -1, h = -1;
    hud::measure(16, "abcd", &w, &h);
    CHECK(w == 4 * 16 * (hud::FONT_ADVANCE / hud::FONT_CELL), "measure width is glyphs * advance");
    CHECK(h == 16, "measure height is the font size");
    hud::measure(16, "", &w, &h);
    CHECK(w == 0 && h == 16, "measure of empty text is zero wide");
}

static void test_culling()
{
    hud::Frame f;
    hud::rect(f, 0, 0, 0, 10, 0xff0000ff);
    CHECK(f.cmds.empty(), "zero-width rect is dropped");
    hud::rect(f, 0, 0, 10, 10, 0x00000000);
    CHECK(f.cmds.empty(), "fully transparent rect is dropped");
    hud::line(f, 5, 5, 5, 5, 1, 0xffffffff);
    CHECK(f.cmds.empty(), "zero-length line is dropped");
    hud::text(f, 0, 0, 16, 0xffffffff, "");
    CHECK(f.cmds.empty(), "empty text is dropped");
    hud::rect_outline(f, 0, 0, 10, 10, 0, 0xffffffff);
    CHECK(f.cmds.empty(), "zero-thickness outline is dropped");
}

static void test_commands()
{
    hud::Frame f;
    hud::rect(f, 10, 20, 30, 40, 0x11223344);
    hud::line(f, 1, 2, 3, 4, 2.5f, 0xaabbccdd);
    hud::text(f, 7, 8, 18, 0xffffffff, "hi");
    hud::image(f, 3, 100, 100, 64, 64, 0x80808080);
    CHECK(f.cmds.size() == 4, "four visible draw calls appended four commands");
    CHECK(f.cmds[0].op == hud::Op::rect && f.cmds[0].x == 10 && f.cmds[0].h == 40 && f.cmds[0].color == 0x11223344,
          "rect command keeps its coordinates and colour");
    CHECK(f.cmds[1].op == hud::Op::line && f.cmds[1].thickness == 2.5f, "line command keeps its thickness");
    CHECK(f.cmds[2].op == hud::Op::text && f.cmds[2].text == "hi" && f.cmds[2].font_size == 18,
          "text command keeps its string and size");
    CHECK(f.cmds[3].op == hud::Op::image && f.cmds[3].image == 3 && f.cmds[3].color == 0x80808080,
          "image command keeps its slot and tint");
}

static void test_registry()
{
    hud::Registry reg;
    CHECK(reg.add(nullptr, nullptr, "a") == 0, "on_paint with a null callback yields no token");
    CHECK(reg.count() == 0, "a refused registration is not stored");
    uint32_t a = reg.add(paint_box, nullptr, "a");
    uint32_t b = reg.add(paint_nothing, nullptr, "b");
    CHECK(a != 0 && b != 0 && a != b, "two registrations get distinct non-zero tokens");
    CHECK(!reg.remove(9999), "removing an unknown token fails");
    CHECK(reg.remove(a), "removing a known token succeeds");
    CHECK(reg.count() == 1, "remove drops exactly one subscription");
    CHECK(!reg.remove(a), "a token cannot be removed twice");
}

static void test_build_dispatch()
{
    hud::Registry reg;
    reg.add(paint_box, reinterpret_cast<void *>(intptr_t(7)), "a");
    reg.add(paint_box, reinterpret_cast<void *>(intptr_t(1)), "b");
    hud::Frame f;
    hud::build(reg.subs(), f, 1920, 1080);
    CHECK(f.width == 1920 && f.height == 1080, "build records the drawable size for size()");
    CHECK(f.cmds.size() == 4, "both callbacks ran and each drew a rect and text");
    CHECK(f.cmds[1].op == hud::Op::text && f.cmds[1].text == "seven",
          "the first subscriber's user pointer reached its callback");
    CHECK(f.cmds[3].text == "x", "subscribers run in registration order");
    // A second frame starts clean.
    hud::build(reg.subs(), f, 800, 600);
    CHECK(f.cmds.size() == 4 && f.width == 800, "each frame rebuilds the command list from scratch");
}

static void test_image_registry()
{
    hud::Registry reg;
    int first = reg.intern_image("logo");
    int again = reg.intern_image("logo");
    int other = reg.intern_image("flag");
    CHECK(first == 0 && again == 0, "interning the same id twice returns the same slot");
    CHECK(other == 1, "a new id gets a new slot");
    CHECK(reg.find_image("logo") == 0, "find_image returns a registered slot");
    CHECK(reg.find_image("missing") == -1, "find_image returns -1 for an unknown id");
    CHECK(reg.image_count() == 2, "two distinct images registered");
}

struct Box {
    float x, y, w, h;
    bool operator==(const Box &) const = default;
};

static std::vector<Box> boxes(const char *s, float size = 16)
{
    std::vector<Box> out;
    hud::glyph_rects(100, 50, size, s, [&](float x, float y, float w, float h) { out.push_back({x, y, w, h}); });
    return out;
}

static float area(const std::vector<Box> &b)
{
    float a = 0;
    for (auto &r : b)
        a += r.w * r.h;
    return a;
}

static int lit(const char *s)
{
    int n = 0;
    for (uint8_t row : hud::font::GLYPHS[s[0] - 0x20])
        for (int col = 0; col < 8; col++)
            n += row >> col & 1;
    return n;
}

static void test_glyphs()
{
    CHECK((boxes("|") == std::vector<Box>{{104, 52, 2, 12}}), "a bar is one block, two font pixels per pixel at size 16");
    CHECK(boxes("!").size() == 2, "a gap splits a column into two blocks");
    CHECK(boxes(" ").empty(), "a space draws nothing");
    auto bars = boxes("||");
    CHECK(bars.size() == 2 && bars[1].x == 104 + hud::FONT_ADVANCE * 2, "the second glyph starts one advance along");
    for (const char *c : {"A", "g", "W", "@", "8"})
        CHECK(area(boxes(c, 8)) == float(lit(c)), "the blocks cover each lit pixel once");
    CHECK(boxes("\xe4\xb8\xad") == boxes("?"), "a codepoint the font lacks draws as ?");
    CHECK(boxes("\xc3\xa9") != boxes("?") && !boxes("\xc3\xa9").empty(), "latin-1 letters such as e acute draw");
    CHECK(boxes("\xc3") == boxes("?"), "a cut-off sequence draws as ?");
    CHECK(hud::glyphs("caf\xc3\xa9") == 4, "one glyph per codepoint, as measure counts");
}

static void test_guard()
{
    using namespace hud::guard;
    CHECK(fg_module("sl.dlss_g.dll") && fg_module("C:\\Games\\F1 25\\NVNGX_DLSSG.DLL"), "dlss frame generation modules count");
    CHECK(fg_module("amd_fidelityfx_dx12.dll") && fg_module("libxess_fg.dll"), "fsr and xess frame generation modules count");
    CHECK(!fg_module("sl.interposer.dll") && !fg_module("nvngx_dlss.dll") && !fg_module("F1_25.exe"),
          "the interposer, dlss super resolution and the game do not");
    CHECK(fg_in({"swofty_core.dll", "sl.interposer.dll", "F1_25.exe"}).empty(), "a plain creation stack has no frame generation");
    CHECK(fg_in({"swofty_core.dll", "nvngx_dlssg.dll", "sl.dlss_g.dll", "F1_25.exe"}) == "nvngx_dlssg.dll",
          "the innermost frame generation module is named");
    CHECK(fg_in({"sl.dlss_g.dll", "sl.interposer.dll", "F1_25.exe"}).empty() &&
              streamline_in({"sl.dlss_g.dll", "sl.interposer.dll", "F1_25.exe"}),
          "streamline's frame generation hook alone is not a frame generation swapchain");
    std::string hw = "<x>\n\t<antialiasing taa=\"true\" dlss=\"false\" />\n\t<frame_gen mode=\"0\" />\n\t"
                     "<multi_frame_gen value=\"0\" />\n</x>";
    FgSetting fs = fg_setting(hw);
    CHECK(fs.known && fs.mode == 0 && fs.multi == 0 && !fs.on(), "frame_gen mode 0 reads as off");
    fs = fg_setting("<frame_gen mode=\"1\" /><multi_frame_gen value=\"2\" />");
    CHECK(fs.known && fs.mode == 1 && fs.multi == 2 && fs.on(), "frame_gen mode 1 reads as on");
    CHECK(fg_setting("<frame_gen />").on() && fg_setting("").on(), "a missing frame_gen mode reads as on");
    CHECK(hdr_mode("<resolution>\n\t\t\t<hdr mode=\"1\" peakNits=\"1000\" />") == 1 && hdr_mode("<x/>") == -1,
          "the game's hdr mode reads from hardware settings");
    CHECK(output_for(24, 12, 0) == HDR10_PQ && output_for(24, 0, 1) == SDR && output_for(24, COLOR_SPACE_UNSET, 1) == HDR10_PQ &&
              output_for(24, COLOR_SPACE_UNSET, 0) == SDR && output_for(24, COLOR_SPACE_UNSET, -1) == SDR,
          "a 10-bit swapchain is HDR10 from SetColorSpace1, or from the game's hdr mode when no colour space was set");
    CHECK(output_for(10, COLOR_SPACE_UNSET, 0) == SCRGB && output_for(10, 12, 0) == HDR10_PQ && output_for(28, 12, 1) == SDR,
          "fp16 is scRGB unless PQ was set, and 8-bit is SDR");
    CHECK(chain({"swofty_core.dll", "dxgi.dll", "sl.interposer.dll", "sl.interposer.dll", "", "F1_25.exe", "KERNEL32.DLL"},
                "swofty_core.dll", "dxgi.dll") == "sl.interposer.dll <- F1_25.exe <- KERNEL32.DLL",
          "the caller chain drops the core, the implementation, repeats and unknown frames");
    CHECK(chain({"swofty_core.dll"}, "swofty_core.dll", "") == "unknown", "an empty chain reads unknown");
    CHECK(!removal_after_draw(5000, 0), "a removal with no overlay draw is not attributed");
    CHECK(removal_after_draw(15000, 10000), "a removal 5 s after a draw is attributed");
    CHECK(removal_after_draw(20000, 10000) && !removal_after_draw(20001, 10000), "the window is 10 s");
}

// Models d3d12hooks::exec_original: the hook sits on the native queue's ExecuteCommandLists (vtable
// slot 10) and its trampoline runs that body, which reads this as a native queue. Streamline's queue
// proxy (vtable, refcount, native queue at +0x10) is what a swapchain created through Streamline's
// factory records as its queue.
namespace dispatch {

constexpr size_t EXEC = 10;
constexpr uint32_t NATIVE = 0x51554555;

struct NativeQueue {
    const void *const *vt;
    uint32_t magic;
    unsigned executed;
};

struct ProxyQueue {
    const void *const *vt;
    uint32_t refs;
    NativeQueue *base;
};

using ExecFn = void (*)(void *self, unsigned n);

unsigned misread;

void native_exec(void *self, unsigned n)
{
    uint32_t magic;
    memcpy(&magic, static_cast<const char *>(self) + offsetof(NativeQueue, magic), sizeof magic);
    if (magic != NATIVE) {
        misread++;
        return;
    }
    static_cast<NativeQueue *>(self)->executed += n;
}

void through_vtable(void *self, unsigned n)
{
    reinterpret_cast<ExecFn>((*static_cast<const void *const *const *>(self))[EXEC])(self, n);
}

void proxy_exec(void *self, unsigned n)
{
    through_vtable(static_cast<ProxyQueue *>(self)->base, n);
}

void other_hook(void *self, unsigned n)
{
    native_exec(self, n);
}

const void *hooked()
{
    return reinterpret_cast<const void *>(&native_exec);
}

void exec_original(void *q, unsigned n)
{
    if (hud::guard::vtable_slot_is(q, EXEC, hooked()))
        native_exec(q, n);
    else
        through_vtable(q, n);
}

}

static void test_original_dispatch()
{
    using namespace dispatch;
    const void *native_vt[EXEC + 1] = {}, *proxy_vt[EXEC + 1] = {}, *rehooked_vt[EXEC + 1] = {};
    native_vt[EXEC] = hooked();
    proxy_vt[EXEC] = reinterpret_cast<const void *>(&proxy_exec);
    rehooked_vt[EXEC] = reinterpret_cast<const void *>(&other_hook);
    NativeQueue game{native_vt, NATIVE, 0}, rehooked{rehooked_vt, NATIVE, 0};
    ProxyQueue proxy{proxy_vt, 8, &game};

    CHECK(hud::guard::vtable_slot_is(&game, EXEC, hooked()), "a native queue runs the hooked body");
    CHECK(!hud::guard::vtable_slot_is(&proxy, EXEC, hooked()), "streamline's queue proxy does not");
    CHECK(!hud::guard::vtable_slot_is(&rehooked, EXEC, hooked()), "a queue whose slot another hook moved does not");
    CHECK(!hud::guard::vtable_slot_is(nullptr, EXEC, hooked()) && !hud::guard::vtable_slot_is(&game, EXEC, nullptr),
          "no object or no hook is never the hooked body");

    misread = 0;
    native_exec(&proxy, 1);
    CHECK(misread == 1 && game.executed == 0,
          "the hooked body given the proxy reads the proxy as a queue (the crash at Present)");

    misread = 0;
    exec_original(&game, 1);
    exec_original(&proxy, 2);
    exec_original(&rehooked, 4);
    CHECK(misread == 0, "exec_original never hands the hooked body anything but a native queue");
    CHECK(game.executed == 3, "the proxy's lists reach the native queue behind it");
    CHECK(rehooked.executed == 4, "a queue hooked by someone else still runs, through its vtable");
}

static void test_png(const char *out)
{
    using namespace hud::png;
    uint32_t w = 97, h = 61;
    std::vector<uint8_t> px(size_t(w) * h * 4);
    for (uint32_t y = 0; y < h; y++)
        for (uint32_t x = 0; x < w; x++) {
            uint8_t *p = &px[(size_t(y) * w + x) * 4];
            p[0] = uint8_t(x * 2), p[1] = uint8_t(y * 4), p[2] = uint8_t((x / 8 + y / 8) % 2 ? 255 : 0), p[3] = 255;
        }
    std::vector<uint8_t> rgb;
    CHECK(to_rgb(Fmt::rgba8, px.data(), w * 4, w, h, rgb) && rgb.size() == size_t(w) * h * 3 && rgb[3] == 2 &&
              rgb[size_t(w) * 3 + 1] == 4,
          "rgba8 rows convert to rgb");
    std::vector<uint8_t> bgra(px);
    for (size_t i = 0; i < bgra.size(); i += 4)
        std::swap(bgra[i], bgra[i + 2]);
    std::vector<uint8_t> rgb2;
    CHECK(to_rgb(Fmt::bgra8, bgra.data(), w * 4, w, h, rgb2) && rgb2 == rgb, "bgra8 converts to the same rgb");
    uint32_t v = 1023u | (512u << 10) | (3u << 30);
    std::vector<uint8_t> one;
    CHECK(to_rgb(Fmt::rgb10a2, reinterpret_cast<uint8_t *>(&v), 4, 1, 1, one) && one[0] == 255 && one[1] == 128 &&
              one[2] == 0,
          "r10g10b10a2 keeps the top 8 bits");
    uint16_t hf[4] = {0x3c00, 0x3800, 0x0000, 0x3c00};
    CHECK(to_rgb(Fmt::rgba16f, reinterpret_cast<uint8_t *>(hf), 8, 1, 1, one) && one[0] == 255 && one[1] == 188 &&
              one[2] == 0,
          "scRGB half floats encode to sRGB");
    CHECK(fmt_of(24) == Fmt::rgb10a2 && fmt_of(10) == Fmt::rgba16f && fmt_of(28) == Fmt::rgba8 &&
              fmt_of(87) == Fmt::bgra8 && fmt_of(2) == Fmt::unknown,
          "swapchain formats map to converters");
    std::string png = encode_rgb(rgb.data(), w, h);
    CHECK(png.compare(0, 8, std::string("\x89PNG\r\n\x1a\n", 8)) == 0 && png.find("IHDR") == 12 &&
              png.find("IEND") != std::string::npos,
          "the capture writes a PNG signature, IHDR and IEND");
    if (!out)
        return;
    if (FILE *f = fopen(out, "wb")) {
        fwrite(png.data(), 1, png.size(), f);
        fclose(f);
    }
    if (FILE *f = fopen((std::string(out) + ".rgb").c_str(), "wb")) {
        fwrite(rgb.data(), 1, rgb.size(), f);
        fclose(f);
    }
}

int main(int argc, char **argv)
{
    test_measure();
    test_glyphs();
    test_culling();
    test_commands();
    test_registry();
    test_build_dispatch();
    test_image_registry();
    test_guard();
    test_original_dispatch();
    test_png(argc > 1 ? argv[1] : nullptr);
    if (failures)
        printf("\n%d check(s) FAILED\n", failures);
    else
        printf("\nall hud draw-list checks passed\n");
    return failures ? 1 : 0;
}
