#include "rows_test.hpp"

void engine_tests()
{
    rows::Keys keys;
    keys.parse("@t.rows.row_N\n_metadata_\nstat_id\nspeed\n+part\n_part_instancecount_\n"
               "@t.rows.row_N.part_N\n_metadata_\nweight\n");
    Db db;
    db.put("t.rows._row_instancecount_", 0, "2");
    for (int i = 0; i < 2; i++) {
        std::string r = "t.rows.row_" + std::to_string(i);
        db.put(r + "._metadata_", 4, i ? "beta" : "alpha");
        db.put(r + ".stat_id", 0, std::to_string(40 + i));
        db.put(r + ".speed", 1, i ? "2.5" : "1.5");
        db.put(r + "._part_instancecount_", 0, "1");
        db.put(r + ".part_0._metadata_", 4, "p");
        db.put(r + ".part_0.weight", 0, i ? "9" : "8");
    }
    db.put("t.list.items", 13, "a, b");
    rows::Engine e(db.base(), keys);
    e.attribute("mod.a", "vehicle");
    std::string why = e.apply(op(R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=beta]",
                                     "set":{"_metadata_":"gamma"}})"));
    check(why.empty(), "add-row copies a row by its label (" + why + ")");
    check(written(e, "t.rows.row_2._metadata_") == "gamma", "at the table's next index, with the set value");
    check(written(e, "t.rows.row_2.speed") == "2.5" && written(e, "t.rows.row_2.part_0.weight") == "9",
          "every key of the source row, nested rows included");
    check(written(e, "t.rows.row_2._part_instancecount_") == "1", "with the nested table's count");
    check(written(e, "t.rows._row_instancecount_") == "3", "and the table's count goes up by one");
    check(written(e, "t.rows.row_2.stat_id") == "42", "a top-level table with unique ids gives the highest plus one");
    bool fresh = false, count_fresh = true;
    for (auto &w : e.writes()) {
        if (w.path == "t.rows.row_2.speed")
            fresh = w.fresh && w.owner == "mod.a" && w.tag == 1;
        if (w.path == "t.rows._row_instancecount_")
            count_fresh = w.fresh;
    }
    check(fresh && !count_fresh, "new keys are fresh, the count is not; the source's tag and the owner are kept");

    why = e.apply(op(R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=alpha]",
                         "set":{"_metadata_":"delta","stat_id":"77"}})"));
    check(why.empty() && written(e, "t.rows.row_3.stat_id") == "77", "a set stat_id wins");
    check(e.apply(op(R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=zeta]"})")).find("no row matches") !=
              std::string::npos,
          "a missing source row fails");
    check(e.apply(op(R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=alpha]","set":{"colour":"1"}})"))
                  .find("unknown field") != std::string::npos,
          "a set field the row doesn't have fails");
    check(written(e, "t.rows.row_4._metadata_") == std::nullopt, "a failing operation writes nothing");

    std::size_t mark = e.mark();
    e.attribute("mod.b", "vehicle");
    check(e.apply(op(R"({"op":"set","select":"t.rows.row[_metadata_=gamma]","field":"speed","value":"7"})")).empty(),
          "set resolves a row the engine added");
    check(e.apply(op(R"({"op":"list-append","select":"t","field":"list.items","value":"b,c"})")).empty() &&
              written(e, "t.list.items") == "a,b,c",
          "list-append adds a missing value once");
    e.rollback(mark);
    check(written(e, "t.rows.row_2.speed") == "2.5" && written(e, "t.list.items") == std::nullopt,
          "rollback takes back every write since the mark");
    check(e.apply(op(R"({"op":"set","select":"t.rows.row_0","field":"nothing","value":"1"})")).find("no such key") !=
              std::string::npos,
          "set on a missing key fails");
    check(e.apply(op(R"({"op":"put-record","key":"t.extra","type":2,"value":"6869"})")).empty() && written(e, "t.extra") == "hi",
          "put-record writes the hex value under the path");
    db.put("t.rows.row_9._metadata_", 4, "beta");
    rows::Engine twice(db.base(), keys);
    check(twice.apply(op(R"({"op":"set","select":"t.rows.row[_metadata_=beta]","field":"speed","value":"1"})")).empty(),
          "only rows under the count are searched");
    db.put("t.rows.row_1._metadata_", 4, "alpha");
    check(twice.apply(op(R"({"op":"set","select":"t.rows.row[_metadata_=alpha]","field":"speed","value":"1"})"))
                  .find("2 rows match") != std::string::npos,
          "two rows with the label fail");
    check(rows::shape_of("a.b_12.c.d_3") == "a.b_N.c.d_N" && rows::shape_of("audio.x_2025.y") == "audio.x_N.y", "row shapes");

    rows::Engine guard(db.base(), keys);
    guard.attribute("a.x", "vehicle", "a.x/one");
    guard.apply(op(R"({"op":"put-record","key":"audio.car.sw.bundle","type":2,"value":"61"})"));
    guard.attribute("b.y", "vehicle", "b.y/two");
    check(
        guard.apply(op(R"({"op":"put-record","key":"audio.car.sw.bundle","type":2,"value":"62"})")).find("a.x/one created it") !=
                std::string::npos &&
            written(guard, "audio.car.sw.bundle") == "a",
        "a record never writes a key another record created");
    check(guard.apply(op(R"({"op":"put-record","key":"t.list.items","type":13,"value":"62"})")).empty(),
          "keys the game holds stay shared");
}

