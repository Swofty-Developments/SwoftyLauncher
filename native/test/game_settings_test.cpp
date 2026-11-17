// Host-native unit test for the loading setting overrides (native/src/core/loading/game_settings_logic.hpp):
// which getter names an armed feature answers, and that an unarmed feature or another name passes through.
// Built and run by native/test/run_game_settings_test.sh.

#include "core/loading/game_settings_logic.hpp"

#include <cstdio>
#include <cstring>
#include <set>
#include <string>

namespace loading = runtime::loading;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

static uint32_t bit(const char *feature)
{
    for (size_t i = 0; i < loading::OVERRIDES.size(); i++)
        if (!std::strcmp(loading::OVERRIDES[i].feature, feature))
            return 1u << i;
    return 0;
}

int main()
{
    const uint32_t minimum = bit("loading_skip_minimum_time"), intro = bit("loading_skip_session_intro_video");
    CHECK(minimum && intro && minimum != intro);

    CHECK(loading::match("Game.IgnoreMinimumLoadingTime", 0) == nullptr);
    CHECK(loading::match("Game.IgnoreMinimumLoadingTime", intro) == nullptr);
    const loading::SettingOverride *row = loading::match("Game.IgnoreMinimumLoadingTime", minimum);
    CHECK(row && row->value && !std::strcmp(row->feature, "loading_skip_minimum_time"));
    CHECK(loading::index_of(row) < loading::OVERRIDES.size());

    row = loading::match("Game.SkipIntroVideo", minimum | intro);
    CHECK(row && row->value && !std::strcmp(row->feature, "loading_skip_session_intro_video"));
    CHECK(loading::match("Game.SkipIntroVideo", minimum) == nullptr);

    CHECK(loading::match("Game.IgnoreMinimumLoadingTimeX", minimum) == nullptr);
    CHECK(loading::match("Game.IgnoreMinimumLoading", minimum) == nullptr);
    CHECK(loading::match("game.ignoreminimumloadingtime", minimum) == nullptr);
    CHECK(loading::match("Renderer.Gfx.enableVRS", minimum | intro) == nullptr);
    CHECK(loading::match("Game.SkipGridRundown", minimum | intro) == nullptr);
    CHECK(loading::match("", minimum | intro) == nullptr);
    CHECK(loading::match("Game", minimum | intro) == nullptr);
    CHECK(loading::match(nullptr, minimum | intro) == nullptr);
    CHECK(loading::index_of(nullptr) == loading::OVERRIDES.size());

    CHECK(loading::parse_list(nullptr) == 0);
    CHECK(loading::parse_list("") == 0);
    CHECK(loading::parse_list("loading_skip_minimum_time") == minimum);
    CHECK(loading::parse_list(" loading_skip_minimum_time , loading_skip_session_intro_video ") == (minimum | intro));
    CHECK(loading::parse_list("loading_skip_session_intro_video,,postfx") == intro);
    CHECK(loading::parse_list("loading_skip_minimum") == 0);
    CHECK(loading::parse_list("loading_skip_minimum_time_x") == 0);
    CHECK(loading::parse_list(",") == 0);

    std::set<std::string> features, settings;
    for (const auto &o : loading::OVERRIDES) {
        CHECK(std::strncmp(o.setting, loading::PREFIX, loading::PREFIX_LEN) == 0);
        CHECK(std::strncmp(o.feature, "loading_", 8) == 0);
        CHECK(o.effect && *o.effect);
        features.insert(o.feature);
        settings.insert(o.setting);
    }
    CHECK(features.size() == loading::OVERRIDES.size());
    CHECK(settings.size() == loading::OVERRIDES.size());
    CHECK(loading::OVERRIDES.size() <= 32);

    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("loading setting overrides: all checks passed\n");
    return 0;
}
