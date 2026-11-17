// Host-native unit test for the game setting overrides (native/src/core/loading/game_settings_logic.hpp): the rows a
// feature arms, which getter names an armed row answers with which value, the feature list a measurement run sets,
// and the record of logged reads. The shipped table holds bool rows only; TYPED below covers the numeric getters.
// Built and run by native/test/run_tests.sh game_settings.

#include "core/loading/game_settings_logic.hpp"

#include <cstdio>
#include <cstring>
#include <set>
#include <string>

namespace gs = runtime::game_settings;
using gs::Type;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

static constexpr std::array<gs::Override, 5> TYPED{{
    {"t_bool", "Game.SkipGridRundown", Type::boolean, gs::of_bool(true), "b"},
    {"t_pair", "ErpLoader.readBufferSize", Type::u32, 0x800000, "u"},
    {"t_pair", "ErpLoader.readBufferCount", Type::u32, 8, "u"},
    {"t_int", "JobQueueSystem.workerThreadCount", Type::i32, gs::of_i32(-1), "i"},
    {"t_float", "ResourceMessageHandler.synchronousResourceProcessingTimeBudget", Type::f32, gs::of_f32(20.0f), "f"},
}};

static void shipped_rows()
{
    const uint32_t minimum = gs::rows_of("loading_skip_minimum_time"), intro = gs::rows_of("loading_skip_session_intro_video"),
                   rundown = gs::rows_of("loading_skip_grid_rundown"), talking = gs::rows_of("loading_skip_talking_points");
    CHECK(__builtin_popcount(minimum) == 1 && __builtin_popcount(intro) == 1 && __builtin_popcount(rundown) == 1 &&
          __builtin_popcount(talking) == 1);
    CHECK((minimum | intro | rundown | talking) == gs::rows_all());
    CHECK(!(minimum & intro) && !(intro & rundown) && !(minimum & rundown) && !(talking & (minimum | intro | rundown)));
    CHECK(gs::rows_of("postfx") == 0 && gs::rows_of("") == 0 && gs::rows_of("loading_skip") == 0);
    CHECK(gs::rows_of(gs::LOG_FEATURE) == 0);
    CHECK(gs::rows_typed(gs::rows_all(), Type::boolean) == gs::rows_all());
    CHECK(gs::rows_typed(gs::rows_all(), Type::u32) == 0);

    CHECK(gs::match("Game.IgnoreMinimumLoadingTime", 0) == nullptr);
    const gs::Override *row = gs::match("Game.IgnoreMinimumLoadingTime", minimum);
    CHECK(row && row->bits == 1 && row->type == Type::boolean && !std::strcmp(row->feature, "loading_skip_minimum_time"));
    CHECK(gs::index_of(row) < gs::OVERRIDES.size());
    CHECK(gs::match("Game.IgnoreMinimumLoadingTime", intro) == nullptr);
    row = gs::match("Game.SkipIntroVideo", gs::rows_all());
    CHECK(row && !std::strcmp(row->feature, "loading_skip_session_intro_video") && row->bits == 1);
    row = gs::match("Game.SkipGridRundown", rundown);
    CHECK(row && !std::strcmp(row->feature, "loading_skip_grid_rundown") && row->bits == 1);
    row = gs::match("Game.SkipTalkingPoints", talking);
    CHECK(row && !std::strcmp(row->feature, "loading_skip_talking_points") && row->bits == 1 && row->type == Type::boolean);
    CHECK(gs::match("Game.SkipTalkingPoints", minimum | intro | rundown) == nullptr);
    CHECK(gs::match("Game.SkipCamSequence", gs::rows_all()) == nullptr);
    CHECK(gs::match("Game.SkipWinnerSequence", gs::rows_all()) == nullptr);

    CHECK(gs::match("Game.IgnoreMinimumLoadingTimeX", gs::rows_all()) == nullptr);
    CHECK(gs::match("Game.IgnoreMinimumLoading", gs::rows_all()) == nullptr);
    CHECK(gs::match("game.ignoreminimumloadingtime", gs::rows_all()) == nullptr);
    CHECK(gs::match("Game.ForceTrackReload", gs::rows_all()) == nullptr);
    CHECK(gs::match("Renderer.Gfx.enableVRS", gs::rows_all()) == nullptr);
    CHECK(gs::match("", gs::rows_all()) == nullptr);
    CHECK(gs::match(nullptr, gs::rows_all()) == nullptr);
    CHECK(gs::index_of(nullptr) == gs::OVERRIDES.size());
}