// A small database with the stock rows a package copies.
void room_hole_tests()
{
    rows::Keys keys;
    keys.parse("@t.rows.row_N\n_metadata_\nspeed\n");
    Db db;
    db.put("t.rows._row_instancecount_", 0, "1");
    db.put("t.rows.row_0._metadata_", 4, "stock");
    db.put("t.rows.row_0.speed", 1, "1.5");
    std::vector<std::string> order = {"swofty.swofty-seasons-2015", "swofty.swofty-seasons-2024"};
    auto lay = [&](const std::set<std::string> &skip) {
        auto e = std::make_unique<rows::Engine>(db.base(), keys);
        auto add = [&](const std::string &owner, const std::string &label) {
            if (skip.count(owner))
                return;
            e->attribute(owner, "driver.identity");
            e->apply(op(("{\"op\":\"add-row\",\"table\":\"t.rows\",\"from\":\"t.rows.row[_metadata_=stock]\","
                         "\"set\":{\"_metadata_\":\"" +
                         label + "\"}}")
                            .c_str()));
            e->commit();
        };
        add(order[0], "sw15_driver");
        add(order[1], "sw24_driver");
        add(order[0], "sw-sw15_maldonado");
        return e;
    };
    auto fresh_of = [](const rows::Engine &e) {
        std::map<std::string, std::size_t> out;
        for (auto &w : e.writes())
            if (w.fresh)
                out[w.owner]++;
        return out;
    };
    auto full = lay({});
    auto counted = fresh_of(*full);
    check(counted[order[0]] == 4 && counted[order[1]] == 2 && written(*full, "t.rows._row_instancecount_") == "4",
          "a seat entrant laid out after every registry sits past the later addon's row, and its addon owns the count");
    namespace rm = apply::room;
    std::vector<rm::AddonAsk> asks = {{order[0], counted[order[0]], 0}, {order[1], counted[order[1]], 0}};
    auto per_addon = rm::admit(4, asks);
    auto torn = holes(owned_by(*full, {order[0]}), db.base());
    check(per_addon.addons[0].admitted && !per_addon.addons[1].admitted && torn.size() == 1 && torn[0] == "t.rows.row_2",
          "refusing the later addon after layout leaves the count at 4 with row 2 never created (1121flatpunto45l's "
          "crash): " +
              (torn.empty() ? std::string("none") : torn[0]));
    auto whole = rm::admit_whole(4, asks);
    check(!whole.admitted_keys && !whole.addons[0].admitted && holes(owned_by(*full, {}), db.base()).empty(),
          "past the room after layout, nothing is applied and the table keeps its own rows");
    auto again = content::readmit(order, counted, {}, 4);
    check(again.skip == std::set<std::string>{order[1]}, "readmitted on its counted keys, the later addon is left out");
    auto relaid = lay(again.skip);
    check(holes(relaid->writes(), db.base()).empty() && written(*relaid, "t.rows._row_instancecount_") == "3" &&
              written(*relaid, "t.rows.row_2._metadata_") == "sw-sw15_maldonado" && fresh_of(*relaid)[order[0]] <= 4,
          "laid out again without it, the seat entrant takes row 2 and every counted row exists");
}

