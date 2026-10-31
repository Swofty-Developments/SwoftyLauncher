// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Which year the career's seats show (seats_logic.hpp), over the committed data/years.json.

#include "seats_logic.hpp"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <functional>
#include <sstream>

using namespace progressive;

namespace {

int failures = 0;

void expect(bool ok, const char *what, int line)
{
    if (!ok) {
        std::printf("FAIL line %d: %s\n", line, what);
        ++failures;
    }
}

#define EXPECT(x) expect((x), #x, __LINE__)

season::Data g_data;
std::map<std::string, int> g_vehicles;

int stat(const std::string &label)
{
    const season::PoolDriver *p = g_data.driver(label);
    return p ? p->stat : -1;
}

season::Resolve resolver()
{
    season::Resolve r;
    r.driver_stat = stat;
    r.vehicle_stat = [](const std::string &label) {
        for (auto &s : g_data.slots)
            if (s.vehicle_2026 == label)
                return s.vehicle_2026_stat;
        auto it = g_vehicles.find(label);
        if (it != g_vehicles.end())
            return it->second;
        int id = 2000 + static_cast<int>(g_vehicles.size());
        g_vehicles[label] = id;
        return id;
    };
    r.look_known = [](int) { return true; };
    r.rating = [](uint32_t) { return 75.0; };
    return r;
}

std::vector<int> due_at(const season::Year &y, int k)
{
    std::vector<int> due;
    for (size_t i = 0; i < y.changes.size(); ++i) {
        const season::Change &c = y.changes[i];
        if (c.round <= k && (c.until <= 0 || k < c.until))
            due.push_back(static_cast<int>(i));
    }
    return due;
}

std::vector<seats::Seated> seated_at(int year, int real_round, bool year_cars)
{
    season::Resolve r = resolver();
    const season::Year *y = g_data.year(year);
    std::vector<int> active;
    std::vector<std::string> notes;
    season::Year at = midseason::line_up_with(*y, due_at(*y, real_round), active, notes);
    lineup::Grid g = season::grid_of(at, r, nullptr, nullptr);
    std::vector<seats::Seated> out;
    for (int s = 0; s < season::kSlots; ++s)
        for (auto d : g[s])
            if (d != lineup::kNone)
                out.push_back({s, d, year_cars ? r.vehicle_stat(y->teams[s].vehicle) : g_data.slots[s].vehicle_2026_stat});
    return out;
}

bool sits(const std::vector<seats::Seated> &v, int slot, const std::string &driver)
{
    for (auto &s : v)
        if (s.slot == slot && s.driver == static_cast<uint32_t>(stat(driver)))
            return true;
    return false;
}

int slot_of(const std::string &team)
{
    for (int i = 0; i < season::kSlots; ++i)
        if (g_data.slots[i].team == team)
            return i;
    return -1;
}

// minik8824's 2019 career (bundle n31 1565755736410161313, Progressive 1.2.5, launcher 1.6.11): the save had the cs19
// cars at the sequential ids 550-559 (cs19_mercedes 550 ... cs19_williams 559). The first 1.6.10+ build hashed
// Classic Seasons' add-row ids, so 553/555/556/558/559 named no row (1.2.5 repaired those) while 550, 551, 552, 554
// and 557 named cs20_lotus, cs20_mercedes, cs16_toro_rosso, cs19_force_india and cs23_sauber, and the Montreal race
// ran out of AUDIO_BUNDLE on four years' engine bundles. Ids below: the hashed ids of that build (the five 1.2.5
// repaired match its log).
void moved_ids_back_on_the_year_cars()
{
    const std::map<std::string, int> build = {
        {"cs18_ferrari", 614},    {"cs18_force_india", 676}, {"cs18_haas", 628},       {"cs18_lotus", 644},
        {"cs18_mclaren", 737},    {"cs18_mercedes", 685},    {"cs18_redbull", 747},    {"cs18_sauber", 593},
        {"cs18_toro_rosso", 664}, {"cs18_williams", 575},    {"cs19_ferrari", 604},    {"cs19_force_india", 554},
        {"cs19_haas", 682},       {"cs19_lotus", 674},       {"cs19_mclaren", 571},    {"cs19_mercedes", 643},
        {"cs19_redbull", 673},    {"cs19_sauber", 591},      {"cs19_toro_rosso", 646}, {"cs19_williams", 754},
        {"cs20_lotus", 550},      {"cs20_mercedes", 551},    {"cs16_toro_rosso", 552}, {"cs23_sauber", 557},
        {"cs16_manor", 695}};
    auto vehicles = build;
    for (auto &s : g_data.slots)
        vehicles[s.vehicle_2026] = s.vehicle_2026_stat;
    season::Resolve r = resolver();
    r.vehicle_stat = [&vehicles](const std::string &label) {
        auto it = vehicles.find(label);
        return it == vehicles.end() ? -1 : it->second;
    };
    std::function<bool(int)> known = [&vehicles](int s) {
        for (auto &[label, stat] : vehicles)
            if (stat == s)
                return true;
        return false;
    };
    std::map<int, int> cars = seats::car_years(g_data, r);
    auto key = [](int stat) { return static_cast<uint32_t>(stat) << 16; };
    auto team = [](const char *t) { return static_cast<uint32_t>(g_data.slots[slot_of(t)].team_stat) << 16; };
    auto grid = [&](const std::vector<std::pair<const char *, int>> &teams) {
        std::vector<seats::SeatCar> out;
        for (auto &[t, car] : teams)
            for (int seat = 0; seat < 2; ++seat)
                out.push_back({team(t), key(car) | static_cast<uint32_t>(seat)});
        out.push_back({0x3fff0000u, 0x3fff0000u});
        return out;
    };
    auto run = [&](std::vector<seats::SeatCar> seats, int year, bool year_cars, int &unknown, int &other) {
        unknown = other = 0;
        for (auto &s : seats) {
            auto fix = season::car_repair(g_data, r, year, s.team_key, s.vehicle_ref, known, year_cars);
            if (!fix)
                continue;
            EXPECT(!fix->left);
            if (fix->left)
                continue;
            ++(fix->other_car ? other : unknown);
            s.vehicle_ref = season::repaired_ref(s.vehicle_ref, fix->car.stat);
        }
        return seats;
    };
    auto on_year_cars = [&](const std::vector<seats::SeatCar> &seats, int year) {
        const season::Year *y = g_data.year(year);
        for (auto &s : seats) {
            if (season::empty_key(s.team_key))
                continue;
            int slot = season::slot_of_team(g_data, static_cast<int>(s.team_key >> 16));
            if (slot < 0 || r.vehicle_stat(y->teams[slot].vehicle) != static_cast<int>(s.vehicle_ref >> 16))
                return false;
        }
        return true;
    };

    const std::vector<std::pair<const char *, int>> saved = {
        {"mercedes_26", 550},    {"ferrari_26", 551}, {"redbull_26", 552},    {"mclaren_26", 553},
        {"forceindia_26", 556},  {"renault_26", 554}, {"williams_26", 559},   {"tororosso_26", 555},
        {"sauber_26", 557},   {"haas_26", 558}};
    std::vector<std::pair<const char *, int>> after_125 = saved;
    for (auto &[t, car] : after_125)
        for (auto [old, now] : {std::pair{553, 571}, {556, 554}, {559, 754}, {555, 646}, {558, 682}})
            if (car == old)
                car = now;
    int unknown = 0, other = 0;
    const std::vector<std::pair<const char *, int>> *both[] = {&saved, &after_125};
    for (auto *teams : both) {
        std::vector<seats::SeatCar> seats = grid(*teams);
        EXPECT(!seats::year_cars(g_data, r, 2019, {seats.begin(), seats.begin() + 10}, known, cars));
        EXPECT(seats::year_cars(g_data, r, 2019, seats, known, cars));
        std::vector<seats::SeatCar> fixed = run(seats, 2019, true, unknown, other);
        EXPECT(other == 10);
        EXPECT(unknown == (teams == &saved ? 10 : 0));
        EXPECT(on_year_cars(fixed, 2019));
        EXPECT(seats::year_cars(g_data, r, 2019, fixed, known, cars));
        int again_unknown = 0, again_other = 0;
        run(fixed, 2019, true, again_unknown, again_other);
        EXPECT(again_unknown == 0 && again_other == 0);
        std::vector<seats::SeatCar> old = run(seats, 2019, false, unknown, other);
        EXPECT(other == 0 && unknown == (teams == &saved ? 10 : 0) && !on_year_cars(old, 2019));
    }

    std::vector<seats::SeatCar> on18 = grid({{"mercedes_26", 685}, {"ferrari_26", 614}, {"redbull_26", 747},
                                              {"mclaren_26", 737}, {"forceindia_26", 676}, {"renault_26", 644},
                                              {"williams_26", 575}, {"tororosso_26", 664}, {"sauber_26", 593},
                                              {"haas_26", 628}});
    EXPECT(on_year_cars(on18, 2018));
    EXPECT(!seats::year_cars(g_data, r, 2019, on18, known, cars));
    EXPECT(seats::year_cars(g_data, r, 2018, on18, known, cars));
    on18[0].vehicle_ref = key(643);
    on18[1].vehicle_ref = key(643);
    EXPECT(!seats::year_cars(g_data, r, 2019, on18, known, cars));

    std::vector<std::pair<const char *, int>> stock;
    for (auto &s : g_data.slots)
        if (s.team != "gm_cadillac_26")
            stock.push_back({s.team.c_str(), s.vehicle_2026_stat});
    std::vector<seats::SeatCar> fresh = grid(stock);
    EXPECT(!seats::year_cars(g_data, r, 2019, fresh, known, cars));
    EXPECT(seats::year_cars(g_data, r, 2026, fresh, known, cars));
    EXPECT(!season::car_repair(g_data, r, 2026, team("mercedes_26"), key(550), known, true));

    EXPECT(!season::car_repair(g_data, r, 2019, team("mercedes_26"), key(643), known, true));
    EXPECT(!season::car_repair(g_data, r, 2019, key(999), key(550), known, true));
    EXPECT(!season::car_repair(g_data, r, 2019, 0x3fff0000u, key(550), known, true));
    EXPECT(!season::car_repair(g_data, r, 2019, team("gm_cadillac_26"), key(695), known, true));
    auto manor = season::car_repair(g_data, r, 2019, team("gm_cadillac_26"), key(550), known, true);
    EXPECT(manor && manor->other_car && manor->car.label == "cs16_manor" && manor->car.from_year == 2019);

    vehicles.erase("cs19_mercedes");
    EXPECT(!season::car_repair(g_data, r, 2019, team("mercedes_26"), key(550), known, true));
}

} // namespace

