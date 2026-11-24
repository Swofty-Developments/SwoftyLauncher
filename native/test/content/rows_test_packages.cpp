#include "rows_test.hpp"

// The stock wide rows next to the ones a 24-car package copies (f1_2025 and f1_teams_2024 from stock_package_rows):
// f1_2026 holds 22 entrants and f1_teams_2026 11 teams, as in the game; a creator's own 12-team standings row (made by
// a database operation) sits next to them.
static void stock_wide_rows(Db &db)
{
    stock_package_rows(db);
    auto row = [&](const std::string &prefix, std::initializer_list<std::pair<const char *, std::string>> fields) {
        for (auto &[f, v] : fields)
            db.put(prefix + "." + f, std::string_view(f) == "_metadata_" ? 4 : 2, v);
    };
    db.put("entrants.entrant_lists._entrant_list_instancecount_", 0, "2");
    row("entrants.entrant_lists.entrant_list_1", {{"_metadata_", "f1_2026"},
                                                  {"display_name", "x"},
                                                  {"previous_season_team_standings", "f1_teams_2025"},
                                                  {"previous_season_driver_standings", "x"},
                                                  {"leaderboard", "f1"}});
    db.put("entrants.entrant_lists.entrant_list_1.stat_id", 0, "111");
    db.put("entrants.entrant_lists.entrant_list_1._entrant_instancecount_", 0, "22");
    for (int i = 0; i < 22; i++)
        row("entrants.entrant_lists.entrant_list_1.entrant_" + std::to_string(i), {{"_metadata_", "e" + std::to_string(i)},
                                                                                   {"team", "t"},
                                                                                   {"vehicle", "v"},
                                                                                   {"entrant_reference", "r"},
                                                                                   {"team_position", std::to_string(i % 2)},
                                                                                   {"car_number", "1"},
                                                                                   {"driver", "d"},
                                                                                   {"driver_customisation", "c"}});
    db.put("entrants.standings._standings_instancecount_", 0, "3");
    row("entrants.standings.standings_1", {{"_metadata_", "f1_teams_2026"}});
    db.put("entrants.standings.standings_1._team_reference_instancecount_", 0, "11");
    for (int i = 0; i < 11; i++)
        row("entrants.standings.standings_1.team_reference_" + std::to_string(i),
            {{"_metadata_", "s" + std::to_string(i)}, {"team", "stock_26"}});
    row("entrants.standings.standings_2", {{"_metadata_", "bdx_s5_teams"}});
    db.put("entrants.standings.standings_2._team_reference_instancecount_", 0, "12");
    for (int i = 0; i < 12; i++)
        row("entrants.standings.standings_2.team_reference_" + std::to_string(i),
            {{"_metadata_", "b" + std::to_string(i)}, {"team", "bdx"}});
    db.put("entrants.entrant_setup_modes.entrant_setup_mode_0.max_entrants", 0, "20");
}

static std::string twelve_team_package(const char *extra)
{
    static const char *keys[] = {"redbull", "mclaren",   "ferrari",  "mercedes", "lotus", "forceindia",
                                 "sauber",  "tororosso", "williams", "caterham", "hrt",   "marussia"};
    std::string teams;
    for (int t = 0; t < 12; t++) {
        std::string k = keys[t];
        teams += std::string(t ? "," : "") + R"({ "key": ")" + k + R"(", "vehicle": "game/)" + k + R"(_2012", "drivers": [ )" +
                 R"({ "driver": "game/)" + k + R"(_a", "number": )" + std::to_string(2 * t + 1) + R"(, "look": null }, )" +
                 R"({ "driver": "game/)" + k + R"(_b", "number": )" + std::to_string(2 * t + 2) + R"(, "look": null } ] })";
    }
    return R"({ "content.package": { "f1_2012": { "title": "lng_f2012", "entrant_list": "f2012", "season": 2012,
        "teams": [ )" +
           teams + R"( ], "database": { "entrant_list": "f2012", "entrant_prefix": "f12-", "standings": "f2012_teams",
        "content_sets": ["2025"] )" +
           extra + R"( } } } })";
}

