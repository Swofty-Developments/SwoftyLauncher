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
constexpr const char *ID_DIVIDER = "swofty_divider";
constexpr const char *STOCK_ROUTE_F1_WORLD = "f1_world";
constexpr const char *TEMPLATE_DEFAULT = "fe_tile_button_default";
constexpr const char *TEMPLATE_DIVIDER = "swofty_home_divider";

constexpr const char *TT_TITLE = "Not signed in";
constexpr const char *TT_BODY = "Sign in with Discord in Swofty Launcher to save your laps to the Time Trial boards. "
                                "Laps from this session will not be saved. Press OK to continue to Time Trial.";

constexpr const char *TEMPLATE_TOP_LEFT = "swofty_home_top_left";
constexpr const char *TEMPLATE_TOP_RIGHT = "swofty_home_top_right";
constexpr const char *TEMPLATE_TOP_THIRD = "swofty_home_top_third";
constexpr const char *TEMPLATE_MODES_LEFT = "swofty_home_modes_left";
constexpr const char *TEMPLATE_MODES_RIGHT = "swofty_home_modes_right";
constexpr const char *TEMPLATE_WIDE_A = "swofty_home_wide_a";
constexpr const char *TEMPLATE_WIDE_B = "swofty_home_wide_b";
constexpr const char *TEMPLATE_WIDE_C = "swofty_home_wide_c";

inline const char *template_for(const std::string &id, bool rows = true)
{
    if (!rows)
        return id == ID_THEATRE || id == ID_SHOWROOM ? TEMPLATE_DEFAULT : "";
    if (id == ID_DIVIDER)
        return TEMPLATE_DIVIDER;
    if (id == ID_STORY)
        return TEMPLATE_TOP_LEFT;
    if (id == ID_CAREER)
        return TEMPLATE_TOP_RIGHT;
    if (id == ID_CHALLENGE)
        return TEMPLATE_TOP_THIRD;
    if (id == ID_GRAND_PRIX)
        return TEMPLATE_MODES_LEFT;
    if (id == ID_TIME_TRIAL)
        return TEMPLATE_MODES_RIGHT;
    if (id == ID_THEATRE)
        return TEMPLATE_WIDE_A;
    if (id == ID_SHOWROOM)
        return TEMPLATE_WIDE_B;
    if (id == ID_SETTINGS)
        return TEMPLATE_WIDE_C;
    return "";
}

constexpr uint16_t WATCH_PAD_MASK = 0x0200;
constexpr const char *SHOWROOM_ROUTE = "swofty_showroom";
constexpr const char *HOME_BACK_ROUTE = "swofty_home";

constexpr uint32_t WATCH_KEY_VK = 0x56;
constexpr double PANEL_BUDGET_MS = 4.0;
constexpr double ACCOUNT_AT_X = 3.06, ACCOUNT_AT_Y = 3.81;
constexpr double VIDEO_AT_X = 2.29, VIDEO_AT_Y = -4.95;
constexpr double VIDEO_IMAGE_DX = 3.3, VIDEO_IMAGE_DY = 1.06, VIDEO_IMAGE_W = 6.6, VIDEO_IMAGE_H = 1.8;
constexpr double SCENE_UNITS_TALL = 11.0, SCENE_CENTRE_X_SHIFT = -0.0148, SCENE_CENTRE_Y = 0.4918;

struct Point {
    float x = 0, y = 0;
};

inline float scene_scale(float height) { return height / float(SCENE_UNITS_TALL); }

inline Point scene_to_px(double x, double y, float width, float height)
{
    float s = scene_scale(height);
    return {width * 0.5f + float(SCENE_CENTRE_X_SHIFT) * height + s * float(x), float(SCENE_CENTRE_Y) * height - s * float(y)};
}

constexpr uint32_t djb2(const char *s)
{
    uint32_t h = 5381;
    for (; *s; s++)
        h = h * 33 + uint8_t(*s);
    return h;
}

inline std::string button_name(bool pad) { return pad ? "RB" : "V"; }

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
    if (has(ID_SETTINGS)) {
        plan.cards.push_back({ID_DIVIDER, row++, 0, true});
        plan.cards.push_back({ID_SETTINGS, row++, 0, true});
    }
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
    long long online = 0;
};

inline std::string with_separators(long long n)
{
    std::string digits = std::to_string(n < 0 ? -n : n), out;
    for (size_t i = 0; i < digits.size(); i++) {
        if (i && (digits.size() - i) % 3 == 0)
            out += ',';
        out += digits[i];
    }
    return (n < 0 ? "-" : "") + out;
}

constexpr const char *OFFLINE_TEXT = "Launched with Swofty Launcher";

inline std::string online_line(long long n)
{
    return n <= 0 ? "" : with_separators(n) + " currently playing";
}

inline std::string offline_text(long long n)
{
    std::string line = online_line(n);
    return line.empty() ? std::string(OFFLINE_TEXT) : std::string(OFFLINE_TEXT) + "\n" + line;
}

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
    s.online = doc["online"].num(0);
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

inline bool panel_due(bool patched, bool home_open, bool loaded, bool disabled, bool prepared)
{
    return patched && home_open && loaded && prepared && !disabled;
}

inline bool panel_too_slow(double draw_ms) { return draw_ms > PANEL_BUDGET_MS; }

inline bool time_trial_warning_due(const State &s, bool is_time_trial, bool already_shown)
{
    return s.loaded && is_time_trial && !s.signed_in && !already_shown;
}

}
