#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include "json.hpp"

namespace runtime::main_menu {

constexpr const char *ID_STORY = "story";
constexpr const char *ID_CAREER = "career";
constexpr const char *ID_MOVIE = "zenith_button";
constexpr const char *ID_F1_WORLD = "f1_world";
constexpr const char *ID_CHALLENGE = "home_navigate_to_challenge_event";
constexpr const char *ID_LOCAL = "local_multiplayer";
constexpr const char *ID_LEAGUE = "pro_leagues";
constexpr const char *ID_HUB = "player_hub";
constexpr const char *ID_THEATRE = "theatre";
constexpr const char *ID_SETTINGS = "mod_menu";
constexpr const char *ID_GRAND_PRIX = "swofty_grand_prix";
constexpr const char *ID_TIME_TRIAL = "swofty_time_trial";
constexpr const char *ID_SHOWROOM = "swofty_showroom";

constexpr const char *TT_TITLE = "Not signed in";
constexpr const char *TT_BODY = "Sign in with Discord in Swofty Launcher to save your laps to the Time Trial boards. "
                                "Laps from this session will not be saved.";
constexpr const char *TT_CONTINUE = "Continue";
constexpr const char *TT_BACK = "Back";

constexpr uint32_t WATCH_KEY_VK = 0x56;

constexpr uint32_t djb2(const char *s)
{
    uint32_t h = 5381;
    for (; *s; s++)
        h = h * 33 + uint8_t(*s);
    return h;
}

inline std::string button_name(bool pad) { return pad ? "Y" : "V"; }

inline std::string open_request_json(const std::string &url)
{
    std::string out = "{\"url\":\"";
    for (char c : url) {
        if (c == '"' || c == '\\')
            out += '\\';
        if (uint8_t(c) >= 0x20)
            out += c;
    }
    return out + "\"}";
}

struct Card {
    std::string id;
    int row = 0;
    int column = 0;
    bool wide = false;
};

struct Plan {
    std::vector<Card> cards;
    std::vector<std::string> dropped;
};

inline bool removed(const std::string &id)
{
    return id == ID_F1_WORLD || id == ID_LOCAL || id == ID_LEAGUE || id == ID_HUB || id == ID_MOVIE || id == "divider";
}

inline Plan layout(const std::vector<std::string> &stock, bool showroom_available)
{
    Plan plan;
    auto has = [&](const char *id) {
        for (auto &s : stock)
            if (s == id)
                return true;
        return false;
    };
    int col = 0;
    auto top = [&](const std::string &id) { plan.cards.push_back({id, 0, col++, false}); };
    if (has(ID_STORY))
        top(ID_STORY);
    if (has(ID_CAREER))
        top(ID_CAREER);
    if (has(ID_CHALLENGE))
        top(ID_CHALLENGE);
    for (auto &s : stock) {
        if (s == ID_CAREER || s == ID_STORY || s == ID_CHALLENGE || s == ID_THEATRE || s == ID_SETTINGS)
            continue;
        if (removed(s))
            plan.dropped.push_back(s);
        else
            top(s);
    }
    if (has(ID_F1_WORLD)) {
        plan.cards.push_back({ID_GRAND_PRIX, 1, 0, false});
        plan.cards.push_back({ID_TIME_TRIAL, 1, 1, false});
    }
    int row = 2;
    if (has(ID_THEATRE))
        plan.cards.push_back({ID_THEATRE, row++, 0, true});
    if (showroom_available)
        plan.cards.push_back({ID_SHOWROOM, row++, 0, true});
    if (has(ID_SETTINGS))
        plan.cards.push_back({ID_SETTINGS, row++, 0, true});
    return plan;
}

struct State {
    bool loaded = false;
    std::string version;
    bool signed_in = false;
    std::string name;
    std::string avatar;
    std::string video_title;
    std::string video_url;
    std::string video_thumbnail;
};

inline State parse_state(const std::string &text)
{
    State s;
    json::Value doc;
    if (!json::parse(text, doc) || doc["schema"].num() != 1)
        return s;
    s.loaded = true;
    s.version = doc["launcher_version"].str();
    auto &d = doc["discord"];
    s.signed_in = d["signed_in"].type == json::Value::Bool && d["signed_in"].b;
    s.name = d["name"].str();
    s.avatar = d["avatar"].str();
    auto &v = doc["video"];
    s.video_title = v["title"].str();
    s.video_url = v["url"].str();
    s.video_thumbnail = v["thumbnail"].str();
    return s;
}

inline std::string version_line(const State &s)
{
    return s.version.empty() ? "Swofty Launcher" : "Swofty Launcher " + s.version;
}

inline std::string discord_line(const State &s)
{
    if (!s.signed_in)
        return "Not signed in to Discord";
    return s.name.empty() ? "Signed in to Discord" : "Signed in to Discord as " + s.name;
}

inline bool has_avatar(const State &s) { return s.signed_in && !s.avatar.empty(); }

inline bool has_video(const State &s) { return !s.video_url.empty() && !s.video_thumbnail.empty(); }

inline std::string watch_line(const std::string &button)
{
    return "Press " + button + " to watch";
}

inline bool time_trial_warning_due(const State &s, bool is_time_trial, bool already_shown)
{
    return s.loaded && is_time_trial && !s.signed_in && !already_shown;
}

}
