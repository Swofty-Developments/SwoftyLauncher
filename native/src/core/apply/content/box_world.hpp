#pragma once

#include <algorithm>
#include <cstdlib>
#include <functional>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

// The world of a box with more teams than the f1 world has garages (SWOFTY 2010 to 2014 and 2016 race 11 or 12,
// and the Time Trial lists of 2010 to 2012 hold 12 teams with no season of their own).
// Each session's settings build loads one extra world for every track_data.track_layers row whose setup modes,
// seasons and weekend formats hold the session's setup mode, its entrant list's season (when the list sets one) and
// its weekend format (Game_GameMode_AddTrackLayerWorlds 0x1407a1310). A box's list copies f1_2025's season, so its
// swofty_f1 sessions load the stock f1 layer's world, which has 10 garages at every stock track. choose() decides
// whether a box needs its own world: a season of its own that only its list carries, and a layer row for its setup
// mode and that season, copied from f1_26 (11 garages) for 11 teams or myteam_26 (12) for 12. The season is
// appended to every seasons list that holds the list's season (track data sets, drivers' seasonal infos, branding,
// tyre compound classes), except the track layer rows, so nothing else about the box's sessions changes. A box whose
// list already loads a world with room for its teams (Classic Seasons' swofty_f1 layer gives 11) is left as it is.
// Pure: it reads the database through read, and the lowering (lower.hpp, lower_package) turns the choice into rows.

