#include "core/career/snapshots/snapshots.hpp"
#include "core/career/snapshots/bag.hpp"
#include "core/career/snapshots/book_cache.hpp"
#include "core/career/snapshots/clock.hpp"

#include <algorithm>

namespace career::snapshots {

using swofty::json::Value;
namespace fmt = format;

Book::Book(reg::Store &store, std::function<void(const std::string &)> log)
    : store_(store), log_(std::move(log)), cache_(std::make_unique<Cache>())
{
}

Book::~Book() = default;

void Book::set_ids(Ids ids) { ids_ = std::move(ids); }

std::string Book::idmap(const std::string &career) const { return ids_.map ? ids_.map(career) : std::string("{}"); }

void Book::note(const std::string &s) const
{
    if (log_)
        log_(s);
}

reg::SnapshotSource Book::source()
{
    return
        [this](const std::string &registry, const std::string &id, const reg::Context &ctx) { return read(registry, id, ctx); };
}

void Book::refresh()
{
    std::map<std::string, int> majors;
    for (auto &name : store_.registries())
        if (auto d = store_.def(name); d && d->snapshot)
            majors[name] = d->major;
    {
        std::lock_guard g(mu_);
        cache_->majors = std::move(majors);
    }
    store_.set_snapshot_source(source());
}

void Book::loaded(const Bytes *body)
{
    fmt::Entry e;
    fmt::Decoded st = fmt::Decoded::ok;
    std::string why;
    bool have = body != nullptr;
    if (body) {
        Bytes payload;
        if (!fmt::parse_bag(*body, payload, why))
            st = fmt::Decoded::corrupt;
        else
            st = fmt::decode(payload, e, why);
        if (st != fmt::Decoded::ok)
            e = fmt::Entry{};
    }
    size_t careers = 0, seasons = 0, unreadable = 0;
    {
        std::lock_guard g(mu_);
        for (auto &id : dirty_)
            if (fmt::Career *mine = entry_.career(id)) {
                if (fmt::Career *theirs = e.career(id))
                    *theirs = *mine;
                else
                    e.careers.push_back(*mine);
            }
        entry_ = std::move(e);
        loaded_state_ = st;
        have_entry_ = have;
        pending_.clear();
        cache_->seasons.clear();
        careers = entry_.careers.size();
        for (auto &c : entry_.careers) {
            seasons += c.seasons.size();
            unreadable += c.unreadable;
        }
    }
    if (!have)
        note("[registry] swofty_registry: this profile save has none; each career starts its snapshots at its next "
             "season");
    else if (st == fmt::Decoded::newer)
        note("[registry] swofty_registry: " + why +
             "; kept as is and written back unchanged, and careers read "
             "their registries live");
    else if (st == fmt::Decoded::corrupt)
        note("[registry] swofty_registry: ignored (" + why +
             "); careers read their registries live, and the next "
             "snapshot replaces it");
    else
        note("[registry] swofty_registry: read " + std::to_string(careers) + " career(s), " + std::to_string(seasons) +
             " season snapshot(s) (" + kb(body->size()) + ")" +
             (unreadable ? "; " + std::to_string(unreadable) + " career(s) unreadable, kept as is" : ""));
    refresh();
}

bool Book::saving(Bytes &body)
{
    std::lock_guard g(mu_);
    if (loaded_state_ == fmt::Decoded::newer || (dirty_.empty() && !test_pad_))
        return false;
    fmt::Entry out = entry_;
    size_t provisional = std::erase_if(out.careers, [](const fmt::Career &c) { return seasons::is_provisional(c.id); });
    if (provisional)
        note("[registry] swofty_registry: " + std::to_string(provisional) +
             " career(s) still on a provisional id are left out of this save");
    if (test_pad_) {
        std::erase_if(out.extra, [](const fmt::Item &i) { return i.tag == fmt::T_TEST_PAD; });
        fmt::Item pad;
        pad.tag = fmt::T_TEST_PAD;
        uint32_t x = 0x9e3779b9u;
        pad.body.resize(test_pad_);
        for (auto &b : pad.body)
            x = x * 1664525u + 1013904223u, b = uint8_t(x >> 24);
        out.extra.push_back(std::move(pad));
    }
    Bytes payload = out.encode();
    if (payload.size() > fmt::ENTRY_CAP) {
        note("[registry] swofty_registry: the entry's payload of " + kb(payload.size()) + " is past the " + kb(fmt::ENTRY_CAP) +
             " cap; not written with this save");
        return false;
    }
    if (!fmt::bag_body(payload, body))
        return false;
    pending_ = dirty_;
    test_pad_ = 0;
    return true;
}

void Book::set_test_pad(size_t bytes)
{
    std::lock_guard g(mu_);
    test_pad_ = bytes;
}

size_t test_pad_bytes(const char *text, bool agent_armed)
{
    if (!agent_armed || !text || !*text)
        return 0;
    size_t n = 0;
    for (const char *c = text; *c; ++c) {
        if (*c < '0' || *c > '9' || n > fmt::ENTRY_CAP)
            return 0;
        n = n * 10 + size_t(*c - '0');
    }
    return n >= 1 && n <= fmt::ENTRY_CAP - (64u << 10) ? n : 0;
}

void Book::written(bool ok)
{
    std::lock_guard g(mu_);
    if (ok)
        for (auto &id : pending_)
            dirty_.erase(id);
    pending_.clear();
}

std::string Book::active() const
{
    std::lock_guard g(mu_);
    return active_;
}

bool Book::has_career(const std::string &id) const
{
    std::lock_guard g(mu_);
    return const_cast<fmt::Entry &>(entry_).career(id) != nullptr;
}

bool Book::rename(const std::string &from, const std::string &to)
{
    if (from.empty() || to.empty() || from == to)
        return false;
    std::lock_guard g(mu_);
    fmt::Career *c = entry_.career(from);
    if (!c || entry_.career(to))
        return false;
    c->id = to;
    dirty_.erase(from);
    dirty_.insert(to);
    if (active_ == from)
        active_ = to;
    cache_->seasons.clear();
    return true;
}

bool Book::started(const std::string &id, int year) const
{
    std::lock_guard g(mu_);
    const fmt::Career *c = const_cast<fmt::Entry &>(entry_).career(id);
    if (!c || c->unreadable || c->freeze != fmt::Freeze::per_season)
        return false;
    const fmt::Season *s = c->season(year);
    return s && !s->frozen.empty();
}

fmt::Entry Book::entry() const
{
    std::lock_guard g(mu_);
    return entry_;
}

std::string Book::state() const
{
    std::lock_guard g(mu_);
    return std::string(have_entry_ ? fmt::decoded_name(loaded_state_) : "none") + (dirty_.empty() ? "" : ", unsaved");
}

void Book::career_loaded(const std::string &id, reg::Context ctx)
{
    if (id.empty())
        return;
    std::string map, line;
    bool adopt = false;
    {
        std::lock_guard g(mu_);
        active_ = id;
        cache_->seasons.clear();
        cache_->logged.clear();
        fmt::Career *c = entry_.career(id);
        if (!c) {
            line = "[registry] career " + id +
                   ": no snapshot in swofty_registry (a career from before 1.6, or a new "
                   "one); it reads live and freezes from its next season";
        } else if (c->unreadable) {
            line = "[registry] career " + id + ": its snapshot is unreadable; kept as is, and it reads live";
        } else {
            std::vector<int> years;
            for (auto &s : c->seasons)
                if (s.year)
                    years.push_back(s.year);
            std::sort(years.begin(), years.end());
            line = "[registry] career " + id + ": " + std::to_string(c->seasons.size()) + " season snapshot(s)" +
                   (years.empty() ? "" : " for " + std::to_string(years.front()) + "-" + std::to_string(years.back())) +
                   " read from swofty_registry (" + c->majors + ", freeze " +
                   (c->freeze == fmt::Freeze::at_creation ? "at creation" : "per season") + ")";
            map = c->idmap;
            adopt = true;
        }
    }
    note(line);
    if (adopt && ids_.adopt) {
        std::vector<std::string> clashes = ids_.adopt(id, map);
        if (!clashes.empty())
            note("[registry] career " + id + ": " + std::to_string(clashes.size()) +
                 " stat id(s) in its saved map clash with this install (see the [guards] lines)");
    }
    reg::Context now = store_.context();
    now.career = id;
    if (!ctx.data_group.empty())
        now.data_group = ctx.data_group;
    store_.set_context(now);
    refresh();
}

void Book::career_unloaded()
{
    {
        std::lock_guard g(mu_);
        active_.clear();
        cache_->seasons.clear();
    }
    reg::Context now = store_.context();
    now.career.clear();
    store_.set_context(now);
    refresh();
}

reg::Frozen Book::read(const std::string &registry, const std::string &id, const reg::Context &ctx)
{
    reg::Frozen out;
    std::lock_guard g(mu_);
    if (active_.empty() || ctx.career != active_)
        return out;
    fmt::Career *c = entry_.career(active_);
    if (!c || c->unreadable || c->seasons.empty())
        return out;
    auto known = cache_->majors.find(registry);
    int current = known == cache_->majors.end() ? 0 : known->second;
    int latest = 0;
    for (auto &s : c->seasons)
        latest = std::max(latest, s.year);
    int cur = ctx.has_year ? ctx.year : latest;
    auto decoded = [&](const fmt::Season &s) -> Cache::Decoded * {
        const fmt::Frozen *f = s.find(registry);
        if (!f)
            return nullptr;
        auto key = std::make_pair(s.year, registry);
        auto it = cache_->seasons.find(key);
        if (it == cache_->seasons.end()) {
            Cache::Decoded d;
            d.major = f->major;
            std::string raw;
            if (current && f->major > current) {
                if (cache_->logged.insert(registry + "@" + std::to_string(s.year)).second)
                    note("[registry] career " + active_ + ": " + registry + " " + std::to_string(s.year) +
                         " was frozen with schema " + std::to_string(f->major) + "." + std::to_string(f->minor) +
                         ", newer than this core's " + std::to_string(current) + "; kept in the save, read live");
            } else if (fmt::decompress(f->records, raw)) {
                swofty::json::Error e;
                Value doc = swofty::json::parse(raw, &e);
                if (e.message.empty() && doc.is_object() && doc["records"].is_object()) {
                    d.ok = true;
                    d.records = doc["records"];
                    for (const char *list : {"loose", "ranged"})
                        if (auto *a = doc[list].if_array())
                            for (auto &v : *a)
                                if (v.is_string())
                                    d.loose.insert(std::string(v.as_string()));
                }
            }
            if (!d.ok && !(current && f->major > current) &&
                cache_->logged.insert(registry + "#" + std::to_string(s.year)).second)
                note("[registry] career " + active_ + ": " + registry + " " + std::to_string(s.year) +
                     " snapshot does not decode; this career reads it live");
            it = cache_->seasons.emplace(key, std::move(d)).first;
        }
        return it->second.ok ? &it->second : nullptr;
    };
    auto hit = [&](Cache::Decoded *d) {
        const Value *v = d ? d->records.find(id) : nullptr;
        if (!v)
            return false;
        out.found = true;
        out.value = *v;
        out.major = d->major;
        return true;
    };
    if (const fmt::Season *s = c->season(cur); s && cur && hit(decoded(*s)))
        return out;
    for (auto &s : c->seasons) {
        if (!s.year || s.year == cur)
            continue;
        Cache::Decoded *d = decoded(s);
        if (d && !d->loose.count(id) && hit(d))
            return out;
    }
    if (const fmt::Season *s = c->season(0); s && hit(decoded(*s)))
        return out;
    return out;
}

}
