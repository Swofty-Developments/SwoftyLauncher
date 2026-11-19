#pragma once

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <string_view>
#include <vector>

#include "core/career/career_saves_logic.hpp"
#include "core/runtime/json.hpp"

// The Resume Career list screen's logic (the launcher builds the screen at Play, see resume_list.cpp). The launcher writes
// `resume_list` into settings.json: one entry per row of the screen, in the screen's order, each with its index, its mode
// (driver or my_team), the save slot key (the slot guid as 16 hex digits, else the slot id), whether it is ready and the
// status text the row shows. A press raises the command `swofty_resume:<index>`.

namespace career::resume_list {

constexpr const char *SETTINGS_KEY = "resume_list";
constexpr const char *COMMAND_PREFIX = "swofty_resume:";
constexpr const char *LIST_ROUTE = "swofty_resume_list";
constexpr uint64_t TARGET_LIFE_MS = 15000;

struct Entry {
    size_t index = 0;
    std::string key, status;
    bool my_team = false, ready = true;
};

struct Selection {
    uint64_t id = 0, guid = 0;
};

inline std::vector<Entry> parse(std::string_view text)
{
    std::vector<Entry> out;
    json::Value doc;
    if (text.empty() || !json::parse(std::string(text), doc) || doc.type != json::Value::Obj)
        return out;
    const json::Value &list = doc[SETTINGS_KEY];
    if (list.type != json::Value::Arr)
        return out;
    for (const json::Value &e : list.a) {
        if (e["key"].type != json::Value::Str || e["key"].s.empty() || e["index"].type != json::Value::Num)
            continue;
        Entry r;
        r.index = size_t(e["index"].n);
        r.key = e["key"].s;
        r.my_team = e["mode"].str() == std::string("my_team");
        r.ready = e["ready"].type == json::Value::Bool && e["ready"].b;
        r.status = e["status"].str();
        out.push_back(std::move(r));
    }
    return out;
}

inline const Entry *find(const std::vector<Entry> &entries, size_t index)
{
    for (const Entry &e : entries)
        if (e.index == index)
            return &e;
    return nullptr;
}

inline bool command_index(std::string_view name, size_t &index)
{
    std::string_view p = COMMAND_PREFIX;
    if (name.size() <= p.size() || name.substr(0, p.size()) != p)
        return false;
    std::string digits(name.substr(p.size()));
    for (char c : digits)
        if (c < '0' || c > '9')
            return false;
    index = size_t(strtoull(digits.c_str(), nullptr, 10));
    return true;
}

inline bool select(const std::vector<career::saves::SlotRead> &slots, const Entry &e, Selection &sel)
{
    for (const auto &s : slots)
        if (career::saves::career_slot(s) && career::saves::slot_key(s.id, s.guid) == e.key &&
            !!(s.flags & career::saves::FLAG_MY_TEAM) == e.my_team) {
            sel = {s.id, s.guid};
            return true;
        }
    return false;
}

inline const char *continue_route(bool my_team)
{
    return my_team ? "solo_career_continue_my_team" : "solo_career_continue_driver";
}

inline bool continue_route_of(std::string_view route, bool &my_team)
{
    if (route == "solo_career_continue_driver")
        my_team = false;
    else if (route == "solo_career_continue_my_team")
        my_team = true;
    else
        return false;
    return true;
}

inline bool redirects(bool armed, size_t entries, std::string_view route, bool &my_team)
{
    return armed && entries > 0 && continue_route_of(route, my_team);
}

enum class Action { Load, Ask };

inline Action press(const Entry &e) { return e.ready ? Action::Load : Action::Ask; }

inline std::string dialog_text(const Entry &e)
{
    return "This career needs something this profile does not have: " + e.status + ". Loading it anyway can break it.";
}

inline bool expired(uint64_t set_at, uint64_t now) { return now - set_at > TARGET_LIFE_MS; }

}