int main(int argc, char **argv)
{
    const char *path = argc > 1 ? argv[1] : "../../data/years.json";
    std::ifstream f(path);
    std::stringstream ss;
    ss << f.rdbuf();
    std::string err;
    if (!season::parse(ss.str(), g_data, err)) {
        std::printf("seats_logic_test: cannot parse %s: %s\n", path, err.c_str());
        return EXIT_FAILURE;
    }
    season::Resolve r = resolver();
    std::map<int, int> cars = seats::car_years(g_data, r);
    int tr = slot_of("tororosso_26"), re = slot_of("renault_26");

    std::vector<seats::Seated> late17 = seated_at(2017, 20, true);
    EXPECT(sits(late17, tr, "brendon_hartley") && sits(late17, tr, "pierre_gasly"));
    EXPECT(sits(late17, re, "nico_hulkenberg") && sits(late17, re, "carlos_sainz-jr"));
    seats::Years by_cars = seats::seats_years(g_data, r, late17, cars);
    EXPECT(by_cars.how == "cars");
    EXPECT(by_cars.years == std::vector<int>{2017});

    std::vector<seats::Seated> stock = seated_at(2017, 20, false);
    seats::Years by_lineup = seats::seats_years(g_data, r, stock, cars);
    EXPECT(by_lineup.how == "line-ups");
    EXPECT(by_lineup.years == std::vector<int>{2017});
    EXPECT(seats::line_up_match(g_data, r, *g_data.year(2017), stock) >
           seats::line_up_match(g_data, r, *g_data.year(2018), stock));
    int main_only = 0;
    lineup::Grid g17 = season::grid_of(*g_data.year(2017), r, nullptr, nullptr);
    for (auto &s : stock)
        for (auto d : g17[s.slot])
            main_only += d == s.driver;
    EXPECT(main_only < seats::line_up_match(g_data, r, *g_data.year(2017), stock));

    std::vector<seats::Seated> start18 = seated_at(2018, 0, true);
    EXPECT(seats::seats_years(g_data, r, start18, cars).years == std::vector<int>{2018});
    std::vector<seats::Seated> start16 = seated_at(2016, 0, false);
    EXPECT(seats::seats_years(g_data, r, start16, cars).years == std::vector<int>{2016});

    std::vector<seats::Seated> mixed = seated_at(2017, 20, true);
    for (size_t i = 0; i < mixed.size() / 2 + 1 && i < mixed.size(); ++i)
        mixed[i].vehicle = g_data.slots[mixed[i].slot].vehicle_2026_stat;
    EXPECT(seats::seats_years(g_data, r, mixed, cars).how == "line-ups");

    moved_ids_back_on_the_year_cars();

    if (failures) {
        std::printf("seats_logic_test: %d failure(s)\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("seats_logic_test: ok\n");
    return EXIT_SUCCESS;
}
