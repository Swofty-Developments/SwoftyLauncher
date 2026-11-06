#include "postfx_logic.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <utility>

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

    CHECK(mip_levels(4, 4, 3) == 3 && mip_levels(4, 4, 9) == 3 && mip_levels(4, 4, 0) == 1 && mip_levels(4, 4, 1) == 1);
    CHECK(mip_levels(1920, 1080, 12) == 11 && mip_levels(2048, 1, 99) == 12 && mip_levels(1 << 20, 1, 99) == 12);
    CHECK(mip_levels(1, 1, 5) == 1 && mip_levels(5, 3, 9) == 3 && mip_levels(64, 32, 7) == 7 && mip_levels(64, 32, 8) == 7);
    Size l1 = mip_size(5, 3, 1), l2 = mip_size(5, 3, 2), l9 = mip_size(5, 3, 9), l0 = mip_size(5, 3, 0);
    CHECK(l0.w == 5 && l0.h == 3 && l1.w == 2 && l1.h == 1 && l2.w == 1 && l2.h == 1 && l9.w == 1 && l9.h == 1);
    Size big = mip_size(1920, 1080, 5);
    CHECK(big.w == 60 && big.h == 33);

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

    struct List {};
    List la, lb, many[3];
    SideMarks<List, Res, int, 8, 8> side;
    CHECK(!side.find(&la, false));
    SideMark<List, Res, int> *a = side.find(&la, true);
    CHECK(a && side.find(&la, false) == a && side.find(&lb, true) != a && !side.full());
    bool marked = a->marked || a->depth;
    note(*a, marked, false, true, &rebuilt, 0);
    CHECK(!a->ui && !marked);
    note(*a, marked, true, false, &depth, 16);
    CHECK(a->depth == &depth && a->depth_state == 16 && marked);
    note(*a, marked, false, true, &rebuilt, 0);
    CHECK(a->ui);
    SideMark<List, Res, int> *b = side.find(&lb, false);
    b->marked = true;
    marked = b->marked || b->depth;
    note(*b, marked, false, true, &rebuilt, 0);
    CHECK(b->ui && !b->depth);

    CHECK(!placed_lately(0, LONG_AGO) && !placed_lately(5000, LONG_AGO));
    CHECK(placed_lately(100, 100) && placed_lately(101, 100) && !placed_lately(102, 100) && !placed_lately(103, 100));
    CHECK(placed_lately(0, 0u - 1) && !placed_lately(1, 0u - 1));

    SideMarks<List, Res, int, 2, 2> cramped;
    CHECK(cramped.find(&many[0], true) && cramped.find(&many[1], true));
    CHECK(!cramped.find(&many[2], false) && !cramped.full());
    CHECK(!cramped.find(&many[2], true) && cramped.full());
    CHECK(cramped.find(&many[0], false) && cramped.find(&many[1], false));


    static_assert(OPTIONS_AT == 8 && PARAMS_AT == 36 && CONSTANT_COUNT == 56 && PARAMS_AT + 16 <= CONSTANT_COUNT);
    float packed[SLOTS];
    pack_slots({1, 2, 3}, packed);
    CHECK(packed[0] == 1 && packed[2] == 3 && packed[3] == 0 && packed[31] == 0);
    std::vector<float> forty;
    for (int i = 0; i < 40; i++)
        forty.push_back(float(i));
    pack_slots(forty, packed);
    CHECK(packed[15] == 15 && packed[16] == 16 && packed[31] == 31);

    const char *fx = R"({"version":1,"effects":[
      {"id":"ambient_light","name":"Ambient Light","addon":"","slots":9,
       "textures":[{"name":"half","scale":0.5,"format":"rgba16f"},
                   {"name":"adapt","width":1,"height":1,"format":"rgba16f","persistent":true},
                   {"name":"dirt","format":"rgba8","file":"shaders/tex/ambient_light-dirt.tex"}],
       "passes":[{"name":"bright","dxil":"shaders/fx/ambient_light-00.dxil","inputs":[],"output":"half","threads":[8,8],"budget_ms":1.5},
                 {"name":"apply","dxil":"shaders/fx/ambient_light-01.dxil","inputs":["half","adapt","dirt","original","previous"],"output":""}]},
      {"id":"grain","name":"Grain","slots":2,"textures":[],
       "passes":[{"name":"grain","dxil":"shaders/fx/grain-00.dxil","inputs":[],"output":""}]},
      {"id":"bad_input","slots":1,"passes":[{"name":"a","dxil":"x.dxil","inputs":["nothing"]}]},
      {"id":"self","slots":1,"textures":[{"name":"t"}],"passes":[{"name":"a","dxil":"y.dxil","inputs":["t"],"output":"t"}]},
      {"id":"writes_file","slots":1,"textures":[{"name":"t","file":"f.tex"}],"passes":[{"name":"a","dxil":"z.dxil","output":"t"}]},
      {"id":"huge","slots":33,"passes":[{"name":"a","dxil":"h.dxil"}]},
      {"name":"no id","passes":[{"name":"a","dxil":"q.dxil"}]},
      {"id":"no_passes","slots":1,"passes":[]}]})";
    std::vector<EffectDecl> effects = parse_effects(fx);
    CHECK(effects.size() == 6);
    CHECK(effects[0].id == "ambient_light" && effects[0].slots == 9 && effects[0].textures.size() == 3);
    CHECK(effects[0].textures[0].scale == 0.5 && effects[0].textures[0].format == 10);
    CHECK(effects[0].textures[1].persistent && effects[0].textures[1].width == 1);
    CHECK(effects[0].textures[2].format == 28 && effects[0].textures[2].file == "shaders/tex/ambient_light-dirt.tex");
    std::vector<EffectDecl> chained = parse_effects(
        R"({"effects":[{"id":"m","slots":1,"textures":[{"name":"a","width":64,"height":32,"mips":7},{"name":"b","mips":99},{"name":"c","mips":0},{"name":"d"}],
        "passes":[{"name":"p","dxil":"p.dxil","output":"a"}]}]})");
    CHECK(chained.size() == 1 && chained[0].textures.size() == 4);
    CHECK(chained[0].textures[0].mips == 7 && chained[0].textures[1].mips == MAX_MIPS && chained[0].textures[2].mips == 1 &&
          chained[0].textures[3].mips == 1);
    CHECK(effects[0].textures[0].mips == 1 && !(chained[0].textures[0] == effects[0].textures[0]));
    CHECK(effects[0].passes[1].inputs.size() == 5 && effects[0].passes[1].threads[0] == 8);
    CHECK(parse_effects("not json").empty() && parse_effects("{}").empty());

    ActiveSpec act;
    CHECK(!parse_active("[", act) && !parse_active("[1]", act));
    const char *active_json = R"({"version":1,"serial":42,"name":"Realistic","source":"addon:x/realistic",
      "forced_by":"","toggle_key":"F9","instances":[
      {"effect":"grain","label":"","params":[0.5,1]},
      {"effect":"ambient_light","label":"wide","params":[0.96,0,1,10,0,4,0,0,0]},
      {"effect":"nope","label":"","params":[]},
      {"effect":"ambient_light","label":"","params":[1,1,1,1,1,1,1,1,1]},
      {"effect":"huge","params":[]},
      {"effect":"self","params":[1]}]})";
    CHECK(parse_active(active_json, act));
    CHECK(act.serial == 42 && act.name == "Realistic" && act.toggle_key == "F9" && act.instances.size() == 6);
    CHECK(key_code(act.toggle_key) == 0x78);
    CHECK(act.instances[1].label == "wide" && act.instances[1].params.size() == 9 && act.instances[1].params[5] == 4.0f);

    Plan plan = make_plan(act, effects);
    CHECK(plan.instances.size() == 3);
    CHECK(plan.dropped.size() == 3);
    CHECK(plan.dropped[0] == "nope: no such effect");
    CHECK(plan.instances[0].effect == "grain" && plan.instances[1].effect == "ambient_light" &&
          plan.instances[1].label == "wide" && plan.instances[2].label.empty());
    CHECK(plan.instances[1].options[0] == 0.96f && plan.instances[1].options[5] == 4.0f && plan.instances[1].options[20] == 0);
    CHECK(plan.textures.size() == 6);
    CHECK(plan.textures[0].name == "1/half" && plan.textures[1].name == "1/adapt" && plan.textures[2].name == "1/dirt");
    CHECK(plan.textures[3].name == "2/half");
    CHECK(plan.passes.size() == 5);
    CHECK(plan.passes[0].instance == 0 && plan.passes[0].n_inputs == 0 && plan.passes[0].output == IN_NONE);
    CHECK(plan.passes[1].instance == 1 && plan.passes[1].output == 0);
    const PlanPass &apply = plan.passes[2];
    CHECK(apply.n_inputs == 5 && apply.inputs[0] == 0 && apply.inputs[1] == 1 && apply.inputs[2] == 2 &&
          apply.inputs[3] == IN_ORIGINAL && apply.inputs[4] == IN_PREVIOUS && apply.output == IN_NONE);
    CHECK(plan.passes[3].instance == 2 && plan.passes[3].output == 3 && plan.passes[4].inputs[0] == 3);
    CHECK(plan.uses_previous());
    CHECK(pass_title("Realistic", "ambient_light", "wide", "apply") ==
          "Realistic \xC2\xB7 ambient_light.wide \xC2\xB7 apply");
    CHECK(pass_title("R", "grain", "", "grain") == "R \xC2\xB7 grain \xC2\xB7 grain");

    ActiveSpec none;
    CHECK(make_plan(none, effects).passes.empty() && !make_plan(none, effects).uses_previous());

    ActiveSpec tweaked = act;
    tweaked.instances[0].params = {0.9f, 0};
    Plan changed = make_plan(tweaked, effects);
    CHECK(plan_shape(changed) == plan_shape(plan));
    CHECK(changed.instances[0].options[0] == 0.9f);
    ActiveSpec reordered = act;
    std::swap(reordered.instances[0], reordered.instances[1]);
    Plan swapped = make_plan(reordered, effects);
    CHECK(plan_shape(swapped) != plan_shape(plan));
    std::vector<int> keep = reuse_textures(plan, swapped);
    CHECK(keep.size() == 6 && keep[0] == -1 && keep[1] == -1 && keep[2] == -1);
    ActiveSpec appended = act;
    appended.instances.insert(appended.instances.begin() + 4, InstanceSpec{"grain", "second", {0, 0}});
    Plan grown = make_plan(appended, effects);
    CHECK(plan_shape(grown) != plan_shape(plan));
    std::vector<int> grown_keep = reuse_textures(plan, grown);
    CHECK(grown_keep.size() == 6 && grown_keep[0] == 0 && grown_keep[1] == 1 && grown_keep[2] == 2 && grown_keep[3] == 3 &&
          grown_keep[5] == 5);
    effects[0].textures[0].scale = 0.25;
    Plan resized = make_plan(act, effects);
    std::vector<int> resized_keep = reuse_textures(plan, resized);
    CHECK(resized_keep[0] == -1 && resized_keep[1] == 1 && resized_keep[3] == -1 && resized_keep[4] == 4);

    uint8_t flat[4 * 4 * 3], quarter[2 * 2 * 3];
    for (auto &b : flat)
        b = 200;
    box_scale_rgb(flat, 4, 4, quarter, 2, 2);
    CHECK(quarter[0] == 200 && quarter[11] == 200);
    uint8_t pair[2 * 1 * 3] = {0, 100, 255, 100, 100, 1}, one[3];
    box_scale_rgb(pair, 2, 1, one, 1, 1);
    CHECK(one[0] == 50 && one[1] == 100 && one[2] == 128);
    uint8_t same[2 * 1 * 3], keepit[2 * 1 * 3] = {1, 2, 3, 4, 5, 6};
    box_scale_rgb(keepit, 2, 1, same, 2, 1);
    CHECK(memcmp(same, keepit, 6) == 0);
    std::vector<uint8_t> src(1920 * 1080 * 3, 90), dst(1280 * 720 * 3);
    box_scale_rgb(src.data(), 1920, 1080, dst.data(), 1280, 720);
    CHECK(dst.front() == 90 && dst.back() == 90 && dst[dst.size() / 2] == 90);
    uint8_t grad[3 * 1 * 3] = {0, 0, 0, 90, 90, 90, 255, 255, 255}, two[2 * 1 * 3];
    box_scale_rgb(grad, 3, 1, two, 2, 1);
    CHECK(two[0] == 30 && two[3] == 200);

    if (failed)
        return 1;
    std::printf("postfx logic: all checks passed\n");
    return 0;
}
