#include "rows_test.hpp"

#include <cstring>
#include <vector>

// A TyreComponent, its CarPhysicsRecord, its CarLive and the CarLive's SuspensionSystem wheels in one buffer, laid out as the game
// reaches them (content::punctured_wheels), with every tyre record's model visibility mask (+0x208) set as
// setTyreVisibility leaves it on a car on track.
struct PitStopCar {
    static constexpr uint64_t BASE = 0x200000, TYRES = BASE, PHYSICS = BASE + 0xa000, CAR = BASE + 0xa100,
                              SUSPENSION = BASE + 0xa800, WHEELS = BASE + 0xa900, WHEEL_SIZE = 0x400;
    std::vector<uint8_t> bytes = std::vector<uint8_t>(0xb000 + 4 * WHEEL_SIZE);

    template <class T> void put(uint64_t at, T v) { std::memcpy(&bytes[at - BASE], &v, sizeof v); }

    PitStopCar()
    {
        put<uint64_t>(TYRES + content::TYRE_PHYSICS, PHYSICS);
        put<uint64_t>(PHYSICS + content::PHYSICS_CAR, CAR);
        put<uint64_t>(CAR + content::CAR_SUSPENSION, SUSPENSION);
        for (size_t i = 0; i < 4; i++)
            put<uint64_t>(SUSPENSION + content::SUSPENSION_WHEELS[i], WHEELS + i * WHEEL_SIZE);
        put<uint64_t>(TYRES + 0x98d0, 4);
        for (uint64_t i = 0; i < 4; i++)
            put<uint32_t>(TYRES + 0x30 + i * 0xde0 + 0x208, 0x1f);
    }
    void tyre_flags(size_t i, uint8_t v) { put<uint8_t>(WHEELS + i * WHEEL_SIZE + content::WHEEL_TYRE_FLAGS, v); }

    auto reader() const
    {
        return [this](uint64_t at, auto &out) {
            if (at < BASE || at + sizeof out > BASE + bytes.size())
                return false;
            std::memcpy(&out, &bytes[at - BASE], sizeof out);
            return true;
        };
    }
};

static void pit_stop_wheel_tests()
{
    using content::TyreChange;
    PitStopCar car;
    uint8_t wheels = content::punctured_wheels(PitStopCar::TYRES, car.reader());
    check(wheels == 0, "a car on track has its tyre models shown (+0x208 = 0x1f) and no punctured wheel");
    check(content::race_tyre_change_pick(true, 15, true, wheels) == TyreChange::kept,
          "a race stop of a car whose tyre models are shown keeps its tyres");
    car.tyre_flags(0, 0x4);
    car.tyre_flags(1, 0x8);
    check(content::punctured_wheels(PitStopCar::TYRES, car.reader()) == 0, "pending hits (bits 2 and 3) are not a puncture");
    car.tyre_flags(3, 0x2);
    wheels = content::punctured_wheels(PitStopCar::TYRES, car.reader());
    check(wheels == 0x8 && content::wheel_names(wheels) == "FR", "a puncture (bit 1) is that wheel's");
    check(content::race_tyre_change_pick(true, 17, true, wheels) == TyreChange::punctured,
          "a race stop with a punctured wheel gets the game's change");
    car.tyre_flags(0, 0x3 | 0x4);
    wheels = content::punctured_wheels(PitStopCar::TYRES, car.reader());
    check(wheels == 0x9 && content::wheel_names(wheels) == "RL, FR", "a burst tyre (bit 0 with bit 1) is punctured too");
    car.put<uint64_t>(PitStopCar::SUSPENSION + content::SUSPENSION_WHEELS[3], 0x900000);
    check(content::punctured_wheels(PitStopCar::TYRES, car.reader()) == 0x1,
          "a wheel that can't be read counts as not punctured");
    car.put<uint64_t>(PitStopCar::PHYSICS + content::PHYSICS_CAR, 0);
    check(content::punctured_wheels(PitStopCar::TYRES, car.reader()) == 0, "a TyreComponent with no car has no punctured wheel");
    check(content::punctured_wheels(0x10, car.reader()) == 0, "a TyreComponent that can't be read has no punctured wheel");
    check(content::wheel_names(0) == "" && content::wheel_names(0xf) == "RL, RR, FL, FR", "wheel names in suspension order");
}

