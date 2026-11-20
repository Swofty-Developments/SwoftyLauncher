#include "core/career/snapshots/snapshots.hpp"
#include "core/career/snapshots/book_cache.hpp"

#include <algorithm>
#include <string>
#include <utility>
#include <vector>

// The book's freezes: what a career's creation and each season start copy out of the registries into its entry,
// and the years and data group a record belongs to.
namespace career::snapshots {

using swofty::json::Value;
namespace fmt = format;

namespace {

std::string owner_of(const std::string &id)
{
    auto at = id.find('/');
    return at == std::string::npos ? std::string() : id.substr(0, at);
}

std::string season_ref(const Value &v)
{
    const Value *s = v.find("season");
    return s && s->is_string() ? std::string(s->as_string()) : std::string();
}

void within_years(const Value &w, std::pair<int, int> &out)
{
    if (const auto *a = w.if_array()) {
        for (auto &e : *a)
            within_years(e, out);
        return;
    }
    if (!w.is_object())
        return;
    const Value *y = w.find("year");
    if (!y)
        return;
    if (y->is_int())
        out = {int(y->as_int()), int(y->as_int())};
    else if (const auto *r = y->if_array(); r && r->size() == 2 && (*r)[0].is_int() && (*r)[1].is_int())
        out = {int((*r)[0].as_int()), int((*r)[1].as_int())};
}

std::string data_group_of(reg::Store &store, const Value &v, const reg::Context &ctx)
{
    if (const Value *g = v.find("data_group"); g && g->is_string())
        return std::string(g->as_string());
    std::string s = season_ref(v);
    if (s.empty())
        return {};
    reg::Record r = store.get_live("career.season", s, ctx);
    if (r.status != reg::Status::ok)
        return {};
    return std::string(r.value["data_group"].as_string());
}

}

std::pair<int, int> years_of(reg::Store &store, const std::string &registry, const Value &v, const reg::Context &ctx)
{
    (void)registry;
    std::pair<int, int> out{0, 0};
    if (const Value *y = v.find("year"); y && y->is_int())
        return {int(y->as_int()), int(y->as_int())};
    if (const Value *s = v.find("season")) {
        if (s->is_int())
            return {int(s->as_int()), int(s->as_int())};
        if (s->is_string()) {
            reg::Record r = store.get_live("career.season", std::string(s->as_string()), ctx);
            if (r.status == reg::Status::ok && r.value["year"].is_int())
                return {int(r.value["year"].as_int()), int(r.value["year"].as_int())};
        }
    }
    if (const Value *w = v.find("within"))
        within_years(*w, out);
    return out;
}

std::vector<fmt::Frozen> Book::freeze(int year, const reg::Context &ctx, bool yearless, bool only_year, const fmt::Career *have,
                                      std::vector<std::string> &refused, std::vector<std::pair<std::string, std::string>> &owners,
                                      Stats &st)
{
    std::vector<fmt::Frozen> out;
    size_t used = have ? have->size() : 0;
    for (auto &name : store_.registries()) {
        auto def = store_.def(name);
        if (!def || !def->snapshot)
            continue;
        if (have)
            if (const fmt::Season *s = have->season(year); s && s->find(name))
                continue;
        Value records = Value::object(), loose = Value::array(), ranged = Value::array();
        size_t n = 0;
        std::string first;
        for (auto &id : store_.list(name, "")) {
            reg::Record r = store_.get_live(name, id, ctx);
            if (r.status != reg::Status::ok)
                continue;
            if (!ctx.data_group.empty()) {
                std::string dg = data_group_of(store_, r.value, ctx);
                if (!dg.empty() && dg != ctx.data_group)
                    continue;
            }
            auto [from, to] = years_of(store_, name, r.value, ctx);
            bool none = from == 0 && to == 0;
            if (none ? !yearless : (only_year || year < from || year > to))
                continue;
            if (none)
                loose.push_back(Value(id));
            else if (from != to)
                ranged.push_back(Value(id));
            records[id] = std::move(r.value);
            if (first.empty())
                first = id;
            n++;
        }
        if (!n)
            continue;
        Value doc = Value::object({{"records", std::move(records)}, {"loose", std::move(loose)}, {"ranged", std::move(ranged)}});
        std::string raw = swofty::json::dump(doc);
        fmt::Frozen f;
        f.registry = name;
        f.major = uint16_t(def->major), f.minor = uint16_t(def->minor);
        if (!fmt::compress(raw, f.records))
            continue;
        std::string label = name + (year ? " " + std::to_string(year) : " (every season)");
        size_t item = f.encode().size() + 16;
        if (f.records.size() > def->budget) {
            refused.push_back(name);
            owners.push_back({owner_of(first), label + ": snapshot " + kb(f.records.size()) + " compressed, budget " +
                                                   kb(def->budget) + "; not saved, this career reads it live" +
                                                   (year ? " for " + std::to_string(year) : "")});
            st.refused++;
            continue;
        }
        if (used + item > fmt::CAREER_CAP) {
            refused.push_back(name);
            owners.push_back({owner_of(first), label + ": snapshot " + kb(f.records.size()) +
                                                   " compressed would take the career's entry past " + kb(fmt::CAREER_CAP) +
                                                   "; not saved, this career reads it live" +
                                                   (year ? " for " + std::to_string(year) : "")});
            st.refused++;
            continue;
        }
        used += item;
        st.registries++, st.records += n, st.raw += raw.size(), st.compressed += f.records.size();
        out.push_back(std::move(f));
    }
    return out;
}

namespace {

void merge_season(fmt::Career &c, int year, std::vector<fmt::Frozen> frozen, const std::vector<std::string> &refused)
{
    fmt::Season *s = c.season(year);
    if (!s) {
        c.seasons.push_back(fmt::Season{});
        s = &c.seasons.back();
        s->year = year;
    }
    for (auto &f : frozen)
        if (!s->find(f.registry))
            s->frozen.push_back(std::move(f));
    for (auto &r : refused)
        if (std::find(s->refused.begin(), s->refused.end(), r) == s->refused.end())
            s->refused.push_back(r);
}

void merge_majors(fmt::Career &c, const std::vector<fmt::Frozen> &frozen)
{
    swofty::json::Error e;
    Value m = swofty::json::parse(c.majors, &e);
    if (!e.message.empty() || !m.is_object())
        m = Value::object();
    for (auto &f : frozen)
        m[f.registry] = Value(std::to_string(f.major) + "." + std::to_string(f.minor));
    c.majors = swofty::json::dump(m);
}

}

void Book::season_start(int year, reg::Context ctx)
{
    std::string id;
    fmt::Career copy;
    bool newer = false;
    {
        std::lock_guard g(mu_);
        newer = loaded_state_ == fmt::Decoded::newer;
        id = active_;
        if (id.empty()) {
            note("[registry] career season " + std::to_string(year) + " starts with no career loaded; nothing frozen");
            return;
        }
        if (!entry_.career(id)) {
            fmt::Career c;
            c.id = id;
            entry_.careers.push_back(std::move(c));
        }
        copy = *entry_.career(id);
    }
    if (copy.unreadable || newer) {
        note("[registry] career season " + std::to_string(year) + " starts in " + id +
             ": its saved entry is not one this core can extend; nothing frozen, it reads live");
        return;
    }
    if (copy.freeze == fmt::Freeze::at_creation) {
        note("[registry] career season " + std::to_string(year) + " starts in " + id +
             ": frozen at creation; nothing new to freeze");
        return;
    }
    if (const fmt::Season *s = copy.season(year); s && !s->frozen.empty()) {
        note("[registry] career season " + std::to_string(year) + " starts in " + id + ": already frozen (" +
             std::to_string(s->frozen.size()) + " registr" + (s->frozen.size() == 1 ? "y" : "ies") + "); kept");
        return;
    }
    ctx.career = id;
    ctx.year = year;
    ctx.has_year = true;
    std::vector<std::string> refused;
    std::vector<std::pair<std::string, std::string>> owners;
    Stats st;
    auto frozen = freeze(year, ctx, true, false, &copy, refused, owners, st);
    std::string map = idmap(id);
    std::string parts;
    for (auto &f : frozen)
        parts += (parts.empty() ? "" : ", ") + f.registry;
    {
        std::lock_guard g(mu_);
        fmt::Career *c = entry_.career(id);
        if (!c)
            return;
        merge_majors(*c, frozen);
        merge_season(*c, year, std::move(frozen), refused);
        c->idmap = map;
        dirty_.insert(id);
        cache_->seasons.clear();
    }
    note("[registry] career season " + std::to_string(year) + " starts in " + id +
         (ctx.data_group.empty() ? "" : "/" + ctx.data_group) + ": snapshot of " + (parts.empty() ? "nothing" : parts) + " (" +
         std::to_string(st.records) + " record(s), " + kb(st.raw) + " -> " + kb(st.compressed) +
         " compressed); saved with the next profile save");
    for (auto &[owner, msg] : owners) {
        note("[registry] " + msg);
        store_.note_refusal(owner, "", "", "snapshot_budget", msg);
    }
    refresh();
}

void Book::career_created(const std::string &id, fmt::Freeze freeze_mode, reg::Context ctx)
{
    if (id.empty())
        return;
    bool replaced = false;
    {
        std::lock_guard g(mu_);
        replaced = std::erase_if(entry_.careers, [&](const fmt::Career &c) { return c.id == id; }) > 0;
        fmt::Career c;
        c.id = id;
        c.freeze = freeze_mode;
        entry_.careers.push_back(std::move(c));
        active_ = id;
        cache_->seasons.clear();
        dirty_.insert(id);
    }
    std::string map = idmap(id);
    std::string line = "[registry] career " + id + " created (freeze " +
                       (freeze_mode == fmt::Freeze::at_creation ? "at creation" : "per season") + ")" +
                       (replaced ? "; it replaces an older career of the same id in swofty_registry" : "");
    if (freeze_mode != fmt::Freeze::at_creation) {
        {
            std::lock_guard g(mu_);
            if (fmt::Career *c = entry_.career(id))
                c->idmap = map;
        }
        note(line);
        reg::Context now = store_.context();
        now.career = id;
        if (!ctx.data_group.empty())
            now.data_group = ctx.data_group;
        store_.set_context(now);
        refresh();
        return;
    }
    ctx.career = id;
    std::set<int> years;
    for (auto &name : store_.registries()) {
        auto def = store_.def(name);
        if (!def || !def->snapshot)
            continue;
        for (auto &rid : store_.list(name, "")) {
            reg::Record r = store_.get_live(name, rid, ctx);
            if (r.status != reg::Status::ok)
                continue;
            if (!ctx.data_group.empty()) {
                std::string dg = data_group_of(store_, r.value, ctx);
                if (!dg.empty() && dg != ctx.data_group)
                    continue;
            }
            auto [from, to] = years_of(store_, name, r.value, ctx);
            if (from && to && to - from < 64)
                for (int y = from; y <= to; y++)
                    years.insert(y);
        }
    }
    Stats all;
    std::vector<std::pair<std::string, std::string>> owners;
    std::vector<std::pair<int, std::vector<fmt::Frozen>>> seasons;
    std::vector<std::pair<int, std::vector<std::string>>> refusals;
    fmt::Career running;
    running.id = id;
    for (int y : years) {
        reg::Context c = ctx;
        c.year = y, c.has_year = true;
        std::vector<std::string> refused;
        auto frozen = freeze(y, c, false, false, &running, refused, owners, all);
        merge_season(running, y, frozen, refused);
        seasons.push_back({y, std::move(frozen)});
        refusals.push_back({y, std::move(refused)});
    }
    {
        std::vector<std::string> refused;
        auto frozen = freeze(0, ctx, true, true, &running, refused, owners, all);
        merge_season(running, 0, frozen, refused);
        seasons.push_back({0, std::move(frozen)});
        refusals.push_back({0, std::move(refused)});
    }
    {
        std::lock_guard g(mu_);
        if (fmt::Career *c = entry_.career(id)) {
            for (size_t i = 0; i < seasons.size(); i++) {
                merge_majors(*c, seasons[i].second);
                if (!seasons[i].second.empty() || !refusals[i].second.empty())
                    merge_season(*c, seasons[i].first, std::move(seasons[i].second), refusals[i].second);
            }
            c->idmap = map;
        }
        cache_->seasons.clear();
    }
    note(line + ": froze " + std::to_string(years.size()) + " season(s)" +
         (years.empty() ? "" : " " + std::to_string(*years.begin()) + "-" + std::to_string(*years.rbegin())) + ", " +
         std::to_string(all.registries) + " registry snapshot(s), " + std::to_string(all.records) + " record(s), " + kb(all.raw) +
         " -> " + kb(all.compressed) + " compressed");
    for (auto &[owner, msg] : owners) {
        note("[registry] " + msg);
        store_.note_refusal(owner, "", "", "snapshot_budget", msg);
    }
    reg::Context now = store_.context();
    now.career = id;
    if (!ctx.data_group.empty())
        now.data_group = ctx.data_group;
    store_.set_context(now);
    refresh();
}

}
