#pragma once

#include <algorithm>
#include <cstdint>
#include <string>
#include <vector>

#include "core/career/career_saves_logic.hpp"
#include "core/career/resume_gate_logic.hpp"

namespace career::resume_list {

struct Row {
    std::string key, name, mode, detail, status;
    uint64_t id = 0, date = 0;
    bool my_team = false, ready = true;
};

inline const char *mode_label(bool my_team) { return my_team ? "My Team" : "Driver Career"; }

inline std::string detail_line(const career::saves::SlotRead &s)
{
    std::string out = mode_label(!!(s.flags & career::saves::FLAG_MY_TEAM));
    if (s.season != career::saves::NO_INDEX)
        out += " - Season " + std::to_string(s.season + 1);
    if (s.weekend != career::saves::NO_INDEX)
        out += ", Race " + std::to_string(s.weekend + 1);
    return out;
}

inline std::string status_of(const resume_gate::Entry *e) { return resume_gate::status_line(e); }

inline std::vector<Row> rows(const std::vector<career::saves::SlotRead> &slots, const resume_gate::Table &t)
{
    std::vector<Row> out;
    for (const auto &s : slots) {
        if (!career::saves::career_slot(s))
            continue;
        Row r;
        r.key = career::saves::slot_key(s.id, s.guid);
        r.id = s.id;
        r.date = s.date;
        r.my_team = !!(s.flags & career::saves::FLAG_MY_TEAM);
        r.mode = mode_label(r.my_team);
        r.name = s.name.empty() ? r.mode : s.name;
        r.detail = detail_line(s);
        const resume_gate::Entry *e = resume_gate::find(t, r.key);
        r.ready = !e || resume_gate::unmet(*e).empty();
        r.status = status_of(e);
        out.push_back(std::move(r));
    }
    std::stable_sort(out.begin(), out.end(), [](const Row &a, const Row &b) {
        return a.date != b.date ? a.date > b.date : a.id < b.id;
    });
    return out;
}

enum class Action { Load, Ask };

inline Action press(const Row &r) { return r.ready ? Action::Load : Action::Ask; }

}
