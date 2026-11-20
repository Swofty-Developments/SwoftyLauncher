// Host-native unit test for the character overrides' rules (native/src/core/career/characters_logic.hpp): entries act
// only in careers of their group, the last swap of a character wins, mutes by speaker, and the default language.
// Built and run by native/test/run_tests.sh characters_logic.

#include "core/career/characters_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace ch = career::characters;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

struct Swap {
    std::string group, character, replacement;
};

struct Mute {
    std::string group, speaker;
};

int main()
{
    std::vector<Swap> swaps = {{"f1_2010", "driver_a", "driver_b"},
                               {"f1_2011", "driver_a", "driver_c"},
                               {"f1_2010", "engineer", "engineer_old"},
                               {"f1_2010", "driver_a", "driver_d"}};
    CHECK(ch::replacement_in(swaps, "f1_2010", "driver_a") == "driver_d");
    CHECK(ch::replacement_in(swaps, "f1_2011", "driver_a") == "driver_c");
    CHECK(ch::replacement_in(swaps, "f1_2010", "engineer") == "engineer_old");
    CHECK(ch::replacement_in(swaps, "f1_2011", "engineer").empty());
    CHECK(ch::replacement_in(swaps, "", "driver_a").empty());
    CHECK(ch::replacement_in(swaps, "f1_2012", "driver_a").empty());
    CHECK(ch::replacement_in(std::vector<Swap>{}, "f1_2010", "driver_a").empty());

    CHECK(ch::group_in(swaps, "f1_2010"));
    CHECK(ch::group_in(swaps, "f1_2011"));
    CHECK(!ch::group_in(swaps, "f1_2012"));
    CHECK(!ch::group_in(swaps, ""));

    std::vector<Mute> mutes = {{"f1_2010", "race_engineer"}, {"f1_2011", "commentator"}};
    CHECK(ch::muted_in(mutes, "f1_2010", "race_engineer"));
    CHECK(!ch::muted_in(mutes, "f1_2010", "commentator"));
    CHECK(ch::muted_in(mutes, "f1_2011", "commentator"));
    CHECK(!ch::muted_in(mutes, "", "race_engineer"));
    CHECK(!ch::muted_in(mutes, "f1_2010", nullptr));
    CHECK(!ch::muted_in(mutes, "f1_2010", "race_engineer_2"));

    CHECK(ch::language_of(nullptr) == "*");
    CHECK(ch::language_of("") == "*");
    CHECK(ch::language_of("de") == "de");

    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("characters_logic: all checks passed\n");
    return 0;
}
