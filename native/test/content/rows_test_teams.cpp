#include "rows_test.hpp"

void stock_team_rows(Db &db)
{
    auto row = [&](const std::string &path, std::initializer_list<std::pair<const char *, const char *>> fields) {
        for (auto &[f, v] : fields)
            db.put(path + "." + f, 2, v);
    };
    db.put("entrants.teams._team_instancecount_", 0, "2");
    const char *names[] = {"mercedes", "tororosso"};
    for (int i = 0; i < 2; i++) {
        std::string p = "entrants.teams.team_" + std::to_string(i);
        row(p, {{"_metadata_", names[i]},
                {"display_name", "lng_stock"},
                {"display_name_code", "STK"},
                {"display_name_short", "lng_stock_s"},
                {"display_name_super_short", "lng_stock_ss"},
                {"team_base", "lng_base"},
                {"livery_name", names[i]},
                {"timing_tower_texture_path", names[i]},
                {"animation_set", "f1_2025"},
                {"nationality", "britain"},
                {"team_boss", "boss"},
                {"expression_label", names[i]}});
        db.put(p + ".stat_id", 0, i ? "3" : "0");
        db.put(p + ".available", 5, "False");
        db.put(p + ".audio_id", 0, "4");
        db.put(p + ".prestige", 0, "-1");
        for (const char *c : {"primary_colour", "secondary_colour", "tertiary_colour", "team_wear_primary_colour",
                              "team_wear_secondary_colour", "team_wear_tertiary_colour", "ui_team_colour"})
            db.put(p + "." + c, 11, "-1000");
        db.put(p + "._story_info_instancecount_", 0, "1");
        row(p + ".story_info_0", {{"_metadata_", "s"}, {"description", i ? "lng_desc_tr" : "lng_desc_mer"}, {"hashtag", "#x"}});
    }
    db.put("entrants.nationality._nationality_instancecount_", 0, "3");
    const char *nations[] = {"default", "britain", "italy"};
    for (int i = 0; i < 3; i++)
        row("entrants.nationality.nationality_" + std::to_string(i), {{"_metadata_", nations[i]}});
    db.put("pit_lane.pit_times._team_pit_times_instancecount_", 0, "2");
    for (int i = 0; i < 2; i++) {
        std::string p = "pit_lane.pit_times.team_pit_times_" + std::to_string(i);
        row(p, {{"_metadata_", names[i]},
                {"team", names[i]},
                {"min_pit_time", i ? "2.9" : "2.4"},
                {"max_pit_time", "3.5"},
                {"min_pit_time_fully_upgraded", "2"},
                {"max_pit_time_fully_upgraded", "3"}});
    }
    db.put("bank.initial_team_cash._InitialTeamCash_instancecount_", 0, "1");
    row("bank.initial_team_cash.InitialTeamCash_0", {{"_metadata_", "tororosso"}, {"team", "tororosso"}, {"cash", "5000"}});
    db.put("facilities.facilities._Facilities_instancecount_", 0, "2");
    for (int g = 0; g < 2; g++) {
        std::string gp = "facilities.facilities.Facilities_" + std::to_string(g);
        row(gp, {{"_metadata_", g ? "2021" : "base"}});
        db.put(gp + ".team_developments._TeamDevelopment_instancecount_", 0, g ? "0" : "1");
    }
    std::string td = "facilities.facilities.Facilities_0.team_developments.TeamDevelopment_0";
    row(td, {{"_metadata_", "tr"}, {"team", "tororosso"}, {"installed", "spec_1,spec_2"}, {"purchased", ""}, {"shutdown", ""}});
    db.put(td + "._date_instancecount_", 0, "1");
    row(td + ".date_0", {{"_metadata_", "d"}, {"day", "1"}, {"month", "2"}, {"year", "2012"}});
    db.put("driver_market.driver_market._DataGroup_instancecount_", 0, "1");
    row("driver_market.driver_market.DataGroup_0", {{"_metadata_", "2025"}});
    db.put("driver_market.driver_market.DataGroup_0.team_driver_rating_range_requirements._TeamDriverRatingRangeRequirement_"
           "instancecount_",
           0, "1");
    row("driver_market.driver_market.DataGroup_0.team_driver_rating_range_requirements.TeamDriverRatingRangeRequirement_0",
        {{"_metadata_", "tororosso"}, {"team", "tororosso"}, {"rating_range", "60,80"}});
    db.put("showroom.teams_history._team_history_instancecount_", 0, "1");
    row("showroom.teams_history.team_history_0", {{"_metadata_", "tororosso"}, {"team", "tororosso"}, {"team_name", "n"}});
}

