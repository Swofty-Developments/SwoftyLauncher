// Host unit test for a box's own world (core/apply/content/box_world.hpp and lower_box_world in lower.hpp): which
// boxes get a season and a track layer of their own, the seasons lists that gain the season, and the rows the
// lowering adds, over a hand-made database.
#include "core/apply/content/lower.hpp"

#include <cstdio>
#include <map>
#include <string>

namespace content = swofty_core::apply::content;
namespace bw = swofty_core::apply::content::box_world;
using swofty::json::Value;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

struct Db {
    std::map<std::string, std::string> keys;

    void rows(const std::string &container, const std::string &word, const std::vector<std::map<std::string, std::string>> &rows)
    {
        keys[container + "._" + word + "_instancecount_"] = std::to_string(rows.size());
        for (std::size_t i = 0; i < rows.size(); i++)
            for (auto &[f, v] : rows[i])
                keys[container + "." + word + "_" + std::to_string(i) + "." + f] = v;
    }

    // Resolves one [_metadata_=label] selector, as the row engine's reads do.
    std::optional<std::string> read(const std::string &path) const
    {
        std::string p = path;
        auto at = p.find("[_metadata_=");
        if (at != std::string::npos) {
            auto end = p.find(']', at);
            std::string label = p.substr(at + 12, end - at - 12);
            std::string head = p.substr(0, at), rest = p.substr(end + 1);
            bool found = false;
            for (int i = 0; i < 64 && !found; i++)
                if (auto it = keys.find(head + "_" + std::to_string(i) + "._metadata_");
                    it != keys.end() && it->second == label) {
                    p = head + "_" + std::to_string(i) + rest;
                    found = true;
                }
            if (!found)
                return std::nullopt;
        }
        auto it = keys.find(p);
        return it == keys.end() ? std::nullopt : std::optional<std::string>(it->second);
    }

    bw::Read reader() const
    {
        return [this](const std::string &path) { return read(path); };
    }
};

static Db stock(bool classic_seasons = false)
{
    Db d;
    std::vector<std::map<std::string, std::string>> layers = {
        {{"_metadata_", "f1"},
         {"game_setup_modes", classic_seasons ? "f1" : "f1,swofty_f1"},
         {"seasons", "season_2023,season_2024,season_2025"},
         {"worldEri", "eri/f1"},
         {"statId", "0"}},
        {{"_metadata_", "f1_26"},
         {"game_setup_modes", "f1_26,f1a_mp"},
         {"seasons", "season_2026"},
         {"worldEri", "eri/f1_26"},
         {"statId", "8"}},
        {{"_metadata_", "myteam_26"},
         {"game_setup_modes", ""},
         {"seasons", "season_2026"},
         {"worldEri", "eri/f1_26_myteam"},
         {"statId", "9"}},
    };
    if (classic_seasons)
        layers.push_back({{"_metadata_", "swofty_f1"},
                          {"game_setup_modes", "swofty_f1"},
                          {"seasons", "season_2023,season_2024,season_2025"},
                          {"worldEri", "eri/f1_26"},
                          {"statId", "20"}});
    d.rows("track_data.track_layers", "TrackLayer", layers);
    d.rows("game_setup.seasons", "season",
           {{{"_metadata_", "season_2025"}, {"statId", "7"}}, {{"_metadata_", "season_2026"}, {"statId", "8"}}});
    d.rows("track_data.game_setup", "TrackDataSet",
           {{{"_metadata_", "a"}, {"seasons", "season_2024,season_2025"}},
            {{"_metadata_", "b"}, {"seasons", "season_2026"}},
            {{"_metadata_", "c"}, {"seasons", "season_2025"}}});
    d.rows("game_setup.branding", "game_setup_branding", {{{"_metadata_", "x"}, {"seasons", "season_2026"}}});
    d.rows("game_setup.branding.game_setup_branding_0", "branding_videos",
           {{{"seasons", "season_2026"}}, {{"seasons", "season_2023,season_2025"}}});
    d.rows("tyres.compound_classes", "CompoundClass", {{{"seasons", "season_2025"}}});
    d.keys["entrants.entrant_lists._entrant_list_instancecount_"] = "1";
    d.keys["entrants.entrant_lists.entrant_list_0._metadata_"] = "f1_2025";
    d.keys["entrants.entrant_lists.entrant_list_0.season"] = "season_2025";
    return d;
}