void stock_package_rows(Db &db)
{
    auto row = [&](const std::string &prefix, std::initializer_list<std::pair<const char *, std::string>> fields) {
        for (auto &[f, v] : fields)
            db.put(prefix + "." + f, std::string_view(f) == "_metadata_" ? 4 : 2, v);
    };
    db.put("entrants.entrant_lists.entrants._entrant_instancecount_", 0, "1");
    row("entrants.entrant_lists.entrants.entrant_0",
        {{"_metadata_", "max_verstappen"}, {"driver", "max_verstappen"}, {"driver_customisation", "x"}, {"car_number", "1"}});
    db.put("entrants.standings._standings_instancecount_", 0, "1");
    row("entrants.standings.standings_0", {{"_metadata_", "f1_teams_2024"}});
    db.put("entrants.standings.standings_0._team_reference_instancecount_", 0, "2");
    for (int i = 0; i < 2; i++)
        row("entrants.standings.standings_0.team_reference_" + std::to_string(i),
            {{"_metadata_", "t" + std::to_string(i)}, {"team", "stock"}});
    db.put("entrants.entrant_lists._entrant_list_instancecount_", 0, "1");
    row("entrants.entrant_lists.entrant_list_0", {{"_metadata_", "f1_2025"},
                                                  {"display_name", "x"},
                                                  {"previous_season_team_standings", "x"},
                                                  {"previous_season_driver_standings", "x"},
                                                  {"leaderboard", "f1"}});
    db.put("entrants.entrant_lists.entrant_list_0.stat_id", 0, "104");
    db.put("entrants.entrant_lists.entrant_list_0._entrant_instancecount_", 0, "2");
    for (int i = 0; i < 2; i++)
        row("entrants.entrant_lists.entrant_list_0.entrant_" + std::to_string(i), {{"_metadata_", "e"},
                                                                                   {"team", "t"},
                                                                                   {"vehicle", "v"},
                                                                                   {"entrant_reference", "r"},
                                                                                   {"team_position", "0"},
                                                                                   {"car_number", "1"},
                                                                                   {"driver", "d"},
                                                                                   {"driver_customisation", "c"}});
    db.put("game_setup.scenarios._scenario_instancecount_", 0, "1");
    row("game_setup.scenarios.scenario_0", {{"_metadata_", "f1"},
                                            {"display_name", "x"},
                                            {"entrant_list", "f1_2025"},
                                            {"game_setup_mode", "f1"},
                                            {"race_class", "f1"},
                                            {"use_custom_vehicle_spec", "False"}});
    db.put("game_setup.scenarios.scenario_0.statId", 0, "14");
    std::string opt = "options_system.options.multiplayer.options_folder_2";
    db.put(opt + "._option_instancecount_", 0, "1");
    row(opt + ".option_0", {{"_metadata_", "category"}});
    db.put(opt + ".option_0._value_instancecount_", 0, "1");
    row(opt + ".option_0.value_0", {{"_metadata_", "f1_2025"},
                                    {"confirmation_dialogue", ""},
                                    {"description_override", ""},
                                    {"game_value", "f1_2025"},
                                    {"is_default", "False"},
                                    {"loc_tag", "x"},
                                    {"loc_tag_alternate", ""},
                                    {"vr_is_default", "False"}});
    db.put("entrants.entrant_setup_modes._entrant_setup_mode_instancecount_", 0, "1");
    row("entrants.entrant_setup_modes.entrant_setup_mode_0", {{"_metadata_", "swofty_f1"}});
    db.put("entrants.entrant_setup_modes.entrant_setup_mode_0.entrant_lists", 13, "f1_2025");
    db.put("entrants.entrant_setup_modes.entrant_setup_mode_0.selectable_entrant_lists", 13, "f1_2025");
    db.put("audio.audio_bundle_settings._audio_bundle_settings_instancecount_", 0, "1");
    row("audio.audio_bundle_settings.audio_bundle_settings_0", {{"_metadata_", "mercedes_2025"},
                                                                {"bundle_name", "mercedes_2025"},
                                                                {"era", "x"},
                                                                {"engine", "e"},
                                                                {"exhaust", "x"},
                                                                {"cabin", "c"}});
    for (auto &[f, v] : std::initializer_list<std::pair<const char *, const char *>>{
             {"_metadata_", "f1_m_mercedes"}, {"bundle_name", "mercedes_2025.bdl"}, {"engine_offset_x", "0.1"}})
        db.put(std::string("audio.audio_car_data.f1_m_mercedes.") + f, std::string_view(f) == "engine_offset_x" ? 1 : 2, v);
    db.put("entrants.vehicles._vehicle_instancecount_", 0, "1");
    row("entrants.vehicles.vehicle_0",
        {{"_metadata_", "mercedes_2025"}, {"performance_tier", "0"}, {"tyre_wear_scale", "1"}, {"drs_capable", "True"}});
    db.put("entrants.vehicles.vehicle_0.stat_id", 0, "508");
    db.put("content_sets._content_set_instancecount_", 0, "1");
    row("content_sets.content_set_0", {{"_metadata_", "2025"}});
    db.put("content_sets.content_set_0.available_entrant_lists_0.entrant_lists", 13, "f1_2025");
    db.put("leaderboards._leaderboard_descriptor_instancecount_", 0, "5");
    const char *boards[5][3] = {
        {"f1", "0", "f1"}, {"classic", "1", "classic"}, {"f2", "3", "f2"}, {"f1_26", "2", "f1"}, {"f2_26", "5", "f2"}};
    for (int i = 0; i < 5; i++) {
        std::string at = "leaderboards.leaderboard_descriptor_" + std::to_string(i);
        row(at, {{"_metadata_", boards[i][0]}, {"default_ghost_folder", boards[i][2]}});
        db.put(at + ".stat_id", 0, boards[i][1]);
    }
}