void race_fuel_tests()
{
    using content::FuelCar;
    using content::FuelPlan;
    using content::FuelRules;
    Value p2008 = swofty::json::parse(
        R"({"entrant_list":"f1_2008","rules":{"refuelling":true,"race_fuel_kg":90,"refuel_stint_kg":60,"quali_fuel_kg":45,"race_start_fuel_from_quali":true}})");
    Value p1994 = swofty::json::parse(R"({"entrant_list":"f1_1994","rules":{"refuelling":true,"race_fuel_kg":80}})");
    Value p2017 = swofty::json::parse(R"({"entrant_list":"f1_2017","rules":{"refuelling":false,"race_fuel_kg":105}})");
    Value pbad = swofty::json::parse(
        R"({"entrant_list":"f1_bad","rules":{"refuelling":"yes","quali_fuel_kg":-3,"race_start_fuel_from_quali":1}})");
    std::vector<std::pair<std::string, const Value *>> boxes = {
        {"mod.a/f1_2008", &p2008}, {"mod.a/f1_1994", &p1994}, {"mod.a/f1_2017", &p2017}, {"mod.a/f1_bad", &pbad}};
    FuelRules r08 = content::fuel_rules_pick("f1_2008", boxes);
    std::printf("      %s\n", r08.line.c_str());
    check(r08.refuelling && r08.race_fuel_kg == 90.f && r08.refuel_stint_kg == 60.f && r08.quali_fuel_kg == 45.f &&
              r08.start_from_quali,
          "a box's content.package gives its refuelling, race, stint and qualifying fuel and parc ferme rule");
    FuelRules r94 = content::fuel_rules_pick("f1_1994", boxes);
    std::printf("      %s\n", r94.line.c_str());
    check(r94.any() && !r94.start_from_quali && !r94.quali_fuel_kg, "refuelling without the 1.5 fields");
    check(!content::fuel_rules_pick("f1_2017", boxes).any() && !content::fuel_rules_pick("f1_bad", boxes).any() &&
              !content::fuel_rules_pick("f1_2025", boxes).any(),
          "refuelling off, malformed values or a stock box: no fuel rule (the game's own fuel)");

    auto car = [](float fuel, int lap, uint8_t stops) {
        FuelCar c;
        c.present = true;
        c.pit_stops_ok = true;
        c.pit_stops = stops;
        c.lap = lap;
        c.fuel = fuel;
        c.capacity = 110;
        return c;
    };
    // 1994: race start fuel, then a refill at each counted stop.
    FuelPlan plan;
    FuelCar cars[2] = {car(30, 1, 0), car(120, 1, 0)};
    cars[1].capacity = 70;
    auto w = plan.tick(r94, 15, 1, cars, 2);
    check(w.size() == 2 && w[0].car == 0 && w[0].kg == 80.f && w[1].kg == 70.f, "race start: race_fuel_kg, capped at the tank");
    cars[0] = car(80, 1, 0);
    check(plan.tick(r94, 15, 1, cars, 1).empty(), "the start fuel held: nothing written again");
    cars[0] = car(100, 1, 0);
    w = plan.tick(r94, 15, 1, cars, 1);
    check(w.size() == 1 && w[0].kg == 80.f, "the start sequence loaded its own fuel: the start fuel is written again");
    cars[0] = car(79, 1, 0);
    check(plan.tick(r94, 15, 1, cars, 1).empty(), "the car burns: the start fuel is done");
    cars[0] = car(100, 1, 0);
    check(plan.tick(r94, 15, 1, cars, 1).empty(), "after the start, a higher fuel is left alone");
    cars[0] = car(12, 20, 1);
    w = plan.tick(r94, 15, 1, cars, 1);
    check(w.size() == 1 && w[0].kg == 80.f && std::string(w[0].why) == "pit stop refuel",
          "a counted pit stop refuels to the stint (race_fuel_kg)");
    check(plan.tick(r94, 15, 1, cars, 1).empty(), "one refuel per stop");
    plan.failed(0);
    w = plan.tick(r94, 15, 1, cars, 1);
    check(w.size() == 1 && w[0].kg == 80.f, "a refuel the core could not write is tried again");
    cars[0] = car(95, 30, 2);
    check(plan.tick(r94, 15, 1, cars, 1).empty(), "a stop never drains a car that holds more than the stint");
    cars[0] = car(5, 1, 0);
    cars[0].pit_stops_ok = false;
    w = plan.tick(r94, 15, 2, cars, 1);
    check(w.size() == 1 && w[0].kg == 80.f, "a restarted race (new generation) writes the start fuel again");
    check(plan.tick(r94, 1, 3, cars, 1).empty(), "practice: nothing");

    // 2008: qualifying fuel, parc ferme race start, stint refuels.
    FuelPlan pf;
    FuelCar q[2] = {car(100, 0, 0), car(100, 0, 0)};
    w = pf.tick(r08, 8, 1, q, 2);
    check(w.size() == 2 && w[0].kg == 45.f && w[1].kg == 45.f, "qualifying starts on quali_fuel_kg");
    q[0] = car(45, 0, 0);
    q[1] = car(45, 0, 0);
    pf.tick(r08, 8, 1, q, 2);
    q[0] = car(31.5f, 3, 0);
    q[1].present = false;
    pf.tick(r08, 8, 1, q, 2);
    float left = 0;
    check(pf.quali_known(0, left) && left == 31.5f, "the fuel left at the end of qualifying is kept per car");
    FuelCar race[3] = {car(100, 1, 0), car(100, 1, 0), car(100, 1, 0)};
    w = pf.tick(r08, 15, 2, race, 3);
    check(w.size() == 3 && w[0].kg == 31.5f && w[1].kg == 45.f && w[2].kg == 45.f,
          "parc ferme: the race starts on the fuel left after qualifying, quali_fuel_kg for a car without it");
    std::printf("      car 0: %s; car 2: %s\n", w[0].why, w[2].why);
    race[0] = car(10, 25, 1);
    w = pf.tick(r08, 15, 2, race, 1);
    check(w.size() == 1 && w[0].kg == 60.f, "a stop refuels to refuel_stint_kg");
    pf.reset();
    check(!pf.quali_known(0, left), "a box change forgets the qualifying fuel");

    FuelRules none;
    check(FuelPlan{}.tick(none, 15, 1, race, 3).empty(), "no rule: nothing written");

    // 2005: one tyre set for qualifying and race (race_tyre_change false).
    Value p2005 = swofty::json::parse(
        R"({"entrant_list":"f1_2005","rules":{"refuelling":true,"race_fuel_kg":75,"race_tyre_change":false}})");
    Value pbadt = swofty::json::parse(R"({"entrant_list":"f1_badt","rules":{"race_tyre_change":"no"}})");
    std::vector<std::pair<std::string, const Value *>> tb = {
        {"mod.a/f1_2005", &p2005}, {"mod.a/f1_badt", &pbadt}, {"mod.a/f1_1994", &p1994}};
    FuelRules r05 = content::fuel_rules_pick("f1_2005", tb);
    std::printf("      %s\n", r05.line.c_str());
    check(r05.tyres_kept() && r05.any(), "race_tyre_change false: the box keeps its tyres at race stops (and refuels)");
    check(!content::fuel_rules_pick("f1_badt", tb).tyres_kept() && !content::fuel_rules_pick("f1_1994", tb).tyres_kept(),
          "race_tyre_change missing or malformed: the game's tyre change");
    Value ponly = swofty::json::parse(R"({"entrant_list":"f1_t","rules":{"race_tyre_change":false}})");
    std::vector<std::pair<std::string, const Value *>> to = {{"mod.a/f1_t", &ponly}};
    FuelRules rt = content::fuel_rules_pick("f1_t", to);
    check(rt.tyres_kept() && !rt.any() && rt.line.find("keep the tyres") != std::string::npos,
          "a tyre rule alone is logged and handed over without a fuel rule");
    using content::TyreChange;
    check(content::race_tyre_change_pick(true, 15, true, 0) == TyreChange::kept, "race stop, no punctured wheel: tyres kept");
    check(content::race_tyre_change_pick(true, 16, true, 0x4) == TyreChange::punctured,
          "race stop with a puncture: the game's change");
    check(content::race_tyre_change_pick(true, 15, false, 0) == TyreChange::game, "the garage actor's call (flag 0): the game's");
    check(content::race_tyre_change_pick(true, 15, false, 0xf) == TyreChange::game,
          "the garage actor's call with punctures: the game's");
    check(content::race_tyre_change_pick(true, 8, true, 0) == TyreChange::game, "qualifying: the game's change");
    check(content::race_tyre_change_pick(false, 15, true, 0) == TyreChange::game, "no rule: the game's change");
    pit_stop_wheel_tests();
}

void setup_locks_tests()
{
    Value p1950 = swofty::json::parse(R"({"entrant_list":"f5_1950","rules":{"setup_locked":)"
                                      R"(["front_wing","rear_wing","differential","brake_bias","drs_flap"]}})");
    Value pnone = swofty::json::parse(R"({"entrant_list":"f1_2005","rules":{"refuelling":true}})");
    Value pbad = swofty::json::parse(R"({"entrant_list":"f1_bad","rules":{"setup_locked":"front_wing"}})");
    std::vector<std::pair<std::string, const Value *>> boxes = {
        {"vmknolle.season-1950/f5_1950", &p1950}, {"mod.a/f1_2005", &pnone}, {"mod.a/f1_bad", &pbad}};
    auto p = content::setup_locks_pick("f5_1950", boxes);
    // front wing 0, rear wing 1, diff 2-6 and ramp diff 28, brake bias 18
    check(p.mask == 0x1004007full, "f5_1950 locks wings, the differential group and brake bias");
    check(p.package == "vmknolle.season-1950/f5_1950", "the pick names the box's package");
    check(p.line.find("unknown labels left out: drs_flap") != std::string::npos, "an unknown label is named, not locked");
    check(content::setup_locks_pick("f1_2005", boxes).mask == 0, "a box without setup_locked locks nothing");
    check(content::setup_locks_pick("f1_bad", boxes).mask == 0, "setup_locked that is not a list locks nothing");
    check(content::setup_locks_pick("f1_none", boxes).line.find("no content.package") != std::string::npos,
          "a stock box keeps the game's locks");
    check(content::setup_lock_bits("ballast") == (1ull << 25) && content::setup_lock_bits("tyre_pressures") == 0x1e00000ull,
          "ballast is slider 25, dry pressures 21-24");
    uint64_t added = 0;
    // parc ferme bits {1, 7-17, 25, 28} stay; the box adds only what they lack
    uint64_t parc = (1ull << 1) | 0x3ff80ull | (1ull << 25) | (1ull << 28);
    check(content::setup_locks_merge(parc, p.mask, &added) == (parc | p.mask) && added == (p.mask & ~parc),
          "the merge keeps the game's parc ferme bits");
    check(content::setup_locks_enabled("1") && !content::setup_locks_enabled("0") && !content::setup_locks_enabled(""),
          "SWOFTY_BOX_SETUP=1 only");
}

