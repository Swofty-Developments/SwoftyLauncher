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

int main()
{
    test_who_gets_a_world();
    test_classic_seasons();
    test_unknown_and_missing();
    test_lowering_rows();
    test_pools();
    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