void team_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.a", "mod.b"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"team.identity", "team.constructor", "team.pit_times", "team.logo"}) {
        std::string schema = slurp(regs + "/" + name + (std::string(name) == "team.identity" ? "/1.1.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int r1 = registry::load_data_file("mod.a", "registry/teams.json", R"j({
      "team.logo": { "toro_rosso_2017": { "images": { "hud": "logos/hud.png", "driver_select": "logos/ds.png",
                                                       "hub": { "stock": "tororosso" } } },
                     "plain": { "images": { "team_select": "logos/ts.png" } } },
      "team.identity": {
        "toro_rosso_2017": { "donor": "game/tororosso", "constructor": "constructor/toro_rosso",
          "provides": [ "constructor/toro_rosso" ],
          "names": { "title": "lng_cs_toro", "short_name": { "literal": "Toro Rosso" }, "super_short": { "literal": "STR" }, "code": "STR" },
          "colours": { "menu": "#0A1E3C", "primary": "#0A1E3C", "secondary": "#C8102E", "tertiary": "#80FFFFFF" },
          "wear": null, "livery_name": "cs17_toro_rosso", "logo": "mod.a/toro_rosso_2017", "prestige": 2,
          "nationality": "italy", "stat_id": 600, "team_boss": "franz_tost", "team_base": { "literal": "Faenza, Italy" } },
        "lost": { "donor": "game/nobody", "names": { "code": "LST" }, "livery_name": "x" },
        "mercedes": { "donor": "game/mercedes", "names": { "code": "MER" }, "livery_name": "x" },
        "plain_team": { "donor": "game/mercedes", "names": { "code": "PLN" }, "livery_name": "cs_plain",
                        "logo": "mod.a/plain", "stat_id": "auto", "nationality": "atlantis" } },
      "team.pit_times": { "toro_rosso": { "team": "mod.a/toro_rosso_2017", "from": "game/mercedes", "min_pit_time": 2.3 } } })j");
    int r2 = registry::load_data_file("mod.b", "registry/teams.json", R"j({
      "team.identity": { "toro_rosso_b": { "extends": "mod.a/toro_rosso_2017", "livery_name": "b_toro_rosso",
                                           "names": { "title": { "literal": "B Toro" } }, "stat_id": "auto" } } })j");
    st.end_register();
    for (auto &r : st.refusals())
        std::printf("      refused: %s %s %s %s: %s\n", r.addon.c_str(), r.registry.c_str(), r.id.c_str(), r.reason.c_str(),
                    r.message.c_str());
    check(r1 == SWOFTY_ASSET_OK && r2 == SWOFTY_ASSET_OK, "the team records register");
    Db db;
    stock_team_rows(db);
    content::Read stock_read = [&](const std::string &path) -> std::optional<std::string> {
        auto r = db.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    for (auto &c : content::CONSTRUCTOR_SEEDS)
        st.add_base(
            "team.constructor", c.key,
            Value::object({{"name", Value(std::string(c.name))}, {"country", Value(std::string(c.country))}, {"team", Value()}}));
    auto stock = content::stock_teams(stock_read);
    for (auto &t : stock)
        st.add_base("team.identity", t.label, t.record);
    st.stock_loaded("team.identity", true);
    check(stock.size() == 2 && stock[0].label == "mercedes" && stock[1].record["stat_id"].as_int() == 3,
          "the stock team rows become team.identity bases with their stat ids");
    auto stock_mer = st.get("team.identity", "game/mercedes");
    check(stock_mer.status == registry::Status::ok && stock_mer.value["names"]["title"]["literal"].as_string() == "lng_stock" &&
              stock_mer.value["colours"]["primary"].as_string() == "#FFFC18" &&
              stock_mer.value["provides"][0].as_string() == "constructor/mercedes",
          "a stock base takes its names and colours from the row and provides its real team's constructor id");
    auto cm = st.get("team.constructor", "constructor/mercedes");
    check(cm.status == registry::Status::ok && cm.value["team"].as_string() == "game/mercedes",
          "constructor/mercedes resolves to the stock row");
    auto pins = content::pins(st);
    check(pins.size() == 1 && pins[0].registry == "team.identity" && pins[0].stat_id == 600, "the pinned team stat id");
    apply::ids::Store ids;
    auto text = [&](const std::string &path) -> std::optional<std::string> {
        auto r = db.base()(content::gvdb::fnv64(path));
        return r ? std::optional<std::string>(r->text) : std::nullopt;
    };
    bool pool = false;
    for (auto &scan : content::scan_all_pools(text)) {
        ids.define_pool(scan.registry, {{{scan.lo, scan.hi}}});
        pool |= scan.registry == "team.identity" && scan.lo == 4 && scan.hi == 16382;
    }
    check(pool, "the team stat id pool starts above the table's ids");
    for (auto &p : pins)
        ids.pin(p.registry, p.id, p.stat_id, "test");
    content::StatIds stat = [&](const std::string &reg, const std::string &id, int64_t pin) -> int64_t {
        return pin >= 0 ? ids.pin(reg, id, pin, "test").id : ids.allocate(reg, id, "test").id;
    };
    content::Expansion x(db.base());
    content::expand(st, x, stat);
    for (auto &l : x.lines["team.identity"])
        std::printf("      %s\n", l.c_str());
    for (auto &l : x.lines["team.pit_times"])
        std::printf("      %s\n", l.c_str());
    check(x.records["team.identity"] == 3 && x.refused["team.identity"] == 1 &&
              st.get("team.identity", "mod.a/lost").status != registry::Status::ok,
          "three teams apply; the one with a taken label does not, and with the stock rows declared the one whose donor is no "
          "stock team never registers");
    check(written(x.engine, "entrants.teams._team_instancecount_") == "5", "the team row count grows by three");
    auto field = [&](const std::string &row, const char *f) { return written(x.engine, "entrants.teams." + row + "." + f); };
    auto team_row = [&](const char *label) {
        for (int i = 2; i < 5; i++)
            if (written(x.engine, "entrants.teams.team_" + std::to_string(i) + "._metadata_") == label)
                return "team_" + std::to_string(i);
        return std::string("none");
    };
    auto side_row = [&](const std::string &table, const std::string &row, const char *label) {
        for (int i = 0; i < 8; i++)
            if (written(x.engine, table + "." + row + "_" + std::to_string(i) + ".team") == label)
                return table + "." + row + "_" + std::to_string(i);
        return std::string("none");
    };
    std::string tr = team_row("toro_rosso_2017"), pl = team_row("plain_team"), vrow = team_row("toro_rosso_b");
    check(tr != "none" && pl != "none" && vrow != "none" && field(tr, "stat_id") == "600",
          "each applied team has a row at the next index; the pinned id is kept");
    check(field(tr, "display_name") == "lng_cs_toro" && field(tr, "display_name_short") == "Toro Rosso" &&
              field(tr, "display_name_super_short") == "STR" && field(tr, "display_name_code") == "STR",
          "names: a lang key stays one, a literal is the text");
    check(field(tr, "livery_name") == "cs17_toro_rosso" && field(tr, "nationality") == "italy" && field(tr, "prestige") == "2",
          "livery name, nationality and prestige");
    bool told_nation = false;
    for (auto &l : x.lines["team.identity"])
        told_nation |=
            l == "team.identity mod.a/plain_team: nationality atlantis is not an entrants.nationality row, default used";
    check(field(pl, "nationality") == "default" && field(vrow, "nationality") == "italy" && told_nation,
          "a nationality with no entrants.nationality row becomes default, and the line says so");
    auto signed_argb = [](uint32_t v) { return std::to_string(int32_t(v)); };
    check(field(tr, "primary_colour") == signed_argb(0xFF0A1E3C) && field(tr, "ui_team_colour") == signed_argb(0xFF0A1E3C) &&
              field(tr, "tertiary_colour") == signed_argb(0x80FFFFFF),
          "colours are signed ARGB, #RRGGBB opaque");
    check(field(tr, "team_wear_primary_colour") == signed_argb(0xFF0A1E3C) &&
              field(tr, "team_wear_secondary_colour") == signed_argb(0xFFC8102E),
          "no wear colours: the kit follows the primary triple");
    check(field(tr, "timing_tower_texture_path") == "toro_rosso_2017", "a hud image gives the team its own timing tower key");
    check(field(tr, "story_info_0.description") == "lng_desc_tr" && field(tr, "_story_info_instancecount_") == "1" &&
              field(pl, "team_base") == "lng_base" && field(pl, "team_boss") == "boss" && field(tr, "animation_set") == "f1_2025",
          "every other key, story_info included, is the donor's");
    check(field(tr, "team_boss") == "franz_tost" && field(tr, "team_base") == "Faenza, Italy",
          "team_boss and team_base: the principal's character label and the base text go on the row");
    check(field(tr, "available") == "False", "available stays false until an entrant list names the team");
    check(field(pl, "timing_tower_texture_path") == "plain_team",
          "a team with an image but none for hud or broadcast gets its own timing tower key");
    check(written(x.engine, "pit_lane.pit_times._team_pit_times_instancecount_") == "5", "three pit time rows are added");
    std::string pit = side_row("pit_lane.pit_times", "team_pit_times", "toro_rosso_2017");
    check(pit != "none" && written(x.engine, pit + ".min_pit_time") == "2.3" &&
              written(x.engine, pit + ".max_pit_time") == "3.5" && x.engine.text(pit + ".min_pit_time") == "2.3" &&
              written(x.engine, pit + "._metadata_") == "toro_rosso_2017",
          "a team.pit_times record copies the named team's row and sets its values (the identity did not copy its own)");
    std::string pit_pl = side_row("pit_lane.pit_times", "team_pit_times", "plain_team");
    check(pit_pl != "none" && written(x.engine, pit_pl + ".min_pit_time") == "2.4",
          "without a record the pit times are the donor's");
    std::string pit_b = side_row("pit_lane.pit_times", "team_pit_times", "toro_rosso_b");
    check(pit_b != "none" && written(x.engine, pit_b + ".min_pit_time") == "2.9",
          "a variant's pit times are its donor's: it inherits the base's donor");
    std::string bank = side_row("bank.initial_team_cash", "InitialTeamCash", "toro_rosso_2017");
    check(bank != "none" && written(x.engine, bank + ".cash") == "5000" &&
              written(x.engine, "bank.initial_team_cash._InitialTeamCash_instancecount_") == "3",
          "initial cash is the donor's");
    std::string fac = side_row("facilities.facilities.Facilities_0.team_developments", "TeamDevelopment", "toro_rosso_2017");
    check(fac != "none" && written(x.engine, fac + ".installed") == "spec_1,spec_2" &&
              written(x.engine, fac + ".date_0.year") == "2012" && written(x.engine, fac + "._date_instancecount_") == "1" &&
              side_row("facilities.facilities.Facilities_1.team_developments", "TeamDevelopment", "toro_rosso_2017") == "none",
          "facilities: the data group that holds the donor's row gets one, with its nested dates; the others none");
    std::string dm = side_row("driver_market.driver_market.DataGroup_0.team_driver_rating_range_requirements",
                              "TeamDriverRatingRangeRequirement", "toro_rosso_2017");
    check(dm != "none" && written(x.engine, dm + ".rating_range") == "60,80" &&
              side_row("showroom.teams_history", "team_history", "toro_rosso_2017") != "none",
          "driver market and showroom history rows");
    check(vrow != "none" && field(vrow, "livery_name") == "b_toro_rosso" && field(vrow, "display_name") == "B Toro" &&
              field(vrow, "stat_id") != "600" && field(vrow, "timing_tower_texture_path") == "toro_rosso_b",
          "a variant copies its base's row, takes its own stat id and its own names");
    check(field(vrow, "story_info_0.description") == "lng_desc_tr", "and the base team's copied keys");
    bool hud = false, ds = false, hub_alias = false, ts = false;
    for (auto &l : x.logos) {
        hud |= l.array == "osd.timing_tower.team_badges_hud" && l.key == "toro_rosso_2017" && l.file == "logos/hud.png";
        ds |= l.array == "teams.driver_select_team_logos" && l.key == "toro_rosso_2017" && l.context == "driver_select";
        hub_alias |= l.context == "hub" && !l.stock.empty();
        ts |= l.array == "team_logos.driver_career_2024_team_select" && l.key == "plain_team" && l.file == "logos/ts.png";
    }
    check(hud && ds && ts && !hub_alias,
          "logo images: the hud array by the timing tower key, the label arrays by the label, driver select wins over hub");
    auto fell = [&](const char *team, const char *array) {
        for (auto &l : x.logos)
            if (l.team == team && l.array == array && l.fallback)
                return l.stock + "|" + l.key + "|" + l.context;
        return std::string();
    };
    auto image = [&](const char *team, const char *array) {
        for (auto &l : x.logos)
            if (l.team == team && l.array == array && !l.fallback && l.stock.empty())
                return l.file + "|" + l.key + "|" + l.context;
        return std::string();
    };
    bool any_fell = false;
    for (auto &l : x.logos)
        any_fell |= (l.team == "plain_team" || l.team == "toro_rosso_2017") && l.fallback;
    check(!any_fell, "a team with an image of its own never takes its donor's slice");
    check(image("plain_team", "teams.driver_select_team_logos") == "logos/ts.png|plain_team|driver_select" &&
              image("plain_team", "teams.badges_broadcast") == "logos/ts.png|plain_team|broadcast" &&
              image("plain_team", "team_logos.driver_career_2024_team_select") == "logos/ts.png|plain_team|team_select",
          "a context with no image takes the team's own image of another context");
    check(image("plain_team", "osd.timing_tower.team_badges_hud") == "logos/ts.png|plain_team|hud" &&
              image("plain_team", "osd.timing_tower.team_badges_broadcast") == "logos/ts.png|plain_team|broadcast",
          "the timing tower arrays take it too, under the team's own badge key");
    check(image("toro_rosso_2017", "team_logos.driver_career_2024_team_select") == "logos/ds.png|toro_rosso_2017|team_select" &&
              image("toro_rosso_2017", "teams.badges_broadcast") == "logos/ds.png|toro_rosso_2017|broadcast" &&
              image("toro_rosso_2017", "osd.timing_tower.team_badges_broadcast") == "logos/hud.png|toro_rosso_2017|broadcast" &&
              image("toro_rosso_2017", "teams.driver_select_team_logos") == "logos/ds.png|toro_rosso_2017|driver_select",
          "a menu array borrows the driver select image first, a timing tower array the other tower context's");
    int said = 0, donor_said = 0;
    for (auto &l : x.lines["team.identity"]) {
        said += l == "team.logo: plain_team uses its team_select image for driver_select";
        said += l == "team.logo: toro_rosso_2017 uses its hud image for broadcast";
        said += l == "team.logo: toro_rosso_2017 uses its driver_select image for broadcast";
        donor_said += l.find("mercedes's") != std::string::npos || l.find("tororosso's") != std::string::npos;
    }
    check(said == 3 && donor_said == 0, "the borrowed image is logged once per team, image and context; no donor line");
    {
        content::Plan bare, ig;
        content::lower_logo_images("bare", "williams", "mod.c/bare", nullptr, "", bare);
        bool donor_only = !bare.logos.empty();
        for (auto &l : bare.logos)
            donor_only &= l.fallback && l.stock == "williams" && l.file.empty() && l.array.rfind("osd.timing_tower.", 0) != 0;
        check(donor_only && bare.lines.size() == 3 && bare.lines[0] == "team.logo: bare uses williams's driver_select logo",
              "only a team with no image at all falls back to the donor's slice, outside the timing tower");
        Value pack = Value::object({{"images", Value::object({{"driver_select", Value("logos/ig_racing_driver_select.png")},
                                                              {"team_select", Value("logos/ig_racing_team_select.png")},
                                                              {"hud", Value("logos/ig_racing_hud.png")}})}});
        content::lower_logo_images("ig_racing", "williams", "ig.teampack/ig_racing", &pack, "ig_racing", ig);
        std::map<std::string, std::string> by_array;
        bool fallback = false;
        for (auto &l : ig.logos) {
            by_array[l.array] = l.file;
            fallback |= l.fallback;
        }
        check(
            !fallback && by_array["teams.badges_broadcast"] == "logos/ig_racing_driver_select.png" &&
                by_array["broadcast_ui_generic.grid_team_logos"] == "logos/ig_racing_driver_select.png" &&
                by_array["osd.timing_tower.team_badges_broadcast"] == "logos/ig_racing_hud.png" &&
                by_array["osd.timing_tower.team_badges_hud"] == "logos/ig_racing_hud.png" &&
                by_array["teams.driver_select_team_logos"] == "logos/ig_racing_driver_select.png" && by_array.size() == 7,
            "ig.teampack's three images: the broadcast badge (Choose Driver's card) is its driver select image, not williams's");
        std::vector<std::string> want = {"team.logo: ig_racing uses its hud image for broadcast",
                                         "team.logo: ig_racing uses its driver_select image for broadcast"};
        check(ig.lines == want, "and the core logs where each borrowed image came from");
    }
    std::map<std::string, Value> keep;
    auto lookup = content::own_lookup(st, keep);
    auto refs = content::refs_of(st, lookup);
    check(refs.constructor_team["toro_rosso"] == "mod.b/toro_rosso_b",
          "constructor/toro_rosso is the last enabled team in load order that is that constructor");
    check(content::team_label(Value("constructor/toro_rosso"), refs) == "toro_rosso_b",
          "and a constructor/ reference resolves to its label");
    check(refs.lines.size() == 1 && refs.lines[0].find("provided by mod.b/toro_rosso_b") != std::string::npos,
          "the constructor report line");

    namespace ct = career::ops::teams;
    auto season = [](int year, const char *team) {
        ct::SeasonIn s;
        s.year = year;
        ct::SlotIn slot;
        slot.slot = 1;
        slot.team = team;
        slot.seats = {"a", "b"};
        s.slots.push_back(slot);
        return s;
    };
    ct::Table table = ct::build_table({season(2012, "mod.a/toro_rosso_2017"), season(2013, "mod.b/toro_rosso_b")}, false);
    ct::Plan plan = ct::plan_season(table, season(2013, "mod.b/toro_rosso_b"));
    check(table.ok() && plan.ok() && plan.count(ct::Kind::dressed) == 1, "apply_teams plans slot 1 as dressed in 2013");
    std::vector<uint8_t> record(0x768, 0);
    std::vector<std::string> bufs(5, std::string(0x80, 'x'));
    for (std::size_t i = 0; i < 5; i++) {
        char *p = bufs[i].data();
        std::size_t at[] = {content::team_record::DISPLAY_NAME, content::team_record::SHORT, content::team_record::SUPER_SHORT,
                            content::team_record::CODE, content::team_record::TEAM_BASE};
        std::memcpy(record.data() + at[i], &p, sizeof p);
    }
    record[content::team_record::TEAM_BOSS] = 9;
    std::vector<std::string> log;
    auto &live = content::live();
    live = content::Live();
    live.log = [&](const std::string &l) { log.push_back(l); };
    live.identity = [&](const std::string &id) -> const Value * {
        auto r = st.get("team.identity", id);
        return r.status == registry::Status::ok ? &(keep["live|" + id] = r.value) : nullptr;
    };
    live.stat_of = [&](const std::string &id) -> int64_t { return ids.get("team.identity", id).id; };
    live.character_stat = [](const std::string &label) -> int64_t { return label == "franz_tost" ? 41 : -1; };
    ct::dresser() = &content::dress_slot;
    auto run_dresser = [&]() {
        int dressed = 0, pending = 0;
        for (auto &a : plan.actions)
            if (a.kind == ct::Kind::dressed) {
                if (ct::dresser() &&
                    ct::dresser()(table.rows.empty() ? "" : "mod.a.2012", a.slot, a.row.c_str(), a.team.c_str(), -1))
                    dressed++;
                else
                    pending++;
            }
        return std::pair<int, int>(dressed, pending);
    };
    auto none = run_dresser();
    check(none.first == 0 && none.second == 1 && live.pending.size() == 1,
          "without a way to the loaded team record the dressing is queued and reported pending");
    live.record = [&](uint16_t stat) -> uint8_t * { return stat == 600 ? record.data() : nullptr; };
    live.pending.clear();
    auto done = run_dresser();
    int32_t primary;
    std::memcpy(&primary, record.data() + content::team_record::PRIMARY, sizeof primary);
    check(done.first == 1 && done.second == 0, "apply_teams dresses the slot row through the dresser");
    check(std::string(bufs[0].c_str()) == "B Toro" && std::string(bufs[3].c_str()) == "STR" && primary == int32_t(0xFF0A1E3C),
          "the loaded record takes the season team's names and colours");
    check(std::string(bufs[4].c_str()) == "Faenza, Italy" && record[content::team_record::TEAM_BOSS] == 41,
          "and its base text and principal: team_boss is the stat id of the character row its label names");
    std::vector<uint8_t> reloaded(0x768, 0);
    std::vector<std::string> bufs2(4, std::string(0x80, 'y'));
    for (std::size_t i = 0; i < 4; i++) {
        char *p = bufs2[i].data();
        std::size_t at[] = {content::team_record::DISPLAY_NAME, content::team_record::SHORT, content::team_record::SUPER_SHORT,
                            content::team_record::CODE};
        std::memcpy(reloaded.data() + at[i], &p, sizeof p);
    }
    check(content::redress(600, reloaded.data()) && std::string(bufs2[0].c_str()) == "B Toro",
          "a record the loader stores again is dressed again from the slot's remembered team");
    check(!content::redress(601, reloaded.data()), "and a stat id nobody dressed is left alone");

    live.stat_of = [&](const std::string &id) -> int64_t {
        int64_t got = ids.get("team.identity", id).id;
        if (got >= 0)
            return got;
        auto rec = st.get("team.identity", id);
        return rec.status == registry::Status::ok && rec.value["stat_id"].is_int() ? rec.value["stat_id"].as_int() : -1;
    };
    std::vector<uint8_t> stock_record(0x768, 0);
    std::vector<std::string> bufs3(5, std::string(0x80, 'z'));
    bufs3[4] = std::string("lng_base") + std::string(0x80 - 8, '\0');
    for (std::size_t i = 0; i < 5; i++) {
        char *p = bufs3[i].data();
        std::size_t at[] = {content::team_record::DISPLAY_NAME, content::team_record::SHORT, content::team_record::SUPER_SHORT,
                            content::team_record::CODE, content::team_record::TEAM_BASE};
        std::memcpy(stock_record.data() + at[i], &p, sizeof p);
    }
    stock_record[content::team_record::TEAM_BOSS] = 7;
    live.record = [&](uint16_t stat) -> uint8_t * { return stat == 3 ? stock_record.data() : nullptr; };
    check(ct::dresser()("g", 2, "game/tororosso", "mod.b/toro_rosso_b", -1) && std::string(bufs3[0].c_str()) == "B Toro" &&
              std::string(bufs3[4].c_str()) == "Faenza, Italy" && stock_record[content::team_record::TEAM_BOSS] == 41,
          "a stock slot row dresses through the stat id of its stock record");
    check(ct::dresser()("g", 2, "game/tororosso", "game/tororosso", -1) && std::string(bufs3[0].c_str()) == "lng_stock" &&
              !live.wears.count(3),
          "a slot that returns to its own team gets the stock names back and is forgotten");
    check(std::string(bufs3[4].c_str()) == "lng_base" && stock_record[content::team_record::TEAM_BOSS] == 7 &&
              !live.kept.count(3),
          "and its own base and principal, as the record held them before the dress");
    live.character_stat = nullptr;
    check(ct::dresser()("g", 2, "game/tororosso", "mod.b/toro_rosso_b", -1) && std::string(bufs3[4].c_str()) == "Faenza, Italy" &&
              stock_record[content::team_record::TEAM_BOSS] == 7 &&
              log.back().find("its team_boss franz_tost is not an entrants.characters row") != std::string::npos,
          "a principal whose character row can't be found leaves the record's principal and says so");
    check(content::undress_all("the career unloaded") == 1 && std::string(bufs3[0].c_str()) == "lng_stock" &&
              std::string(bufs3[4].c_str()) == "lng_base" && stock_record[content::team_record::TEAM_BOSS] == 7 &&
              live.wears.empty() && live.kept.empty() &&
              log.back() == "undress stat id 3: mod.b/toro_rosso_b taken off, game/tororosso back in its own names and colours "
                            "(the career unloaded)",
          "a career unload puts every dressed record back in its own identity, base and principal");
    ct::dresser() = nullptr;
}