static void test_who_gets_a_world()
{
    Db d = stock();
    auto ten = bw::choose("sw_2013", "swofty_f1", 10, "season_2025", d.reader());
    check(!ten.apply && ten.line.empty(), "a 10-team box keeps the f1 world, with no line");

    auto eleven = bw::choose("sw_2013", "swofty_f1", 11, "season_2025", d.reader());
    check(eleven.apply && eleven.from && std::string(eleven.from->layer) == "f1_26" && eleven.now == 10,
          "an 11-team box on the 10-garage f1 world takes f1_26");
    check(eleven.season == "swofty_box_sw_2013" && eleven.layer == "swofty_box_sw_2013_world" &&
              eleven.from_season == "season_2025",
          "its season and layer labels, copied from the list's season");
    std::vector<std::string> want = {"game_setup.branding.game_setup_branding_0.branding_videos_1",
                                     "track_data.game_setup.TrackDataSet_0", "track_data.game_setup.TrackDataSet_2",
                                     "tyres.compound_classes.CompoundClass_0"};
    auto got = eleven.appends;
    std::sort(got.begin(), got.end());
    check(got == want, "the season goes on every seasons list that holds season_2025 (nested rows too), and on no "
                       "track layer row");

    auto twelve = bw::choose("sw_2016", "swofty_f1", 12, "season_2025", d.reader());
    check(twelve.apply && std::string(twelve.from->layer) == "myteam_26" && twelve.from->garages == 12,
          "a 12-team box takes myteam_26");

    auto thirteen = bw::choose("big", "swofty_f1", 13, "season_2025", d.reader());
    check(!thirteen.apply && thirteen.line.find("more than 12 garages") != std::string::npos, "13 teams: no world, with a line");

    auto other = bw::choose("sw_2013", "f1_26", 11, "season_2026", d.reader());
    check(!other.apply && other.line.empty(), "a list whose world already has 11 garages is left as it is");
}

static void test_classic_seasons()
{
    Db d = stock(true);
    auto eleven = bw::choose("sw_2013", "swofty_f1", 11, "season_2025", d.reader());
    check(!eleven.apply && eleven.line.empty(), "with Classic Seasons' swofty_f1 layer (an f1_26 copy) 11 teams fit already");
    auto twelve = bw::choose("sw_2016", "swofty_f1", 12, "season_2025", d.reader());
    check(twelve.apply && twelve.now == 11 && std::string(twelve.from->layer) == "myteam_26",
          "12 teams still get myteam_26 on top of it");
}

static void test_unknown_and_missing()
{
    Db d = stock();
    d.keys["track_data.track_layers.TrackLayer_0.worldEri"] = "eri/someone_elses";
    d.keys["track_data.track_layers.TrackLayer_0._metadata_"] = "addon_world";
    auto c = bw::choose("sw_2013", "swofty_f1", 11, "season_2025", d.reader());
    check(!c.apply && c.line.find("no garage count") != std::string::npos,
          "a layer the core cannot count garages for is left alone, with a line");

    Db none;
    auto n = bw::choose("sw_2013", "swofty_f1", 11, "season_2025", none.reader());
    check(!n.apply && n.line.find("no track_data.track_layers rows") != std::string::npos,
          "no layer rows to read: left alone, with a line");

    Db s = stock();
    auto missing = bw::choose("sw_2013", "swofty_f1", 11, "season_1999", s.reader());
    check(!missing.apply, "a season with no game_setup.seasons row matches no layer and is not copied");
}

static std::vector<std::string> ops_of(const content::Plan &plan)
{
    std::vector<std::string> out;
    for (std::size_t i = 0; i < plan.ops.size(); i++) {
        const Value &op = plan.ops[i];
        std::string s = std::string(op["op"].as_string()) + " ";
        if (op["table"].is_string())
            s += std::string(op["table"].as_string()) + " from " + std::string(op["from"].as_string());
        else
            s += std::string(op["select"].as_string()) + "." + std::string(op["field"].as_string()) +
                 " += " + std::string(op["value"].as_string());
        out.push_back(s);
    }
    return out;
}