void twelve_team_tests(const std::string &regs)
{
    for (int pass = 0; pass < 2; pass++) {
        bool own_row = pass == 1;
        registry::reset_store();
        auto &st = registry::store();
        st.set_order({"bydanix.f1-2012-season"});
        st.set_validator(&registry::schema_validator());
        const SwoftyRegistry *t = registry::table();
        for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
            std::string schema = slurp(regs + "/" + name + "/1.0.json");
            t->declare(name, SwoftyText{schema.data(), schema.size()});
        }
        std::string data = twelve_team_package(own_row ? R"(, "standings_from": "bdx_s5_teams")" : "");
        int r = registry::load_data_file("bydanix.f1-2012-season", "registry/content.json", data);
        st.end_register();
        check(r == SWOFTY_ASSET_OK,
              std::string("a 12-team, 24-car package registers") + (own_row ? " (own standings_from)" : ""));
        Db db;
        stock_wide_rows(db);
        content::Expansion x(db.base());
        content::expand(st, x, {});
        int refused = 0;
        for (auto &[k, n] : x.refused)
            refused += n;
        for (auto &l : x.lines["content.package"])
            std::printf("      %s\n", l.c_str());
        std::string standings, list;
        for (int i = 0; i < 8; i++) {
            std::string sp = "entrants.standings.standings_" + std::to_string(i);
            std::string lp = "entrants.entrant_lists.entrant_list_" + std::to_string(i);
            if (written(x.engine, sp + "._metadata_") == "f2012_teams")
                standings = sp;
            if (written(x.engine, lp + "._metadata_") == "f2012")
                list = lp;
        }
        auto text = [&](const std::string &path) -> std::optional<std::string> {
            if (auto w = written(x.engine, path))
                return w;
            auto rec = db.base()(content::gvdb::fnv64(path));
            return rec ? std::optional<std::string>(rec->text) : std::nullopt;
        };
        bool added_line = false;
        for (auto &l : x.lines["content.package"])
            added_line |=
                l.find("standings f2012_teams has 12 teams; f1_teams_2024 holds 2, the rest are added") != std::string::npos;
        if (!own_row) {
            check(refused == 0 && x.records["content.package"] == 1,
                  "with no standings_from, a 12-team package applies on the stock f1_teams_2024 row");
            check(!standings.empty() && text(standings + "._team_reference_instancecount_") == "12",
                  "its standings row is grown to 12 teams");
            check(text(standings + ".team_reference_10.team") == "hrt_2012" &&
                      text(standings + ".team_reference_11.team") == "marussia_2012" &&
                      text(standings + ".team_reference_11._metadata_") == "marussia" &&
                      text(standings + ".team_reference_0.team") == "redbull_2012",
                  "in package order: the 12th is the package's 12th team");
            check(text("entrants.standings.standings_1._team_reference_instancecount_") == "11" &&
                      !text("entrants.standings.standings_1.team_reference_11._metadata_"),
                  "the stock f1_teams_2026 row keeps its 11 teams");
            check(added_line, "the boot log says the standings row was grown");
            check(!list.empty() && text(list + "._entrant_instancecount_") == "24" &&
                      text(list + ".entrant_23.entrant_reference") == "f12-marussia_b" &&
                      text(list + ".entrant_22.team_position") == "0" && text(list + ".entrant_23.team") == "marussia_2012",
                  "its entrant list is grown to 24 entrants");
            check(text("entrants.entrant_lists.entrant_list_1._entrant_instancecount_") == "22",
                  "the stock f1_2026 list keeps its 22 entrants");
            check(text("entrants.entrant_setup_modes.entrant_setup_mode_0.max_entrants") == "24",
                  "the setup mode takes 24 entrants");
        } else {
            check(refused == 0 && x.records["content.package"] == 1, "with standings_from naming a 12-team row it applies");
            check(!standings.empty() && text(standings + "._team_reference_instancecount_") == "12" &&
                      text(standings + ".team_reference_11.team") == "marussia_2012" &&
                      text(standings + ".team_reference_11._metadata_") == "b11",
                  "the creator's row is copied as it is and filled, nothing added");
            check(!added_line, "and no grown row is logged");
        }
    }
}

void short_list_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"swofty.swofty-seasons-2012"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
        std::string schema = slurp(regs + "/" + name + "/1.0.json");
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    static const char *keys[] = {"redbull", "mclaren",   "ferrari",  "mercedes", "lotus", "forceindia",
                                 "sauber",  "tororosso", "williams", "caterham", "hrt",   "marussia"};
    std::string teams;
    for (int i = 0; i < 12; i++) {
        std::string k = keys[i];
        teams += std::string(i ? "," : "") + R"({ "key": ")" + k + R"(", "vehicle": "game/)" + k + R"(_2012", "drivers": [ )" +
                 R"({ "driver": "game/)" + k + R"(_a", "number": )" + std::to_string(i + 1) + R"(, "look": null } ] })";
    }
    std::string tt = R"({ "content.package": { "f1_2012_tt": { "title": "lng_f2012", "entrant_list": "f2012_tt", "season": 2012,
        "teams": [ )" +
                     teams +
                     R"( ], "database": { "entrant_list": "f2012_tt", "entrant_prefix": "f12-", "standings": "f2012_tt_teams",
        "entrant_list_from": "f1_2026", "content_sets": ["2025"] } },
        "f1_2012_one": { "title": "lng_f2012", "entrant_list": "f2012_one", "season": 2012,
        "teams": [ { "key": "redbull", "vehicle": "game/redbull_2012", "drivers": [ { "driver": "game/redbull_a", "number": 1, "look": null } ] } ],
        "database": { "entrant_list": "f2012_one", "entrant_prefix": "f12o-", "standings": "f2012_one_teams", "content_sets": ["2025"] } } } })";
    int r = registry::load_data_file("swofty.swofty-seasons-2012", "registry/content.json", tt);
    st.end_register();
    check(r == SWOFTY_ASSET_OK, "a one-car-per-team package registers");
    Db db;
    stock_wide_rows(db);
    content::Expansion x(db.base());
    content::expand(st, x, {});
    int refused = 0;
    for (auto &[k, n] : x.refused)
        refused += n;
    for (auto &l : x.lines["content.package"])
        std::printf("      %s\n", l.c_str());
    auto text = [&](const std::string &path) -> std::optional<std::string> {
        if (auto w = written(x.engine, path))
            return w;
        auto rec = db.base()(content::gvdb::fnv64(path));
        return rec ? std::optional<std::string>(rec->text) : std::nullopt;
    };
    auto row_of = [&](const char *table, const char *row, const std::string &label) {
        for (int i = 0; i < 8; i++) {
            std::string p = std::string(table) + "." + row + "_" + std::to_string(i);
            if (written(x.engine, p + "._metadata_") == label)
                return p;
        }
        return std::string();
    };
    std::string list = row_of("entrants.entrant_lists", "entrant_list", "f2012_tt");
    std::string one = row_of("entrants.entrant_lists", "entrant_list", "f2012_one");
    std::string one_teams = row_of("entrants.standings", "standings", "f2012_one_teams");
    bool line = false;
    for (auto &l : x.lines["content.package"])
        line |= l.find("entrant list f2012_tt has 12 entrants; f1_2026 holds 22, the rest are left out") != std::string::npos;
    check(refused == 0 && x.records["content.package"] == 2, "both short packages apply");
    check(!list.empty() && text(list + "._entrant_instancecount_") == "12" && text(list + ".entrant_11.team") == "marussia_2012",
          "a 12-car list copied from a 22-car one holds 12 entrants, not the source's last 10");
    check(text("entrants.entrant_lists.entrant_list_1._entrant_instancecount_") == "22",
          "the stock f1_2026 list keeps its 22 entrants");
    check(line, "the boot log says the rest of the source list is left out");
    check(!one.empty() && text(one + "._entrant_instancecount_") == "1", "a one-car list holds one entrant");
    check(!one_teams.empty() && text(one_teams + "._team_reference_instancecount_") == "1" &&
              text(one_teams + ".team_reference_0.team") == "redbull_2012",
          "a one-team package's standings hold one team");
    check(text("entrants.standings.standings_0._team_reference_instancecount_") == "2",
          "the stock f1_teams_2024 row keeps its 2 teams");
}