void points_tests()
{
    std::printf("== session.points\n");
    Value rec = swofty::json::parse(R"({"points":[8,6,4,3,2],"fastest_lap":1})");
    content::Plan plan;
    content::lower_session_points("mod.a/f1_1950", rec, plan);
    for (auto &l : plan.lines)
        std::printf("      %s\n", l.c_str());
    std::string label = content::points_label("mod.a/f1_1950");
    std::string row = std::string(content::POINTS_PRESETS) + "[_metadata_=" + label + "]";
    bool full = false, levels = true;
    size_t sets = 0;
    for (std::size_t i = 0; i < plan.ops.size(); i++) {
        const Value &op = plan.ops[i];
        if (op["op"].as_string() == "add-row") {
            full = op["table"].as_string() == std::string(content::POINTS_TABLE) &&
                   op["set"]["points"].as_string() == "8,6,4,3,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0" &&
                   op["set"]["fastest_lap_bonus"].as_string() == "1" &&
                   op["set"]["fastest_lap_rule"].as_string() == "BestOfTop10";
        } else if (op["op"].as_string() == "set") {
            sets++;
            levels = levels && op["select"].as_string().rfind(row + ".reduced_points_data[_metadata_=", 0) == 0;
        }
    }
    check(plan.refused == 0 && full && sets == 9 && levels,
          "a points system becomes one standing preset row (26 places, BestOfTop10) and its three reduced levels");

    Value none = swofty::json::parse(R"({"points":[9,6,4,3,2,1],"fastest_lap":null,"reduced":[[4,3,2,1,1]]})");
    auto s = content::points_system("mod.a/f1_1990", none, nullptr);
    check(s && s->rule == 0 && s->bonus == 0 && s->reduced.size() == 3 && s->reduced[0][0] == 4 && s->reduced[1][0] == 9 &&
              s->full.size() == content::POINTS_PLACES,
          "no fastest lap: rule None; a missing reduced level gives the full points");
    for (const char *bad : {R"({"points":"25,18"})", R"({"points":[]})", R"({"points":[1,-1]})",
                            R"({"points":[1],"fastest_lap":"1"})", R"({"points":[1],"reduced":[[1],[1],[1],[1]]})"}) {
        content::Plan p;
        content::lower_session_points("mod.a/bad", swofty::json::parse(bad), p);
        check(p.refused == 1 && p.ops.size() == 0, std::string("refused, nothing applied: ") + bad);
    }

    std::map<std::string, content::PointsSystem> systems = {
        {"mod.a/f1_1950", *content::points_system("mod.a/f1_1950", rec, nullptr)}};
    Value b1950 = swofty::json::parse(R"({"entrant_list":"f1_1950","rules":{"points":"mod.a/f1_1950"}})");
    Value b1951 = swofty::json::parse(R"({"entrant_list":"f1_1951","rules":{"points":"mod.a/missing"}})");
    Value b1952 = swofty::json::parse(R"({"entrant_list":"f1_1952","rules":{}})");
    std::vector<std::pair<std::string, const Value *>> boxes = {
        {"mod.a/f1_1950", &b1950}, {"mod.a/f1_1951", &b1951}, {"mod.a/f1_1952", &b1952}};
    auto pick = content::points_pick("f1_1950", boxes, systems);
    std::printf("      %s\n", pick.line.c_str());
    check(pick.system && pick.system->label == label && pick.package == "mod.a/f1_1950",
          "a box whose package names an applied points system runs it");
    check(!content::points_pick("f1_1951", boxes, systems).system && !content::points_pick("f1_1952", boxes, systems).system &&
              !content::points_pick("f1_2025", boxes, systems).system,
          "a system not added at boot, no rule, or a stock box: the mode keeps its own points");
}

