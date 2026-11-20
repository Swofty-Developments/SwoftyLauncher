#include "rows_test.hpp"

void twelve_slot_tests(const std::string &regs)
{
    namespace ct = career::ops::teams;
    registry::reset_store();
    auto &st = registry::store();
    const std::string by = "bydanix.f1-2012-season", group = "bydanix_2012";
    st.set_order({by});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (auto [name, file] : {std::pair{"team.identity", "1.1.json"}, {"team.logo", "1.0.json"}, {"career.season", "1.0.json"}}) {
        std::string schema = slurp(regs + "/" + name + "/" + file);
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    const char *teams[] = {"red_bull", "mclaren",    "ferrari",  "mercedes", "lotus",    "force_india",
                           "sauber",   "toro_rosso", "williams", "caterham", "marussia", "hrt"};
    std::string logos, identities, slots, stock_slots;
    for (int i = 0; i < 12; i++) {
        std::string n = teams[i], sep = i ? "," : "";
        logos += sep + "\"" + n + "\": { \"images\": { \"team_select\": \"logos/" + n + "_ts.png\", \"hud\": \"logos/" + n +
                 "_hud.png\" } }";
        identities += sep + "\"" + n + "\": { \"donor\": \"game/williams_26\", \"names\": { \"title\": { \"literal\": \"" + n +
                      " 2012\" }, \"code\": \"T" + std::to_string(i) + "\" }, \"colours\": { \"menu\": \"#10203" +
                      std::to_string(i % 10) + "\", \"primary\": \"#10203" + std::to_string(i % 10) +
                      "\" }, \"livery_name\": \"c12_" + n + "\", \"logo\": \"" + by + "/" + n + "\", \"team_boss\": \"boss_" + n +
                      "\", \"team_base\": { \"literal\": \"Base " + n + "\" } }";
        slots += sep + "{ \"slot\": " + std::to_string(i + 1) + ", \"team\": \"" + by + "/" + n + "\" }";
    }
    for (int i = 0; i < 11; i++)
        stock_slots += std::string(i ? "," : "") + "{ \"slot\": " + std::to_string(i + 1) + ", \"team\": \"game/team26_" +
                       std::to_string(i) + "\" }";
    int r = registry::load_data_file(
        by, "registry/teams.json",
        "{ \"team.logo\": {" + logos + "}, \"team.identity\": {" + identities +
            "}, \"career.season\": { \"2012\": { \"data_group\": \"" + group + "\", \"year\": 2012, \"teams\": [" + slots +
            "] }, \"stock_2026\": { \"data_group\": \"f1_career_2026\", \"year\": 2026, \"teams\": [" + stock_slots + "] } } }");
    st.end_register();
    Value zero("#00000000");
    Value stock =
        Value::object({{"names", Value::object({{"title", Value::object({{"literal", Value("lng_stock")}})}})},
                       {"colours", Value::object({{"menu", zero}, {"primary", zero}, {"secondary", zero}, {"tertiary", zero}})},
                       {"livery_name", Value("x")}});
    st.add_base("team.identity", "williams_26", stock);
    for (int i = 0; i < 12; i++)
        st.add_base("team.identity", "c12_slot_" + std::to_string(i), stock);
    for (int i = 0; i < 11; i++)
        st.add_base("team.identity", "team26_" + std::to_string(i), stock);
    st.stock_loaded("team.identity", true);
    for (auto &f : st.refusals())
        std::printf("      refused: %s %s %s: %s\n", f.addon.c_str(), f.registry.c_str(), f.id.c_str(), f.message.c_str());
    check(r == SWOFTY_ASSET_OK, "12 team.identity records with logos, principals and bases and a 12-slot season register");

    std::vector<ct::SeasonIn> seasons = ct::seasons_of(st, group);
    check(seasons.size() == 1 && seasons[0].year == 2012 && seasons[0].slots.size() == 12,
          "the data group's seasons are its own 2012 record, not the f1_career_2026 one");
    check(ct::build_table(seasons, true).refusals.size() == 1, "12 slots with My Team stays refused (13 teams)");
    ct::Table table = ct::build_table(seasons, false);
    std::vector<std::string> physical;
    for (int i = 0; i < 12; i++)
        physical.push_back("c12_slot_" + std::to_string(i));
    ct::anchor_rows(table, physical);
    ct::Plan plan = ct::plan_season(table, seasons[0]);
    check(table.ok() && plan.ok() && plan.count(ct::Kind::dressed) == 12,
          "apply_teams plans all 12 slots dressed: " + ct::summary(plan));

    struct Rec {
        std::vector<uint8_t> bytes = std::vector<uint8_t>(0x768, 0);
        std::vector<std::string> bufs = std::vector<std::string>(5, std::string(0x80, '\0'));
        std::vector<uint8_t> before;
    };
    std::map<uint16_t, Rec> loaded;
    auto load = [&](uint16_t stat, const std::string &label) {
        Rec &rec = loaded[stat];
        std::size_t at[] = {content::team_record::DISPLAY_NAME, content::team_record::SHORT, content::team_record::SUPER_SHORT,
                            content::team_record::CODE, content::team_record::TEAM_BASE};
        for (std::size_t i = 0; i < 5; i++) {
            std::memcpy(rec.bufs[i].data(), label.data(), std::min<std::size_t>(label.size(), 0x7f));
            char *p = rec.bufs[i].data();
            std::memcpy(rec.bytes.data() + at[i], &p, sizeof p);
        }
        rec.bytes[content::team_record::TEAM_BOSS] = 3;
        rec.before = rec.bytes;
    };
    for (int i = 0; i < 12; i++)
        load(uint16_t(700 + i), "c12_slot_" + std::to_string(i));
    for (int i = 0; i < 11; i++)
        load(uint16_t(i), "team26_" + std::to_string(i));
    std::map<std::string, Value> keep;
    std::vector<std::string> log;
    auto &live = content::live();
    live = content::Live();
    live.log = [&](const std::string &l) { log.push_back(l); };
    live.identity = [&](const std::string &id) -> const Value * {
        auto rec = st.get("team.identity", id);
        return rec.status == registry::Status::ok ? &(keep[id] = rec.value) : nullptr;
    };
    live.stat_of = [](const std::string &) -> int64_t { return -1; };
    live.record = [&](uint16_t stat) -> uint8_t * {
        auto it = loaded.find(stat);
        return it == loaded.end() ? nullptr : it->second.bytes.data();
    };
    live.character_stat = [&](const std::string &label) -> int64_t {
        for (int i = 0; i < 12; i++)
            if (label == std::string("boss_") + teams[i])
                return 50 + i;
        return -1;
    };
    ct::dresser() = &content::dress_slot;
    int dressed = 0;
    for (auto &a : plan.actions)
        dressed += a.kind == ct::Kind::dressed && ct::dresser()(group.c_str(), a.slot, ct::identity_id(a.row).c_str(),
                                                                ct::identity_id(a.team).c_str(), 700 + a.slot - 1);
    bool all = dressed == 12;
    for (int i = 0; i < 12; i++) {
        Rec &rec = loaded[uint16_t(700 + i)];
        all &= std::string(rec.bufs[0].c_str()) == std::string(teams[i]) + " 2012" &&
               std::string(rec.bufs[3].c_str()) == "T" + std::to_string(i) &&
               std::string(rec.bufs[4].c_str()) == std::string("Base ") + teams[i] &&
               rec.bytes[content::team_record::TEAM_BOSS] == 50 + i;
    }
    check(all, "the 12 slot rows' loaded records wear the 12 teams' names, codes, bases and principals (" +
                   std::to_string(dressed) + " dressed)");
    bool untouched = true;
    for (int i = 0; i < 11; i++)
        untouched &= loaded[uint16_t(i)].bytes == loaded[uint16_t(i)].before &&
                     std::string(loaded[uint16_t(i)].bufs[0].c_str()) == "team26_" + std::to_string(i);
    check(untouched, "the f1_career_2026 rows' loaded records are untouched");
    content::Plan logo_plan;
    for (int i = 0; i < 12; i++) {
        auto logo = st.get("team.logo", by + "/" + teams[i]);
        Value v = logo.value;
        content::lower_logo_images(teams[i], "williams_26", by + "/" + teams[i],
                                   logo.status == registry::Status::ok ? &v : nullptr, teams[i], logo_plan);
    }
    int own_slices = 0;
    for (auto &l : logo_plan.logos)
        own_slices += l.array == std::string("team_logos.driver_career_2024_team_select") && !l.fallback && l.key == l.team;
    check(own_slices == 12,
          "each team's logo is built as slices under its own label, not the slot row's (" + std::to_string(own_slices) + ")");
    check(content::undress_all("the career unloaded") == 12 && loaded[700].bytes == loaded[700].before &&
              loaded[711].bytes == loaded[711].before && std::string(loaded[705].bufs[4].c_str()) == "c12_slot_5",
          "after the career unloads the 12 slot rows are their own rows again, base and principal included");
    ct::dresser() = nullptr;
}

void slot_tests()
{
    std::map<uintptr_t, uintptr_t> mem;
    uintptr_t vector = 0x1000, entries = 0x2000;
    mem[vector + 8] = entries;
    mem[vector + 0x10] = entries + 4 * 0x10;
    mem[entries + 3 * 0x10 + 8] = 0xabc000;
    auto read = [&](uintptr_t at, uintptr_t &out) {
        auto it = mem.find(at);
        if (it == mem.end())
            return false;
        out = it->second;
        return true;
    };
    check(team_slots::record_in(vector, 3, read) == 0xabc000, "a stat id's record is read through the slot vector");
    check(team_slots::record_in(vector, 4, read) == 0 && team_slots::record_in(0, 3, read) == 0,
          "a stat id past the vector's end has none");
    mem[vector + 0x10] = entries + 4 * 0x10 + 3;
    check(team_slots::record_in(vector, 3, read) == 0, "a vector whose size is not a whole number of entries is not read");
    check(team_slots::stat_of(600u << 16) == 600 && team_slots::stat_of(3u << 16) == 3,
          "the key carries the stat id in its high word");
    check(team_slots::database_key(600u << 16) && team_slots::database_key(0),
          "a loader key has no set byte: the low word is zero");
    check(!team_slots::database_key((600u << 16) | 1u), "a key with a set or flag byte set is not a database loader key");
}

void seat_entrant_tests()
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"ig.teampack", "swofty.progressive"});
    for (const char *name : {"career.season", "driver.identity", "team.identity"})
        st.declare(name, swofty::json::parse(R"({"type":"object"})"), false, "");
    st.add("ig.teampack", "team.identity", "ig_racing", swofty::json::parse(R"({"donor":"game/williams"})"), nullptr);
    st.add("ig.teampack", "driver.identity", "ig_driver",
           swofty::json::parse(R"({"donor":"game/jarno_opmeer","look":"ig.teampack/ig_look","number":77})"), nullptr);
    st.add("ig.teampack", "driver.identity", "stub", swofty::json::parse(R"({"names":{"code":"STB"}})"), nullptr);
    st.add("swofty.progressive", "career.season", "2016",
           swofty::json::parse(R"({"data_group":"f1_career_progressive","year":2016,"teams":[
               {"slot":1,"team":"game/cs16_mercedes","seats":["game/lewis_hamilton","game/nico_rosberg"]},
               {"slot":11,"team":"ig.teampack/ig_racing","seats":["ig.teampack/ig_driver","ig.teampack/stub"]}]})"),
           nullptr);
    st.add("swofty.progressive", "career.season", "2017",
           swofty::json::parse(R"({"data_group":"f1_career_progressive","year":2017,"teams":[
               {"slot":11,"team":"ig.teampack/ig_racing","seats":["ig.teampack/ig_driver",""]}]})"),
           nullptr);
    st.end_register();
    auto seats = content::added_seats(st);
    check(seats.size() == 2 && seats["ig.teampack/ig_driver"].size() == 2 && !seats.count("game/lewis_hamilton"),
          "an added team's seats are found in every season that names it; a game/ team's seats are not");

    Db db;
    auto row = [&](const std::string &prefix, std::initializer_list<std::pair<const char *, std::string>> fields) {
        for (auto &[f, v] : fields)
            db.put(prefix + "." + f, std::string_view(f) == "_metadata_" ? 4 : 2, v);
    };
    db.put("career.career_setup._CareerSetupDataGroup_instancecount_", 0, "1");
    row("career.career_setup.CareerSetupDataGroup_0", {{"_metadata_", "f1_career_progressive"}});
    db.put("career.career_setup.CareerSetupDataGroup_0.ComCareerSetup_0.entrant_list", 4, "f1_career_progressive");
    db.put("entrants.drivers._driver_instancecount_", 0, "2");
    row("entrants.drivers.driver_0", {{"_metadata_", "jarno_opmeer"}});
    row("entrants.drivers.driver_1", {{"_metadata_", "ig_driver"}});
    db.put("entrants.entrant_lists.entrants._entrant_instancecount_", 0, "1");
    row("entrants.entrant_lists.entrants.entrant_0", {{"_metadata_", "pg-daniel_ricciardo"},
                                                      {"driver", "daniel_ricciardo"},
                                                      {"driver_customisation", "x"},
                                                      {"car_number", "3"},
                                                      {"team", ""},
                                                      {"team_position", "-1"},
                                                      {"vehicle", ""},
                                                      {"vehicle_customisation", ""},
                                                      {"badge_customisation", ""},
                                                      {"entrant_reference", ""}});
    db.put("entrants.entrant_lists._entrant_list_instancecount_", 0, "3");
    row("entrants.entrant_lists.entrant_list_0",
        {{"_metadata_", "f1_career_progressive"}, {"entrant_list_references", "f1_2026,f1_career_progressive_driver_pool"}});
    db.put("entrants.entrant_lists.entrant_list_0._entrant_instancecount_", 0, "0");
    row("entrants.entrant_lists.entrant_list_1", {{"_metadata_", "f1_2026"}});
    db.put("entrants.entrant_lists.entrant_list_1._entrant_instancecount_", 0, "1");
    row("entrants.entrant_lists.entrant_list_1.entrant_0",
        {{"_metadata_", "e"}, {"team", "mercedes_26"}, {"team_position", "0"}, {"entrant_reference", "pg-daniel_ricciardo"}});
    row("entrants.entrant_lists.entrant_list_2", {{"_metadata_", "f1_career_progressive_driver_pool"}});
    db.put("entrants.entrant_lists.entrant_list_2._entrant_instancecount_", 0, "1");
    row("entrants.entrant_lists.entrant_list_2.entrant_0", {{"_metadata_", "pg_pool_daniel_ricciardo"},
                                                            {"entrant_reference", "pg-daniel_ricciardo"},
                                                            {"team", ""},
                                                            {"vehicle", ""},
                                                            {"team_position", "-1"},
                                                            {"car_number", "255"},
                                                            {"badge_customisation", ""},
                                                            {"vehicle_customisation", ""},
                                                            {"driver", ""},
                                                            {"driver_customisation", ""}});
    content::Expansion x(db.base());
    std::map<std::string, Value> keep;
    content::Lookup lookup = content::own_lookup(st, keep);
    content::Read read = [&x](const std::string &path) -> std::optional<std::string> {
        auto p = x.engine.resolve(path);
        return p ? x.engine.text(*p) : std::nullopt;
    };
    content::lower_seat_entrants(st, x, lookup, read, {});
    for (auto &l : x.lines["driver.identity"])
        std::printf("      %s\n", l.c_str());
    check(written(x.engine, "entrants.entrant_lists.entrants.entrant_1._metadata_") == "sw-ig_driver" &&
              written(x.engine, "entrants.entrant_lists.entrants.entrant_1.driver") == "ig_driver" &&
              written(x.engine, "entrants.entrant_lists.entrants.entrant_1.team_position") == "-1" &&
              written(x.engine, "entrants.entrant_lists.entrants.entrant_1.car_number") == "77",
          "the added team's new driver gets one teamless entrant with his number");
    check(written(x.engine, "entrants.entrant_lists.entrant_list_2.entrant_1._metadata_") == "sw_pool_ig_driver" &&
              written(x.engine, "entrants.entrant_lists.entrant_list_2.entrant_1.entrant_reference") == "sw-ig_driver" &&
              written(x.engine, "entrants.entrant_lists.entrant_list_2._entrant_instancecount_") == "2",
          "and an entry next to the pool's own in the teamless list the career list references");
    check(!written(x.engine, "entrants.entrant_lists.entrant_list_1._entrant_instancecount_") &&
              !written(x.engine, "entrants.entrant_lists.entrant_list_0._entrant_instancecount_"),
          "the seated list and the 0-entry career list are left alone");
    check(x.lines["driver.identity"].size() == 1, "a driver with no donor (a row something else made) gets nothing");
    content::Expansion y(db.base());
    content::Read ready = [&y](const std::string &path) -> std::optional<std::string> {
        auto p = y.engine.resolve(path);
        return p ? y.engine.text(*p) : std::nullopt;
    };
    content::lower_seat_entrants(st, y, lookup, ready, {"ig.teampack"});
    check(y.engine.writes().empty(), "an addon left out of the boot apply adds no entrant");
}