// The team and car rows a package's teams are checked against: three team rows inside the table's count and one
// past it, two cars.
void package_team_row_tests()
{
    std::map<std::string, std::string> db = {
        {"entrants.teams._team_instancecount_", "3"},
        {"entrants.teams.team_0._metadata_", "cs16_mercedes"},
        {"entrants.teams.team_1._metadata_", "f1_world"},
        {"entrants.teams.team_2._metadata_", "f1_generic"},
        {"entrants.teams.team_3._metadata_", "beyond_count"},
        {"entrants.vehicles._vehicle_instancecount_", "2"},
        {"entrants.vehicles.vehicle_0._metadata_", "cs16_mercedes"},
        {"entrants.vehicles.vehicle_1._metadata_", "world_car"},
    };
    content::Read read = [&](const std::string &p) -> std::optional<std::string> {
        auto it = db.find(p);
        if (it != db.end())
            return it->second;
        auto at = p.find("[_metadata_=");
        if (at == std::string::npos)
            return std::nullopt;
        std::string table = p.substr(0, at), want = p.substr(at + 12, p.find(']', at) - at - 12);
        std::string container = table.substr(0, table.rfind('.')), row = table.substr(table.rfind('.') + 1);
        auto count = db.find(container + "._" + row + "_instancecount_");
        int n = count == db.end() ? 0 : std::atoi(count->second.c_str());
        for (int i = 0; i < n; i++) {
            auto m = db.find(table + "_" + std::to_string(i) + "._metadata_");
            if (m != db.end() && m->second == want)
                return want;
        }
        return std::nullopt;
    };
    std::map<std::string, Value> vehicles = {{"mod.a/mercedes", Value::object({{"label", "cs16_mercedes"}})},
                                             {"mod.a/haas", Value::object({{"label", "cs16_haas"}})}};
    content::Lookup lookup = [&](const std::string &reg, const std::string &id) -> const Value * {
        auto it = vehicles.find(id);
        return reg == "vehicle" && it != vehicles.end() ? &it->second : nullptr;
    };
    auto team = [](const char *key, Value identity, const char *vehicle) {
        return Value::object({{"key", key},
                              {"team", std::move(identity)},
                              {"vehicle", vehicle},
                              {"drivers", Value::array({Value::object({{"driver", "game/d"}, {"number", 9}})})}});
    };
    content::ConstructorTeams constructors = {{"world", "mod.b/f1_world"}};
    auto lower = [&](Value teams, const content::Read &with) {
        content::Plan plan;
        Value rec = Value::object({{"entrant_list", "cs_f1_2016"}, {"title", "x"}, {"teams", std::move(teams)}});
        content::lower_package("mod.a/f1_2016", rec, lookup, plan, {}, nullptr, with, &constructors);
        return plan;
    };
    auto field = [](const content::Plan &p, const std::string &at, const char *name) -> std::string {
        for (std::size_t i = 0; i < p.ops.size(); i++) {
            const Value &op = p.ops[i];
            if (op["op"].as_string() == "set" && op["select"].as_string() == at && op["field"].as_string() == name)
                return std::string(op["value"].as_string());
        }
        return "<none>";
    };
    std::string list = std::string(content::LISTS) + "[_metadata_=cs_f1_2016]";

    content::Plan ok = lower(
        Value::array({team("mercedes", Value(), "mod.a/mercedes"), team("f1_world", Value("mod.b/f1_world"), "game/world_car")}),
        read);
    check(!ok.refused && ok.ops.size() > 0, "a package whose team and car rows all exist applies");
    check(field(ok, list + ".entrant_0", "team") == "cs16_mercedes" &&
              field(ok, list + ".entrant_0", "vehicle") == "cs16_mercedes",
          "a team with no team record names the row labelled like its car");
    check(field(ok, list + ".entrant_1", "team") == "f1_world" && field(ok, list + ".entrant_1", "vehicle") == "world_car" &&
              field(ok, list + ".entrant_1", "_metadata_") == "world_car_driver_01",
          "a team with a team record names that record's row, and its car stays its own");

    content::Plan no_team =
        lower(Value::array({team("mercedes", Value(), "mod.a/mercedes"), team("f1_world", Value(), "game/world_car")}), read);
    bool said = false;
    for (auto &l : no_team.lines)
        said |= l.find("team f1_world names entrants.teams row world_car, which the game database lacks") != std::string::npos;
    check(no_team.refused == 1 && no_team.ops.size() == 0 && said,
          "a team whose row the game lacks refuses the whole package and says which row");
    check(lower(Value::array({team("x", Value("mod.b/beyond_count"), "game/world_car")}), read).refused == 1,
          "a team row past the table's instance count counts as missing");
    content::Plan season =
        lower(Value::array({team("mercedes", Value(), "mod.a/mercedes"), team("haas", Value(), "mod.a/haas")}), read);
    bool expected = false;
    for (auto &l : season.lines)
        expected |= l.find("team haas names entrants.teams row cs16_haas, which the game database does not hold yet") !=
                    std::string::npos;
    check(!season.refused && season.ops.size() > 0 && expected && field(season, list + ".entrant_1", "team") == "cs16_haas" &&
              field(season, list + ".entrant_1", "vehicle") == "cs16_haas",
          "a Classic Seasons style package applies when its own car's team row is one its addon's database operations add");
    check(lower(Value::array({team("x", Value("mod.b/no_such_team"), "mod.a/haas")}), read).refused == 1,
          "a team record that names no row the game sees, with no row for its car either, refuses the package");
    content::Plan no_car = lower(Value::array({team("x", Value("game/f1_generic"), "game/no_such_car")}), read);
    bool warned = false;
    for (auto &l : no_car.lines)
        warned |= l.find("team x names entrants.vehicles row no_such_car, which the game database lacks") != std::string::npos;
    check(!no_car.refused && warned, "a car the game database lacks is reported, and the package still applies");
    content::Plan by_constructor = lower(Value::array({team("w", Value("constructor/world"), "game/world_car")}), read);
    check(!by_constructor.refused && field(by_constructor, list + ".entrant_0", "team") == "f1_world",
          "a constructor/ team names the row of the team that provides it");
    content::Plan fell = lower(Value::array({team("mercedes", Value("mod.b/no_row"), "mod.a/mercedes")}), read);
    bool noted = false;
    for (auto &l : fell.lines)
        noted |= l.find("its team mod.b/no_row gives no entrants.teams row (no_row) the game sees; it uses its car's row "
                        "cs16_mercedes") != std::string::npos;
    check(!fell.refused && field(fell, list + ".entrant_0", "team") == "cs16_mercedes" && noted,
          "a team record without a row falls back to its car's row, and says so");
    content::Read empty = [](const std::string &) -> std::optional<std::string> { return std::nullopt; };
    check(!lower(Value::array({team("x", Value(), "game/no_such_car")}), empty).refused,
          "with no teams or vehicles table in the database nothing is checked");
}