static void stock_career_rows(Db &db)
{
    auto node = [&](const std::string &c, int i, const char *label, const char *type, const char *dept, const char *links) {
        std::string r = c + ".upgrade_" + std::to_string(i);
        db.put(r + "._metadata_", 2, label);
        db.put(r + ".upgrade_name", 2, std::string("n_") + label);
        db.put(r + ".upgrade_type", 3, type);
        db.put(r + ".upgrade_size", 3, "Minor");
        db.put(r + ".upgrade_department", 3, dept);
        db.put(r + ".cost", 0, "500");
        db.put(r + ".cost_to_develop", 0, "1000");
        db.put(r + ".weighting", 1, "1.0");
        db.put(r + ".unlock_at_facility_level", 0, "0");
        db.put(r + ".supplier_delivery_date_index", 0, "-1");
        db.put(r + ".links", 13, links);
        db.put(r + ".fitting_times.num_days_to_research", 0, "20");
        db.put(r + ".fitting_times.num_days_to_develop", 0, "25");
        db.put(r + ".grid_coordinates.x", 1, "0");
        db.put(r + ".grid_coordinates.y", 1, std::to_string(-5 + i));
    };
    for (const char *group : {"2026", "myteam_2025"}) {
        std::string t = std::string("upgrades.") + group + ".trees.mercedes.";
        db.put(t + "aerodynamics._upgrade_instancecount_", 0, "2");
        node(t + "aerodynamics", 0, "drag_minor_1", "Drag", "Aerodynamics", "drag_major_1");
        node(t + "aerodynamics", 1, "drag_major_1", "Drag", "Aerodynamics", "");
        db.put(t + "chassis._upgrade_instancecount_", 0, "1");
        node(t + "chassis", 0, "brakes_minor_1", "Brakes", "Chassis", "");
        db.put(t + "fake._upgrade_instancecount_", 0, "1");
        node(t + "fake", 0, "default", "Drag", "Fake", "drag_minor_1,brakes_minor_1");
    }
    db.put("driver_market.driver_market._DataGroup_instancecount_", 0, "2");
    for (int gi = 0; gi < 2; gi++) {
        std::string g = "driver_market.driver_market.DataGroup_" + std::to_string(gi);
        db.put(g + "._metadata_", 2, gi ? "2025" : "2026");
        db.put(g + ".driver_retirements._DriverRetirementSchedule_instancecount_", 0, "2");
        const char *retiring[] = {"fernando_alonso", "lewis_hamilton"};
        for (int s = 0; s < 2; s++) {
            std::string sc = g + ".driver_retirements.DriverRetirementSchedule_" + std::to_string(s);
            db.put(sc + "._metadata_", 2, "DriverRetirementSchedule_" + std::to_string(s));
            db.put(sc + ".statId", 0, std::to_string(s));
            db.put(sc + "._DriverRetirement_instancecount_", 0, "2");
            for (int i = 0; i < 2; i++) {
                std::string r = sc + ".DriverRetirement_" + std::to_string(i);
                db.put(r + "._metadata_", 2, retiring[i]);
                db.put(r + ".driver", 4, retiring[i]);
                db.put(r + "._date_time_instancecount_", 0, "2");
                const char *labels[] = {"Retirement", "Announcement"};
                for (int d = 0; d < 2; d++) {
                    std::string dt = r + ".date_time_" + std::to_string(d);
                    db.put(dt + "._metadata_", 2, labels[d]);
                    db.put(dt + ".year", 0, "2026");
                    db.put(dt + ".month", 3, d ? "September" : "December");
                    db.put(dt + ".day", 0, "14");
                    db.put(dt + ".hour", 0, "0");
                    db.put(dt + ".minute", 0, "0");
                    db.put(dt + ".second", 0, "0");
                    db.put(dt + ".daylight_saving", 5, "False");
                }
            }
        }
        db.put(g + ".driver_firings._DriverFiredSchedule_instancecount_", 0, "1");
        std::string fs = g + ".driver_firings.DriverFiredSchedule_0";
        db.put(fs + "._metadata_", 2, "DriverFiredSchedule");
        db.put(fs + ".statId", 0, "0");
        db.put(fs + "._DriverFired_instancecount_", 0, "3");
        for (int i = 0; i < 3; i++) {
            std::string r = fs + ".DriverFired_" + std::to_string(i);
            db.put(r + "._metadata_", 2, "Season_1_DriverFired_" + std::to_string(i));
            db.put(r + ".driver", 4, "lance_stroll");
            db.put(r + ".season_index", 0, std::to_string(i));
        }
        std::string tr = g + ".team_driver_rating_range_requirements";
        db.put(tr + "._TeamDriverRatingRangeRequirement_instancecount_", 0, "1");
        db.put(tr + ".TeamDriverRatingRangeRequirement_0._metadata_", 2, "mclaren");
        db.put(tr + ".TeamDriverRatingRangeRequirement_0.team", 4, "mclaren_26");
        db.put(tr + ".TeamDriverRatingRangeRequirement_0.rating_range", 0, "99");
    }
    db.put("entrants.drivers._driver_instancecount_", 0, "4");
    const char *drivers[] = {"fernando_alonso", "lewis_hamilton", "lance_stroll", "max_verstappen"};
    for (int i = 0; i < 4; i++)
        db.put("entrants.drivers.driver_" + std::to_string(i) + "._metadata_", 2, drivers[i]);
    db.put("entrants.teams._team_instancecount_", 0, "2");
    db.put("entrants.teams.team_0._metadata_", 2, "mclaren_26");
    db.put("entrants.teams.team_1._metadata_", 2, "ferrari_26");
}