void leaderboard_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.lb"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
        std::string schema = slurp(regs + "/" + name + (std::string_view(name) == "content.package" ? "/1.5.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    auto package = [](const char *list, const char *board) {
        std::string leaderboard = board ? std::string(R"("leaderboard": ")") + board + R"(", )" : std::string();
        return std::string(R"({ "title": "lng_x", "entrant_list": ")") + list + R"(", "season": 2017, "stat_id": "auto", )" +
               leaderboard + R"("teams": [ { "key": "mercedes", "vehicle": null,
                 "drivers": [ { "driver": "game/lewis_hamilton", "number": 44, "look": null } ] } ],
                 "database": { "entrant_list": ")" +
               list + R"(", "scenario_stat_id": null, "content_sets": ["2025"],
                 "looks": { "mercedes/lewis_hamilton": "x" } } })";
    };
    std::string json = "{ \"content.package\": {\n";
    const std::pair<const char *, const char *> pkgs[] = {
        {"lb_one", nullptr},          {"lb_two", "box"},         {"lb_three", "addon"},      {"lb_four", "addon"},
        {"lb_five", "stock:classic"}, {"lb_six", "group:eight"}, {"lb_seven", "group:eight"}};
    for (std::size_t i = 0; i < std::size(pkgs); i++)
        json += std::string(i ? ",\n" : "") + "\"p" + std::to_string(i) + "\": " + package(pkgs[i].first, pkgs[i].second);
    json += "} }";
    check(registry::load_data_file("mod.lb", "registry/content.json", json) == SWOFTY_ASSET_OK,
          "the leaderboard packages register");
    st.end_register();

    struct Run {
        std::map<std::string, std::string> writes;
        std::vector<std::string> lines;
        std::string ids;
        int refused = 0;
    };
    auto run = [&](const std::string &kept) {
        Db db;
        stock_package_rows(db);
        Run out;
        apply::ids::Store ids;
        if (!kept.empty())
            ids.load(kept);
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
        for (auto &[k, n] : x.refused)
            out.refused += n;
        for (auto &w : x.engine.writes())
            out.writes[w.path] = w.text;
        out.lines = x.lines["content.package"];
        out.ids = ids.dump();
        return out;
    };
    auto row_of = [](const Run &r, const std::string &table, const std::string &row, const std::string &label) {
        for (int i = 0; i < 40; i++) {
            std::string at = table + "." + row + "_" + std::to_string(i);
            auto it = r.writes.find(at + "._metadata_");
            if (it != r.writes.end() && it->second == label)
                return at;
        }
        return std::string();
    };
    auto board_of_list = [&](const Run &r, const char *list) {
        auto at = row_of(r, "entrants.entrant_lists", "entrant_list", list);
        auto it = r.writes.find(at + ".leaderboard");
        return it == r.writes.end() ? std::string("none") : it->second;
    };
    auto count_boards = [&](const Run &r) {
        int n = 0;
        for (int i = 5; i < 40; i++)
            n += r.writes.count("leaderboards.leaderboard_descriptor_" + std::to_string(i) + "._metadata_");
        return n;
    };
    Run first = run({});
    check(first.refused == 0, "every package applies");
    for (auto &l : first.lines)
        std::printf("      %s\n", l.c_str());
    std::string one = row_of(first, "leaderboards", "leaderboard_descriptor", "swofty_lb_one");
    check(!one.empty() && first.writes[one + ".default_ghost_folder"] == "classic",
          "a package gets a descriptor of its own, copied from classic with its ghost folder");
    check(board_of_list(first, "lb_one") == "swofty_lb_one" && board_of_list(first, "lb_two") == "swofty_lb_two",
          "its entrant list points at that label, by default and with \"box\"");
    std::string two = row_of(first, "leaderboards", "leaderboard_descriptor", "swofty_lb_two");
    check(!two.empty() && first.writes[one + ".stat_id"] != first.writes[two + ".stat_id"] &&
              std::stoi(first.writes[one + ".stat_id"]) > 5 && std::stoi(first.writes[two + ".stat_id"]) > 5,
          "each box's id is its own, above the stock rows");
    check(board_of_list(first, "lb_three") == "swofty_addon_mod_lb" && board_of_list(first, "lb_four") == "swofty_addon_mod_lb" &&
              !row_of(first, "leaderboards", "leaderboard_descriptor", "swofty_addon_mod_lb").empty(),
          "\"addon\" shares one descriptor across the addon's packages");
    check(board_of_list(first, "lb_six") == "swofty_group_mod_lb_eight" &&
              board_of_list(first, "lb_seven") == "swofty_group_mod_lb_eight",
          "\"group:<name>\" shares one among the packages that name it");
    check(board_of_list(first, "lb_five") == "classic" &&
              row_of(first, "leaderboards", "leaderboard_descriptor", "classic").empty(),
          "\"stock:classic\" keeps the game's descriptor and adds no row");
    check(count_boards(first) == 4 && first.writes["leaderboards._leaderboard_descriptor_instancecount_"] == "9",
          "one row per box, per addon group and per named group: 4 rows added, the count is 9");
    std::set<std::string> ids;
    for (int i = 5; i < 9; i++)
        ids.insert(first.writes["leaderboards.leaderboard_descriptor_" + std::to_string(i) + ".stat_id"]);
    check(ids.size() == 4 && ids.count("") == 0, "no two added descriptors share an id");

    Run second = run(first.ids);
    check(second.writes == first.writes && second.ids == first.ids,
          "a second build from the kept ids writes the same rows, labels and ids");

    Db full;
    stock_package_rows(full);
    full.put("leaderboards._leaderboard_descriptor_instancecount_", 0, "5");
    auto text = [&](const std::string &path) -> std::optional<std::string> {
        auto r = full.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    bool capped = false;
    for (auto &scan : content::scan_all_pools(text))
        if (scan.registry == content::LEADERBOARD_IDS)
            capped = scan.lo == 6 && scan.hi == 255;
    check(capped, "descriptor ids run from the table's highest plus one up to 255, the byte the game reads");
}