void garage_tests()
{
    std::map<std::string, std::string> db = {
        {std::string(content::VEHICLES) + "[_metadata_=cs20_lotus].vehicle_package", "cs_2020_lotus"},
        {std::string(content::VEHICLES) + "[_metadata_=cs20_mercedes].vehicle_package", "cs_2020_mercedes"},
        {std::string(content::VEHICLES) + "[_metadata_=renault_2025].vehicle_package", "f1_2025_renault"},
        {std::string(content::PACKAGES) + "[_metadata_=cs_2020_lotus].resource_path",
         "cs_2020_vehicle_package/teams/lotus/wep/lotus"},
        {std::string(content::PACKAGES) + "[_metadata_=cs_2020_mercedes].resource_path",
         "cs_2020_vehicle_package/teams/mercedes/wep/mercedes"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_2025_renault].resource_path",
         "f1_2025_vehicle_package/teams/renault/wep/renault"},
    };
    content::Read read = [&](const std::string &p) -> std::optional<std::string> {
        auto it = db.find(p);
        return it == db.end() ? std::nullopt : std::optional<std::string>(it->second);
    };
    auto team = [](const char *key, const char *vehicle, const char *identity) {
        return Value::object({{"key", key}, {"team", identity}, {"vehicle", vehicle}});
    };
    std::map<std::string, Value> recs = {
        {"vehicle|mod.a/lotus", Value::object({{"label", "cs20_lotus"}})},
        {"vehicle|mod.a/mercedes", Value::object({{"label", "cs20_mercedes"}})},
        {"vehicle|mod.b/renault", Value::object({{"label", "renault_2025"}})},
        {"content.package|mod.a/f1_2020",
         Value::object(
             {{"teams", Value::array({team("lotus", "mod.a/lotus", ""), team("mercedes", "mod.a/mercedes", "mod.a/merc")})}})},
        {"content.package|mod.b/f1_2025",
         Value::object({{"teams", Value::array({team("renault", "mod.b/renault", "game/renault")})}})}};
    content::Lookup lookup = [&](const std::string &reg, const std::string &id) -> const Value * {
        auto it = recs.find(reg + "|" + id);
        return it == recs.end() ? nullptr : &it->second;
    };
    auto lower = [&](const char *json) {
        content::Plan plan;
        Value rec = swofty::json::parse(json);
        content::lower_garage("mod.a/renault_2020", rec, lookup, read, plan);
        return plan;
    };

    content::Plan by_entry = lower(R"j({"package":"mod.a/f1_2020","entry":"lotus","textures":"garage/renault_2020"})j");
    check(by_entry.garages.size() == 1 && by_entry.garages[0].groups.size() == 1 &&
              by_entry.garages[0].groups[0] == "asset_groups/cs_2020_vehicle_package/teams/lotus/wep/lotus.assetgroup",
          "a garage by team entry key goes to that team's car group in the box");
    check(by_entry.garages[0].folder == "garage/renault_2020/" && by_entry.garages[0].srgb &&
              by_entry.garages[0].root == "eaid://environment_package/core_assets/objects/textures/",
          "the folder gains a slash and root and srgb take their defaults");
    check(by_entry.garages[0].game_path == "2025_asset_groups/swofty_garage/mod_a_renault_2020.erp",
          "the archive has a game path of its own per record");
    content::Plan by_team =
        lower(R"j({"package":"mod.a/f1_2020","team":"mod.a/merc","textures":"g","root":"eaid://x/","srgb":false})j");
    check(by_team.garages.size() == 1 &&
              by_team.garages[0].groups[0] == "asset_groups/cs_2020_vehicle_package/teams/mercedes/wep/mercedes.assetgroup" &&
              by_team.garages[0].root == "eaid://x/" && !by_team.garages[0].srgb,
          "a garage by team identity matches the package entry that names it");
    check(lower(R"j({"package":"mod.b/f1_2025","entry":"renault","textures":"g"})j").garages[0].groups[0] ==
              "asset_groups/f1_2025_vehicle_package/teams/renault/wep/renault.assetgroup",
          "another box's garage lands on that box's own car group, never the 2020 ones");
    content::Plan other = lower(R"j({"package":"mod.a/f1_2020","entry":"lotus","textures":"g"})j");
    bool leaks = false;
    for (auto &g : other.garages[0].groups)
        leaks = leaks || g.find("f1_2025") != std::string::npos || g.find("mercedes") != std::string::npos;
    check(!leaks, "a box's garage names only the groups of its team's cars");
    for (const char *bad :
         {R"j({"package":"mod.a/f1_2020","entry":"ferrari","textures":"g"})j",
          R"j({"package":"mod.a/nope","entry":"lotus","textures":"g"})j", R"j({"package":"mod.a/f1_2020","textures":"g"})j",
          R"j({"package":"mod.a/f1_2020","entry":"lotus"})j"}) {
        content::Plan p = lower(bad);
        check(p.garages.empty() && p.refused == 1, std::string("refused: ") + bad);
    }
    db.erase(std::string(content::VEHICLES) + "[_metadata_=cs20_lotus].vehicle_package");
    check(lower(R"j({"package":"mod.a/f1_2020","entry":"lotus","textures":"g"})j").garages.empty(),
          "a car without a vehicle row in the game database leaves nothing to attach to");
    check(!content::garage_art_enabled(nullptr) && !content::garage_art_enabled("") && !content::garage_art_enabled("0") &&
              !content::garage_art_enabled("on") && !content::garage_art_enabled("11") && content::garage_art_enabled("1"),
          "garage art stays fenced off unless SWOFTY_GARAGE_ART is exactly 1");
}