void career_registry_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.a"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"career.rnd.tree", "career.market"}) {
        std::string schema = slurp(regs + "/" + name + "/1.0.json");
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int r1 = registry::load_data_file("mod.a", "registry/career.json", R"j({
      "career.rnd.tree": {
        "mercedes": { "token": "mercedes", "nodes": [
          { "label": "sw_wing_1", "department": "aerodynamics", "type": "Drag", "size": "major", "cost": 650, "weighting": 2.5,
            "x": 1, "y": -5, "after": [ "default", "drag_minor_1" ] },
          { "label": "drag_major_1", "department": "aerodynamics", "cost": 900, "links": [ "sw_wing_1" ] },
          { "label": "brakes_minor_1", "department": "aerodynamics", "cost": 1 },
          { "label": "sw_hover_1", "department": "chassis", "type": "Hover" } ] },
        "ferrari": { "token": "ferrari", "nodes": [ { "label": "sw_x_1" } ] },
        "mercedes_26": { "data_group": "2026", "token": "mercedes", "nodes": [ { "label": "drag_minor_1", "cost": 333 } ] } },
      "career.market": {
        "history": { "data_group": "2026",
          "retirements": [ { "driver": "game/fernando_alonso", "retire": "2027-11-30", "announce": "2027-07-04" },
                           { "driver": "game/lewis_hamilton", "retire": "2028-12-14", "announce": "2028-09-23" },
                           { "driver": "game/max_verstappen", "retire": "2030-12-01", "announce": "2030-08-01" } ],
          "firings": [ { "driver": "game/lance_stroll", "season_index": 1 } ],
          "team_ratings": [ { "team": "game/mclaren_26", "rating": 97 }, { "team": "game/ferrari_26", "rating": 90 },
                            { "team": "game/nobody", "rating": 80 } ] },
        "lost": { "data_group": "1990", "team_ratings": [ { "team": "game/mclaren_26", "rating": 50 } ] },
        "everywhere": { "team_ratings": [ { "team": "game/mclaren_26", "rating": 97 } ] } } })j");
    st.end_register();
    for (auto &r : st.refusals())
        std::printf("      refused: %s %s %s %s: %s\n", r.addon.c_str(), r.registry.c_str(), r.id.c_str(), r.reason.c_str(),
                    r.message.c_str());
    check(r1 == SWOFTY_ASSET_OK && st.list("career.rnd.tree", "").size() == 3 && st.list("career.market", "").size() == 3,
          "the career.rnd.tree and career.market records register");
    Db db;
    stock_career_rows(db);
    content::Expansion x(db.base());
    content::StatIds stat = [](const std::string &, const std::string &, int64_t pin) -> int64_t { return pin; };
    content::expand(st, x, stat);
    for (const char *reg : {"career.rnd.tree", "career.market"})
        for (auto &l : x.lines[reg])
            std::printf("      %s\n", l.c_str());
    std::string a = "upgrades.2026.trees.mercedes.aerodynamics.";
    check(x.records["career.rnd.tree"] == 2 && x.refused["career.rnd.tree"] == 1,
          "the mercedes trees apply (two of their nodes refused); the ferrari record, with no such tree in any group, does not");
    std::string m = "upgrades.myteam_2025.trees.mercedes.aerodynamics.";
    check(written(x.engine, m + "_upgrade_instancecount_") == "3" &&
              written(x.engine, m + "upgrade_2._metadata_") == "sw_wing_1" && written(x.engine, m + "upgrade_1.cost") == "900" &&
              written(x.engine, "upgrades.myteam_2025.trees.mercedes.fake.upgrade_0.links") ==
                  "drag_minor_1,brakes_minor_1,sw_wing_1",
          "a record without a data_group edits the tree in every upgrades group that has it");
    check(written(x.engine, a + "upgrade_0.cost") == "333" && !written(x.engine, m + "upgrade_0.cost"),
          "a record that names its data_group edits only that group's tree");
    check(written(x.engine, a + "_upgrade_instancecount_") == "3" && written(x.engine, a + "upgrade_2._metadata_") == "sw_wing_1",
          "a new node is a row added to its department");
    check(written(x.engine, a + "upgrade_2.upgrade_size") == "Major" &&
              written(x.engine, a + "upgrade_2.upgrade_type") == "Drag" && written(x.engine, a + "upgrade_2.cost") == "650" &&
              written(x.engine, a + "upgrade_2.weighting") == "2.5" &&
              written(x.engine, a + "upgrade_2.grid_coordinates.x") == "1" && written(x.engine, a + "upgrade_2.links") == "" &&
              written(x.engine, a + "upgrade_2.upgrade_department") == "Aerodynamics" &&
              written(x.engine, a + "upgrade_2.fitting_times.num_days_to_research") == "20",
          "the new node takes the given fields over its source row's, no links of its own, and its department");
    check(written(x.engine, "upgrades.2026.trees.mercedes.fake.upgrade_0.links") == "drag_minor_1,brakes_minor_1,sw_wing_1" &&
              written(x.engine, a + "upgrade_0.links") == "drag_major_1,sw_wing_1",
          "the after nodes, the hub included, link to the new node");
    check(written(x.engine, a + "upgrade_1.cost") == "900" && written(x.engine, a + "upgrade_1.links") == "sw_wing_1" &&
              !written(x.engine, a + "upgrade_1.upgrade_name"),
          "a stock label edits that node's given fields only");
    check(!written(x.engine, "upgrades.2026.trees.mercedes.chassis.upgrade_0.cost") &&
              !written(x.engine, "upgrades.2026.trees.mercedes.chassis._upgrade_instancecount_"),
          "a node named under the wrong department and a type the trees don't use are refused");
    std::string g = "driver_market.driver_market.DataGroup_0.";
    std::string s0 = g + "driver_retirements.DriverRetirementSchedule_0.",
                s1 = g + "driver_retirements.DriverRetirementSchedule_1.";
    check(written(x.engine, s0 + "_DriverRetirement_instancecount_") == "3" &&
              written(x.engine, s1 + "_DriverRetirement_instancecount_") == "3" &&
              written(x.engine, s0 + "DriverRetirement_2.driver") == "max_verstappen" &&
              written(x.engine, s1 + "DriverRetirement_2.date_time_0.year") == "2030" &&
              written(x.engine, s1 + "DriverRetirement_2.date_time_1.month") == "August",
          "every retirement schedule takes the list, a third driver's row copied with its dates");
    check(written(x.engine, s0 + "DriverRetirement_0.date_time_0.month") == "November" &&
              written(x.engine, s0 + "DriverRetirement_0.date_time_0.day") == "30" &&
              written(x.engine, s0 + "DriverRetirement_0.date_time_1.year") == "2027",
          "a stock retirement row is rewritten in place, Retirement and Announcement by their labels");
    std::string f0 = g + "driver_firings.DriverFiredSchedule_0.";
    check(written(x.engine, f0 + "_DriverFired_instancecount_") == "1" &&
              written(x.engine, f0 + "DriverFired_0.season_index") == "1",
          "a shorter firing list cuts the schedule by its instance count");
    std::string tr = g + "team_driver_rating_range_requirements.";
    check(written(x.engine, tr + "TeamDriverRatingRangeRequirement_0.rating_range") == "97" &&
              written(x.engine, tr + "_TeamDriverRatingRangeRequirement_instancecount_") == "2" &&
              written(x.engine, tr + "TeamDriverRatingRangeRequirement_1.team") == "ferrari_26" &&
              written(x.engine, tr + "TeamDriverRatingRangeRequirement_1.rating_range") == "90",
          "a team's rating is set on its row, or a row is added for a team without one; a team with no entrants.teams row is "
          "left out");
    check(x.records["career.market"] == 2 && x.refused["career.market"] == 1,
          "a market group the database doesn't have is refused");
    std::string g25 = "driver_market.driver_market.DataGroup_1.";
    check(written(x.engine, g25 + "team_driver_rating_range_requirements.TeamDriverRatingRangeRequirement_0.rating_range") ==
                  "97" &&
              !written(x.engine, g25 + "driver_retirements.DriverRetirementSchedule_0._DriverRetirement_instancecount_") &&
              !written(x.engine, g25 + "driver_firings.DriverFiredSchedule_0._DriverFired_instancecount_"),
          "a record without a data_group writes every market group; one that names 2026 leaves 2025 alone");
}

static void older_rows(Db &db)
{
    auto row = [&](const std::string &prefix, std::initializer_list<std::pair<const char *, std::string>> fields) {
        for (auto &[f, v] : fields)
            db.put(prefix + "." + f, std::string_view(f) == "_metadata_" ? 4 : 2, v);
    };
    db.put("entrants.vehicles._vehicle_instancecount_", 0, "2");
    row("entrants.vehicles.vehicle_1",
        {{"_metadata_", "bdx_s5_car_mclaren_3"}, {"performance_tier", "0"}, {"tyre_wear_scale", "1"}});
    db.put("entrants.vehicles.vehicle_1.stat_id", 0, "520");
    db.put("entrants.entrant_lists._entrant_list_instancecount_", 0, "2");
    row("entrants.entrant_lists.entrant_list_1", {{"_metadata_", "bdx_s5_2012"}, {"display_name", "x"}, {"leaderboard", "f1"}});
    db.put("entrants.entrant_lists.entrant_list_1.stat_id", 0, "106");
    db.put("game_setup.scenarios._scenario_instancecount_", 0, "2");
    row("game_setup.scenarios.scenario_1",
        {{"_metadata_", "bdx_s5_2012"}, {"entrant_list", "bdx_s5_2012"}, {"game_setup_mode", "f1"}});
    db.put("game_setup.scenarios.scenario_1.statId", 0, "16");
}