void variant_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"swofty.classic-seasons-2017", "mod.esp"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound", "vehicle.package", "vehicle", "livery", "content.package", "era.box"}) {
        std::string schema = slurp(regs + "/" + name + (std::string_view(name) == "vehicle" ? "/1.1.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int r1 = registry::load_data_file("swofty.classic-seasons-2017", "registry/content.json", R"j({
      "engine_sound": { "mercedes": { "bundle": "sounds/cs17_mercedes.bdl", "settings_from": "game/mercedes_2025" } },
      "vehicle": { "mercedes": { "label": "cs17_mercedes", "donor": "game/mercedes_2025", "stat_id": 520,
                                 "engine_sound": "swofty.classic-seasons-2017/mercedes",
                                 "audio_switch": "f1_M_mercedes", "audio_from": "f1_m_mercedes" } },
      "content.package": { "f1_2017": {
        "title": "lng_cs_f1_2017", "entrant_list": "cs_f1_2017", "season": 2017, "stat_id": 106,
        "teams": [ { "key": "mercedes", "vehicle": null,
                     "drivers": [ { "driver": "game/lewis_hamilton", "number": 44, "look": null } ] },
                   { "key": "redbull", "vehicle": null,
                     "drivers": [ { "driver": "game/max_verstappen", "number": 33, "look": null } ] } ],
        "database": { "entrant_list": "cs_f1_2017", "scenario_stat_id": 16, "entrant_prefix": "cs2017-",
                      "standings": "cs_f1_teams_2017", "content_sets": ["2025"],
                      "looks": { "mercedes/lewis_hamilton": "2017-cs_mercedes_lewis_hamilton",
                                 "redbull/max_verstappen": "2017-cs_redbull_max_verstappen" } } } } })j");
    int r2 = registry::load_data_file("mod.esp", "registry/content.json", R"j({
      "engine_sound": { "v10_screamer": { "bundle": "sounds/v10_screamer.bdl", "settings_from": "game/mercedes_2025" },
                        "mercedes_mix": { "extends": "swofty.classic-seasons-2017/mercedes", "settings_from": "game/mercedes_2025" } },
      "vehicle": { "mercedes_v10": { "extends": "swofty.classic-seasons-2017/mercedes", "label": "esp_cs17_mercedes",
                                     "engine_sound": "mod.esp/v10_screamer", "drs_capable": false, "tyre_group": "F1_1990" },
                   "mercedes_copy": { "extends": "swofty.classic-seasons-2017/mercedes" } },
      "content.package": { "f1_2017_v10": {
        "extends": "swofty.classic-seasons-2017/f1_2017",
        "title": { "literal": "F1 2017 (V10)" }, "entrant_list": "esp_f1_2017_v10", "stat_id": "auto" } } })j");
    st.end_register();
    check(r1 == SWOFTY_ASSET_OK && r2 == SWOFTY_ASSET_OK, "the package and its variant register");
    auto v = st.get("content.package", "mod.esp/f1_2017_v10");
    check(v.status == registry::Status::ok && v.value["database"]["entrant_prefix"].as_string() == "cs2017-",
          "the variant inherits the base's private database block");

    Db db;
    stock_package_rows(db);
    auto pins = content::pins(st);
    check(pins.size() == 3 && pins[0].stat_id == 520 && pins[1].stat_id == 106 && pins[2].stat_id == 16 &&
              pins[2].registry == content::SCENARIO_IDS,
          "the bases pin their vehicle, entrant list and scenario ids; the variants inherit none");
    apply::ids::Store ids;
    auto text = [&](const std::string &path) -> std::optional<std::string> {
        auto r = db.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    for (auto &scan : content::scan_all_pools(text))
        ids.define_pool(scan.registry, {{{scan.lo, scan.hi}}});
    for (auto &p : pins)
        ids.pin(p.registry, p.id, p.stat_id, "test");
    content::StatIds stat = [&](const std::string &reg, const std::string &id, int64_t pin) -> int64_t {
        return pin >= 0 ? ids.pin(reg, id, pin, "test").id : ids.allocate(reg, id, "test").id;
    };
    content::Expansion x(db.base());
    content::expand(st, x, stat);
    int refused = 0;
    for (auto &[k, n] : x.refused)
        refused += n;
    check(refused == 0 && x.records["content.package"] == 2, "both packages apply");
    for (auto &l : x.lines["content.package"])
        std::printf("      %s\n", l.c_str());
    auto has_label = [&](const std::string &table, const std::string &row, const std::string &label) {
        for (int i = 0; i < 8; i++)
            if (written(x.engine, table + "." + row + "_" + std::to_string(i) + "._metadata_") == label)
                return table + "." + row + "_" + std::to_string(i);
        return std::string();
    };
    std::string base_list = has_label("entrants.entrant_lists", "entrant_list", "cs_f1_2017");
    std::string var_list = has_label("entrants.entrant_lists", "entrant_list", "esp_f1_2017_v10");
    check(!base_list.empty() && !var_list.empty(), "each gets its own entrant list");
    check(written(x.engine, base_list + ".stat_id") == "106", "the base keeps its pinned entrant list id");
    check(written(x.engine, var_list + ".stat_id") == "105", "the variant's comes from the pool above the table's ids");
    check(!has_label("entrants.entrant_lists.entrants", "entrant", "cs2017-lewis_hamilton").empty() &&
              !has_label("entrants.entrant_lists.entrants", "entrant", "esp_f1_2017_v10-lewis_hamilton").empty(),
          "the variant's entrants are named for its own list, not the base's prefix");
    check(written(x.engine, var_list + ".entrant_0.entrant_reference") == "esp_f1_2017_v10-lewis_hamilton",
          "and its list points at them");
    std::string var_standings = has_label("entrants.standings", "standings", "esp_f1_2017_v10_teams");
    check(!has_label("entrants.standings", "standings", "cs_f1_teams_2017").empty() && !var_standings.empty(),
          "each gets its own standings row");
    check(written(x.engine, var_list + ".previous_season_team_standings") == "esp_f1_2017_v10_teams",
          "the variant's list reads its own standings");
    std::string base_sc = has_label("game_setup.scenarios", "scenario", "cs_f1_2017");
    std::string var_sc = has_label("game_setup.scenarios", "scenario", "esp_f1_2017_v10");
    check(written(x.engine, base_sc + ".statId") == "16" && written(x.engine, var_sc + ".statId") == "15",
          "the base keeps its scenario id, the variant gets the first free one above the stock scenarios");
    check(written(x.engine, var_list + ".display_name") == "F1 2017 (V10)", "a literal title is the list's name");
    std::string car = has_label("entrants.vehicles", "vehicle", "cs17_mercedes");
    std::string v10 = has_label("entrants.vehicles", "vehicle", "esp_cs17_mercedes");
    check(!car.empty() && written(x.engine, car + ".stat_id") == "520", "the base car keeps its pinned stat id");
    check(!v10.empty() && written(x.engine, v10 + ".stat_id") == "510" &&
              written(x.engine, has_label("entrants.vehicles", "vehicle", "mercedes_copy") + ".stat_id") == "509",
          "its variant gets one of its own from the pool, not the base's");
    std::string base_sw = "audio.audio_car_data.f1_M_mercedes.", v10_sw = "audio.audio_car_data.esp_cs17_mercedes.";
    check(written(x.engine, base_sw + "_metadata_") == "f1_M_mercedes" &&
              written(x.engine, base_sw + "bundle_name") == "cs17_mercedes.bdl",
          "the base car's sound rows keep the base's sound");
    check(written(x.engine, v10 + ".drs_capable") == "False" && written(x.engine, car + ".drs_capable") == "True",
          "a vehicle's drs_capable reaches its row; a row that names none keeps the donor's");
    check(written(x.engine, v10_sw + "_metadata_") == "esp_cs17_mercedes" &&
              written(x.engine, v10_sw + "bundle_name") == "v10_screamer.bdl" &&
              written(x.engine, v10_sw + "engine_offset_x") == "0.1",
          "its variant's sound rows are under an audio switch of its own, named by its label");
    std::string copy = has_label("entrants.vehicles", "vehicle", "mercedes_copy");
    check(!copy.empty() && has_label("entrants.vehicles", "vehicle", "cs17_mercedes") == car &&
              written(x.engine, "audio.audio_car_data.mercedes_copy.bundle_name") == "cs17_mercedes.bdl",
          "a variant that inherits the label gets its key as its label and audio switch");
    int bundles = 0;
    for (int i = 0; i < 8; i++)
        bundles += written(x.engine, "audio.audio_bundle_settings.audio_bundle_settings_" + std::to_string(i) + "._metadata_") ==
                   "cs17_mercedes";
    check(bundles == 1 && !has_label("audio.audio_bundle_settings", "audio_bundle_settings", "v10_screamer").empty(),
          "an engine sound variant with its base's bundle adds no second row for it");
    int refused_rows = 0;
    for (auto &[k, n] : x.refused)
        refused_rows += n;
    check(refused_rows == 0 && x.records["vehicle"] == 3 && x.records["engine_sound"] == 3, "every base and variant applies");
    check(written(x.engine, "entrants.entrant_setup_modes.entrant_setup_mode_0.entrant_lists") ==
              "f1_2025,cs_f1_2017,esp_f1_2017_v10",
          "both lists join the setup mode, in load order");
    std::size_t base_keys = x.addon_fresh["swofty.classic-seasons-2017"], var_keys = x.addon_fresh["mod.esp"];
    check(base_keys > 0 && var_keys > 0 && x.fresh["vehicle"] > 0, "each addon's new keys are counted to it");

    auto adm = content::admit(st.order(), x.addon_fresh, base_keys + var_keys - 1);
    check(adm.skip.size() == 1 && adm.skip.count("mod.esp"), "past the room, the later addon is left out whole");
    content::Expansion y(db.base());
    content::expand(st, y, stat, {"swofty.classic-seasons-2017"});
    check(y.records["content.package"] == 1 &&
              written(y.engine, "entrants.entrant_lists.entrant_list_1._metadata_") == "esp_f1_2017_v10",
          "with the first addon left out, the next one's rows start at the table's next index");

    content::Expansion p(db.base());
    content::expand(st, p, content::planning_ids());
    check(p.addon_fresh == x.addon_fresh && p.fresh == x.fresh, "the plan counts the new keys the boot writes, per addon");
    std::vector<std::string> order = st.order();
    check(holes(x.engine.writes(), db.base()).empty(), "a boot applied whole leaves no table claiming a missing row");
    namespace rm = apply::room;
    std::vector<rm::AddonAsk> asks;
    for (auto &a : order)
        asks.push_back({a, x.addon_fresh[a], 0});
    uint64_t room = std::min(base_keys, var_keys) == var_keys ? var_keys : base_keys + var_keys - 1;
    auto per_addon = rm::admit(room, asks);
    std::set<std::string> kept;
    for (auto &v : per_addon.addons)
        if (v.admitted)
            kept.insert(v.addon);
    auto torn = holes(owned_by(x.engine, kept), db.base());
    std::printf("      per-addon refusal at room %llu keeps %zu addon(s), %zu missing row(s)%s\n", (unsigned long long)room,
                kept.size(), torn.size(), torn.empty() ? "" : (": first " + torn.front()).c_str());
    if (var_keys < base_keys)
        check(!torn.empty() && kept.count("mod.esp") && !kept.count("swofty.classic-seasons-2017"),
              "refusing the first addon of a laid-out boot keeps the second's rows past the first's: the table claims "
              "rows that were never created (1121flatpunto45l's crash)");
    auto whole = rm::admit_whole(room, asks);
    check(!whole.admitted_keys && holes(owned_by(x.engine, {}), db.base()).empty(),
          "past the room, nothing of the laid-out boot is applied, so no table claims a missing row");
    auto again = content::readmit(order, x.addon_fresh, {}, room);
    check(again.skip.size() == 1, "the counted keys leave one addon out whole");
    content::Expansion z(db.base());
    content::expand(st, z, stat, again.skip);
    std::size_t z_keys = 0;
    for (auto &[a, n] : z.addon_fresh)
        z_keys += n;
    check(z_keys <= room && holes(z.engine.writes(), db.base()).empty(),
          "laid out again without it, the boot fits the room and no table claims a missing row");
    auto settled = content::readmit(order, z.addon_fresh, again, room);
    check(settled.skip == again.skip, "the second layout fits, so the readmission settles");
    auto none = content::readmit(order, x.addon_fresh, {}, base_keys + var_keys);
    check(none.skip.empty(), "a boot that fits the room is laid out once");
    std::map<std::string, std::size_t> core_first = x.addon_fresh;
    core_first["swofty.core"] = 5;
    check(content::readmit(order, core_first, {}, base_keys + var_keys).skip.size() == 1,
          "keys of owners outside the load order are charged before any addon");
}

static void selector_index_tests()
{
    rows::Keys keys;
    keys.parse("@t.rows.row_N\n_metadata_\nstat_id\nspeed\n+part\n_part_instancecount_\n"
               "@t.rows.row_N.part_N\n_metadata_\nweight\n");
    Db db;
    db.put("t.rows._row_instancecount_", 0, "4");
    for (int i = 0; i < 3; i++) {
        std::string r = "t.rows.row_" + std::to_string(i);
        db.put(r + "._metadata_", 4, std::string(1, char('a' + i)));
        db.put(r + ".stat_id", 0, std::to_string(40 + i));
        db.put(r + ".speed", 1, std::to_string(i % 2));
        db.put(r + "._part_instancecount_", 0, "1");
        db.put(r + ".part_0._metadata_", 4, "p");
        db.put(r + ".part_0.weight", 0, std::to_string(i));
    }
    db.put("t.rows.row_3.speed", 1, "7");
    rows::Engine scan(db.base(), keys), indexed(db.base(), keys);
    indexed.index_selectors(true);
    auto same = [&](const std::string &path) {
        std::string why_scan, why_indexed;
        auto a = scan.resolve(path, &why_scan), b = indexed.resolve(path, &why_indexed);
        return a == b && why_scan == why_indexed;
    };

    check(indexed.resolve("t.rows.row[_metadata_=b]") == std::optional<std::string>("t.rows.row_1"),
          "an indexed selector finds the row the scan finds");
    std::string why;
    check(!indexed.resolve("t.rows.row[speed=0]", &why) && why.find("2 rows match") != std::string::npos,
          "and refuses a value two rows hold, as the scan does");
    indexed.attribute("mod.a", "vehicle");
    scan.attribute("mod.a", "vehicle");
    const char *dup = R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=a]","set":{"_metadata_":"b"}})";
    check(indexed.apply(op(dup)).empty() && scan.apply(op(dup)).empty(), "a row added with a label another row holds");
    check(!indexed.resolve("t.rows.row[_metadata_=b]", &why) && why.find("2 rows match") != std::string::npos,
          "makes the indexed selector refuse it: the added row joins the kept values");
    std::size_t mark_i = indexed.mark(), mark_s = scan.mark();
    const char *rename = R"({"op":"set","select":"t.rows.row[_metadata_=c]","field":"speed","value":"9"})";
    check(indexed.apply(op(rename)).empty() && scan.apply(op(rename)).empty() &&
              indexed.resolve("t.rows.row[speed=9]") == std::optional<std::string>("t.rows.row_2"),
          "a set field is found by its new value");
    indexed.rollback(mark_i);
    scan.rollback(mark_s);
    check(!indexed.resolve("t.rows.row[speed=9]") && same("t.rows.row[speed=9]") && same("t.rows.row[speed=0]"),
          "and a rollback takes it back");
    check(same("t.rows.row[_metadata_=a].part[weight=0]") && same("t.rows.row_1.part[weight=1]"),
          "nested tables are answered the same");
    check(!indexed.resolve("t.rows.row[speed=7]") && same("t.rows.row[speed=7]"), "a row with no _metadata_ is not matched");
    const char *label = R"({"op":"set","select":"t.rows.row_3","field":"_metadata_","value":"d","create":true,"type":4})";
    check(indexed.apply(op(label)).empty() && scan.apply(op(label)).empty() &&
              indexed.resolve("t.rows.row[speed=7]") == std::optional<std::string>("t.rows.row_3") && same("t.rows.row[speed=7]"),
          "and is matched by its other fields once it gets one");
    std::string count = indexed.text("t.rows._row_instancecount_").value_or("");
    const char *shrink = R"({"op":"set","select":"t.rows","field":"_row_instancecount_","value":"3"})";
    std::string why_shrink = indexed.apply(op(shrink));
    check(why_shrink.empty() && scan.apply(op(shrink)).empty() && !indexed.resolve("t.rows.row[speed=7]") &&
              same("t.rows.row[speed=7]"),
          "a row past a lowered instance count is not matched " + why_shrink);
    std::string grow = R"({"op":"set","select":"t.rows","field":"_row_instancecount_","value":")" + count + R"("})";
    check(indexed.apply(op(grow.c_str())).empty() && scan.apply(op(grow.c_str())).empty() &&
              indexed.resolve("t.rows.row[speed=7]") == std::optional<std::string>("t.rows.row_3"),
          "and is matched again once the count takes it back in");

    {
        rows::Engine kept(db.base(), keys), plain(db.base(), keys);
        kept.index_selectors(true);
        kept.attribute("mod.a", "vehicle");
        plain.attribute("mod.a", "vehicle");
        auto both = [&](const std::string &o) { return kept.apply(op(o.c_str())).empty() && plain.apply(op(o.c_str())).empty(); };
        auto agree = [&](const std::string &path) {
            std::string a, b;
            return kept.resolve(path, &a) == plain.resolve(path, &b) && a == b;
        };
        check(kept.resolve("t.rows.row[_metadata_=a]") && kept.selector_builds() == 1, "the first selector reads its field once");
        bool added = true;
        for (int i = 0; i < 6; i++)
            added = added && both(R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=a]","set":{"_metadata_":"z)" +
                                  std::to_string(i) + R"(","speed":")" + std::to_string(20 + i) + R"("}})");
        check(added && kept.resolve("t.rows.row[_metadata_=z5]") == plain.resolve("t.rows.row[_metadata_=z5]") &&
                  agree("t.rows.row[_metadata_=a]") && kept.selector_builds() == 1,
              "rows added after it are found without reading the field again");
        check(agree("t.rows.row[speed=24]") && kept.selector_builds() == 2, "a second field is read once");
        check(both(R"({"op":"set","select":"t.rows.row[_metadata_=z4]","field":"speed","value":"31"})") &&
                  agree("t.rows.row[speed=31]") && agree("t.rows.row[speed=24]") && kept.selector_builds() == 2,
              "and a set value moves its row to the new value without reading the field again");
        check(both(R"({"op":"set","select":"t.rows","field":"_row_instancecount_","value":"5"})") &&
                  agree("t.rows.row[_metadata_=z3]") && agree("t.rows.row[speed=31]") && agree("t.rows.row[_metadata_=a]") &&
                  kept.selector_builds() == 2,
              "a lowered instance count takes the rows past it out in place");
    }

    uint64_t seed = 11;
    auto next = [&seed] {
        seed = seed * 6364136223846793005ull + 1442695040888963407ull;
        return seed >> 33;
    };
    bool agree = true;
    int ops = 0, failed = 0, rollbacks = 0;
    std::vector<std::pair<std::size_t, std::size_t>> marks;
    std::vector<std::string> labels = {"a", "b", "c"};
    auto pick = [&] { return labels[next() % labels.size()]; };
    for (int step = 0; step < 600 && agree; step++) {
        std::string label = pick(), fresh = next() % 8 ? "n" + std::to_string(step) : pick();
        std::string value = std::to_string(next() % 4), o;
        switch (next() % 7) {
        case 0:
        case 1:
        case 2:
            o = R"({"op":"add-row","table":"t.rows","from":"t.rows.row[_metadata_=)" + label + R"(]","set":{"_metadata_":")" +
                fresh + R"(","speed":")" + value + R"("}})";
            break;
        case 3:
            o = R"({"op":"set","select":"t.rows.row[_metadata_=)" + label + R"(]","field":")" +
                (next() % 2 ? "speed" : "_metadata_") + R"(","value":")" + (next() % 2 ? value : fresh) + R"("})";
            break;
        case 4:
            o = R"({"op":"add-row","table":"t.rows.row[_metadata_=)" + label + R"(]","from":"t.rows.row[_metadata_=)" + label +
                R"(].part[_metadata_=p]","set":{"_metadata_":")" + (next() % 3 ? "q" + std::to_string(step) : std::string("p")) +
                R"(","weight":")" + value + R"("}})";
            break;
        case 5:
            marks.push_back({indexed.mark(), scan.mark()});
            break;
        default:
            if (!marks.empty() && next() % 2) {
                indexed.rollback(marks.back().first);
                scan.rollback(marks.back().second);
                marks.pop_back();
                rollbacks++;
            } else {
                indexed.commit();
                scan.commit();
                marks.clear();
            }
        }
        if (!o.empty()) {
            std::string a = indexed.apply(op(o.c_str())), b = scan.apply(op(o.c_str()));
            agree = agree && a == b;
            ops++;
            failed += !a.empty();
            if (a.empty() && o.find(fresh) != std::string::npos)
                labels.push_back(fresh);
        }
        for (int l = 0; l < 8 && agree; l++) {
            std::string x = pick();
            agree = same("t.rows.row[_metadata_=" + x + "]") && same("t.rows.row[speed=" + std::to_string(l % 4) + "]") &&
                    same("t.rows.row[_metadata_=" + x + "].part[_metadata_=p]") &&
                    same("t.rows.row[_metadata_=" + x + "].part[weight=" + std::to_string(l % 4) + "]");
        }
    }
    agree = agree && indexed.writes().size() == scan.writes().size();
    for (std::size_t i = 0; agree && i < scan.writes().size(); i++)
        agree = indexed.writes()[i].path == scan.writes()[i].path && indexed.writes()[i].text == scan.writes()[i].text;
    std::printf("selector index: %d operations (%d refused), %d rollbacks, %zu writes\n", ops, failed, rollbacks,
                scan.writes().size());
    check(agree && ops > 300 && failed > 20 && ops - failed > 150 && rollbacks > 10,
          "a long run of added rows, set fields and rollbacks resolves and writes the same with the selector index");
}

int main(int argc, char **argv)
{
    box_time_trial_tests();
    seat_entrant_tests();
    slot_tests();
    engine_tests();
    selector_index_tests();
    room_hole_tests();
    livery_tests();
    garage_tests();
    package_team_row_tests();
    points_tests();
    if (argc > 1)
        variant_tests(argv[1]);
    if (argc > 1)
        leaderboard_tests(argv[1]);
    if (argc > 1)
        team_tests(argv[1]);
    if (argc > 1)
        twelve_team_tests(argv[1]);
    if (argc > 1)
        short_list_tests(argv[1]);
    if (argc > 1)
        stock_only_tests(argv[1]);
    if (argc > 1)
        twelve_slot_tests(argv[1]);
    if (argc > 1)
        drs_tests(argv[1]);
    race_fuel_tests();
    if (argc > 1)
        career_registry_tests(argv[1]);
    if (argc > 1)
        shared_switch_tests(argv[1]);
    if (argc > 1)
        id_clash_tests(argv[1]);
    if (argc > 1)
        virtual_list_tests(argv[1]);
    if (argc > 1)
        late_write_tests(argv[1]);
    if (argc > 1)
        ahead_tests(argv[1]);
    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