static Value package_of(int teams, int seats)
{
    Value list = Value::array();
    for (int t = 0; t < teams; t++) {
        Value drivers = Value::array();
        for (int d = 0; d < seats; d++)
            drivers.push_back(
                Value::object({{"driver", "game/d" + std::to_string(t * seats + d)}, {"number", t * seats + d + 1}}));
        list.push_back(Value::object({{"key", "t" + std::to_string(t)}, {"drivers", drivers}}));
    }
    return Value::object({{"entrant_list", "ex_f1_2012"}, {"teams", list}});
}

void box_time_trial_tests()
{
    Value grid = package_of(12, 2), small = package_of(11, 2);
    content::Lookup lookup = [&](const std::string &reg, const std::string &id) -> const Value * {
        if (reg != "content.package")
            return nullptr;
        return id == "mod.example/f1_2012" ? &grid : id == "mod.example/f1_2025ish" ? &small : nullptr;
    };
    auto lower = [&](const char *json) {
        content::Plan plan;
        content::lower_box("mod.example/box", swofty::json::parse(json), lookup, plan);
        return plan;
    };
    auto warned = [](const content::Plan &p) {
        for (auto &l : p.lines)
            if (l.find("more than a Time Trial session holds (23)") != std::string::npos)
                return true;
        return false;
    };
    content::Plan p = lower(R"({"package":"mod.example/f1_2012","time_trial":{"entrant_list":"ex_f1_2012_tt"}})");
    check(p.boxes.size() == 1 && p.boxes[0].entrant_list == "ex_f1_2012" && p.boxes[0].time_trial == "ex_f1_2012_tt" &&
              !warned(p),
          "an era.box's time_trial entrant_list reaches the box, and a 24-car package with one is not warned about");
    p = lower(R"({"package":"mod.example/f1_2012"})");
    check(p.boxes.size() == 1 && p.boxes[0].time_trial.empty() && warned(p),
          "a 24-car package's box without a Time Trial list is kept and warned about");
    p = lower(R"({"package":"mod.example/f1_2012","time_trial":{"entrant_list":"ex_f1_2012"}})");
    check(p.boxes.size() == 1 && p.boxes[0].time_trial.empty() && warned(p),
          "a Time Trial list equal to the package's list is the package's list");
    p = lower(R"({"package":"mod.example/f1_2025ish"})");
    check(p.boxes.size() == 1 && p.boxes[0].time_trial.empty() && !warned(p), "a 22-car package's box needs no Time Trial list");
    p = lower(R"({"package":"mod.example/f1_2025ish"})");
    check(p.boxes.size() == 1 && p.boxes[0].only == 0, "an era.box without only shows on both pickers");
    p = lower(R"({"package":"mod.example/f1_2025ish","only":"time_trial"})");
    check(p.boxes.size() == 1 && p.boxes[0].only == 2, "an era.box's only time_trial reaches the box");
    p = lower(R"({"package":"mod.example/f1_2012","only":"grand_prix"})");
    check(p.boxes.size() == 1 && p.boxes[0].only == 1 && !warned(p),
          "a Grand Prix only box is not warned about its Time Trial car count");
    p = lower(R"({"package":"mod.example/f1_2025ish","only":null})");
    check(p.boxes.size() == 1 && p.boxes[0].only == 0, "a null only is both pickers");
}