namespace swofty_core::apply::content::box_world {

using Read = std::function<std::optional<std::string>(const std::string &path)>;

inline constexpr const char *LAYERS = "track_data.track_layers";
inline constexpr const char *LAYER_ROW = "TrackLayer";
inline constexpr const char *SEASONS = "game_setup.seasons";
inline constexpr const char *SEASON_ROW = "season";
inline constexpr const char *SEASON_IDS = "content.package.season";
inline constexpr const char *LAYER_IDS = "content.package.layer";
// The game keeps a season id in a bit of a u32 mask (0x142a0ddb0) and a layer's in a u8 its world's name takes.
inline constexpr int64_t SEASON_ID_MAX = 31;
inline constexpr int64_t LAYER_ID_MAX = 255;
// The setup mode whose boxes get a world of their own; an addon's own setup mode loads its own layers.
inline constexpr const char *SETUP_MODE = "swofty_f1";
// The season a list with no season of its own copies.
inline constexpr const char *DEFAULT_SEASON = "season_2025";
// The game environment variable that turns box worlds off (SWOFTY_BOX_WORLDS=0); unset or any other value leaves
// them on.
inline constexpr const char *ENV = "SWOFTY_BOX_WORLDS";

inline bool enabled(std::string_view env) { return env != "0"; }

struct World {
    const char *layer, *world;
    int garages;
};

// The stock layer rows and the garages their world holds at every stock track (track-facts.json of the stock game).
inline constexpr World STOCK[] = {
    {"f1", "f1", 10},
    {"f1_26", "f1_26", 11},
    {"myteam_26", "f1_26_myteam", 12},
    {"classic", "classic", 11},
    {"myteam", "f1_myteam", 11},
    {"f1_story", "f1_story", 11},
    {"f1_previous_story", "f1_previous_story", 11},
    {"myteam_previous", "f1_previous_myteam", 11},
    {"f2", "f2", 11},
    {"f2_26", "f2_26", 12},
};

// The seasons lists that hold a season the box's sessions read, from the gamedb catalogue: every table with a
// seasons field but the track layers and the driver statistics data sets.
inline constexpr const char *SEASON_LISTS[] = {
    "entrants.drivers.seasonal_infos.seasonal_info_N",
    "entrants.entrant_specialisation.specialisation_sets.specialisation_set_N",
    "game_setup.branding.game_setup_branding_N",
    "game_setup.branding.game_setup_branding_N.branding_videos_N",
    "practice_programmes.practice_programme_folder_N",
    "schedule.dynamic_event_data.default_ui_track_list_data.custom_dynamic_available_track_list_data_N",
    "schedule.dynamic_event_data.default_ui_track_list_data.track_selections.tracks_selection_N",
    "track_data.game_setup.TrackDataSet_N",
    "track_data.restricted_tracks.restricted_tracks_N",
    "tyres.compound_classes.CompoundClass_N",
};

inline std::vector<std::string> items(std::string_view v)
{
    std::vector<std::string> out;
    std::size_t at = 0;
    while (at <= v.size()) {
        std::size_t end = v.find(',', at);
        if (end == std::string_view::npos)
            end = v.size();
        std::string_view s = v.substr(at, end - at);
        while (!s.empty() && s.front() == ' ')
            s.remove_prefix(1);
        while (!s.empty() && s.back() == ' ')
            s.remove_suffix(1);
        if (!s.empty())
            out.emplace_back(s);
        at = end + 1;
    }
    return out;
}

inline bool holds(std::string_view list, std::string_view v)
{
    auto all = items(list);
    return std::find(all.begin(), all.end(), v) != all.end();
}

inline std::optional<std::size_t> count(const Read &read, const std::string &container, const std::string &row)
{
    auto n = read(container + "._" + row + "_instancecount_");
    if (!n || n->empty())
        return std::nullopt;
    char *end = nullptr;
    unsigned long long v = std::strtoull(n->c_str(), &end, 10);
    if (!end || *end)
        return std::nullopt;
    return static_cast<std::size_t>(v);
}

// The rows of a catalogue table pattern (each row word ends in _N), as the database holds them.
inline void expand_rows(const Read &read, const std::string &prefix, std::string_view rest, std::vector<std::string> &out)
{
    std::size_t n = rest.find("_N");
    if (n == std::string_view::npos) {
        out.push_back(prefix + std::string(rest));
        return;
    }
    std::size_t dot = rest.rfind('.', n);
    if (dot == std::string_view::npos)
        return;
    std::string container = prefix + std::string(rest.substr(0, dot));
    std::string word(rest.substr(dot + 1, n - dot - 1));
    auto rows = count(read, container, word);
    if (!rows)
        return;
    for (std::size_t i = 0; i < *rows; i++) {
        std::string row = container + "." + word + "_" + std::to_string(i);
        if (n + 2 == rest.size())
            out.push_back(row);
        else
            expand_rows(read, row, rest.substr(n + 2), out);
    }
}

struct Layer {
    std::string label, modes, seasons, eri;
};

inline std::vector<Layer> layers(const Read &read)
{
    std::vector<Layer> out;
    auto n = count(read, LAYERS, LAYER_ROW);
    if (!n)
        return out;
    for (std::size_t i = 0; i < *n; i++) {
        std::string row = std::string(LAYERS) + "." + LAYER_ROW + "_" + std::to_string(i);
        auto field = [&](const char *f) { return read(row + "." + f).value_or(std::string()); };
        out.push_back({field("_metadata_"), field("game_setup_modes"), field("seasons"), field("worldEri")});
    }
    return out;
}

// The garages of a layer row's world: a stock row's, or a copy's whose worldEri is a stock row's; -1 when unknown.
inline int garages_of(const Layer &l, const std::vector<Layer> &all)
{
    for (auto &w : STOCK) {
        if (l.label == w.layer)
            return w.garages;
        for (auto &s : all)
            if (s.label == w.layer && !s.eri.empty() && s.eri == l.eri)
                return w.garages;
    }
    return -1;
}

inline const World *source_for(std::size_t teams)
{
    if (teams == 11)
        return &STOCK[1];
    if (teams == 12)
        return &STOCK[2];
    return nullptr;
}

inline std::string season_label(const std::string &list) { return "swofty_box_" + list; }

inline std::string layer_label(const std::string &list) { return "swofty_box_" + list + "_world"; }

struct Choice {
    bool apply = false;
    std::string season, from_season, layer;
    const World *from = nullptr;
    int now = -1;
    std::vector<std::string> appends;
    std::string line;
};

// season: the box's list's season (empty when it sets none).
inline Choice choose(const std::string &list, const std::string &mode, std::size_t teams, const std::string &season,
                     const Read &read)
{
    Choice c;
    if (!read || teams <= std::size_t(STOCK[0].garages))
        return c;
    std::vector<Layer> all = layers(read);
    if (all.empty()) {
        c.line = "no " + std::string(LAYERS) + " rows to read; its " + std::to_string(teams) + " teams keep the world as it is";
        return c;
    }
    int room = -1;
    bool unknown = false;
    for (auto &l : all)
        if (holds(l.modes, mode) && (season.empty() || holds(l.seasons, season))) {
            int g = garages_of(l, all);
            unknown = unknown || g < 0;
            room = std::max(room, g);
        }
    if (room >= int(teams))
        return c;
    if (unknown) {
        c.line = "its " + mode + " layers load a world the core has no garage count for; its " + std::to_string(teams) +
                 " teams keep the world as it is";
        return c;
    }
    const World *src = source_for(teams);
    if (!src) {
        c.line = std::to_string(teams) + " teams: no world holds more than 12 garages; the box keeps the world as it is";
        return c;
    }
    bool has_src = std::any_of(all.begin(), all.end(), [&](const Layer &l) { return l.label == src->layer; });
    std::string from = season.empty() ? DEFAULT_SEASON : season;
    if (!has_src || !read(std::string(SEASONS) + "." + SEASON_ROW + "[_metadata_=" + from + "]._metadata_")) {
        c.line = "no " + std::string(has_src ? from : src->layer) + " row to copy; its " + std::to_string(teams) +
                 " teams keep the world as it is";
        return c;
    }
    c.apply = true;
    c.season = season_label(list);
    c.from_season = from;
    c.layer = layer_label(list);
    c.from = src;
    c.now = room;
    for (const char *pattern : SEASON_LISTS) {
        std::vector<std::string> rows;
        expand_rows(read, "", pattern, rows);
        for (auto &r : rows)
            if (auto v = read(r + ".seasons"); v && holds(*v, from) && !holds(*v, c.season))
                c.appends.push_back(r);
    }
    return c;
}

// A box that names its world (content.package 1.7 "world", on with SWOFTY_BOX_OWN_WORLDS=1): its sessions load
// [FILENAME]_<name> at every track, the archive environment_package/tracks/<code>/wep/<code>_<name>.erp an addon
// ships, whatever its team count. The box gets a season and a layer row like a garage world, but the layer row's
// worldAssetGroup and worldEri name the box's world, and the season, the layer (and the branding row, when the record
// has one) are shared by every box that names the same season: a season id is a bit of a u32 mask (0 to 31, and
// with SWOFTY seasons installed about seven are free), so an era's boxes share one season, one world and one branding.
// A layer row has no track criterion (Trk_TrackData_CollectLayersForQuery matches the setup mode, season and weekend
// format masks only), so the box's sessions load its world at every track: at boot the core lists the track folders
// that hold the archive, a box whose world no track holds keeps the world its list's layers load, and the box's track
// list leaves out the tracks that lack it (tracks/fits.hpp, the world rule), so no session asks for a world that is
// not there.
inline constexpr const char *OWN_ENV = "SWOFTY_BOX_OWN_WORLDS";

inline bool own_enabled(std::string_view env) { return env == "1"; }

inline constexpr const char *BRANDING = "game_setup.branding";
inline constexpr const char *BRANDING_ROW = "game_setup_branding";
inline constexpr const char *BRANDING_LIST = "game_setup.branding.game_setup_branding_N";
inline constexpr const char *BRANDING_IDS = "content.package.branding";
// Stock branding rows hold stat ids 0 to 12; what keeps the id is not read (HYPOTHESIS: a byte, like the scenarios').
inline constexpr int64_t BRANDING_ID_MAX = 254;
// The stock rows a box's own layer and branding copy when its record names none.
inline constexpr const char *OWN_FROM_LAYER = "f1_26";
inline constexpr const char *OWN_FROM_LAYER_12 = "myteam_26";
inline constexpr const char *OWN_FROM_BRANDING = "f1_26";
inline constexpr const char *TRACKS_FOLDER = "2025_asset_groups/environment_package/tracks/";
inline constexpr std::size_t WORLD_NAME_MAX = 24;

inline bool name_ok(std::string_view s)
{
    if (s.empty() || s.size() > WORLD_NAME_MAX)
        return false;
    for (char c : s)
        if (!((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_'))
            return false;
    return true;
}

inline std::string world_group(const std::string &name)
{
    return "asset_groups/environment_package/tracks/[FILENAME]/wep/[FILENAME]_" + name;
}

inline std::string world_eri(const std::string &name)
{
    return "eaid://environment_package/tracks/[FILENAME]/wep/[FILENAME]_" + name + ".wep";
}

// The archive a track needs for the world, relative to the game folder.
inline std::string world_file(const std::string &code, const std::string &name)
{
    return std::string(TRACKS_FOLDER) + code + "/wep/" + code + "_" + name + ".erp";
}

inline std::string share_season(const std::string &share) { return "swofty_world_" + share; }

inline std::string share_layer(const std::string &share) { return "swofty_world_" + share + "_world"; }

inline std::string share_branding(const std::string &share) { return "swofty_world_" + share + "_branding"; }

// The track codes whose folder holds <code>_<name>.erp; nullopt when the caller cannot look (host tools).
using WorldTracks = std::function<std::optional<std::vector<std::string>>(const std::string &name)>;

struct Own {
    std::string name, share, from_layer, from_branding;
    bool branding = false;
};

struct Named {
    bool apply = false;
    std::string name, share, season, from_season, layer, from_layer, branding, from_branding;
    // false: another box that names the same season made the row first
    bool new_season = false, new_layer = false, new_branding = false;
    std::vector<std::string> appends, tracks;
    bool checked = false;
    std::string line;
};

inline bool has_row(const Read &read, const char *table, const char *row, const std::string &label)
{
    return read && read(std::string(table) + "." + row + "[_metadata_=" + label + "]._metadata_").has_value();
}

// season: the box's list's season (empty when it sets none).
inline Named choose_named(std::size_t teams, const std::string &season, const Own &own, const Read &read,
                          const WorldTracks &tracks)
{
    Named c;
    c.name = own.name;
    c.share = own.share.empty() ? own.name : own.share;
    const std::string keeps = "; the box keeps the world its list's layers load";
    if (!name_ok(c.name) || !name_ok(c.share)) {
        c.line = "world " + c.name + " (season " + c.share + "): a name is not 1 to 24 of a-z, 0-9 and _" + keeps;
        return c;
    }
    if (!read)
        return c;
    if (auto t = tracks ? tracks(c.name) : std::nullopt) {
        c.checked = true;
        c.tracks = *t;
        std::sort(c.tracks.begin(), c.tracks.end());
        if (c.tracks.empty()) {
            c.line = "world " + c.name + ": no track folder holds " + world_file("<code>", c.name) + keeps;
            return c;
        }
    }
    c.from_layer = !own.from_layer.empty() ? own.from_layer : teams >= 12 ? OWN_FROM_LAYER_12 : OWN_FROM_LAYER;
    c.season = share_season(c.share);
    c.layer = share_layer(c.share);
    c.from_season = season.empty() ? DEFAULT_SEASON : season;
    c.new_season = !has_row(read, SEASONS, SEASON_ROW, c.season);
    c.new_layer = !has_row(read, LAYERS, LAYER_ROW, c.layer);
    if (c.new_layer && !has_row(read, LAYERS, LAYER_ROW, c.from_layer)) {
        c.line = "world " + c.name + ": no " + c.from_layer + " track layer row to copy" + keeps;
        return c;
    }
    if (c.new_season && !has_row(read, SEASONS, SEASON_ROW, c.from_season)) {
        c.line = "world " + c.name + ": no " + c.from_season + " season row to copy" + keeps;
        return c;
    }
    if (own.branding) {
        c.branding = share_branding(c.share);
        c.from_branding = own.from_branding.empty() ? OWN_FROM_BRANDING : own.from_branding;
        c.new_branding = !has_row(read, BRANDING, BRANDING_ROW, c.branding);
        if (c.new_branding && !has_row(read, BRANDING, BRANDING_ROW, c.from_branding)) {
            c.line = "world " + c.name + ": no " + c.from_branding + " branding row to copy; the box keeps the branding of " +
                     c.from_season;
            c.branding.clear();
            c.new_branding = false;
        }
    }
    c.apply = true;
    if (!c.new_season)
        return c;
    for (const char *pattern : SEASON_LISTS) {
        // A box with a branding row of its own keeps its season off the stock branding rows, so one row matches.
        if (!c.branding.empty() && std::string_view(pattern) == BRANDING_LIST)
            continue;
        std::vector<std::string> rows;
        expand_rows(read, "", pattern, rows);
        for (auto &r : rows)
            if (auto v = read(r + ".seasons"); v && holds(*v, c.from_season) && !holds(*v, c.season))
                c.appends.push_back(r);
    }
    return c;
}

}