static void test_lowering_rows()
{
    Db d = stock();
    std::map<std::string, int64_t> next = {{bw::SEASON_IDS, 16}, {bw::LAYER_IDS, 21}};
    content::StatIds stat = [&next](const std::string &registry, const std::string &, int64_t) { return next[registry]++; };
    content::Plan plan;
    Value row = Value::object({{"_metadata_", "sw_2013"}});
    content::lower_box_world("swofty.swofty-seasons-2013/sw_2013", "sw_2013", "swofty_f1", 11, "f1_2025", row, d.reader(), stat,
                             plan);
    auto ops = ops_of(plan);
    check(ops.size() == 6, "one season row, four appends, one layer row (" + std::to_string(ops.size()) + " ops)");
    check(!ops.empty() && ops.front() == "add-row game_setup.seasons from game_setup.seasons.season[_metadata_=season_2025]",
          "the season row copies season_2025");
    check(!ops.empty() &&
              ops.back() == "add-row track_data.track_layers from track_data.track_layers.TrackLayer[_metadata_=f1_26]",
          "the layer row copies f1_26");
    const Value &layer = plan.ops[plan.ops.size() - 1]["set"];
    check(layer["statId"].as_string() == "21" && layer["game_setup_modes"].as_string() == "swofty_f1" &&
              layer["seasons"].as_string() == "swofty_box_sw_2013",
          "the layer row's id, setup mode and season");
    check(plan.ops[0]["set"]["statId"].as_string() == "16", "the season's id comes from the content.package.season pool");
    check(row["season"].as_string() == "swofty_box_sw_2013", "the list row carries the box's season");
    check(!plan.lines.empty() && plan.lines.back().find("11 teams, its world holds 10 garages: season swofty_box_sw_2013 "
                                                        "(statId 16), layer swofty_box_sw_2013_world (f1_26, 11 garages, "
                                                        "statId 21), 4 season lists") != std::string::npos,
          "one line names the season, the layer and the lists");

    content::Plan full;
    Value row2 = Value::object({{"_metadata_", "sw_2013"}});
    content::StatIds none = [](const std::string &, const std::string &, int64_t) -> int64_t { return -1; };
    content::lower_box_world("swofty.swofty-seasons-2013/sw_2013", "sw_2013", "swofty_f1", 11, "f1_2025", row2, d.reader(), none,
                             full);
    check(full.ops.size() == 0 && !row2["season"].is_string(), "no free season id: no rows, the list keeps its season");
    check(!full.lines.empty() && full.lines.back().find("no free season id (0 to 31)") != std::string::npos,
          "and a line says so");

    content::Plan own;
    Value row3 = Value::object({{"_metadata_", "sw_2012"}, {"season", "season_2026"}});
    content::lower_box_world("swofty.swofty-seasons-2012/sw_2012", "sw_2012", "swofty_f1", 12, "f1_2025", row3, d.reader(), stat,
                             own);
    check(own.ops.size() > 0 &&
              ops_of(own).front() == "add-row game_setup.seasons from game_setup.seasons.season[_metadata_=season_2026]" &&
              row3["season"].as_string() == "swofty_box_sw_2012",
          "a list that sets its own season works from that season");
    check(!own.lines.empty() && own.lines.back().find("12 teams, no layer gives its list a world") != std::string::npos,
          "a list no layer gives a world says so");
}

static void test_time_trial_list()
{
    Db d = stock();
    auto &k = d.keys;
    k["track_data.track_layers._TrackLayer_instancecount_"] = "4";
    k["track_data.track_layers.TrackLayer_3._metadata_"] = "sw12_world";
    k["track_data.track_layers.TrackLayer_3.game_setup_modes"] = "swofty_f1";
    k["track_data.track_layers.TrackLayer_3.seasons"] = "sw12_season";
    k["track_data.track_layers.TrackLayer_3.worldEri"] = "eri/f1_26_myteam";
    k["game_setup.seasons._season_instancecount_"] = "3";
    k["game_setup.seasons.season_2._metadata_"] = "sw12_season";
    k["game_setup.seasons.season_2.statId"] = "15";

    auto gp = bw::choose("sw_f1_2012", "swofty_f1", 12, "sw12_season", d.reader());
    check(!gp.apply && gp.line.empty(), "a Grand Prix list on the addon's own season and My Team layer keeps it");

    auto tt = bw::choose("sw_f1_2012_tt", "swofty_f1", 12, "season_2025", d.reader());
    check(tt.apply && tt.now == 10 && std::string(tt.from->layer) == "myteam_26" && tt.season == "swofty_box_sw_f1_2012_tt",
          "its Time Trial list, on f1_2025's season, loads the 10-garage f1 world and gets a My Team world of its own");
}

static void test_enabled()
{
    check(bw::enabled("") && bw::enabled("1") && bw::enabled("yes"),
          "box worlds are on when SWOFTY_BOX_WORLDS is unset or not 0");
    check(!bw::enabled("0"), "SWOFTY_BOX_WORLDS=0 turns them off");
}

