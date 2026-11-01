// Host test for list.hpp: the calendar first by venue (with ? and | venues and dropped rounds), "More tracks" in base
// order with per_package order and offered, left-out reasons, the [tracks] box line and the catalog JSON.
// run_track_lists_test.sh builds and runs it.
#include "fixture.hpp"

int main()
{
    trk::Facts facts = trk::parse_facts(FACTS);
    trk::Base base = cs_base();
    trk::BoxInfo box = cs2016();
    trk::Entries none;

    std::vector<trk::Round> rounds = {{3, "venue/shanghai"}, {1, "venue/albert_park"}, {2, "venue/bahrain"},
                                      {4, "venue/sochi"},    {5, "venue/sepang"},      {6, "venue/monaco"}};
    trk::List l = trk::build_list(base, &facts, box, trk::Mode::grand_prix, rounds, none);
    expect((l.calendar == std::vector<std::string>{"game/melbourne", "game/bahrain", "game/shanghai", "test.bahrain24/sepang"}),
           "the calendar in round order, the record track at Sepang's round");
    expect(l.rounds == 6 && l.missing == std::vector<std::string>{"venue/sochi"}, "a round with no providing track is dropped");
    expect((l.more == std::vector<std::string>{"game/melbourne_reverse", "mod.miami24/igora_drive", "mod.okayama/okayama"}),
           "More tracks: every other fitting base track in base order, Igora Drive's 10 garages included");
    expect(l.left_out.size() == 1 && l.left_out[0].track == "game/monaco", "Monaco (grid) is left out");
    expect(trk::list_line(box.package, trk::Mode::grand_prix, l) ==
               "[tracks] box swofty.classic-seasons-2016/f1_2016 (grand_prix): calendar 4 of 6 rounds (venue/sochi: no "
               "track), more tracks 3; left out game/monaco (its swofty_f1 world has 20 grid slots; the box runs 22 cars)",
           "the [tracks] box line");

    l = trk::build_list(base, &facts, box, trk::Mode::grand_prix,
                        {{1, "venue/sochi|venue/albert_park"}, {2, "venue/sochi?"}, {3, "venue/okayama?"}}, none);
    expect((l.calendar == std::vector<std::string>{"game/melbourne", "mod.okayama/okayama"}) &&
               l.missing == std::vector<std::string>{"venue/sochi"},
           "a | venue takes the first that resolves; a ? venue with no track is dropped");

    l = trk::build_list(base, &facts, box, trk::Mode::grand_prix,
                        {{1, "venue/bahrain"}, {2, "venue/albert_park"}, {3, "venue/bahrain"}, {4, "venue/sochi"},
                         {5, "venue/sochi?"}, {6, "venue/albert_park"}},
                        none);
    expect((l.calendar == std::vector<std::string>{"game/bahrain", "game/melbourne"}) && l.rounds == 3 &&
               l.missing == std::vector<std::string>{"venue/sochi"},
           "a venue a calendar repeats (2020's two races at one track) is listed and counted once");
    expect(trk::list_line(box.package, trk::Mode::grand_prix, l).find("calendar 2 of 3 rounds (venue/sochi: no track)") !=
               std::string::npos,
           "the [tracks] box line counts a repeated venue once");

    trk::List tt = trk::build_list(base, &facts, box, trk::Mode::time_trial, {}, none);
    int no_tt = 0;
    for (auto &o : tt.left_out)
        no_tt += o.reason == "no Time Trial support";
    expect(tt.calendar.empty() && no_tt == 1 && tt.more.size() == 6 && trk::listed(tt.more, "mod.okayama/okayama"),
           "a Time Trial box leaves out the record track without time_trial and keeps the tracks made for 1.5 (D5)");
    expect(trk::list_line("game/f1_2026", trk::Mode::time_trial, tt) ==
               "[tracks] box game/f1_2026 (time_trial): 6 tracks; left out game/monaco (its swofty_f1 world has 20 grid "
               "slots; the box runs 22 cars), test.bahrain24/sepang (no Time Trial support)",
           "the Time Trial line names each left-out track with its reason");

    trk::Entries e;
    e.by_track["game/bahrain"] = {"cs/f1_2016/bahrain", "game/bahrain", false, 0, {}};
    e.by_track["mod.okayama/okayama"] = {"cs/f1_2016/okayama", "mod.okayama/okayama", true, 1, {}};
    l = trk::build_list(base, &facts, box, trk::Mode::grand_prix, {{1, "venue/bahrain"}, {2, "venue/albert_park"}}, e);
    expect(l.calendar == std::vector<std::string>{"game/melbourne"} && l.left_out.front().track == "game/bahrain" &&
               l.left_out.front().reason == "not offered in this box",
           "offered = false leaves a calendar track out");
    expect((l.more == std::vector<std::string>{"mod.okayama/okayama", "game/shanghai", "game/melbourne_reverse",
                                               "test.bahrain24/sepang", "mod.miami24/igora_drive"}),
           "order 1 puts a track first under More tracks; order 0 keeps the base order");
    e.by_track["mod.okayama/okayama"].order = 99;
    l = trk::build_list(base, &facts, box, trk::Mode::grand_prix, {}, e);
    expect(l.more.back() == "mod.okayama/okayama", "an order past the end puts the track last");

    trk::List f1990 = trk::build_list(base, &facts, [] {
        trk::BoxInfo b = cs2016(10);
        for (auto &c : b.ai)
            c.folder = "f1_1990", c.asset = "f1_1990_car";
        return b;
    }(), trk::Mode::grand_prix, {}, none);
    expect((f1990.more == std::vector<std::string>{"game/melbourne", "game/shanghai", "game/melbourne_reverse"}),
           "F1 1990 keeps Melbourne (its AI data) and the tracks with no facts; the rest are left out");
    expect(f1990.left_out.size() == 5, "F1 1990 leaves out the five tracks without its AI folder");

    Value js = trk::list_json(l);
    expect(js["calendar"].size() == 0 && js["more"].size() == l.more.size() && js["left_out"][0]["reason"].is_string(),
           "list_json writes calendar, more and left_out as the header's TrackList");
    Value cal = Value::object({{"rounds", Value::array({Value::object({{"round", Value(2)}, {"venue", Value("venue/monaco")}}),
                                                        Value::object({{"round", Value(1)}})})}});
    auto r = trk::rounds_of(cal);
    expect(r.size() == 2 && r[0].round == 2 && r[0].venue == "venue/monaco" && r[1].venue.empty(),
           "rounds_of reads a career.calendar record");
    l = trk::build_list(base, &facts, box, trk::Mode::grand_prix, r, none);
    expect(l.missing == std::vector<std::string>{"round 1"}, "a round with no venue is dropped and named by its number");

    Value doc = swofty::json::parse(R"({"tracks": {"game/bdx_s5_2012": {"grand_prix": {"calendar": ["game/monaco"]}},
                                                   "mod.esp/f1_2017_v10": {"grand_prix": {}}}})");
    trk::put_catalog(doc, "Classic Seasons", "a1", "swofty.classic-seasons-2016/f1_2016", trk::Mode::grand_prix, trk::list_json(l));
    const Value &cat = doc;
    expect(cat["profile"].as_string() == "Classic Seasons" && cat["build"].as_string() == "a1" && cat["tracks"].size() == 1 &&
               cat["tracks"]["swofty.classic-seasons-2016/f1_2016"]["grand_prix"].is_object(),
           "a catalog.json an older core wrote (no profile or build) starts again at the first box press");
    trk::put_catalog(doc, "Classic Seasons", "a1", "swofty.classic-seasons-2017/f1_2017", trk::Mode::time_trial, trk::list_json(l));
    trk::put_catalog(doc, "Classic Seasons", "a1", "swofty.classic-seasons-2016/f1_2016", trk::Mode::time_trial, trk::list_json(l));
    expect(cat["tracks"].size() == 2 && cat["tracks"]["swofty.classic-seasons-2016/f1_2016"].size() == 2 &&
               cat["tracks"]["swofty.classic-seasons-2017/f1_2017"]["time_trial"].is_object(),
           "box presses of one build add boxes and modes");
    trk::put_catalog(doc, "Tails what-if", "a1", "game/f1_2025", trk::Mode::grand_prix, trk::list_json(l));
    expect(cat["profile"].as_string() == "Tails what-if" && cat["tracks"].size() == 1 && cat["tracks"]["game/f1_2025"].is_object(),
           "a box press under another profile drops the other profile's boxes");
    trk::put_catalog(doc, "Tails what-if", "b2", "game/bdx_s5_2012", trk::Mode::grand_prix, trk::list_json(l));
    expect(cat["build"].as_string() == "b2" && cat["tracks"].size() == 1 && cat["tracks"]["game/bdx_s5_2012"].is_object(),
           "a box press of a new build of the same profile drops the earlier build's boxes");

    trk::Facts with2012 = trk::parse_facts(R"({"version": 1, "grid_size": 24, "layers": [
        {"label": "bdx_s5_track_24", "setup_modes": ["bdx_2012_mode"], "seasons": ["season_2025"], "world": "2012"}],
      "tracks": {
        "melbourne": {"worlds": {"2012": {"grid": 24, "garages": 12, "pit_boxes": 12}}},
        "bahrain": {"worlds": {"2012": {"grid": 24, "garages": 12, "pit_boxes": 12}}},
        "igora_drive": {"worlds": {"2012": {"grid": 24, "garages": 10, "pit_boxes": 10}}},
        "okayama": {"worlds": {"f1_26": {"grid": 24, "garages": 11, "pit_boxes": 11}}}}})");
    trk::BoxInfo b2012;
    b2012.package = "game/bdx_s5_2012";
    b2012.entrant_list = "bdx_s5_2012";
    b2012.setup_mode = "bdx_2012_mode";
    b2012.season = "season_2025";
    b2012.teams = 12;
    b2012.cars = 24;
    trk::List l2012 = trk::build_list(base, &with2012, b2012, trk::Mode::grand_prix, {}, none);
    bool okayama_out = false;
    for (auto &o : l2012.left_out)
        okayama_out = okayama_out || (o.track == "mod.okayama/okayama" &&
                                      o.reason == "its files hold no 2012 world, which the box's bdx_2012_mode layer loads");
    expect(okayama_out && l2012.left_out.size() == 1 && trk::listed(l2012.more, "mod.miami24/igora_drive") &&
               l2012.more.front() == "game/melbourne",
           "a 12-team era box made from its entrant list: a track without its world is left out, a 10-garage world fits");

    printf("%s\n", fails ? "list_test: FAILED" : "list_test: all passed");
    return fails ? 1 : 0;
}
