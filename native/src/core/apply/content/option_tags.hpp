#pragma once

#include "core/apply/content/rows.hpp"

#include <swofty/json.hpp>

#include <algorithm>
#include <functional>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <vector>

// ui.option: where a game option shows. An option's restriction tags are database rows (proven on the stock database
// and by the F1 1990 addon's database ops): options_system.options.<section>.options_folder_F.option_O holds
// `additional_tags_N` rows whose `tags` key is a list "<kind>,<tag>,<tag>..." (kind = a folder of
// options_system.context, e.g. game_setup_mode, game_state, multiplayer_context; the tags are that folder's values,
// e.g. gamesetup_f1). The option shows where the running context's tag of that kind is in the list. The core seeds one
// stock record per option ("game/<option>": id and the flat tags of every row), layers change the list, and the boot
// applier turns the difference into `set` operations on the rows' `tags` keys. A new tag value of a kind comes from the
// addon's own database rows (a game_setup_mode_tag row of the context folder). A tag whose kind the option has no row
// for is refused: the row engine adds no option rows. Pure, for the host tests.

namespace swofty_core::apply::content::option_tags {

using Value = swofty::json::Value;
using Read = std::function<std::optional<std::string>(const std::string &path)>;

inline constexpr const char *SECTIONS[] = {"game", "career", "ea_trax", "multiplayer"};
inline constexpr const char *CONTEXT = "options_system.context";
inline constexpr const char *CONTEXT_KINDS[] = {"game_state", "game_mode_tag", "game_setup_mode_tag", "multiplayer_context_tag",
                                                "general_tag"};

struct TagRow {
    std::string path;
    std::string kind;
    std::vector<std::string> tags;
};

struct StockOption {
    std::string section, name;
    std::vector<TagRow> rows;
};

inline std::vector<std::string> split(const std::string &text)
{
    std::vector<std::string> out;
    std::size_t at = 0;
    while (at <= text.size()) {
        std::size_t end = text.find(',', at);
        if (end == std::string::npos)
            end = text.size();
        std::string item = text.substr(at, end - at);
        if (!item.empty())
            out.push_back(std::move(item));
        at = end + 1;
    }
    return out;
}

inline std::string join(const std::vector<std::string> &items)
{
    std::string out;
    for (auto &i : items)
        out += (out.empty() ? "" : ",") + i;
    return out;
}

inline int64_t count_of(const Read &read, const std::string &path)
{
    auto t = read(path);
    auto n = t ? rows::parse_int(*t) : std::nullopt;
    return n && *n > 0 ? *n : 0;
}

inline std::vector<StockOption> stock_options(const Read &read)
{
    std::vector<StockOption> out;
    for (const char *section : SECTIONS) {
        std::string base = std::string("options_system.options.") + section;
        int64_t folders = count_of(read, base + "._options_folder_instancecount_");
        for (int64_t f = 0; f < folders; f++) {
            std::string folder = base + ".options_folder_" + std::to_string(f);
            int64_t options = count_of(read, folder + "._option_instancecount_");
            for (int64_t o = 0; o < options; o++) {
                std::string at = folder + ".option_" + std::to_string(o);
                auto name = read(at + "._metadata_");
                if (!name || name->empty())
                    continue;
                StockOption opt{section, *name, {}};
                int64_t rows = count_of(read, at + "._additional_tags_instancecount_");
                for (int64_t r = 0; r < rows; r++) {
                    std::string row = at + ".additional_tags_" + std::to_string(r);
                    auto tags = read(row + ".tags");
                    if (!tags)
                        continue;
                    std::vector<std::string> items = split(*tags);
                    if (items.empty())
                        continue;
                    std::string kind = items.front();
                    items.erase(items.begin());
                    opt.rows.push_back({row, std::move(kind), std::move(items)});
                }
                out.push_back(std::move(opt));
            }
        }
    }
    return out;
}

// Every tag value of options_system.context, with the folder (kind) it belongs to.
inline std::map<std::string, std::string> context_tags(const Read &read)
{
    std::map<std::string, std::string> out;
    int64_t folders = count_of(read, std::string(CONTEXT) + "._tag_folder_instancecount_");
    for (int64_t f = 0; f < folders; f++) {
        std::string folder = std::string(CONTEXT) + ".tag_folder_" + std::to_string(f);
        auto kind = read(folder + "._metadata_");
        if (!kind || kind->empty())
            continue;
        for (const char *word : CONTEXT_KINDS) {
            int64_t n = count_of(read, folder + "._" + word + "_instancecount_");
            for (int64_t i = 0; i < n; i++)
                if (auto tag = read(folder + "." + word + "_" + std::to_string(i) + "._metadata_"); tag && !tag->empty())
                    out.emplace(*tag, *kind);
        }
    }
    return out;
}

inline std::vector<std::string> flat_tags(const StockOption &o)
{
    std::vector<std::string> out;
    for (auto &r : o.rows)
        for (auto &t : r.tags)
            if (std::find(out.begin(), out.end(), t) == out.end())
                out.push_back(t);
    return out;
}

inline Value stock_record(const StockOption &o)
{
    Value tags = Value::array();
    for (auto &t : flat_tags(o))
        tags.push_back(Value(t));
    return Value::object({{"id", Value(o.name)}, {"restriction_tags", std::move(tags)}});
}

// One record's wish for one option. exact: the record is the option's whole tag list (the stock record and its
// layers), so tags the stock list has and it lacks are removed; otherwise its tags are only added.
struct Want {
    std::string owner, record, option;
    std::vector<std::string> tags;
    bool exact = false;
};

struct Result {
    std::vector<std::string> lines;
    int options = 0, added = 0, removed = 0, refused = 0;
};

inline std::string option_name(const std::string &id, std::string *section = nullptr)
{
    auto slash = id.find('/');
    if (slash == std::string::npos)
        return id;
    if (section)
        *section = id.substr(0, slash);
    return id.substr(slash + 1);
}

inline std::string similar(const std::vector<StockOption> &all, const std::string &name)
{
    std::string out;
    int n = 0;
    for (auto &o : all)
        if (n < 5 && (o.name.find(name) != std::string::npos || name.find(o.name) != std::string::npos)) {
            out += (out.empty() ? "" : ", ") + o.name;
            n++;
        }
    return out;
}

inline Result apply_wants(rows::Engine &engine, const std::vector<Want> &wants)
{
    Result res;
    Read read = [&engine](const std::string &path) { return engine.text(path); };
    std::vector<StockOption> all = stock_options(read);
    std::map<std::string, std::string> ctx = context_tags(read);
    auto refuse = [&res](const Want &w, const std::string &why) {
        res.refused++;
        res.lines.push_back("ui.option " + w.record + ": " + why);
    };
    for (const Want &w : wants) {
        engine.attribute(w.owner, "ui.option", w.record);
        std::string section;
        std::string name = option_name(w.option, &section);
        auto it = std::find_if(all.begin(), all.end(), [&](const StockOption &o) { return o.name == name; });
        if (it == all.end()) {
            std::string near = similar(all, name);
            refuse(w, "no option \"" + w.option + "\" in options_system (" + std::to_string(all.size()) + " options" +
                          (near.empty() ? "" : "; similar: " + near) + "); an addon record cannot declare a new option");
            continue;
        }
        if (!section.empty() && section != it->section) {
            refuse(w, "option \"" + name + "\" is in the " + it->section + " section, not " + section);
            continue;
        }
        std::vector<TagRow> rows;
        for (auto &r : it->rows) {
            auto text = engine.text(r.path + ".tags");
            std::vector<std::string> items = text ? split(*text) : std::vector<std::string>();
            if (items.empty())
                continue;
            std::string kind = items.front();
            items.erase(items.begin());
            rows.push_back({r.path, std::move(kind), std::move(items)});
        }
        std::set<std::string> have, want(w.tags.begin(), w.tags.end());
        for (auto &r : rows)
            have.insert(r.tags.begin(), r.tags.end());
        std::vector<std::string> add, drop;
        for (auto &t : w.tags)
            if (!have.count(t) && std::find(add.begin(), add.end(), t) == add.end())
                add.push_back(t);
        if (w.exact)
            for (auto &t : have)
                if (!want.count(t))
                    drop.push_back(t);
        std::string problem;
        std::map<std::string, std::vector<std::string>> into;
        for (auto &t : add) {
            auto c = ctx.find(t);
            if (c == ctx.end()) {
                std::string same;
                for (auto &[tag, kind] : ctx)
                    if (kind == "game_setup_mode")
                        same += (same.empty() ? "" : ", ") + tag;
                problem = "tag \"" + t + "\" is not a value of any options_system.context folder (the game_setup_mode ones are " +
                          same + "); add the value with a context tag_folder row first";
                break;
            }
            auto row = std::find_if(rows.begin(), rows.end(), [&](const TagRow &r) { return r.kind == c->second; });
            if (row == rows.end()) {
                std::string kinds;
                for (auto &r : rows)
                    kinds += (kinds.empty() ? "" : ", ") + r.kind;
                problem = "option \"" + name + "\" has no " + c->second + " tag row for \"" + t + "\" (it has " +
                          (kinds.empty() ? "none" : kinds) + "); a tag row cannot be added";
                break;
            }
            into[row->path].push_back(t);
        }
        if (!problem.empty()) {
            refuse(w, problem);
            continue;
        }
        bool wrote = false;
        for (auto &r : rows) {
            std::vector<std::string> next = r.tags;
            int gone = 0, came = 0;
            for (auto &t : drop) {
                auto at = std::find(next.begin(), next.end(), t);
                if (at != next.end()) {
                    next.erase(at);
                    gone++;
                }
            }
            for (auto &t : into[r.path]) {
                next.push_back(t);
                came++;
            }
            if (!gone && !came)
                continue;
            next.insert(next.begin(), r.kind);
            std::string why = engine.apply(Value::object(
                {{"op", Value("set")}, {"select", Value(r.path)}, {"field", Value("tags")}, {"value", Value(join(next))}}));
            if (!why.empty()) {
                refuse(w, why);
                continue;
            }
            res.added += came;
            res.removed += gone;
            wrote = true;
        }
        if (wrote)
            res.options++;
    }
    return res;
}

}
