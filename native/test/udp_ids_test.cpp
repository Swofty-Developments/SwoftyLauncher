#include "../src/core/udp_ids_logic.hpp"

#include <cstdio>
#include <cstdlib>
#include <vector>

namespace ids = runtime::udp_ids;

static int failures;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                         \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

static const char *MAP = R"j({
  "version": 1,
  "teams": {"507": 4, "518": 1, "700": 2},
  "drivers": {"197": 255, "198": 8, "300": 255, "513": 255},
  "teams_byte": {"251": 4, "6": 1, "188": 2},
  "drivers_byte": {"197": 255, "198": 8, "45": 255},
  "lines": ["team 507 -> 4 (forceindia)", "driver 197 -> 255 (no stock driver)"]
})j";

static ids::Map load_map()
{
    ids::Map m;
    std::string why;
    CHECK(ids::load(MAP, m, why));
    return m;
}

using Bytes = std::vector<uint8_t>;

static Bytes header(int format, int id, size_t size)
{
    Bytes b(size, 0);
    b[0] = uint8_t(format);
    b[1] = uint8_t(format >> 8);
    b[2] = uint8_t(format % 100);
    b[3] = 1;
    b[4] = 2;
    b[5] = 1;
    b[6] = uint8_t(id);
    b[7] = 0x11;
    b[28] = 1;
    return b;
}

static void put16(Bytes &b, size_t at, int v)
{
    b[at] = uint8_t(v);
    b[at + 1] = uint8_t(v >> 8);
}

static Bytes participants(int format, size_t size, size_t stride, int cars, const int *driver, const int *team, bool wide)
{
    Bytes b = header(format, 4, size);
    b[29] = uint8_t(cars);
    for (int i = 0; i < cars; i++) {
        size_t e = 30 + size_t(i) * stride;
        b[e] = 1;
        if (wide) {
            put16(b, e + 1, driver[i]);
            put16(b, e + 5, team[i]);
        } else {
            b[e + 1] = uint8_t(driver[i]);
            b[e + 3] = uint8_t(team[i]);
        }
        if (!wide)
            b[e + 6] = uint8_t(10 + i);
        for (int k = 0; k < 8; k++)
            b[e + 9 + size_t(k)] = uint8_t('A' + i);
    }
    return b;
}

static void test_load()
{
    ids::Map m = load_map();
    CHECK(m.any);
    CHECK(m.teams.at(507) == 4);
    CHECK(m.drivers.at(513) == 255);
    CHECK(m.team8[251] == 4 && m.team8[0] == -1);
    CHECK(m.driver8[198] == 8);
    CHECK(m.lines.size() == 2);
    ids::Map bad;
    std::string why;
    CHECK(!ids::load("[1]", bad, why));
    CHECK(!ids::load("{\"version\": 2}", bad, why));
    CHECK(!ids::load("nonsense", bad, why));
    ids::Map empty;
    CHECK(ids::load("{\"version\":1}", empty, why) && !empty.any);
}

static void test_participants_2025()
{
    ids::Map m = load_map();
    int driver[4] = {1, 197, 198, 45}, team[4] = {2, 251, 6, 188};
    Bytes b = participants(2025, 1284, 57, 4, driver, team, false);
    Bytes was = b;
    std::vector<ids::Change> changes;
    CHECK(ids::rewrite(m, b.data(), b.size(), &changes));
    CHECK(b[30 + 1] == 1 && b[30 + 3] == 2);
    CHECK(b[30 + 57 + 1] == 255 && b[30 + 57 + 3] == 4);
    CHECK(b[30 + 114 + 1] == 8 && b[30 + 114 + 3] == 1);
    CHECK(b[30 + 171 + 1] == 255 && b[30 + 171 + 3] == 2);
    CHECK(changes.size() == 6);
    for (size_t i = 0; i < b.size(); i++) {
        bool id = false;
        for (int k = 0; k < 4; k++)
            id |= i == 30 + size_t(k) * 57 + 1 || i == 30 + size_t(k) * 57 + 3;
        if (!id)
            CHECK(b[i] == was[i]);
    }
}

