#pragma once

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

#include "core/runtime/json.hpp"

namespace career::resume_gate {

constexpr const char *SETTINGS_KEY = "resume_needs";
constexpr uint32_t CMD_SLOT_PRESSED = 0x8d43acfc;
constexpr uint32_t CMD_BACK_PRESSED = 0x493fe92b;
constexpr uint32_t CMD_TRANSITION_IN = 0xc22b77ef;
constexpr size_t MAX_NAMES = 4;

struct Need {
    std::string id, name, version;
    bool on = false, at_version = false;
    std::string made_with, wants;
};

struct Entry {
    std::string key, mode_label;
    std::vector<Need> needs;
};

struct Table {
    bool present = false;
    std::vector<Entry> entries;
};

inline Table parse(std::string_view text)
{
    Table t;
    json::Value doc;
    if (text.empty() || !json::parse(std::string(text), doc) || doc.type != json::Value::Obj)
        return t;
    const json::Value &list = doc[SETTINGS_KEY];
    if (list.type != json::Value::Arr)
        return t;
    t.present = true;
    for (const json::Value &e : list.a) {
        if (e["key"].type != json::Value::Str || e["key"].s.empty())
            continue;
        Entry entry;
        entry.key = e["key"].s;
        entry.mode_label = e["mode_label"].str();
        for (const json::Value &n : e["addons"].a) {
            if (n["id"].type != json::Value::Str)
                continue;
            Need need;
            need.id = n["id"].s;
            need.name = n["name"].str(need.id.c_str());
            need.version = n["version"].str();
            need.on = n["on"].type == json::Value::Bool && n["on"].b;
            need.at_version = n["at_version"].type == json::Value::Bool && n["at_version"].b;
            need.made_with = n["made_with"].str();
            need.wants = n["wants"].str();
            entry.needs.push_back(std::move(need));
        }
        t.entries.push_back(std::move(entry));
    }
    return t;
}

inline const Entry *find(const Table &t, const std::string &key)
{
    if (!t.present || key.empty())
        return nullptr;
    for (const Entry &e : t.entries)
        if (e.key == key)
            return &e;
    return nullptr;
}

inline std::string other_version_text(const Need &n)
{
    std::string installed = n.version.empty() ? std::string() : "installed " + n.version;
    std::string asked = !n.wants.empty() ? "needs " + n.wants : !n.made_with.empty() ? "saved with " + n.made_with : "";
    if (asked.empty())
        return installed.empty() ? n.name : n.name + " " + n.version;
    return n.name + " (" + asked + (installed.empty() ? "" : ", " + installed) + ")";
}

inline std::vector<std::string> missing(const Entry &e)
{
    std::vector<std::string> out;
    for (const Need &n : e.needs)
        if (!n.on)
            out.push_back(n.name);
    return out;
}

inline std::vector<std::string> other_versions(const Entry &e)
{
    std::vector<std::string> out;
    for (const Need &n : e.needs)
        if (n.on && !n.at_version)
            out.push_back(other_version_text(n));
    return out;
}

inline std::vector<std::string> unmet(const Entry &e)
{
    std::vector<std::string> out = missing(e);
    for (std::string &v : other_versions(e))
        out.push_back(std::move(v));
    return out;
}

inline std::string join(const std::vector<std::string> &names)
{
    std::string out;
    size_t shown = names.size() < MAX_NAMES ? names.size() : MAX_NAMES;
    for (size_t i = 0; i < shown; i++)
        out += (i ? ", " : "") + names[i];
    if (names.size() > shown)
        out += " and " + std::to_string(names.size() - shown) + " more";
    return out;
}

inline std::string status_line(const Entry *e)
{
    if (!e)
        return {};
    std::vector<std::string> off = missing(*e), other = other_versions(*e);
    if (off.empty() && other.empty())
        return "All addons ready";
    std::string out = off.empty() ? std::string() : "Missing: " + join(off);
    if (!other.empty())
        out += (out.empty() ? "" : "; ") + std::string("Other version: ") + join(other);
    return out;
}

inline std::string needs_text(const Entry &e)
{
    std::vector<std::string> off = missing(e), other = other_versions(e);
    std::string out;
    if (!off.empty())
        out = "This career was made with add-ons this profile does not have on: " + join(off) + ".";
    if (!other.empty())
        out += (off.empty() ? "This career was saved with other versions of add-ons this profile has on: "
                            : " It was saved with other versions of: ") +
               join(other) + ".";
    return out;
}

inline std::string prompt_text(const Entry &e)
{
    return needs_text(e) + " Loading it can break it. Press the career again to load it anyway, or press Back.";
}

constexpr const char *ANSWER_LOAD = "load_anyway";
constexpr const char *STOCK_TITLE_KEY = "lng_homescreen_driver_career";

inline std::string dialog_text(const Entry &e) { return needs_text(e) + " Loading it anyway can break it."; }

inline bool loads(std::string_view answer) { return answer == ANSWER_LOAD; }

struct State {
    std::string armed;
};

enum class Verdict { Load, Block };

inline Verdict press(const Entry *e, const std::string &key, State &s)
{
    if (!e || unmet(*e).empty())
        return Verdict::Load;
    if (s.armed == key) {
        s.armed.clear();
        return Verdict::Load;
    }
    s.armed = key;
    return Verdict::Block;
}

inline void screen_changed(State &s) { s.armed.clear(); }

}