static void test_pools()
{
    bool season = false, layer = false;
    for (auto &p : content::POOLS) {
        season = season || (std::string(p.registry) == bw::SEASON_IDS && p.cap == 31 &&
                            std::string(p.table) == "game_setup.seasons" && std::string(p.field) == "statId");
        layer = layer || (std::string(p.registry) == bw::LAYER_IDS && p.cap == 255 &&
                          std::string(p.table) == "track_data.track_layers" && std::string(p.row) == "TrackLayer");
    }
    check(season && layer, "the season pool stops at 31 and the layer pool at 255");
}

static Db with_branding()
{
    Db d = stock();
    d.rows("game_setup.branding", "game_setup_branding",
           {{{"_metadata_", "f1_26"}, {"seasons", "season_2025,season_2026"}, {"stat_id", "3"}},
            {{"_metadata_", "f2_26"}, {"seasons", "season_2026"}}});
    return d;
}

static void test_named_world()
{
    Db d = with_branding();
    bw::Own own{"e1950", "e1950s", "", "", true};
    bw::WorldTracks tracks = [](const std::string &name) -> std::optional<std::vector<std::string>> {
        if (name == "e1950")
            return std::vector<std::string>{"spa_francorchamps", "monza", "silverstone"};
        return std::vector<std::string>{};
    };
    auto c = bw::choose_named(10, "season_2025", own, d.reader(), tracks);
    check(c.apply && c.season == "swofty_world_e1950s" && c.layer == "swofty_world_e1950s_world" && c.from_layer == "f1_26",
          "a 10-team box that names a world gets the shared season and a layer copied from f1_26");
    check(c.checked && c.tracks == std::vector<std::string>({"monza", "silverstone", "spa_francorchamps"}),
          "the tracks that hold the archive, sorted");
    check(c.branding == "swofty_world_e1950s_branding" && c.from_branding == "f1_26" && c.new_branding,
          "its branding row copies f1_26");
    bool stock_branding = false;
    for (auto &r : c.appends)
        stock_branding = stock_branding || r == "game_setup.branding.game_setup_branding_0";
    check(!stock_branding, "with a branding row of its own the season stays off the stock branding rows");
    check(bw::world_eri("e1950") == "eaid://environment_package/tracks/[FILENAME]/wep/[FILENAME]_e1950.wep" &&
              bw::world_group("e1950") == "asset_groups/environment_package/tracks/[FILENAME]/wep/[FILENAME]_e1950" &&
              bw::world_file("monza", "e1950") == "2025_asset_groups/environment_package/tracks/monza/wep/monza_e1950.erp",
          "the world's group, eri and archive");

    auto twelve = bw::choose_named(12, "season_2025", bw::Own{"e1950", "", "", "", false}, d.reader(), tracks);
    check(twelve.apply && twelve.from_layer == "myteam_26" && twelve.share == "e1950" && twelve.branding.empty(),
          "12 teams copy myteam_26; the season defaults to the world's name");

    auto none = bw::choose_named(10, "season_2025", bw::Own{"e1960", "", "", "", false}, d.reader(), tracks);
    check(!none.apply && none.line.find("no track folder holds") != std::string::npos,
          "a world no track folder holds leaves the box's world as it is, with a line");

    auto bad = bw::choose_named(10, "season_2025", bw::Own{"E1950", "", "", "", false}, d.reader(), tracks);
    check(!bad.apply && bad.line.find("a name is not") != std::string::npos, "a bad name is refused with a line");

    auto unchecked = bw::choose_named(10, "", bw::Own{"e1950", "", "", "", false}, d.reader(), {});
    check(unchecked.apply && !unchecked.checked && unchecked.from_season == "season_2025",
          "a host tool that cannot look applies it; a list with no season copies season_2025");

    check(bw::own_enabled("1") && !bw::own_enabled("") && !bw::own_enabled("0"), "SWOFTY_BOX_OWN_WORLDS is off unless 1");
}