static void test_participants_2024()
{
    ids::Map m = load_map();
    int driver[2] = {197, 3}, team[2] = {251, 4};
    Bytes b = participants(2024, 1350, 60, 2, driver, team, false);
    CHECK(ids::rewrite(m, b.data(), b.size()));
    CHECK(b[31] == 255 && b[33] == 4);
    CHECK(b[30 + 60 + 1] == 3 && b[30 + 60 + 3] == 4);
}

static void test_participants_2026()
{
    ids::Map m = load_map();
    int driver[3] = {300, 513, 8}, team[3] = {700, 507, 4};
    Bytes b = participants(2026, 1470, 60, 3, driver, team, true);
    CHECK(ids::rewrite(m, b.data(), b.size()));
    CHECK((b[31] | b[32] << 8) == 255 && (b[35] | b[36] << 8) == 2);
    CHECK((b[91] | b[92] << 8) == 255 && (b[95] | b[96] << 8) == 4);
    CHECK((b[151] | b[152] << 8) == 8 && (b[155] | b[156] << 8) == 4);
}

static void test_lobby()
{
    ids::Map m = load_map();
    for (auto [format, size, stride] : {std::tuple{2024, size_t(1306), size_t(58)}, {2025, size_t(954), size_t(42)}}) {
        Bytes b = header(format, 9, size);
        b[29] = 2;
        b[30] = 1;
        b[31] = 251;
        b[30 + stride] = 0;
        b[30 + stride + 1] = 3;
        CHECK(ids::rewrite(m, b.data(), b.size()));
        CHECK(b[31] == 4 && b[30 + stride + 1] == 3);
        CHECK(b[30] == 1 && b[30 + stride] == 0);
    }
}

static void test_lobby_2026()
{
    ids::Map m = load_map();
    Bytes b = header(2026, 9, 1062);
    b[29] = 3;
    for (int i = 0; i < 3; i++) {
        b[30 + size_t(i) * 43] = uint8_t(i + 1);
        b[30 + size_t(i) * 43 + 3] = 77;
        b[30 + size_t(i) * 43 + 5] = 'N';
    }
    put16(b, 31, 700);
    put16(b, 31 + 43, 507);
    put16(b, 31 + 86, 9);
    Bytes was = b;
    CHECK(ids::rewrite(m, b.data(), b.size()));
    CHECK((b[31] | b[32] << 8) == 2 && (b[74] | b[75] << 8) == 4 && (b[117] | b[118] << 8) == 9);
    for (size_t i = 0; i < b.size(); i++)
        if (i != 31 && i != 32 && i != 74 && i != 75)
            CHECK(b[i] == was[i]);
    b = header(2026, 9, 1062);
    b[29] = 25;
    put16(b, 31, 700);
    CHECK(!ids::rewrite(m, b.data(), b.size()));
}

static void test_time_trial_2026()
{
    ids::Map m = load_map();
    Bytes b = header(2026, 14, 104);
    b[29] = 5;
    put16(b, 30, 700);
    b[29 + 25] = 6;
    put16(b, 29 + 25 + 1, 507);
    b[29 + 50] = 7;
    put16(b, 29 + 50 + 1, 3);
    Bytes was = b;
    CHECK(ids::rewrite(m, b.data(), b.size()));
    CHECK((b[30] | b[31] << 8) == 2 && (b[55] | b[56] << 8) == 4 && (b[80] | b[81] << 8) == 3);
    CHECK(b[29] == 5 && b[54] == 6 && b[79] == 7);
    for (size_t i = 0; i < b.size(); i++)
        if (i != 30 && i != 31 && i != 55 && i != 56)
            CHECK(b[i] == was[i]);
    b = header(2026, 14, 101);
    put16(b, 30, 700);
    CHECK(!ids::rewrite(m, b.data(), b.size()));
}