void shared_switch_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"swofty.classic-seasons-2017", "swofty.swofty-seasons-2017"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
        std::string schema = slurp(regs + "/" + name + (std::string_view(name) == "vehicle" ? "/1.1.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int r1 = registry::load_data_file("swofty.classic-seasons-2017", "registry/content.json", R"j({
      "engine_sound": { "honda": { "bundle": "sounds/cs17_honda.bdl", "settings_from": "game/mercedes_2025" } },
      "vehicle": { "toro_rosso": { "label": "cs17_toro_rosso", "donor": "game/mercedes_2025", "stat_id": 526,
                                   "engine_sound": "swofty.classic-seasons-2017/honda",
                                   "audio_switch": "f1_M_mercedes", "audio_from": "f1_m_mercedes" } } })j");
    int r2 = registry::load_data_file("swofty.swofty-seasons-2017", "registry/content.json", R"j({
      "engine_sound": { "renault": { "bundle": "sounds/sw17_renault.bdl", "settings_from": "game/mercedes_2025" } },
      "vehicle": { "lotus": { "label": "sw17_lotus", "donor": "game/mercedes_2025", "stat_id": 919,
                              "engine_sound": "swofty.swofty-seasons-2017/renault",
                              "audio_switch": "f1_M_mercedes", "audio_from": "f1_m_mercedes" } },
      "content.package": { "sw_2017": {
        "title": { "literal": "SWOFTY 2017" }, "entrant_list": "sw_f1_2017", "season": 2017,
        "teams": [ { "key": "lotus", "vehicle": "swofty.swofty-seasons-2017/lotus",
                     "drivers": [ { "driver": "game/max_verstappen", "number": 33, "look": null } ] } ],
        "database": { "entrant_list_from": "f1_2025", "standings_from": "f1_teams_2024", "content_sets": ["2025"] } } } })j");
    st.end_register();
    check(r1 == SWOFTY_ASSET_OK && r2 == SWOFTY_ASSET_OK, "two packs whose cars name one sound switch register");

    Db db;
    stock_package_rows(db);
    apply::ids::Store ids;
    auto text = [&](const std::string &path) -> std::optional<std::string> {
        auto r = db.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    for (auto &scan : content::scan_all_pools(text))
        ids.define_pool(scan.registry, {{{scan.lo, scan.hi}}});
    for (auto &p : content::pins(st))
        ids.pin(p.registry, p.id, p.stat_id, "test");
    content::StatIds stat = [&](const std::string &reg, const std::string &id, int64_t pin) -> int64_t {
        return pin >= 0 ? ids.pin(reg, id, pin, "test").id : ids.allocate(reg, id, "test").id;
    };
    content::Expansion x(db.base());
    content::expand(st, x, stat);
    for (auto &l : x.lines["vehicle"])
        std::printf("      %s\n", l.c_str());
    auto has_label = [&](const std::string &table, const std::string &row, const std::string &label) {
        for (int i = 0; i < 8; i++)
            if (written(x.engine, table + "." + row + "_" + std::to_string(i) + "._metadata_") == label)
                return table + "." + row + "_" + std::to_string(i);
        return std::string();
    };
    check(x.records["vehicle"] == 2 && x.refused["vehicle"] == 0 && x.left_out.empty(),
          "both cars apply and nothing is left out");
    std::string first = has_label("entrants.vehicles", "vehicle", "cs17_toro_rosso");
    std::string second = has_label("entrants.vehicles", "vehicle", "sw17_lotus");
    check(!first.empty() && !second.empty() && written(x.engine, second + ".stat_id") == "919",
          "the later car keeps its row, so every entrant list that names sw17_lotus finds it");
    check(written(x.engine, "audio.audio_car_data.f1_M_mercedes.bundle_name") == "cs17_honda.bdl",
          "the switch's sound rows stay the earlier car's");
    bool said = false;
    for (auto &l : x.lines["vehicle"])
        said |= l.find("vehicle swofty.swofty-seasons-2017/lotus: sound switch f1_M_mercedes is "
                       "swofty.classic-seasons-2017/toro_rosso's") == 0;
    check(said, "the log names the car that has the switch");
    check(x.records["content.package"] == 1 && !has_label("entrants.entrant_lists", "entrant_list", "sw_f1_2017").empty(),
          "the later car's own season list is not left out");
}

void id_clash_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"swofty.classic-seasons-2017"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
        std::string schema = slurp(regs + "/" + name + "/1.0.json");
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int r = registry::load_data_file("swofty.classic-seasons-2017", "registry/content.json", R"j({
      "engine_sound": { "mercedes": { "bundle": "sounds/cs17_mercedes.bdl", "settings_from": "game/mercedes_2025" } },
      "vehicle": { "mercedes": { "label": "cs17_mercedes", "donor": "game/mercedes_2025", "stat_id": 520,
                                 "engine_sound": "swofty.classic-seasons-2017/mercedes",
                                 "audio_switch": "f1_M_mercedes", "audio_from": "f1_m_mercedes" },
                   "redbull": { "label": "cs17_redbull", "donor": "game/mercedes_2025", "stat_id": 521 } },
      "content.package": { "f1_2017": {
        "title": "lng_cs_f1_2017", "entrant_list": "cs_f1_2017", "season": 2017, "stat_id": 106,
        "teams": [ { "key": "mercedes", "vehicle": null,
                     "drivers": [ { "driver": "game/lewis_hamilton", "number": 44, "look": null } ] } ],
        "database": { "entrant_list": "cs_f1_2017", "scenario_stat_id": 16, "entrant_prefix": "cs2017-",
                      "standings": "cs_f1_teams_2017", "content_sets": ["2025"],
                      "looks": { "mercedes/lewis_hamilton": "2017-cs_mercedes_lewis_hamilton" } } } } })j");
    st.end_register();
    check(r == SWOFTY_ASSET_OK, "the CS-style records register");

    struct Run {
        std::vector<std::string> lines;
        std::vector<std::pair<std::string, std::string>> writes;
    };
    auto run = [&](Db &db, apply::ids::Store &ids, bool settle) {
        Run out;
        auto text = [&](const std::string &path) -> std::optional<std::string> {
            auto r = db.base()(content::gvdb::fnv64(path));
            return r ? std::optional<std::string>(r->text) : std::nullopt;
        };
        std::map<std::string, content::PoolScan> scans;
        for (auto &scan : content::scan_all_pools(text)) {
            ids.define_pool(scan.registry, {{{scan.lo, scan.hi}}});
            scans[scan.registry] = scan;
        }
        if (settle)
            out.lines = content::settle_ids(ids, st, scans, "test");
        else
            for (auto &p : content::pins(st))
                ids.pin(p.registry, p.id, p.stat_id, "test");
        content::StatIds stat = [&](const std::string &reg, const std::string &id, int64_t pin) -> int64_t {
            return pin >= 0 ? ids.pin(reg, id, pin, "test").id : ids.allocate(reg, id, "test").id;
        };
        content::Expansion x(db.base());
        content::expand(st, x, stat);
        for (auto &w : x.engine.writes())
            out.writes.push_back({w.path, w.text});
        return out;
    };
    auto stat_of = [](const Run &r, const std::string &table, const std::string &row, const std::string &label,
                      const char *field) {
        std::string at;
        for (auto &[path, text] : r.writes)
            if (text == label && path.rfind(table + "." + row + "_", 0) == 0 && path.size() > 11 &&
                path.substr(path.size() - 11) == "._metadata_")
                at = path.substr(0, path.size() - 10);
        for (auto &[path, text] : r.writes)
            if (!at.empty() && path == at + field)
                return text;
        return std::string("none");
    };

    Db stock;
    stock_package_rows(stock);
    apply::ids::Store old_ids, new_ids;
    Run before = run(stock, old_ids, false), after = run(stock, new_ids, true);
    check(after.lines.size() == 4 && after.lines[0].find("[registry] stat ids:") == 0,
          "no clash: only the usual pin lines (" + std::to_string(after.lines.size()) + ")");
    check(before.writes == after.writes && !after.writes.empty(), "no clash: the plan is byte-identical to before");
    check(old_ids.dump() == new_ids.dump(), "no clash: ids.json is identical");

    Db older;
    stock_package_rows(older);
    older_rows(older);
    apply::ids::Store ids;
    Run moved = run(older, ids, true);
    for (auto &l : moved.lines)
        std::printf("      %s\n", l.c_str());
    auto has = [&](const std::string &line) {
        return std::find(moved.lines.begin(), moved.lines.end(), line) != moved.lines.end();
    };
    check(has("[registry] ids: swofty.classic-seasons-2017/mercedes pinned vehicle stat id 520 is held by "
              "bdx_s5_car_mclaren_3 from the database; uses 522"),
          "a pin on a 1.5 row's vehicle id moves past the other pins, with the log line");
    check(has("[registry] ids: swofty.classic-seasons-2017/f1_2017 pinned content.package stat id 106 is held by "
              "bdx_s5_2012 from the database; uses 107") &&
              has("[registry] ids: swofty.classic-seasons-2017/f1_2017 pinned content.package.scenario stat id 16 is "
                  "held by bdx_s5_2012 from the database; uses 17"),
          "so do the entrant list and scenario ids");
    check(stat_of(moved, "entrants.vehicles", "vehicle", "cs17_mercedes", "stat_id") == "522" &&
              stat_of(moved, "entrants.vehicles", "vehicle", "cs17_redbull", "stat_id") == "521",
          "the moved car's row carries its new id; the car with no clash keeps its pin");
    check(stat_of(moved, "entrants.entrant_lists", "entrant_list", "cs_f1_2017", "stat_id") == "107" &&
              stat_of(moved, "game_setup.scenarios", "scenario", "cs_f1_2017", "statId") == "17",
          "and so do the list and scenario rows");
    bool untouched = true;
    for (auto &[path, text] : moved.writes)
        untouched &= path.rfind("entrants.vehicles.vehicle_1.", 0) != 0 && path.rfind("entrants.vehicles.vehicle_0.", 0) != 0;
    check(untouched, "the stock and 1.5 rows are not written");
    Run again = run(older, ids, true);
    check(again.writes == moved.writes, "the next boot moves it to the same id");
}