void drs_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.a", "mod.b"});
    st.set_validator(&registry::schema_validator());
    std::string schema = slurp(regs + "/rules.drs/1.0.json");
    registry::table()->declare("rules.drs", SwoftyText{schema.data(), schema.size()});
    int ra = registry::load_data_file("mod.a", "registry/drs.json", R"j({
      "rules.drs": { "never": { "race_start": 200, "red_flag_restart": 200, "safety_car_end": 200 },
                     "stock": {},
                     "late": { "extends": "mod.a/stock", "race_start": 3 } } })j");
    registry::load_data_file("mod.b", "registry/drs.json", R"j({ "rules.drs": { "bad": { "race_start": -1 } } })j");
    st.end_register();
    check(ra == SWOFTY_ASSET_OK && st.get("rules.drs", "mod.a/late").status == registry::Status::ok &&
              st.get("rules.drs", "mod.b/bad").status != registry::Status::ok,
          "rules.drs records register; a negative lap is refused by the schema");

    Db db;
    auto group = [&](int i, const char *label, const char *start) {
        std::string g = std::string(content::DRS_GROUPS) + "_" + std::to_string(i);
        db.put(g + "._metadata_", 2, label);
        db.put(g + "._drs_allowed_lap_instancecount_", 0, "3");
        const char *laps[] = {"RaceStart", "RedFlagRestart", "FullSafetyCarEnd"};
        for (int j = 0; j < 3; j++) {
            db.put(g + ".drs_allowed_lap_" + std::to_string(j) + "._metadata_", 2, laps[j]);
            db.put(g + ".drs_allowed_lap_" + std::to_string(j) + ".lap_number", 0, j ? "1" : start);
        }
    };
    db.put("drs.drs_allowed_lap._drs_allowed_lap_group_instancecount_", 0, "3");
    group(0, "f1_2024", "1");
    group(1, "f2_2023", "1");
    group(2, "f1_elimination", "0");
    content::StatIds stat = [](const std::string &, const std::string &, int64_t pin) -> int64_t { return pin >= 0 ? pin : 10; };
    content::Expansion x(db.base());
    content::expand(st, x, stat);
    for (auto &l : x.lines["rules.drs"])
        std::printf("      %s\n", l.c_str());
    check(x.records["rules.drs"] == 3 && x.refused["rules.drs"] == 0 && x.applied["rules.drs"].size() == 3,
          "three rules.drs records apply");
    check(written(x.engine, "drs.drs_allowed_lap._drs_allowed_lap_group_instancecount_") == "6",
          "the group count grows by three");
    std::map<std::string, std::string> rows;
    for (int i = 3; i < 6; i++)
        if (auto l = written(x.engine, std::string(content::DRS_GROUPS) + "_" + std::to_string(i) + "._metadata_"))
            rows[*l] = std::string(content::DRS_GROUPS) + "_" + std::to_string(i);
    std::string never = content::drs_label("mod.a/never"), late = content::drs_label("mod.a/late");
    check(rows.count(never) && rows.count(late) && rows.count(content::drs_label("mod.a/stock")),
          "each record is a group row labelled " + never);
    auto lap = [&](const std::string &label, int j) {
        return written(x.engine, rows[label] + ".drs_allowed_lap_" + std::to_string(j) + ".lap_number").value_or("-");
    };
    auto label_of = [&](const std::string &label, int j) {
        return written(x.engine, rows[label] + ".drs_allowed_lap_" + std::to_string(j) + "._metadata_").value_or("-");
    };
    check(lap(never, 0) == "200" && lap(never, 1) == "200" && lap(never, 2) == "200" && label_of(never, 0) == "RaceStart" &&
              label_of(never, 2) == "FullSafetyCarEnd",
          "the never group's RaceStart, RedFlagRestart and FullSafetyCarEnd rows hold 200");
    check(lap(late, 0) == "3" && lap(late, 1) == "1" && lap(late, 2) == "1",
          "a variant sets its race start and keeps the base's 1s");
    check(written(x.engine, std::string(content::DRS_GROUPS) + "_0.drs_allowed_lap_0.lap_number") == std::nullopt,
          "the stock f1_2024 group is not written");

    check(never.rfind("swofty_drs_never_", 0) == 0 && never.size() == 25 && never != content::drs_label("mod.b/never"),
          "a group label is swofty_drs_<key>_<8 hex of the id>, unique per id");
    std::string long_label = content::drs_label("mod.a/" + std::string(80, 'k') + "-x");
    check(long_label.size() <= content::DRS_LABEL_MAX && long_label.find('-') == std::string::npos,
          "a long or unplain key is cut and made plain within 63 characters");
    std::string why;
    check(!content::drs_laps(swofty::json::parse(R"({"race_start": 10000})"), &why) && !why.empty() &&
              content::drs_laps(swofty::json::parse("{}"), nullptr)->race_start == 1,
          "a lap past 9999 is refused; a missing lap is the stock 1");

    std::map<std::string, content::DrsGroup> groups = {{"mod.a/never", {never, {200, 200, 200}}}};
    Value p2010 = swofty::json::parse(R"({"entrant_list":"cs_2010","rules":{"drs":"mod.a/never"}})");
    Value p2011 = swofty::json::parse(R"({"entrant_list":"cs_2011","rules":{"drs":null}})");
    Value p2012 = swofty::json::parse(R"({"entrant_list":"cs_2012","rules":{"drs":"mod.b/bad"}})");
    std::vector<std::pair<std::string, const Value *>> packages = {
        {"mod.a/f1_2010", &p2010}, {"mod.a/f1_2011", &p2011}, {"mod.a/f1_2012", &p2012}};
    auto pick = content::drs_pick("cs_2010", packages, groups);
    std::printf("      %s\n", pick.line.c_str());
    check(pick.group && pick.group->label == never && pick.package == "mod.a/f1_2010" && pick.rule == "mod.a/never",
          "a box whose package names an applied rule runs its group");
    check(!content::drs_pick("cs_2011", packages, groups).group && !content::drs_pick("cs_2012", packages, groups).group &&
              !content::drs_pick("f1_2025", packages, groups).group,
          "no rule, a rule not added at boot, or a stock box: no group");
    Value p2010b = swofty::json::parse(R"({"entrant_list":"cs_2010"})");
    packages.push_back({"mod.b/f1_2010", &p2010b});
    check(!content::drs_pick("cs_2010", packages, groups).group, "the last package with the box's entrant list decides");

    Value w1959 = swofty::json::parse(R"({"entrant_list":"f1_1959","rules":{"tyre_warmers":false}})");
    Value w2009 = swofty::json::parse(R"({"entrant_list":"f1_2009","rules":{"tyre_warmers":null}})");
    Value w2010 = swofty::json::parse(R"({"entrant_list":"f1_2010","rules":{"tyre_warmers":"off"}})");
    std::vector<std::pair<std::string, const Value *>> boxes = {
        {"mod.a/f1_1959", &w1959}, {"mod.a/f1_2009", &w2009}, {"mod.a/f1_2010", &w2010}};
    auto warm = content::tyre_warmers_pick("f1_1959", boxes);
    std::printf("      %s\n", warm.line.c_str());
    check(warm.warmers == false && warm.package == "mod.a/f1_1959",
          "a box whose package sets rules.tyre_warmers runs that setting");
    check(!content::tyre_warmers_pick("f1_2009", boxes).warmers && !content::tyre_warmers_pick("f1_2010", boxes).warmers &&
              !content::tyre_warmers_pick("f1_2025", boxes).warmers,
          "null, a non-bool or a stock box: the mode keeps its own tyre warmer setting");
    Value w1959b = swofty::json::parse(R"({"entrant_list":"f1_1959","rules":{"tyre_warmers":true}})");
    boxes.push_back({"mod.b/f1_1959", &w1959b});
    check(content::tyre_warmers_pick("f1_1959", boxes).warmers == true, "the last package with the box's entrant list decides");
    Value c1950 = swofty::json::parse(R"({"entrant_list":"f1_1950","rules":{"mandatory_tyre_change":false}})");
    Value c2007 = swofty::json::parse(R"({"entrant_list":"f1_2007","rules":{"mandatory_tyre_change":true}})");
    Value c2008 = swofty::json::parse(R"({"entrant_list":"f1_2008","rules":{"mandatory_tyre_change":null}})");
    std::vector<std::pair<std::string, const Value *>> change = {
        {"mod.a/f1_1950", &c1950}, {"mod.a/f1_2007", &c2007}, {"mod.a/f1_2008", &c2008}};
    auto chg = content::tyre_change_pick("f1_1950", change);
    std::printf("      %s\n", chg.line.c_str());
    check(chg.required == false && content::tyre_change_pick("f1_2007", change).required == true,
          "a box whose package sets rules.mandatory_tyre_change runs that rule");
    check(!content::tyre_change_pick("f1_2008", change).required && !content::tyre_change_pick("f1_2025", change).required,
          "null or a stock box: the mode keeps its own tyre regulations");

    Value k1959 = swofty::json::parse(
        R"({"entrant_list":"f1_1959","rules":{"race_time_limit_s":10800,"weekend_mode":"swofty_f1_short_quali"}})");
    Value k2004 = swofty::json::parse(R"({"entrant_list":"f1_2004","rules":{"weekend_mode":"swofty_f1_one_shot"}})");
    Value k2009 = swofty::json::parse(R"({"entrant_list":"f1_2009","rules":{"race_time_limit_s":30,"weekend_mode":"Bad Name"}})");
    Value k2010 =
        swofty::json::parse(R"({"entrant_list":"f1_2010","rules":{"race_time_limit_s":"3h","weekend_mode":"swofty_f1"}})");
    std::vector<std::pair<std::string, const Value *>> weekends = {
        {"mod.a/f1_1959", &k1959}, {"mod.a/f1_2004", &k2004}, {"mod.a/f1_2009", &k2009}, {"mod.a/f1_2010", &k2010}};
    auto wk = content::weekend_pick("f1_1959", weekends);
    std::printf("      %s\n", wk.line.c_str());
    check(wk.time_limit_s == 10800u && wk.weekend_mode == "swofty_f1_short_quali" && wk.package == "mod.a/f1_1959",
          "a box whose package sets rules.race_time_limit_s and rules.weekend_mode runs both");
    auto wk04 = content::weekend_pick("f1_2004", weekends);
    check(!wk04.time_limit_s && wk04.weekend_mode == "swofty_f1_one_shot", "the weekend rule alone keeps the mode's time limit");
    auto wk09 = content::weekend_pick("f1_2009", weekends);
    auto wk10 = content::weekend_pick("f1_2010", weekends);
    check(!wk09.time_limit_s && wk09.weekend_mode.empty() && !wk10.time_limit_s && wk10.weekend_mode.empty(),
          "a limit under 60 s or not a whole number, a malformed mode name or the host itself: the mode keeps its own");
    auto wk25 = content::weekend_pick("f1_2025", weekends);
    check(!wk25.time_limit_s && wk25.weekend_mode.empty(), "a stock box: the mode keeps its own time limit and weekend");
    Value k1959b = swofty::json::parse(R"({"entrant_list":"f1_1959","rules":{"race_time_limit_s":7200}})");
    weekends.push_back({"mod.b/f1_1959", &k1959b});
    auto wkb = content::weekend_pick("f1_1959", weekends);
    check(wkb.time_limit_s == 7200u && wkb.weekend_mode.empty(), "the last package with the box's entrant list decides");

    // The helper row is made available at boot (the game parses only available rows into its setup mode store).
    std::map<std::string, std::string> avail = {
        {"game_setup.game_setup_modes.game_setup_mode[_metadata_=swofty_f1_short_quali].available", "False"},
        {"game_setup.game_setup_modes.game_setup_mode[_metadata_=swofty_f1_one_shot].available", "True"}};
    content::Read aread = [&](const std::string &p) -> std::optional<std::string> {
        auto it = avail.find(p);
        return it == avail.end() ? std::nullopt : std::optional<std::string>(it->second);
    };
    content::Plan wp;
    content::lower_weekend_mode("mod.a/f1_1959", k1959, "swofty_f1", aread, wp);
    check(wp.ops.size() == 1 && wp.ops[0]["op"].as_string() == "set" && wp.ops[0]["field"].as_string() == "available" &&
              wp.ops[0]["value"].as_string() == "True" &&
              wp.ops[0]["select"].as_string() == "game_setup.game_setup_modes.game_setup_mode[_metadata_=swofty_f1_short_quali]",
          "a box's weekend mode row that ships available=False is set available");
    content::Plan wp04, wp09, wp10, wpx;
    content::lower_weekend_mode("mod.a/f1_2004", k2004, "swofty_f1", aread, wp04);
    content::lower_weekend_mode("mod.a/f1_2009", k2009, "swofty_f1", aread, wp09);
    content::lower_weekend_mode("mod.a/f1_2010", k2010, "swofty_f1", aread, wp10);
    check(wp04.ops.size() == 0 && wp09.ops.size() == 0 && wp10.ops.size() == 0,
          "an already available row, a malformed name or the host itself: nothing written");
    Value kx = swofty::json::parse(R"({"entrant_list":"f1_1951","rules":{"weekend_mode":"f51_wk"}})");
    content::lower_weekend_mode("mod.a/f1_1951", kx, "swofty_f1", aread, wpx);
    check(wpx.ops.size() == 0 && wpx.lines.size() == 1 && wpx.lines[0].find("f51_wk") != std::string::npos,
          "a missing row writes nothing (the package still applies) and says so");

    Value s1959 = swofty::json::parse(
        R"({"entrant_list":"f1_1959","rules":{"safety_car":"off","formation_lap":false,"penalties":"corner_cutting_only","corner_cutting":"regular"}})");
    Value s1993 = swofty::json::parse(
        R"({"entrant_list":"f1_1993","rules":{"safety_car":"standard","formation_lap":true,"penalties":"on"}})");
    Value s2000 = swofty::json::parse(R"({"entrant_list":"f1_2000","rules":{"safety_car":null,"formation_lap":null}})");
    Value s2001 = swofty::json::parse(
        R"({"entrant_list":"f1_2001","rules":{"safety_car":"none","formation_lap":"off","penalties":"reduced","corner_cutting":"lax"}})");
    std::vector<std::pair<std::string, const Value *>> sc = {
        {"mod.a/f1_1959", &s1959}, {"mod.a/f1_1993", &s1993}, {"mod.a/f1_2000", &s2000}, {"mod.a/f1_2001", &s2001}};
    const size_t SC = size_t(content::box_option_index("safety_car")), FL = size_t(content::box_option_index("formation_lap")),
                 PE = size_t(content::box_option_index("penalties")), CC = size_t(content::box_option_index("corner_cutting"));
    check(content::box_option_index("refuelling") == -1 && SC != FL && PE != CC, "the four option rules have their own slots");
    auto sc59 = content::box_options_pick("f1_1959", sc);
    std::printf("      %s\n", sc59.line.c_str());
    check(sc59.settings[SC] == std::optional<std::string>("off") && sc59.package == "mod.a/f1_1959" &&
              content::box_options_pick("f1_1993", sc).settings[SC] == std::optional<std::string>("standard"),
          "a box whose package sets rules.safety_car runs that setting");
    check(sc59.settings[FL] == std::optional<std::string>("off") &&
              content::box_options_pick("f1_1993", sc).settings[FL] == std::optional<std::string>("on"),
          "rules.formation_lap false / true picks the option's off / on");
    check(sc59.settings[PE] == std::optional<std::string>("corner_cutting_only") &&
              sc59.settings[CC] == std::optional<std::string>("regular") &&
              content::box_options_pick("f1_1993", sc).settings[PE] == std::optional<std::string>("on") &&
              !content::box_options_pick("f1_1993", sc).settings[CC],
          "rules.penalties and rules.corner_cutting pick their settings; a missing rule picks none");
    auto sc01 = content::box_options_pick("f1_2001", sc);
    check(!content::box_options_pick("f1_2000", sc).settings[SC] && !content::box_options_pick("f1_2000", sc).settings[FL] &&
              !sc01.settings[SC] && !sc01.settings[FL] && !sc01.settings[PE] && !sc01.settings[CC] &&
              !content::box_options_pick("f1_2025", sc).settings[SC],
          "null, an unknown setting, a wrong type or a stock box: the player's settings stay");
    std::vector<std::string> names = {"safety_car_standard", "Off", "reduced", "increased"};
    check(content::box_option_value(names, "safety_car", "off") == 1 &&
              content::box_option_value(names, "safety_car", "standard") == 0 &&
              content::box_option_value({"low", "normal", "high", "none"}, "safety_car", "off") == 3 &&
              content::box_option_value({"low", "normal"}, "safety_car", "off") == -1,
          "a setting matches a value named after it, one ending in _<name>, or a known alias; none: -1");
    // the stock value lists (options_system.options.game, gamedatabases.2025.bin)
    check(content::box_option_value({"off", "reduced", "standard", "increased"}, "safety_car", "increased") == 3 &&
              content::box_option_value({"off", "on"}, "formation_lap", "off") == 0 &&
              content::box_option_value({"off", "on"}, "formation_lap", "on") == 1 &&
              content::box_option_value({"off", "corner_cutting_only", "eSports", "on"}, "penalties", "esports") == 2 &&
              content::box_option_value({"off", "corner_cutting_only", "eSports", "on"}, "penalties", "corner_cutting_only") ==
                  1 &&
              content::box_option_value({"off", "corner_cutting_only", "eSports", "on"}, "penalties", "on") == 3 &&
              content::box_option_value({"regular", "strict"}, "corner_cutting", "strict") == 1,
          "every setting finds its value in the stock option's value list");
    check(std::string(content::BOX_OPTION_RULES[SC].options[0]) == "safety_car_frequency" &&
              std::string(content::BOX_OPTION_RULES[FL].options[0]) == "formation_lap" &&
              std::string(content::BOX_OPTION_RULES[PE].options[0]) == "rules_and_penalties_mode" &&
              std::string(content::BOX_OPTION_RULES[CC].options[0]) == "corner_cutting_mode",
          "each rule tries the stock database's option name first");
    std::vector<int> opt = {2};
    auto get = [&](const void *) { return opt[0]; };
    auto set = [&](const void *, int i) { opt[0] = i; };
    content::OptionForce f;
    int o1 = 0, o2 = 0;
    check(f.force(&o1, 0, get, set) && opt[0] == 0 && f.prior == 2, "a press sets the box's index and keeps the player's");
    check(f.force(&o1, 0, get, set) && f.prior == 2, "a second press of the same box keeps the player's index");
    check(f.put_back(&o1, get, set) == 1 && opt[0] == 2 && !f.active(), "another box gives the player's index back");
    f.force(&o1, 0, get, set);
    opt[0] = 3; // the player picks another setting after the press
    check(f.put_back(&o1, get, set) == 0 && opt[0] == 3, "a setting the player picks after the press stays");
    f.force(&o1, 0, get, set);
    check(f.put_back(&o2, get, set) == 0 && opt[0] == 0 && !f.active(), "a changed option object is left as it is");

    Value g1990 = swofty::json::parse(R"({"entrant_list":"f1_1990","rules":{"grid":{"qualifying":"best_of","rows":null}}})");
    Value g2000 = swofty::json::parse(R"({"entrant_list":"f1_2000","rules":{"grid":{"rule_107":true}}})");
    Value g2005 = swofty::json::parse(R"({"entrant_list":"f1_2005","rules":{"grid":{"qualifying":"aggregate"}}})");
    Value g1955 = swofty::json::parse(R"({"entrant_list":"f1_1955","rules":{"grid":{"qualifying":"best_of","rows":[3,2]}}})");
    Value g2008 = swofty::json::parse(R"({"entrant_list":"f1_2008","rules":{"grid":null}})");
    std::vector<std::pair<std::string, const Value *>> grids = {{"mod.a/f1_1990", &g1990},
                                                                {"mod.a/f1_2000", &g2000},
                                                                {"mod.a/f1_2005", &g2005},
                                                                {"mod.a/f1_1955", &g1955},
                                                                {"mod.a/f1_2008", &g2008}};
    auto gp = content::grid_pick("f1_1955", grids);
    std::printf("      %s\n", gp.line.c_str());
    check(gp.rule && gp.rule->qualifying == runtime::box_grid::Scoring::best_of && gp.rule->rows == std::vector<int>({3, 2}) &&
              !gp.rule->rule_107,
          "rules.grid best_of with a 3-2 row pattern");
    auto g00 = content::grid_pick("f1_2000", grids);
    check(g00.rule && g00.rule->qualifying == runtime::box_grid::Scoring::best_lap && g00.rule->rule_107 && g00.rule->reranks(),
          "rule_107 alone keeps best_lap scoring and re-ranks");
    check(content::grid_pick("f1_2005", grids).rule->qualifying == runtime::box_grid::Scoring::aggregate &&
              content::grid_pick("f1_1990", grids).rule->rows.empty(),
          "aggregate scoring; rows null is the track's own grid");
    check(!content::grid_pick("f1_2008", grids).rule && !content::grid_pick("f1_2025", grids).rule,
          "grid null or a stock box: the game's own grid");

    Value q1985 = swofty::json::parse(R"({"entrant_list":"f1_1985","rules":{"qualifying_tyres":["softer"]}})");
    Value q1990 =
        swofty::json::parse(R"({"entrant_list":"f1_1990","rules":{"qualifying_tyres":["medium","softer","softer","wet"]}})");
    Value q1991 = swofty::json::parse(R"({"entrant_list":"f1_1991","rules":{"qualifying_tyres":null}})");
    Value q1992 = swofty::json::parse(R"({"entrant_list":"f1_1992","rules":{"qualifying_tyres":"softer"}})");
    std::vector<std::pair<std::string, const Value *>> qboxes = {
        {"mod.a/f1_1985", &q1985}, {"mod.a/f1_1990", &q1990}, {"mod.a/f1_1991", &q1991}, {"mod.a/f1_1992", &q1992}};
    auto q = content::quali_tyres_pick("f1_1985", qboxes);
    std::printf("      %s\n", q.line.c_str());
    check(q.mask == 1 && q.package == "mod.a/f1_1985", "rules.qualifying_tyres [softer] masks class option 0");
    check(content::quali_tyres_pick("f1_1990", qboxes).mask == 3, "names map to options 0..2, duplicates and wet are left out");
    check(!content::quali_tyres_pick("f1_1991", qboxes).mask && !content::quali_tyres_pick("f1_1992", qboxes).mask &&
              !content::quali_tyres_pick("f1_2025", qboxes).mask,
          "null, a non-array or a stock box: no qualifying tyre rule");
    Value q1985b = swofty::json::parse(R"({"entrant_list":"f1_1985","rules":{}})");
    qboxes.push_back({"mod.b/f1_1985", &q1985b});
    check(!content::quali_tyres_pick("f1_1985", qboxes).mask,
          "the last package with the box's entrant list decides (qualifying tyres)");
    check(content::quali_refused(15, 0, 1) && content::quali_refused(16, 0, 1) && content::quali_refused(17, 0, 1),
          "a masked option is refused in Race, Race2 and Race3");
    check(!content::quali_refused(5, 0, 1) && !content::quali_refused(9, 0, 1) && !content::quali_refused(1, 0, 1) &&
              !content::quali_refused(12, 0, 1) && !content::quali_refused(18, 0, 1),
          "a masked option stays usable in practice, qualifying and the sprint shootout");
    check(!content::quali_refused(15, 1, 1) && !content::quali_refused(15, 3, 7) && !content::quali_refused(15, 4, 0xff) &&
              !content::quali_refused(15, 0, 0) && !content::quali_refused(15, 0xffffffffu, 0xff),
          "other options, inter / wet, no mask and an unknown option are never refused");
    uint32_t pref[5] = {0, 1, 2, 3, 4}, out[5] = {};
    check(content::quali_filter_preference(15, 1, pref, out) == 1 && out[0] == content::QUALI_NO_SLOT && out[1] == 1 &&
              out[2] == 2 && out[3] == 3 && out[4] == 4,
          "in a race the set choice skips the qualifying option and keeps the order of the rest");
    uint32_t pref2[5] = {2, 0, 1, 3, 4};
    check(content::quali_filter_preference(5, 1, pref2, out) == 0 && !memcmp(out, pref2, sizeof out),
          "in qualifying the preference list is unchanged");
}
