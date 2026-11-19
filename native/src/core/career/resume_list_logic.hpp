#pragma once

#include <algorithm>
#include <cstdint>
#include <string_view>
#include <string>
#include <vector>

#include "core/career/career_saves_logic.hpp"
#include "core/career/resume_gate_logic.hpp"

namespace career::resume_list {

struct Row {
    std::string key, name, mode, detail, status;
    uint64_t id = 0, guid = 0, date = 0;
    bool my_team = false, ready = true;
};

inline const char *mode_label(bool my_team) { return my_team ? "My Team" : "Driver Career"; }

inline std::string detail_line(const career::saves::SlotRead &s, const resume_gate::Entry *e = nullptr)
{
    std::string out = e && !e->mode_label.empty() ? e->mode_label : mode_label(!!(s.flags & career::saves::FLAG_MY_TEAM));
    if (s.season != career::saves::NO_INDEX)
        out += " - Season " + std::to_string(s.season + 1);
    if (s.weekend != career::saves::NO_INDEX)
        out += ", Race " + std::to_string(s.weekend + 1);
    return out;
}

inline std::string pill(const resume_gate::Entry *e)
{
    if (!e)
        return {};
    std::vector<std::string> out;
    for (const auto &n : e->needs) {
        if (!n.on)
            out.push_back("Missing: " + n.name);
        else if (!n.at_version)
            out.push_back("Made with another version of " + n.name);
    }
    if (out.empty())
        return "Ready";
    std::string text;
    size_t shown = out.size() < 2 ? out.size() : 2;
    for (size_t i = 0; i < shown; i++)
        text += (i ? "; " : "") + out[i];
    if (out.size() > shown)
        text += " and " + std::to_string(out.size() - shown) + " more";
    return text;
}

inline std::string status_of(const resume_gate::Entry *e) { return pill(e); }

inline std::vector<Row> rows(const std::vector<career::saves::SlotRead> &slots, const resume_gate::Table &t)
{
    std::vector<Row> out;
    for (const auto &s : slots) {
        if (!career::saves::career_slot(s))
            continue;
        Row r;
        r.key = career::saves::slot_key(s.id, s.guid);
        r.id = s.id;
        r.guid = s.guid;
        r.date = s.date;
        r.my_team = !!(s.flags & career::saves::FLAG_MY_TEAM);
        const resume_gate::Entry *e = resume_gate::find(t, r.key);
        r.mode = e && !e->mode_label.empty() ? e->mode_label : mode_label(r.my_team);
        r.name = s.name.empty() ? r.mode : s.name;
        r.detail = detail_line(s, e);
        r.ready = !e || resume_gate::unmet(*e).empty();
        r.status = status_of(e);
        out.push_back(std::move(r));
    }
    std::stable_sort(out.begin(), out.end(), [](const Row &a, const Row &b) {
        return a.date != b.date ? a.date > b.date : a.id < b.id;
    });
    return out;
}

inline const char *continue_route(bool my_team) { return my_team ? "solo_career_continue_my_team" : "solo_career_continue_driver"; }

inline const char *list_route(bool my_team) { return my_team ? "career_dlc_select_my_team" : "career_dlc_select_driver"; }

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

inline bool redirects(bool armed, bool target_set, size_t rows, std::string_view route, bool &my_team)
{
    return armed && !target_set && rows > 0 && continue_route_of(route, my_team);
}

inline std::string description(const Row &r) { return r.status.empty() ? r.detail : r.detail + " - " + r.status; }

struct Selection {
    uint64_t id = 0, guid = 0;
};

inline Selection selection_of(const Row &r) { return {r.id, r.guid}; }

enum class Action { Load, Ask };

inline Action press(const Row &r) { return r.ready ? Action::Load : Action::Ask; }

}