void stock_only_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.a"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"team.identity", "team.constructor", "team.pit_times", "team.logo"}) {
        std::string schema = slurp(regs + "/" + name + (std::string(name) == "team.identity" ? "/1.1.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    st.end_register();
    Db db;
    stock_team_rows(db);
    content::Read stock_read = [&](const std::string &path) -> std::optional<std::string> {
        auto r = db.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    check(content::in_load_order(st, "team.identity").empty() && content::in_load_order(st, "team.pit_times").empty(),
          "a profile with no team records has none in load order");
    for (auto &c : content::CONSTRUCTOR_SEEDS)
        st.add_base(
            "team.constructor", c.key,
            Value::object({{"name", Value(std::string(c.name))}, {"country", Value(std::string(c.country))}, {"team", Value()}}));
    for (auto &team : content::stock_teams(stock_read))
        st.add_base("team.identity", team.label, team.record);
    st.stock_loaded("team.identity", true);
    check(st.get("team.identity", "game/mercedes").status == registry::Status::ok &&
              st.get("team.identity", "game/tororosso").status == registry::Status::ok &&
              st.get("team.constructor", "constructor/mercedes").status == registry::Status::ok,
          "the stock team and constructor records exist with no addon team records");
    check(st.list("team.identity", "").size() == 2, "and the stock teams are the only team.identity records");
    check(content::in_load_order(st, "team.identity").empty() && content::in_load_order(st, "team.pit_times").empty(),
          "seeding adds no team.identity or team.pit_times record the content appliers walk in load order");
    std::map<std::string, Value> keep;
    auto lookup = content::own_lookup(st, keep);
    auto refs = content::refs_of(st, lookup);
    bool only_stock = true;
    for (auto &[token, team] : refs.constructor_team)
        only_stock = only_stock && team.rfind("game/", 0) == 0;
    check(refs.constructors == 0 && refs.constructors_refused == 0 && refs.lines.empty() && only_stock && refs.pit_record.empty(),
          "the constructor applier's report is unchanged: nothing applied, refused or logged, every constructor id resolves to a "
          "stock row");
    apply::ids::Store ids;
    ids.define_pool("team.identity", {{{4, 1023}}});
    int64_t out = -1;
    std::string line;
    check(apply::stat_id(st, ids, "team.identity", "game/tororosso", &out, "test", &line) == SWOFTY_ASSET_OK && out == 3,
          "stat_id on a stock record returns the row's own stat id");
    check(apply::stat_id(st, ids, "team.identity", "game/mercedes", &out, "test", &line) == SWOFTY_ASSET_OK && out == 0,
          "including stat id 0");
    ids.allocate("team.identity", "mod.a/new", "test");
    int64_t fresh = ids.allocate("team.identity", "mod.a/other", "test").id;
    check(fresh == 5,
          "a stock stat_id call allocates nothing from the pool: the next free id is still the first above the table");
}

void virtual_list_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.vl"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
        std::string schema = slurp(regs + "/" + name + (std::string_view(name) == "content.package" ? "/1.5.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    auto package = [](const char *list, bool virt) {
        return std::string(R"({ "title": "lng_x", "entrant_list": ")") + list + R"(", "season": 1960, "stat_id": "auto", )" +
               (virt ? R"("virtual_list": true, )" : "") + R"("teams": [ { "key": "mercedes", "vehicle": null,
                 "drivers": [ { "driver": "game/lewis_hamilton", "number": 44, "look": null } ] } ],
                 "database": { "entrant_list": ")" +
               list + R"(", "scenario_stat_id": null, "content_sets": ["2025"],
                 "looks": { "mercedes/lewis_hamilton": "x" } } })";
    };
    std::string json = std::string("{ \"content.package\": { \"real\": ") + package("vl_real", false) +
                       ", \"v1\": " + package("vl_1960", true) + ", \"v2\": " + package("vl_1961", true) + " } }";
    check(registry::load_data_file("mod.vl", "registry/content.json", json) == SWOFTY_ASSET_OK,
          "the virtual list packages register");
    st.end_register();
    Db db;
    stock_package_rows(db);
    apply::ids::Store ids;
    auto text = [&](const std::string &path) -> std::optional<std::string> {
        auto r = db.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    std::map<std::string, content::PoolScan> scans;
    for (auto &scan : content::scan_all_pools(text)) {
        ids.define_pool(scan.registry, {{{scan.lo, scan.hi}}});
        scans[scan.registry] = scan;
    }
    content::settle_ids(ids, st, scans, "test");
    content::StatIds stat = [&](const std::string &reg, const std::string &id, int64_t pin) -> int64_t {
        return pin >= 0 ? ids.pin(reg, id, pin, "test").id : ids.allocate(reg, id, "test").id;
    };
    content::Expansion x(db.base());
    content::expand(st, x, stat);
    auto &e = x.engine;
    const std::string lists = "entrants.entrant_lists.entrant_list_";
    auto label = [&](int i) { return e.text(lists + std::to_string(i) + "._metadata_").value_or(""); };
    std::string all;
    for (auto &l : x.lines["content.package"])
        all += l + "\n";
    check(e.text("entrants.entrant_lists._entrant_list_instancecount_").value_or("") == "3",
          "two virtual lists leave the stock list, the real one and the host counted\n" + all);
    std::set<std::string> counted = {label(0), label(1), label(2)};
    check(counted == std::set<std::string>{"f1_2025", "vl_real", "swofty_virtual_list"},
          "the counted rows are f1_2025, vl_real and the host");
    check(label(10000) == "vl_1960" && label(10001) == "vl_1961", "the virtual lists sit at entrant_list_10000 and 10001");
    check(e.text(lists + "10000.entrant_0.entrant_reference").has_value(), "a virtual list keeps its entrant rows");
    check(!e.text(lists + "10000.stat_id").has_value() || e.text(lists + "10000.stat_id") != e.text(lists + "1.stat_id"),
          "a virtual list does not share the real list's stat id");
    check(all.find("virtual lists: host swofty_virtual_list added") != std::string::npos, "the host is added and logged");
    check(all.find("virtual lists: 2 moved above the count") != std::string::npos, "the move is logged");
}

