// Shared fixtures for the track list host tests (fits_test, list_test, per_package_test): a small base, a facts
// file in the C2 shape and a Classic Seasons 2016-like box.
#pragma once

#include "../../src/core/apply/tracks/list.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace trk = swofty_core::apply::tracks;
using swofty::json::Value;

static int fails;

inline void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

inline trk::BaseTrack track(const char *id, const char *code, std::vector<std::string> provides = {},
                            trk::Kind kind = trk::Kind::stock, const char *name = "")
{
    trk::BaseTrack t;
    t.id = id;
    t.code = code;
    t.provides = std::move(provides);
    t.kind = kind;
    t.time_trial = kind == trk::Kind::stock;
    t.addon = kind == trk::Kind::stock ? "game" : std::string(id).substr(0, std::string(id).find('/'));
    t.name = name;
    return t;
}

inline trk::Base base_of(std::vector<trk::BaseTrack> tracks)
{
    trk::Base b;
    b.tracks = std::move(tracks);
    for (auto &t : b.tracks)
        for (auto &v : t.provides)
            b.venues[v] = t.id;
    b.ready = true;
    return b;
}

inline trk::Base cs_base()
{
    return base_of({track("game/melbourne", "melbourne", {"venue/albert_park"}),
                    track("game/bahrain", "bahrain", {"venue/bahrain"}),
                    track("game/shanghai", "shanghai", {"venue/shanghai"}),
                    track("game/monaco", "monaco", {"venue/monaco"}),
                    track("game/melbourne_reverse", "melbourne_reverse"),
                    track("test.bahrain24/sepang", "bahrain24", {"venue/sepang"}, trk::Kind::record, "Sepang"),
                    track("mod.miami24/igora_drive", "igora_drive", {}, trk::Kind::older, "Igora Drive"),
                    track("mod.okayama/okayama", "okayama", {"venue/okayama"}, trk::Kind::older, "Okayama")});
}

static const char *FACTS = R"({
  "version": 1, "profile": "Classic Seasons", "inputs": "3f9a0c2e71d4b8a5", "grid_size": 22,
  "layers": [
    {"label": "f1_26", "setup_modes": ["f1_26"], "seasons": ["season_2026"], "weekend_formats": ["default"], "world": "f1_26"},
    {"label": "swofty_f1", "setup_modes": ["swofty_f1"], "seasons": ["season_2023", "season_2024", "season_2025"], "weekend_formats": ["default"], "world": "f1_26"},
    {"label": "swofty_f1_old", "setup_modes": ["swofty_f1"], "seasons": ["season_2016"], "weekend_formats": ["default"], "world": "f1"}
  ],
  "tracks": {
    "melbourne": {"worlds": {"f1_26": {"grid": 22, "garages": 11, "pit_boxes": 11}, "f1": {"grid": 22, "garages": 11, "pit_boxes": 11}},
                  "ai": ["f1_2025/f1_2025_car", "f1_1990/f1_1990_car"], "ai_difficulties": ["f1_2025", "f1_1990"]},
    "bahrain": {"worlds": {"f1_26": {"grid": 22, "garages": 11, "pit_boxes": 11}, "f1": {"grid": 22, "garages": 11, "pit_boxes": 11}},
                "ai": ["f1_2025/f1_2025_car"], "ai_difficulties": ["f1_2025"]},
    "monaco": {"worlds": {"f1_26": {"grid": 20, "garages": 11, "pit_boxes": 11}, "f1": {"grid": 20, "garages": 11, "pit_boxes": 11}},
               "ai": ["f1_2025/f1_2025_car"], "ai_difficulties": ["f1_2025"]},
    "bahrain24": {"worlds": {"f1_26": {"grid": 22, "garages": 11, "pit_boxes": 11}, "f1": {"grid": 22, "garages": 11, "pit_boxes": 11}},
                  "ai": ["f1_2025/f1_2025_car"], "ai_difficulties": ["f1_2025"]},
    "igora_drive": {"worlds": {"f1_26": {"grid": 22, "garages": 10, "pit_boxes": 10}, "f1": {"grid": 22, "garages": 10, "pit_boxes": 10}},
                    "ai": ["f1_2025/f1_2025_car"], "ai_difficulties": ["f1_2025"]},
    "okayama": {"worlds": {"f1_26": {"grid": 24, "garages": 12, "pit_boxes": 12}, "f1": {"grid": 24, "garages": 12, "pit_boxes": 12}},
                "ai": ["f1_2025/f1_2025_car"], "ai_difficulties": ["f1_2025"]}
  }
})";

inline trk::BoxInfo cs2016(int teams = 11)
{
    trk::BoxInfo b;
    b.package = "swofty.classic-seasons-2016/f1_2016";
    b.entrant_list = "cs_2016";
    b.title = "F1 2016";
    b.season = "season_2016";
    b.teams = teams;
    b.cars = teams * 2;
    for (int i = 0; i < teams; ++i)
        b.ai.push_back({"team_" + std::to_string(i), "f1_2025", "f1_2025_car"});
    return b;
}