static void typed_rows()
{
    const gs::Table t = TYPED;
    const uint32_t all = gs::rows_all(t), pair = gs::rows_of("t_pair", t);
    CHECK(all == 0x1f);
    CHECK(pair == 0x6);
    CHECK(gs::rows_typed(all, Type::boolean, t) == 0x1);
    CHECK(gs::rows_typed(all, Type::u32, t) == pair);
    CHECK(gs::rows_typed(all, Type::i32, t) == 0x8);
    CHECK(gs::rows_typed(all, Type::f32, t) == 0x10);
    CHECK(gs::rows_typed(pair, Type::f32, t) == 0);

    const uint32_t u32s = gs::rows_typed(all, Type::u32, t), bools = gs::rows_typed(all, Type::boolean, t);
    const gs::Override *row = gs::match("ErpLoader.readBufferSize", u32s, t);
    CHECK(row && row->bits == 0x800000 && gs::index_of(row, t) == 1);
    row = gs::match("ErpLoader.readBufferCount", u32s, t);
    CHECK(row && row->bits == 8 && gs::index_of(row, t) == 2);
    CHECK(gs::match("ErpLoader.readBufferSize", bools, t) == nullptr);
    CHECK(gs::match("Game.SkipGridRundown", u32s, t) == nullptr);
    CHECK(gs::match("ErpLoader.forceSequentialReads", all, t) == nullptr);

    row = gs::match("JobQueueSystem.workerThreadCount", gs::rows_typed(all, Type::i32, t), t);
    int32_t i = 0;
    CHECK(row && (std::memcpy(&i, &row->bits, 4), i == -1));
    row = gs::match("ResourceMessageHandler.synchronousResourceProcessingTimeBudget", gs::rows_typed(all, Type::f32, t), t);
    float f = 0;
    CHECK(row && (std::memcpy(&f, &row->bits, 4), f == 20.0f));
    CHECK(gs::match("ResourceMessageHandler.synchronousResourceProcessingTimeBudget_master", all, t) == nullptr);

    bool log = false;
    CHECK(gs::parse_list("t_pair, t_float", &log, t) == 0x16 && !log);
    CHECK(gs::rows_all(gs::Table{}) == 0);

    CHECK(gs::of_f32(1.0f) == 0x3f800000u && gs::of_i32(-1) == 0xffffffffu);
    CHECK(gs::of_bool(true) == 1 && gs::of_bool(false) == 0);
}

static void lists()
{
    const uint32_t minimum = gs::rows_of("loading_skip_minimum_time"), intro = gs::rows_of("loading_skip_session_intro_video"),
                   rundown = gs::rows_of("loading_skip_grid_rundown");
    bool log = false;
    CHECK(gs::parse_list(nullptr, &log) == 0 && !log);
    CHECK(gs::parse_list("", &log) == 0 && !log);
    CHECK(gs::parse_list("loading_skip_minimum_time") == minimum);
    CHECK(gs::parse_list(" loading_skip_minimum_time , loading_skip_session_intro_video ") == (minimum | intro));
    CHECK(gs::parse_list("loading_skip_grid_rundown\t") == rundown);
    CHECK(gs::parse_list("loading_skip_talking_points") == gs::rows_of("loading_skip_talking_points"));
    CHECK(gs::parse_list("loading_skip_session_intro_video,,postfx") == intro);
    CHECK(gs::parse_list("loading_skip_minimum") == 0);
    CHECK(gs::parse_list("loading_skip_minimum_time_x") == 0);
    CHECK(gs::parse_list(",") == 0);
    log = false;
    CHECK(gs::parse_list("loading_log_settings", &log) == 0 && log);
    log = false;
    CHECK(gs::parse_list("loading_log_settings_x,loading_log", &log) == 0 && !log);
    log = false;
    CHECK(gs::parse_list("loading_skip_grid_rundown, loading_log_settings", &log) == rundown && log);
    CHECK(gs::parse_list("loading_log_settings") == 0);
}

static void seen_table()
{
    static gs::SeenTable<8> t;
    static const char a[] = "Game.A", b[] = "Game.B";
    CHECK(!t.seen(a, Type::boolean, 0));
    CHECK(t.seen(a, Type::boolean, 0));
    CHECK(!t.seen(a, Type::boolean, 1));
    CHECK(t.seen(a, Type::boolean, 1));
    CHECK(!t.seen(a, Type::u32, 1));
    CHECK(!t.seen(b, Type::f32, gs::of_f32(10.0f)));
    CHECK(t.seen(b, Type::f32, gs::of_f32(10.0f)));
    CHECK(t.used == 3);

    static gs::SeenTable<4> small;
    static const char names[6][4] = {"n0", "n1", "n2", "n3", "n4", "n5"};
    int logged = 0;
    for (auto &n : names)
        logged += small.seen(n, Type::u32, 7) ? 0 : 1;
    CHECK(logged == 3);
    CHECK(small.used == 3);
    CHECK(small.seen(names[5], Type::u32, 7));
}

static void table_shape()
{
    std::set<std::string> settings;
    for (const auto &o : gs::OVERRIDES) {
        CHECK(std::strncmp(o.feature, "loading_", 8) == 0);
        CHECK(std::strcmp(o.feature, gs::LOG_FEATURE) != 0);
        CHECK(o.setting && o.setting[0] && std::strchr(o.setting, '.'));
        CHECK(o.effect && *o.effect);
        CHECK(o.type != Type::boolean || o.bits <= 1);
        settings.insert(o.setting);
    }
    CHECK(settings.size() == gs::OVERRIDES.size());
    CHECK(gs::OVERRIDES.size() <= 32);
    for (size_t i = 1; i < gs::OVERRIDES.size(); i++)
        for (size_t j = 0; j + 1 < i; j++)
            CHECK(!gs::same(gs::OVERRIDES[i].feature, gs::OVERRIDES[j].feature) ||
                  gs::same(gs::OVERRIDES[i - 1].feature, gs::OVERRIDES[i].feature));
}

int main()
{
    shipped_rows();
    typed_rows();
    lists();
    seen_table();
    table_shape();
    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("game setting overrides: all checks passed\n");
    return 0;
}