static void test_named_lowering()
{
    Db d = with_branding();
    std::map<std::string, int64_t> next = {{bw::SEASON_IDS, 25}, {bw::LAYER_IDS, 30}, {bw::BRANDING_IDS, 13}};
    content::StatIds stat = [&next](const std::string &registry, const std::string &, int64_t) { return next[registry]++; };
    bw::WorldTracks tracks = [](const std::string &) -> std::optional<std::vector<std::string>> {
        return std::vector<std::string>{"monza"};
    };
    Value branding_block = Value::object({{"timing_tower_logo", "lc_f1_1950_logo"}, {"broadcast_show_f1_logo", false}});
    Value world = Value::object({{"name", "e1950"}, {"season", "e1950s"}, {"branding", branding_block}});
    Value rec = Value::object({{"entrant_list", "f5_1950"}, {"world", world}});
    content::Plan plan;
    Value row = Value::object({{"_metadata_", "f5_1950"}});
    content::lower_named_world("vmknolle.season-1950/f5_1950", rec, "swofty_f1", 10, "f1_2025", row, d.reader(), stat, plan,
                               &tracks);
    auto ops = ops_of(plan);
    const Value *layer = nullptr, *branding = nullptr;
    for (std::size_t i = 0; i < plan.ops.size(); i++) {
        if (ops[i].rfind("add-row track_data.track_layers", 0) == 0)
            layer = &plan.ops[i]["set"];
        if (ops[i].rfind("add-row game_setup.branding from game_setup.branding.game_setup_branding[_metadata_=f1_26]", 0) == 0)
            branding = &plan.ops[i]["set"];
    }
    check(!ops.empty() && ops.front() == "add-row game_setup.seasons from game_setup.seasons.season[_metadata_=season_2025]" &&
              plan.ops[0]["set"]["statId"].as_string() == "25" &&
              plan.ops[0]["set"]["_metadata_"].as_string() == "swofty_world_e1950s",
          "the shared season row copies season_2025 with an id from the season pool");
    check(layer && (*layer)["worldEri"].as_string() == bw::world_eri("e1950") &&
              (*layer)["worldAssetGroup"].as_string() == bw::world_group("e1950") && (*layer)["statId"].as_string() == "30" &&
              (*layer)["seasons"].as_string() == "swofty_world_e1950s" && (*layer)["game_setup_modes"].as_string() == "swofty_f1",
          "the layer row names the box's world for its setup mode and season");
    check(branding && (*branding)["timing_tower_logo_texture_path"].as_string() == "lc_f1_1950_logo" &&
              (*branding)["broadcast_show_f1_logo"].as_string() == "False" && (*branding)["stat_id"].as_string() == "13" &&
              (*branding)["seasons"].as_string() == "swofty_world_e1950s",
          "the branding row carries the lap counter logo and the season");
    check(row["season"].as_string() == "swofty_world_e1950s", "the list row carries the shared season");
    check(!plan.lines.empty() && plan.lines.back().find("1 track(s) hold it (monza)") != std::string::npos,
          "one line names the rows and the tracks");

    Db shared = with_branding();
    shared.keys["game_setup.seasons._season_instancecount_"] = "3";
    shared.keys["game_setup.seasons.season_2._metadata_"] = "swofty_world_e1950s";
    shared.keys["track_data.track_layers._TrackLayer_instancecount_"] = "4";
    shared.keys["track_data.track_layers.TrackLayer_3._metadata_"] = "swofty_world_e1950s_world";
    shared.keys["game_setup.branding._game_setup_branding_instancecount_"] = "3";
    shared.keys["game_setup.branding.game_setup_branding_2._metadata_"] = "swofty_world_e1950s_branding";
    content::Plan again;
    Value row2 = Value::object({{"_metadata_", "f5_1950_tt"}});
    content::lower_named_world("vmknolle.season-1950/f5_1950_tt", rec, "swofty_f1", 10, "f1_2025", row2, shared.reader(), stat,
                               again, &tracks);
    check(again.ops.size() == 0 && row2["season"].as_string() == "swofty_world_e1950s",
          "a second box of the same season adds no rows and takes the season");

    content::Plan full;
    Value row3 = Value::object({{"_metadata_", "f5_1950"}});
    content::StatIds none = [](const std::string &, const std::string &, int64_t) -> int64_t { return -1; };
    content::lower_named_world("vmknolle.season-1950/f5_1950", rec, "swofty_f1", 10, "f1_2025", row3, d.reader(), none, full,
                               &tracks);
    check(full.ops.size() == 0 && !row3["season"].is_string() && !full.lines.empty() &&
              full.lines.back().find("no free season id (0 to 31)") != std::string::npos,
          "no free season id: no rows, with a line naming world.season");
}

int main()
{
    test_who_gets_a_world();
    test_classic_seasons();
    test_unknown_and_missing();
    test_lowering_rows();
    test_time_trial_list();
    test_enabled();
    test_pools();
    test_named_world();
    test_named_lowering();
    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