void sponsors_tests()
{
    std::map<std::string, std::string> db = {
        {std::string(content::VEHICLES) + "[_metadata_=f50_alfa].vehicle_package", "f1_1950_alfa"},
        {std::string(content::VEHICLES) + "[_metadata_=f50_ferrari].vehicle_package", "f1_1950_ferrari"},
        {std::string(content::VEHICLES) + "[_metadata_=renault_2025].vehicle_package", "f1_2025_renault"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_1950_alfa].resource_path",
         "f1_1950_vehicle_package/teams/alfa/wep/alfa"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_1950_ferrari].resource_path",
         "f1_1950_vehicle_package/teams/ferrari/wep/ferrari"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_2025_renault].resource_path",
         "f1_2025_vehicle_package/teams/renault/wep/renault"},
    };
    content::Read read = [&](const std::string &p) -> std::optional<std::string> {
        auto it = db.find(p);
        return it == db.end() ? std::nullopt : std::optional<std::string>(it->second);
    };
    auto team = [](const char *key, const char *vehicle) { return Value::object({{"key", key}, {"vehicle", vehicle}}); };
    std::map<std::string, Value> recs = {
        {"vehicle|mod.s/alfa", Value::object({{"label", "f50_alfa"}})},
        {"vehicle|mod.s/ferrari", Value::object({{"label", "f50_ferrari"}})},
        {"vehicle|mod.b/renault", Value::object({{"label", "renault_2025"}})},
        {"content.package|mod.s/f1_1950",
         Value::object({{"teams", Value::array({team("alfa", "mod.s/alfa"), team("ferrari", "mod.s/ferrari"),
                                                team("alfa_b", "mod.s/alfa")})}})},
        {"content.package|mod.b/f1_2025", Value::object({{"teams", Value::array({team("renault", "mod.b/renault")})}})},
        {"content.package|mod.s/empty", Value::object({{"teams", Value::array()}})}};
    content::Lookup lookup = [&](const std::string &reg, const std::string &id) -> const Value * {
        auto it = recs.find(reg + "|" + id);
        return it == recs.end() ? nullptr : &it->second;
    };
    auto lower = [&](const char *json) {
        content::Plan plan;
        Value rec = swofty::json::parse(json);
        content::lower_sponsors("mod.s/boards_1950", rec, lookup, read, plan);
        return plan;
    };

    content::Plan box = lower(R"j({"package":"mod.s/f1_1950","textures":"sponsors/f1_1950"})j");
    check(box.garages.size() == 1 && box.garages[0].registry == "track.sponsors" && box.garages[0].groups.size() == 2 &&
              box.garages[0].groups[0] == "asset_groups/f1_1950_vehicle_package/teams/alfa/wep/alfa.assetgroup" &&
              box.garages[0].groups[1] == "asset_groups/f1_1950_vehicle_package/teams/ferrari/wep/ferrari.assetgroup",
          "sponsor boards go to the group of every car of the box, each group once");
    check(box.garages[0].vehicles.size() == 2, "a car two team entries share is named once");
    check(box.garages[0].folder == "sponsors/f1_1950/" && box.garages[0].srgb &&
              box.garages[0].root == "eaid://environment_package/core_assets/shared_textures/sponsors/",
          "the folder gains a slash and root and srgb take the sponsor defaults");
    check(box.garages[0].game_path == "2025_asset_groups/swofty_sponsors/mod_s_boards_1950.erp",
          "the boards archive has a game path of its own per record, apart from the garage art's");
    content::Plan garage;
    content::lower_garage("mod.s/g", swofty::json::parse(R"j({"package":"mod.s/f1_1950","entry":"alfa","textures":"g"})j"),
                          lookup, read, garage);
    check(garage.garages.size() == 1 && garage.garages[0].registry == "team.garage" &&
              garage.garages[0].game_path.rfind("2025_asset_groups/swofty_garage/", 0) == 0,
          "garage art keeps its own registry tag and game path");
    bool leaks = false;
    for (auto &g : box.garages[0].groups)
        leaks = leaks || g.find("f1_2025") != std::string::npos;
    check(!leaks, "a box's boards never reach the stock 2025 box's car groups");
    content::Plan other = lower(R"j({"package":"mod.b/f1_2025","textures":"s","root":"eaid://x/","srgb":false})j");
    check(other.garages.size() == 1 && other.garages[0].groups.size() == 1 &&
              other.garages[0].groups[0] == "asset_groups/f1_2025_vehicle_package/teams/renault/wep/renault.assetgroup" &&
              other.garages[0].root == "eaid://x/" && !other.garages[0].srgb,
          "another box's boards land on that box's own car groups, with its own root and srgb");
    for (const char *bad : {R"j({"package":"mod.s/nope","textures":"s"})j", R"j({"package":"mod.s/f1_1950"})j",
                            R"j({"textures":"s"})j", R"j({"package":"mod.s/empty","textures":"s"})j"}) {
        content::Plan p = lower(bad);
        check(p.garages.empty() && p.refused == 1, std::string("refused: ") + bad);
    }
    db.erase(std::string(content::VEHICLES) + "[_metadata_=f50_alfa].vehicle_package");
    content::Plan one = lower(R"j({"package":"mod.s/f1_1950","textures":"s"})j");
    check(one.garages.size() == 1 && one.garages[0].groups.size() == 1,
          "a car without a vehicle row is skipped and the box's other cars still take the boards");
    db.erase(std::string(content::VEHICLES) + "[_metadata_=f50_ferrari].vehicle_package");
    check(lower(R"j({"package":"mod.s/f1_1950","textures":"s"})j").garages.empty(),
          "a box none of whose cars has an asset group is refused");
    check(!content::box_sponsors_enabled(nullptr) && !content::box_sponsors_enabled("") && !content::box_sponsors_enabled("0") &&
              !content::box_sponsors_enabled("on") && !content::box_sponsors_enabled("11") && content::box_sponsors_enabled("1"),
          "box sponsor boards stay fenced off unless SWOFTY_BOX_SPONSORS is exactly 1");
}

