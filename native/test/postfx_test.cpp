#include "postfx_logic.hpp"

#include <cstdio>
#include <cstdlib>

using namespace postfx;

static int failed;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #c);                                                   \
            failed++;                                                                                                  \
        }                                                                                                              \
    } while (0)

int main()
{
    When any;
    CHECK(!ineligible(any, UNKNOWN, 0, false));
    CHECK(!ineligible(any, CHASE_FAR, PAUSED | REPLAY, true));

    When driving{view_bit("cockpit") | view_bit("tcam") | view_bit("tcam_offset"),
                 flag_bit("paused") | flag_bit("replay") | flag_bit("photo") | flag_bit("menu") | flag_bit("cutscene")};
    CHECK(!ineligible(driving, COCKPIT, 0, true));
    CHECK(!ineligible(driving, TCAM_OFFSET, SPECTATING, true));
    CHECK(ineligible(driving, COCKPIT, 0, false));
    CHECK(ineligible(driving, CHASE_NEAR, 0, true));
    CHECK(ineligible(driving, COCKPIT, PAUSED, true));
    CHECK(ineligible(driving, TCAM, REPLAY | MENU, true));
    CHECK(ineligible(driving, UNKNOWN, 0, true));

    When no_pause{0, flag_bit("paused")};
    CHECK(!ineligible(no_pause, UNKNOWN, PAUSED, false));
    CHECK(ineligible(no_pause, OTHER, PAUSED, true));

    CHECK(view_bit("nope") == 0 && flag_bit("nope") == 0 && view_bit("unknown") == 0);
    CHECK(flag_text(0) == "-" && flag_text(PAUSED | MENU) == "paused+menu");
    CHECK(view_text(TCAM) == "tcam" && view_text(99) == "?");

    CHECK(key_code("B") == 'B' && key_code("b") == 'B' && key_code("7") == '7');
    CHECK(key_code("F1") == 0x70 && key_code("F12") == 0x7B && key_code("F24") == 0x87);
    CHECK(key_code("F25") == 0 && key_code("F0") == 0 && key_code("Fx") == 0);
    CHECK(key_code("Page Up") == 0x21 && key_code("Insert") == 0x2D && key_code("None") == 0 && key_code("") == 0);

    CHECK(format_of("rgba8") == 28 && format_of("rgba16f") == 10 && format_of("r11g11b10f") == 26);
    CHECK(format_of("rgb10a2") == 24 && format_of("r32f") == 41 && format_of("bgra8") == 0);

    Size half = texture_size(0.5, 0, 0, 1908, 1028);
    CHECK(half.w == 954 && half.h == 514);
    Size third = texture_size(1.0 / 3, 0, 0, 1921, 1081);
    CHECK(third.w == 640 && third.h == 360);
    Size tiny = texture_size(0.0001, 0, 0, 1920, 1080);
    CHECK(tiny.w == 1 && tiny.h == 1);
    Size fixed = texture_size(0.5, 256, 128, 1920, 1080);
    CHECK(fixed.w == 256 && fixed.h == 128);

    float v = -1;
    CHECK(option_value("true", v) && v == 1);
    CHECK(option_value("false", v) && v == 0);
    CHECK(option_value("35.000000", v) && v == 35.0f);
    CHECK(option_value("-2.5", v) && v == -2.5f);
    v = 7;
    CHECK(!option_value("B", v) && v == 7);
    CHECK(!option_value("", v) && !option_value("3x", v));

    CHECK(groups(1920, 8) == 240 && groups(1921, 8) == 241 && groups(5, 16) == 1 && groups(5, 0) == 0);

    struct Res {
        int refs = 1;
        void AddRef() { refs++; }
        void Release() { refs--; }
    };
    Res depth, rebuilt;
    Res *back_use = nullptr, *slot = nullptr;
    hold(back_use, &depth);
    hold(back_use, &depth);
    CHECK(back_use == &depth && depth.refs == 2);
    depth.Release();
    CHECK(depth.refs == 1);
    hold(slot, back_use);
    CHECK(depth.refs == 2);
    hold(back_use, nullptr);
    CHECK(back_use == nullptr && depth.refs == 1);
    hold(slot, &rebuilt);
    CHECK(slot == &rebuilt && depth.refs == 0 && rebuilt.refs == 2);
    hold(slot, nullptr);
    CHECK(rebuilt.refs == 1);

    Written<Res, int> view;
    CHECK(rewrite(view, &depth, 46));
    CHECK(!rewrite(view, &depth, 46));
    CHECK(rewrite(view, &depth, 41) && view.f == 41);
    CHECK(rewrite(view, &rebuilt, 41) && view.r == &rebuilt);
    CHECK(rewrite(view, static_cast<Res *>(nullptr), 41) && !rewrite(view, static_cast<Res *>(nullptr), 41));
    view.set = false;
    CHECK(rewrite(view, static_cast<Res *>(nullptr), 41));

    if (failed)
        return 1;
    std::printf("postfx logic: all checks passed\n");
    return 0;
}
