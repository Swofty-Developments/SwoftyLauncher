#pragma once

#include <memory>
#include <string>
#include <vector>

#include <swofty/json.hpp>

#include "core/apply/content/setup_layout.hpp"
#include "core/hooks/context.hpp"

// A setup.layout record (packages/sdk/std/registries/setup.layout) as the pure Layout of setup_layout.hpp plus its
// compiled within, and the choice of the layout a scope's facts select: the last one in load order whose within matches,
// a layout without within applying everywhere.

namespace swofty_core::apply::content {

struct ParsedLayout {
    Layout layout;
    std::shared_ptr<hooks::Selectors> within;
};

namespace detail {

inline const swofty::json::Value &member(const swofty::json::Value &v, const char *k)
{
    static const swofty::json::Value none;
    return v.is_object() ? v[k] : none;
}

}

inline bool parse_setup_layout(const std::string &id, const swofty::json::Value &rec, ParsedLayout &out, std::string *why)
{
    using detail::member;
    out = ParsedLayout{};
    out.layout.id = id;
    const auto &sections = member(rec, "sections");
    if (!sections.is_array()) {
        if (why)
            *why = "sections is not a list";
        return false;
    }
    for (size_t i = 0; i < sections.size(); i++) {
        const auto &s = sections[i];
        LayoutSection sec;
        const auto &tab = member(s, "tab");
        if (tab.is_string())
            sec.tab = tab_index(*tab.if_string());
        else if (tab.is_number())
            sec.tab = int(tab.as_int(-1));
        if (const auto *t = member(s, "title").if_string())
            sec.title = *t;
        const auto &rows = member(s, "rows");
        for (size_t j = 0; rows.is_array() && j < rows.size(); j++) {
            const auto &r = rows[j];
            LayoutRow row;
            const auto &slider = member(r, "slider");
            row.slider = slider.is_string() ? slider_index(*slider.if_string()) : int(slider.as_int(-1));
            if (const auto *n = member(r, "name").if_string())
                row.name = *n;
            if (const auto *n = member(r, "desc").if_string())
                row.desc = *n;
            if (const auto *n = member(r, "unit").if_string())
                row.unit = *n;
            if (member(r, "min").is_number() && member(r, "max").is_number()) {
                row.min = float(member(r, "min").as_double());
                row.max = float(member(r, "max").as_double());
            }
            if (member(r, "steps").is_number())
                row.steps = int(member(r, "steps").as_int());
            if (const auto *n = member(r, "format").if_string())
                row.format = *n;
            if (member(r, "decimals").is_number())
                row.decimals = int(member(r, "decimals").as_int());
            row.group = member(r, "group").as_bool(false);
            const auto &labels = member(r, "labels");
            for (size_t k = 0; labels.is_array() && k < labels.size(); k++)
                row.labels.push_back(labels[k].is_string() ? *labels[k].if_string() : std::string());
            if (const auto *n = member(r, "range_text").if_string())
                row.range_text = *n;
            sec.rows.push_back(std::move(row));
        }
        out.layout.sections.push_back(std::move(sec));
    }
    auto v = check_layout(out.layout);
    if (!v.ok) {
        if (why)
            *why = v.why;
        return false;
    }
    const auto &within = member(rec, "within");
    if (!within.is_null()) {
        swofty::json::Value list = within.is_array() ? within : swofty::json::Value::array();
        if (!within.is_array())
            list.push_back(within);
        auto sel = std::make_shared<hooks::Selectors>();
        std::string w;
        if (!hooks::Selectors::compile(swofty::json::dump(list), *sel, &w)) {
            if (why)
                *why = "its within does not compile (" + w + ")";
            return false;
        }
        if (!sel->empty())
            out.within = std::move(sel);
    }
    return true;
}

inline int pick_layout(const std::vector<ParsedLayout> &all, const hooks::Facts *facts)
{
    for (int i = int(all.size()) - 1; i >= 0; i--)
        if (!all[i].within || (facts && all[i].within->match(*facts)))
            return i;
    return -1;
}

}