void box_assets_tests()
{
    std::map<std::string, std::string> db = {
        {std::string(content::VEHICLES) + "[_metadata_=f50_alfa].vehicle_package", "f1_1950_alfa"},
        {std::string(content::VEHICLES) + "[_metadata_=f50_ferrari].vehicle_package", "f1_1950_ferrari"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_1950_alfa].resource_path",
         "f1_1950_vehicle_package/teams/alfa/wep/alfa"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_1950_ferrari].resource_path",
         "f1_1950_vehicle_package/teams/ferrari/wep/ferrari"},
    };
    content::Read read = [&](const std::string &p) -> std::optional<std::string> {
        auto it = db.find(p);
        return it == db.end() ? std::nullopt : std::optional<std::string>(it->second);
    };
    auto team = [](const char *key, const char *vehicle) { return Value::object({{"key", key}, {"vehicle", vehicle}}); };
    std::map<std::string, Value> recs = {
        {"vehicle|mod.s/alfa", Value::object({{"label", "f50_alfa"}})},
        {"vehicle|mod.s/ferrari", Value::object({{"label", "f50_ferrari"}})},
        {"content.package|mod.s/f1_1950",
         Value::object({{"entrant_list", "f5_1950"},
                        {"teams", Value::array({team("alfa", "mod.s/alfa"), team("ferrari", "mod.s/ferrari"),
                                                team("alfa_b", "mod.s/alfa")})}})},
        {"content.package|mod.s/nolist", Value::object({{"teams", Value::array({team("alfa", "mod.s/alfa")})}})}};
    content::Lookup lookup = [&](const std::string &reg, const std::string &id) -> const Value * {
        auto it = recs.find(reg + "|" + id);
        return it == recs.end() ? nullptr : &it->second;
    };
    auto lower = [&](const char *json) {
        content::Plan plan;
        content::lower_box_assets("mod.s/kit_1950", swofty::json::parse(json), lookup, read, plan);
        return plan;
    };

    content::Plan marshals = lower(R"j({"package":"mod.s/f1_1950","kind":"marshals","textures":"kit/marshals",)j"
                                   R"j("resources":["eaid://character_package_shared/"]})j");
    check(marshals.garages.size() == 1 && marshals.garages[0].registry == "content.box_assets" &&
              marshals.garages[0].kind == "marshals" && marshals.garages[0].groups.size() == 2 &&
              marshals.garages[0].folder == "kit/marshals/" && marshals.garages[0].root == "eaid://" &&
              marshals.garages[0].archive.empty() && marshals.garages[0].prefixes.size() == 1,
          "a marshals folder goes to every car group of the box, its folder mirroring whole eaid names");
    check(marshals.garages[0].game_path == "2025_asset_groups/swofty_box_assets/mod_s_kit_1950.erp",
          "a box assets record has a game path of its own, apart from the garage art's and the sponsor boards'");

    content::Plan crowd = lower(R"j({"package":"mod.s/f1_1950","kind":"crowd","archive":"kit/crowd_1950.erp",)j"
                                R"j("resources":["eaid://character_package_shared/crowd/"]})j");
    check(crowd.garages.size() == 1 && crowd.garages[0].archive == "kit/crowd_1950.erp" && crowd.garages[0].folder.empty() &&
              crowd.garages[0].groups.size() == 2,
          "a ready archive is listed as it is on every car group of the box");

    content::Plan garage = lower(R"j({"package":"mod.s/f1_1950","kind":"garage","entry":"ferrari","textures":"g"})j");
    check(garage.garages.size() == 1 && garage.garages[0].groups.size() == 1 &&
              garage.garages[0].groups[0] == "asset_groups/f1_1950_vehicle_package/teams/ferrari/wep/ferrari.assetgroup" &&
              garage.garages[0].root == "eaid://environment_package/core_assets/objects/textures/",
          "an entry keeps the archive to that team's cars, and kind garage takes the garage art's root");
    check(lower(R"j({"package":"mod.s/f1_1950","kind":"sponsors","textures":"s"})j").garages[0].root ==
              "eaid://environment_package/core_assets/shared_textures/sponsors/",
          "kind sponsors takes the sponsor boards' root");
    check(lower(R"j({"package":"mod.s/f1_1950","textures":"s","root":"eaid://x/"})j").garages[0].kind == "other" &&
              lower(R"j({"package":"mod.s/f1_1950","textures":"s","root":"eaid://x/"})j").garages[0].root == "eaid://x/",
          "a record without kind is other, and its own root is kept");

    content::Plan scene = lower(R"j({"package":"mod.s/f1_1950","kind":"scene","archive":"kit/blankets.erp",)j"
                                R"j("groups":["asset_groups/animation_package/scenes/cinematics/cinematicsgrid.assetgroup"],)j"
                                R"j("resources":["eaid://environment_package/cinematics/"]})j");
    check(scene.garages.size() == 1 && scene.garages[0].kind == "scene" && scene.garages[0].box == "f5_1950" &&
              scene.garages[0].groups.size() == 1 &&
              scene.garages[0].groups[0] == "asset_groups/animation_package/scenes/cinematics/cinematicsgrid.assetgroup" &&
              scene.garages[0].vehicles.empty(),
          "kind scene goes to its named stock scene groups for the package's box, not to the car groups");

    content::Plan world =
        lower(R"j({"package":"mod.s/f1_1950","kind":"world","archive":"w/silverstone_common_1950.erp",)j"
              R"j("groups":["asset_groups/environment_package/tracks/silverstone/wep/silverstone_common.assetgroup"],)j"
              R"j("resources":["eaid://environment_package/tracks/silverstone/"]})j");
    check(world.refused == 0 && world.garages.size() == 1 && world.garages[0].kind == "world" &&
              world.garages[0].box == "f5_1950" && world.garages[0].groups.size() == 1 &&
              world.garages[0].groups[0] ==
                  "asset_groups/environment_package/tracks/silverstone/wep/silverstone_common.assetgroup" &&
              world.garages[0].vehicles.empty(),
          "kind world goes to the track's world group for the package's box, not to the car groups");

    for (const char *bad : {
             R"j({"package":"mod.s/f1_1950","kind":"scene","textures":"s","groups":["a/b.assetgroup"]})j",
             R"j({"package":"mod.s/f1_1950","kind":"scene","archive":"a.erp","resources":["eaid://x/"]})j",
             R"j({"package":"mod.s/f1_1950","kind":"scene","archive":"a.erp","resources":["eaid://x/"],"groups":["a/b"]})j",
             R"j({"package":"mod.s/f1_1950","kind":"crowd","archive":"a.erp","resources":["eaid://x/"],)j"
             R"j("groups":["a/b.assetgroup"]})j",
             R"j({"package":"mod.s/nolist","kind":"scene","archive":"a.erp","resources":["eaid://x/"],)j"
             R"j("groups":["a/b.assetgroup"]})j",
             R"j({"package":"mod.s/f1_1950","kind":"world","archive":"a.erp","resources":["eaid://x/"],)j"
             R"j("groups":["asset_groups/animation_package/scenes/cinematics/cinematicsgrid.assetgroup"]})j",
             R"j({"package":"mod.s/f1_1950","kind":"world","archive":"a.erp","resources":["eaid://x/"],)j"
             R"j("groups":["asset_groups/environment_package/tracks/.assetgroup"]})j",
             R"j({"package":"mod.s/f1_1950","kind":"world","archive":"a.erp","resources":["eaid://x/"]})j",
             R"j({"kind":"crowd","textures":"s"})j",
             R"j({"package":"mod.s/nope","textures":"s"})j",
             R"j({"package":"mod.s/f1_1950","kind":"menus","textures":"s"})j",
             R"j({"package":"mod.s/f1_1950"})j",
             R"j({"package":"mod.s/f1_1950","textures":"s","archive":"a.erp","resources":["eaid://x/"]})j",
             R"j({"package":"mod.s/f1_1950","archive":"a.erp"})j",
             R"j({"package":"mod.s/f1_1950","archive":"a.bin","resources":["eaid://x/"]})j",
             R"j({"package":"mod.s/f1_1950","archive":"../a.erp","resources":["eaid://x/"]})j",
             R"j({"package":"mod.s/f1_1950","archive":"a.erp","resources":["x/"]})j",
             R"j({"package":"mod.s/f1_1950","entry":"nobody","textures":"s"})j",
         }) {
        content::Plan p = lower(bad);
        check(p.garages.empty() && p.refused == 1, std::string("box assets refused: ") + bad);
    }

    content::Plan sponsors;
    content::lower_sponsors("mod.s/boards", swofty::json::parse(R"j({"package":"mod.s/f1_1950","textures":"b"})j"), lookup, read,
                            sponsors);
    check(sponsors.garages.size() == 1 && sponsors.garages[0].registry == "track.sponsors" &&
              sponsors.garages[0].kind == "sponsors" && sponsors.garages[0].groups.size() == 2 &&
              sponsors.garages[0].game_path == "2025_asset_groups/swofty_sponsors/mod_s_boards.erp",
          "track.sponsors lowers through the box assets route with kind sponsors and keeps its own game path");

    std::vector<content::BoxArchiveEntry> entries = {
        {"eaid://character_package_shared/crowd/a/textures/a_d.tif.image", "GfxSurfaceRes", 3},
        {"eaid://character_package_shared/crowd/a/textures/a_d.tif", "GfxSRVResource", 1},
        {"eaid://character_package_shared/crowd/a/textures/billboard_d.texarray.image", "GfxSurfaceRes", 2},
        {"eaid://crowd_package/shared/props/textures/umbrella_d.texarray.image", "GfxSurfaceRes", 2},
    };
    content::BoxArchiveCheck in =
        content::check_box_archive(entries, {"eaid://character_package_shared/crowd/", "eaid://crowd_package/"});
    check(in.resources == 4 && in.outside == 0 && in.streamed == 1,
          "an archive inside its prefixes passes and its streamed surfaces are counted");
    content::BoxArchiveCheck out = content::check_box_archive(entries, {"eaid://character_package_shared/crowd/"});
    check(out.outside == 1 && out.first_outside == "eaid://crowd_package/shared/props/textures/umbrella_d.texarray.image",
          "a resource outside the prefixes is found and named");
    check(content::check_box_archive(entries, {}).outside == 0, "no prefixes allow any name");
    check(!content::box_assets_enabled(nullptr) && !content::box_assets_enabled("") && !content::box_assets_enabled("0") &&
              !content::box_assets_enabled("on") && content::box_assets_enabled("1"),
          "box assets stay fenced off unless SWOFTY_BOX_ASSETS is exactly 1");
}

void livery_tests()
{
    std::map<std::string, std::string> db = {
        {std::string(content::VEHICLES) + "[_metadata_=cs17_mercedes].vehicle_package", "cs_2017_mercedes"},
        {std::string(content::VEHICLES) + "[_metadata_=mercedes_2025].vehicle_package", "f1_2025_mercedes"},
        {std::string(content::PACKAGES) + "[_metadata_=cs_2017_mercedes].resource_path",
         "cs_2017_vehicle_package/teams/mercedes/wep/mercedes"},
        {std::string(content::PACKAGES) + "[_metadata_=f1_2025_mercedes].resource_path",
         "f1_2025_vehicle_package/teams/mercedes/wep/mercedes"},
        {std::string(content::SE_LIVERIES) + "._se_livery_instancecount_", "1"},
        {std::string(content::SE_LIVERIES) + ".se_livery_0.id", "9"},
        {std::string(content::SE_LIVERIES) + ".se_livery_0._metadata_", "mercedes_2025_bw"},
        {std::string(content::SE_LIVERIES) + ".se_livery_0.compatible_vehicle", "cs17_mercedes"},
        {"unlockable_content.unlockable_items.unlockable_item_list_0.folder_0._unlockable_se_livery_instancecount_", "1"},
        {"unlockable_content.unlockable_items.unlockable_item_list_0.folder_0.unlockable_se_livery_0._metadata_", "bw"},
        {"unlockable_content.unlockable_items.unlockable_item_list_0.folder_0.unlockable_se_livery_0.statId", "700"},
    };
    content::Read read = [&](const std::string &p) -> std::optional<std::string> {
        auto it = db.find(p);
        return it == db.end() ? std::nullopt : std::optional<std::string>(it->second);
    };
    std::map<std::string, Value> recs = {
        {"mod.a/car", Value::object({{"label", "cs17_mercedes"}, {"donor", "game/mercedes_2025"}})}};
    content::Lookup lookup = [&](const std::string &reg, const std::string &id) -> const Value * {
        auto it = recs.find(id);
        return reg == "vehicle" && it != recs.end() ? &it->second : nullptr;
    };
    content::StatIds stat = [](const std::string &, const std::string &, int64_t pin) -> int64_t { return pin >= 0 ? pin : 10; };

    content::Plan plan;
    Value look = Value::object({{"vehicle", "mod.a/car"}, {"archive", "cars/mercedes.erp"}, {"textures", "tex/"}});
    content::lower_livery("mod.a/look", look, {"mod.a/car"}, lookup, read, plan, stat);
    check(plan.groups.size() == 1 &&
              plan.groups[0].erp == "2025_asset_groups/cs_2017_vehicle_package/teams/mercedes/wep/mercedes.erp",
          "a livery's archive is routed over the car's own wep archive");
    check(plan.groups[0].tmpl == "asset_groups/f1_2025_vehicle_package/teams/mercedes/wep/mercedes.assetgroup" &&
              plan.groups[0].common.size() == 2 &&
              plan.groups[0].common[0] == "2025_asset_groups/cs_2017_vehicle_package/teams/common.erp",
          "its asset group copies the donor's and lists the package's common archive first");
    check(plan.folders.size() == 1 && plan.folders[0].game_dir == "cs_2017_vehicle_package/teams/mercedes/textures/",
          "its textures folder is routed over the car's textures folder");
    check(plan.ops.size() == 0 && !plan.refused, "a plain livery writes no database rows");

    content::Plan none;
    content::lower_livery("mod.a/look", look, {}, lookup, read, none, stat);
    check(none.groups.empty() && none.folders.empty(), "a livery no car wears applies nothing");

    content::Plan special;
    Value sp = Value::object({{"vehicle", "mod.a/car"},
                              {"archive", "cars/mercedes.erp"},
                              {"special", true},
                              {"suffix", "auto"},
                              {"picker_name", "lng_x"},
                              {"unlock", "owned"}});
    int slot = 0;
    content::lower_livery("mod.a/sp", sp, {}, lookup, read, special, stat, &slot);
    check(special.ops.size() == 2 && special.owned.size() == 1 && special.owned[0].label == "cs17_mercedes_sp",
          "a special livery adds its se_livery row and unlock row and is reported owned");
    check(special.groups.size() == 1 &&
              special.groups[0].erp == "2025_asset_groups/cs_2017_vehicle_package/teams/mercedes/wep/mercedes_lx01.erp",
          "its archive takes the next wep slot");
    std::string ops = swofty::json::dump(special.ops);
    check(ops.find("\"wep_override\"") != std::string::npos && ops.find("_lx01") != std::string::npos &&
              ops.find("mercedes_2025_bw") != std::string::npos,
          "the se_livery row is copied from the car's stock special livery with the slot as its wep override");

    check(ops.find("team_customisation") == std::string::npos,
          "a special livery cloned from the car's own stock row keeps that row's team customisation");

    db[std::string(content::VEHICLES) + "[_metadata_=cs23_redbull].vehicle_package"] = "cs_2017_mercedes";
    db[std::string(content::SE_LIVERIES) + ".se_livery_0.compatible_vehicle"] = "mercedes_2025";
    db[std::string(content::SE_LIVERIES) + ".se_livery_0.team_customisation"] = "2025-mclarensilverstone";
    recs["mod.a/car"] = Value::object({{"label", "cs23_redbull"}});
    content::Plan foreign;
    content::lower_livery("mod.a/sp", sp, {}, lookup, read, foreign, stat, &slot);
    std::string fops = swofty::json::dump(foreign.ops);
    check(foreign.ops.size() == 2 && fops.find("2025-mclarensilverstone") == std::string::npos &&
              fops.find("\"team_customisation\":\"\"") != std::string::npos,
          "a special on a car with no stock se_livery row takes no other team's team customisation");

    recs["mod.a/car"] = Value::object({{"label", "cs17_mercedes"}, {"donor", "game/mercedes_2025"}});
    db[std::string(content::SE_LIVERIES) + ".se_livery_0.compatible_vehicle"] = "cs17_mercedes";
    db.erase(std::string(content::SE_LIVERIES) + ".se_livery_0.team_customisation");
    content::Plan booted, unplanned, planned;
    int s1 = 0, s2 = 0, s3 = 0;
    content::lower_livery("mod.a/sp", sp, {}, lookup, read, booted, stat, &s1);
    content::lower_livery("mod.a/sp", sp, {}, lookup, read, unplanned, {}, &s2);
    content::lower_livery("mod.a/sp", sp, {}, lookup, read, planned, content::planning_ids(), &s3);
    check(booted.ops.size() == 2 && unplanned.ops.size() == 0 && unplanned.refused == 1,
          "without stat ids a special livery lowers to nothing, so the old plan missed its rows (315 keys of SWOFTY "
          "2021-2024)");
    check(planned.ops.size() == booted.ops.size() && !planned.refused,
          "the plan's own stat ids lower it to the rows the boot writes");

    content::Plan missing;
    Value bad = Value::object({{"vehicle", "mod.a/car"}, {"archive", "a.erp"}});
    recs["mod.a/car"] = Value::object({{"label", "nope"}});
    content::lower_livery("mod.a/look", bad, {"mod.a/car"}, lookup, read, missing, stat);
    check(missing.groups.empty() && missing.refused == 1, "a car without a vehicle row in the game database refuses the livery");
}
