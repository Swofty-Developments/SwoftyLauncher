// The SDK's table version 8 operation (swofty/systems/career.hpp standings) against a stand-in SwoftyCareerOps table
// on the host.

#include <cstdio>
#include <cstring>
#include <string>

#include <swofty/systems/career.hpp>

using namespace swofty;

static int failures = 0;
#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #c);                                                             \
            ++failures;                                                                                                          \
        }                                                                                                                        \
    } while (0)

static int g_result = SWOFTY_ASSET_OK, g_calls = 0;
static std::string g_json;

static int available() { return 1; }
static int standings(char *out, size_t cap, size_t *len)
{
    ++g_calls;
    if (g_result != SWOFTY_ASSET_OK)
        return g_result;
    *len = g_json.size();
    if (!out || cap < g_json.size() + 1)
        return SWOFTY_TOO_SMALL;
    std::memcpy(out, g_json.c_str(), g_json.size() + 1);
    return SWOFTY_ASSET_OK;
}

static const SwoftyCareerOps kOps7 = {7,       available, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr,
                                      nullptr, nullptr,   nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, standings};
static const SwoftyCareerOps kOps8 = {8,       available, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr,
                                      nullptr, nullptr,   nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, standings};

static const char *kSeason =
    "{\"season\":1,\"rounds\":2,\"drivers\":["
    "{\"driver\":9,\"position\":1,\"points\":58,\"rounds\":["
    "{\"points\":25,\"finishes\":[{\"session\":12,\"position\":2,\"status\":0},{\"session\":15,\"position\":2,\"status\":0}]},"
    "{\"points\":33,\"finishes\":[{\"session\":12,\"position\":1,\"status\":0},{\"session\":15,\"position\":1,\"status\":0}]}]},"
    "{\"driver\":5,\"position\":2,\"points\":44,\"rounds\":["
    "{\"points\":21,\"finishes\":[{\"session\":15,\"position\":3,\"status\":0}]},"
    "{\"points\":23,\"finishes\":[{\"session\":15,\"position\":3,\"status\":3}]}]}]}";

int main()
{
    CHECK(!career::standings());
    career::detail::set_ops_table(&kOps7);
    CHECK(!career::standings());
    CHECK(g_calls == 0);

    career::detail::set_ops_table(&kOps8);
    g_json = kSeason;
    auto s = career::standings();
    CHECK(s && s->season_index == 1 && s->rounds == 2 && s->drivers.size() == 2);
    if (s && s->drivers.size() == 2) {
        const career::StandingsDriver &lead = s->drivers[0], &second = s->drivers[1];
        CHECK(lead.driver_stat == 9 && lead.position == 1 && lead.points == 58);
        CHECK(lead.rounds.size() == 2 && lead.rounds[1].points == 33 && lead.rounds[1].finishes.size() == 2);
        CHECK(lead.rounds.size() == 2 && lead.rounds[0].finishes[0].session == 12 && lead.rounds[0].finishes[0].position == 2);
        CHECK(second.driver_stat == 5 && second.position == 2 && second.points == 44);
        CHECK(second.rounds.size() == 2 && second.rounds[1].finishes.size() == 1 && second.rounds[1].finishes[0].status == 3);
        CHECK(lead.points_in(1) == 25 && lead.points_in(2) == 33 && lead.points_in(3) == 0 && lead.points_in(0) == 0);
        CHECK(s->find(5) == &s->drivers[1] && s->find(9) == &s->drivers[0] && !s->find(7));
    }

    std::string big = "{\"season\":0,\"rounds\":0,\"drivers\":[";
    for (int i = 0; i < 40; i++)
        big += std::string(i ? "," : "") + "{\"driver\":" + std::to_string(100 + i) + ",\"position\":" + std::to_string(i + 1) +
               ",\"points\":0,\"rounds\":[]}";
    big += "]}";
    g_json = big;
    g_calls = 0;
    auto grown = career::standings();
    CHECK(big.size() > 2048 && g_calls == 2);
    CHECK(grown && grown->drivers.size() == 40 && grown->drivers[39].driver_stat == 139 && grown->drivers[39].position == 40);

    g_json = "{\"season\":0,\"rounds\":0,\"drivers\":[]}";
    auto none = career::standings();
    CHECK(none && none->drivers.empty());

    g_result = SWOFTY_ASSET_LATE;
    CHECK(!career::standings());
    g_result = SWOFTY_ASSET_UNAVAILABLE;
    CHECK(!career::standings());
    g_result = SWOFTY_ASSET_OK;

    career::detail::set_ops_table(nullptr);
    if (failures) {
        std::printf("sdk_ops_v8_test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("sdk_ops_v8_test: standings: ok\n");
    return 0;
}