static void test_misfit()
{
    Bytes tt = header(2025, 14, 104);
    CHECK(ids::misfit(tt.data(), tt.size()) && !ids::identify(tt.data(), tt.size()));
    tt = header(2025, 14, 101);
    CHECK(!ids::misfit(tt.data(), tt.size()));
    tt = header(2026, 14, 101);
    CHECK(ids::misfit(tt.data(), tt.size()) && ids::misfit(tt.data(), tt.size())->size == 104);
    tt = header(2026, 2, 1399);
    CHECK(!ids::misfit(tt.data(), tt.size()));
    tt = header(2023, 14, 99);
    CHECK(!ids::misfit(tt.data(), tt.size()));
}

static void test_time_trial()
{
    ids::Map m = load_map();
    for (int format : {2024, 2025}) {
        Bytes b = header(format, 14, 101);
        b[29 + 1] = 251;
        b[29 + 24 + 1] = 6;
        b[29 + 48 + 1] = 9;
        b[29] = 3;
        CHECK(ids::rewrite(m, b.data(), b.size()));
        CHECK(b[30] == 4 && b[54] == 1 && b[78] == 9 && b[29] == 3);
    }
}

static void test_untouched()
{
    ids::Map m = load_map();
    int driver[2] = {197, 198}, team[2] = {251, 6};
    Bytes ok = participants(2025, 1284, 57, 2, driver, team, false);

    Bytes b = ok;
    b[0] = 0xe7;
    Bytes year2023 = b;
    CHECK(!ids::rewrite(m, b.data(), b.size()) && b == year2023);

    b = ok;
    b[6] = 3;
    CHECK(!ids::rewrite(m, b.data(), b.size()));

    b = ok;
    b.push_back(0);
    CHECK(!ids::rewrite(m, b.data(), b.size()));

    b = ok;
    b.pop_back();
    CHECK(!ids::rewrite(m, b.data(), b.size()));

    b = participants(2025, 1350, 60, 2, driver, team, false);
    CHECK(!ids::rewrite(m, b.data(), b.size()));

    b = ok;
    b[29] = 23;
    CHECK(!ids::rewrite(m, b.data(), b.size()));

    b = header(2026, 9, 954);
    b[29] = 1;
    b[31] = 251;
    Bytes lobby26 = b;
    CHECK(!ids::rewrite(m, b.data(), b.size()) && b == lobby26);

    b = header(2025, 3, 45);
    CHECK(!ids::rewrite(m, b.data(), b.size()));

    Bytes junk(300, 0x41);
    CHECK(!ids::rewrite(m, junk.data(), junk.size()));
    CHECK(!ids::rewrite(m, nullptr, 0));
    CHECK(!ids::rewrite(m, ok.data(), 10));

    ids::Map none;
    b = ok;
    CHECK(!ids::rewrite(none, b.data(), b.size()) && b == ok);

    int stock_driver[2] = {5, 6}, stock_team[2] = {1, 2};
    b = participants(2025, 1284, 57, 2, stock_driver, stock_team, false);
    Bytes stock = b;
    CHECK(!ids::rewrite(m, b.data(), b.size()) && b == stock);
}

static void test_identify()
{
    CHECK(ids::identify(participants(2025, 1284, 57, 0, nullptr, nullptr, false).data(), 1284));
    CHECK(!ids::identify(participants(2025, 1284, 57, 0, nullptr, nullptr, false).data(), 1283));
    Bytes lap = header(2025, 2, 1285);
    CHECK(!ids::identify(lap.data(), lap.size()));
}

int main()
{
    test_load();
    test_participants_2025();
    test_participants_2024();
    test_participants_2026();
    test_lobby();
    test_lobby_2026();
    test_time_trial();
    test_time_trial_2026();
    test_misfit();
    test_untouched();
    test_identify();
    if (failures) {
        printf("udp_ids_test: %d failure(s)\n", failures);
        return 1;
    }
    printf("udp_ids_test: ok\n");
    return 0;
}
