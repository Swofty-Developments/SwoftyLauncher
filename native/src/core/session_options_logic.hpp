#pragma once
#include "json.hpp"

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

// The launcher's one-session options for a game it can't start itself. The EA app starts an EA app copy, so the
// variables the launcher sets for the game never reach it; the launcher writes them to state\session_options.json in
// its data folder (runtime::state_root) instead: {"version": 1, "id": "<launch>", "expires": <unix seconds>,
// "env": {"<name>": "<value>"}}. The core reads the file once at start, deletes it, and takes the listed variables
// only before `expires`. A game that took them keeps the files it trades with the launcher in session\swofty\ of the
// data folder, which it can always write (it can't create files in swofty\ of a copy under Program Files).
namespace runtime::session_options {

constexpr const wchar_t *FILE = L"state\\session_options.json";
constexpr const wchar_t *FILES_DIR = L"session\\swofty\\";
constexpr const char *ARMING = "SWOFTY_NETPLAY";
constexpr const char *NAMES[] = {ARMING, "SWOFTY_NET_TRACE", "SWOFTY_NET_TRACE_CAP_KB", "SWOFTY_GAME_LOG_CHANNELS"};
// The launcher's own life for the file is 10 minutes; a file that claims more than this was not written by it.
constexpr int64_t MAX_LIFE_S = 30 * 60;
constexpr size_t MAX_VALUE = 1000;

struct Options {
    std::string id;
    int64_t expires = 0;
    std::vector<std::pair<std::string, std::string>> env;
};

enum class Verdict { ok, unusable, version, no_expiry, expired, too_far, bad_value, not_armed };

inline bool plain(const std::string &v)
{
    if (v.empty() || v.size() > MAX_VALUE)
        return false;
    for (unsigned char c : v)
        if (c < 0x20 || c == 0x7f)
            return false;
    return true;
}

inline Verdict parse(const std::string &text, int64_t now_unix, Options &out)
{
    out = Options{};
    json::Value doc;
    if (!json::parse(text, doc) || doc.type != json::Value::Obj)
        return Verdict::unusable;
    if (doc["version"].type != json::Value::Num || doc["version"].num() != 1)
        return Verdict::version;
    out.id = doc["id"].str();
    if (doc["expires"].type != json::Value::Num || doc["expires"].num() <= 0)
        return Verdict::no_expiry;
    out.expires = doc["expires"].num();
    if (now_unix > out.expires)
        return Verdict::expired;
    if (out.expires - now_unix > MAX_LIFE_S)
        return Verdict::too_far;
    const json::Value &env = doc["env"];
    for (const char *name : NAMES) {
        const json::Value &v = env[name];
        if (v.type == json::Value::Null)
            continue;
        if (v.type != json::Value::Str || !plain(v.s))
            return Verdict::bad_value;
        out.env.emplace_back(name, v.s);
    }
    for (auto &[name, value] : out.env)
        if (name == ARMING)
            return Verdict::ok;
    out.env.clear();
    return Verdict::not_armed;
}

inline const char *why(Verdict v)
{
    switch (v) {
    case Verdict::ok: return "usable";
    case Verdict::unusable: return "not a JSON object";
    case Verdict::version: return "not version 1";
    case Verdict::no_expiry: return "without an expiry time";
    case Verdict::expired: return "out of date";
    case Verdict::too_far: return "dated too far ahead";
    case Verdict::bad_value: return "carrying a value that is not plain text";
    case Verdict::not_armed: return "without SWOFTY_NETPLAY";
    }
    return "?";
}

// Whether swofty\direct_start.json asks for a room, which a game that is not armed can't open.
inline bool room_start(const std::string &direct_start)
{
    json::Value doc;
    return !direct_start.empty() && json::parse(direct_start, doc) && doc["target"].str() == "room";
}

// Whether a variable already in the game's environment wins over the file's: a launcher that started the game itself
// set it, and the file is not for this game.
inline bool from_file(bool arming_in_environment)
{
    return !arming_in_environment;
}

}