void setup_mode_cap_tests(const std::string &regs)
{
    const std::string setup = "entrants.entrant_setup_modes.entrant_setup_mode_0.";
    auto run = [&](int packages, bool virt, int db_lists, auto &&checks) {
        registry::reset_store();
        auto &st = registry::store();
        st.set_order({"mod.cap"});
        st.set_validator(&registry::schema_validator());
        const SwoftyRegistry *t = registry::table();
        for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
            std::string schema =
                slurp(regs + "/" + name + (std::string_view(name) == "content.package" ? "/1.5.json" : "/1.0.json"));
            t->declare(name, SwoftyText{schema.data(), schema.size()});
        }
        auto package = [](const std::string &list, bool v) {
            return R"({ "title": "lng_x", "entrant_list": ")" + list + R"(", "season": 1960, "stat_id": "auto", )" +
                   (v ? R"("virtual_list": true, )" : "") + R"("teams": [ { "key": "mercedes", "vehicle": null,
                     "drivers": [ { "driver": "game/lewis_hamilton", "number": 44, "look": null } ] } ],
                     "database": { "entrant_list": ")" +
                   list + R"(", "scenario_stat_id": null, "content_sets": ["2025"],
                     "looks": { "mercedes/lewis_hamilton": "x" } } })";
        };
        std::string json = "{ \"content.package\": { ";
        for (int i = 0; i < packages; i++) {
            char n[8];
            std::snprintf(n, sizeof n, "%02d", i);
            json += (i ? ", " : "") + ("\"p" + std::string(n) + "\": ") + package("cl_" + std::string(n), false);
        }
        if (virt)
            json += ", \"v\": " + package("cl_virtual", true);
        json += " } }";
        check(registry::load_data_file("mod.cap", "registry/content.json", json) == SWOFTY_ASSET_OK,
              "the setup mode cap packages register");
        st.end_register();
        Db db;
        stock_package_rows(db);
        if (db_lists) {
            std::string names = "f1_2025";
            for (int i = 1; i < db_lists; i++)
                names += ",db_" + std::to_string(i);
            db.put(setup + "entrant_lists", 13, names);
            db.put(setup + "selectable_entrant_lists", 13, names);
        }
        apply::ids::Store ids;
        auto text = [&](const std::string &path) -> std::optional<std::string> {
            auto r = db.base()(content::gvdb::fnv64(path));
            return r ? std::optional<std::string>(r->text) : std::nullopt;
        };
        std::map<std::string, content::PoolScan> scans;
        for (auto &scan : content::scan_all_pools(text)) {
            ids.define_pool(scan.registry, {{{scan.lo, scan.hi}}});
            scans[scan.registry] = scan;
        }
        content::settle_ids(ids, st, scans, "test");
        content::StatIds stat = [&](const std::string &reg, const std::string &id, int64_t pin) -> int64_t {
            return pin >= 0 ? ids.pin(reg, id, pin, "test").id : ids.allocate(reg, id, "test").id;
        };
        content::Expansion x(db.base());
        content::expand(st, x, stat);
        checks(x);
    };
    auto names = [&](content::Expansion &x, const char *field) {
        return rows::Engine::items(x.engine.text(setup + field).value_or(""));
    };
    auto has = [](const std::vector<std::string> &v, const std::string &s) {
        return std::find(v.begin(), v.end(), s) != v.end();
    };
    auto joined = [](const std::vector<std::string> &v) {
        std::string all;
        for (auto &l : v)
            all += l + "\n";
        return all;
    };
    auto expect = [](bool ok, const std::string &what, const std::string &detail) {
        check(ok, ok ? what : what + "\n" + detail);
    };
    run(33, false, 0, [&](content::Expansion &x) {
        auto lists = names(x, "entrant_lists"), selectable = names(x, "selectable_entrant_lists");
        expect(lists.size() == 32 && selectable.size() == 32,
               "33 packages on swofty_f1 with the stock list leave 32 lists in each field",
               std::to_string(lists.size()) + " and " + std::to_string(selectable.size()) + "\n" +
                   joined(x.lines["content.package"]));
        check(has(lists, "cl_30") && !has(lists, "cl_31") && !has(lists, "cl_32") && !has(selectable, "cl_31"),
              "the last packages in load order are the ones left out");
        check(x.refused["content.package"] == 2, "the two packages past the cap are refused");
        check(!x.engine.resolve("entrants.entrant_lists.entrant_list[_metadata_=cl_31]"),
              "a package left out writes no entrant list row");
        auto &status = x.status["mod.cap"];
        expect(status.size() == 2 && status[0].find("entrant list cl_31 left out") != std::string::npos &&
                   status[0].find("32 at most") != std::string::npos,
               "the addon's status names each list left out", joined(status));
        check(x.status.count("swofty.core") == 0, "the backstop has nothing to trim");
    });
    run(31, true, 0, [&](content::Expansion &x) {
        auto lists = names(x, "entrant_lists");
        expect(lists.size() == 32 && has(lists, std::string(content::VIRTUAL_HOST_LIST)),
               "with a virtual package the host keeps its slot in swofty_f1", joined(x.lines["content.package"]));
        check(has(lists, "cl_29") && !has(lists, "cl_30"), "the host's slot leaves room for 30 package lists");
        expect(x.status["mod.cap"].size() == 1 &&
                   x.status["mod.cap"][0].find("keeps a slot for swofty_virtual_list") != std::string::npos,
               "the status says the host holds the slot", joined(x.status["mod.cap"]));
    });
    run(1, false, 40, [&](content::Expansion &x) {
        auto lists = names(x, "entrant_lists"), selectable = names(x, "selectable_entrant_lists");
        expect(lists.size() == 32 && selectable.size() == 32 && lists.front() == "f1_2025" && lists.back() == "db_31",
               "the backstop trims 40 database lists to the first 32", joined(x.lines["content.package"]));
        check(x.refused["content.package"] == 1 && !has(lists, "cl_00"), "a package finds no room in a full mode");
        auto &core = x.status["swofty.core"];
        expect(core.size() == 2 && core[0].find("named 40 lists") != std::string::npos &&
                   core[0].find("left out db_32,db_33") != std::string::npos,
               "the backstop names what it left out", joined(core));
        bool staged = false;
        for (auto &w : x.engine.writes())
            staged = staged || (w.path == setup + "entrant_lists" && w.registry == "content.package");
        check(staged, "the trimmed field is written under content.package, so its applier stages it");
    });
}
