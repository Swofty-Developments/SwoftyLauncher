#pragma once

#include "core/apply/content/boot.hpp"
#include "core/apply/ids.hpp"

#include <functional>
#include <memory>
#include <unordered_map>

// The boot apply's stat-id steps and expansion, shared by the boot inside the game database load (appliers.cpp
// run_boot) and the boot expansion made ahead on a worker. Ahead, the steps run on a copy of the stat-id store with
// every addon admitted, and their log lines are held. The boot adopts that work only when nothing it read has
// changed since and the room leaves every addon in; otherwise it runs the same steps again on the live store.

namespace swofty_core::apply::content {

using LogLine = std::function<void(const std::string &)>;
using OlderEdits = std::unordered_map<uint64_t, rows::Rec>;

inline rows::Base base_over(const gvdb::File &file, std::shared_ptr<const OlderEdits> edits)
{
    return [&file, edits](uint64_t key) -> std::optional<rows::Rec> {
        if (auto it = edits->find(key); it != edits->end())
            return it->second;
        if (auto r = file.get(key))
            return rows::Rec{r->tag, std::move(r->text)};
        return std::nullopt;
    };
}

// The pools the database leaves for stat ids, the pinned and settled ids, and the records each pooled registry
// holds now. The pool lines are for the report; the settle lines go to `log` as they come.
inline std::map<std::string, PoolScan> boot_ids(registry::Store &st, ids::Store &ids, const rows::Base &base,
                                                const std::string &day, bool box_worlds, std::vector<std::string> &lines,
                                                const LogLine &log)
{
    auto text = [&base](const std::string &path) -> std::optional<std::string> {
        auto r = base(gvdb::fnv64(path));
        if (!r)
            return std::nullopt;
        return r->text;
    };
    std::map<std::string, PoolScan> scans;
    for (auto &scan : scan_all_pools(text)) {
        ids::Pool pool;
        if (!scan.holes.empty())
            pool.ranges = scan.holes;
        else if (scan.lo <= scan.hi)
            pool.ranges.push_back({scan.lo, scan.hi});
        for (auto &spec : POOLS)
            if (scan.registry == spec.registry)
                pool.cap = spec.cap;
        ids.define_pool(scan.registry, pool);
        lines.push_back("stat-id pool for " + scan.registry + ": " +
                        (scan.lo <= scan.hi ? std::to_string(scan.lo) + "-" + std::to_string(scan.hi) : std::string("none")) +
                        " (" + scan.table + " holds ids to " + std::to_string(scan.lo - 1) + ")");
        scans[scan.registry] = std::move(scan);
    }
    lines.push_back(box_worlds ? "box worlds on: a box with more teams than its world has garages gets a season and a "
                                 "track layer of its own (SWOFTY_BOX_WORLDS=0 turns them off)"
                               : "box worlds off (SWOFTY_BOX_WORLDS=0): every box keeps the world its list's layers load");
    for (auto &line : settle_ids(ids, st, scans, day))
        log(line);
    for (const char *registry : {"vehicle", "vehicle.package", "content.package", "team.identity"})
        if (scans.count(registry))
            ids.set_live(registry, st.list(registry, ""));
    return scans;
}

// The boot's stat ids: a record's pin, else its kept or a newly allocated id; each new allocation's line goes to
// `log`.
inline StatIds boot_stat_ids(ids::Store &ids, const std::string &day, LogLine log)
{
    return [&ids, day, log = std::move(log)](const std::string &registry, const std::string &id, int64_t pin) -> int64_t {
        if (pin >= 0)
            return ids.pin(registry, id, pin, day).id;
        ids::Result r = ids.allocate(registry, id, day);
        if (r.how != "kept" && !r.line.empty())
            log(r.line);
        return r.id;
    };
}

// What the boot expansion made ahead read and made.
struct Ahead {
    uint64_t store_generation = 0, ids_generation = 0;
    bool box_worlds = false, own_worlds = false;
    box_world::WorldTracks world_tracks;
    std::string day;
    std::shared_ptr<const OlderEdits> edits;
    std::unique_ptr<Expansion> boot;
    std::unique_ptr<ids::Store> ids;
    std::vector<std::string> lines, logs;
};

// The stat-id steps and the expansion with every addon admitted, on a copy of `live`, over `base` (the database
// file under a.edits). The caller fills box_worlds, day and edits first; the store's generation is read before
// anything else reads the store.
inline void boot_ahead(registry::Store &st, const ids::Store &live, rows::Base base, Ahead &a)
{
    a.store_generation = st.generation();
    a.ids = std::make_unique<ids::Store>(live);
    a.ids_generation = a.ids->generation();
    a.boot = std::make_unique<Expansion>(base);
    a.boot->box_worlds = a.box_worlds;
    a.boot->own_worlds = a.own_worlds;
    a.boot->world_tracks = a.world_tracks;
    LogLine hold = [&a](const std::string &line) { a.logs.push_back(line); };
    boot_ids(st, *a.ids, base, a.day, a.box_worlds, a.lines, hold);
    expand(st, *a.boot, boot_stat_ids(*a.ids, a.day, hold));
}

// What the boot sees when it would adopt the expansion made ahead.
struct AheadNow {
    uint64_t store_generation = 0;
    bool box_worlds = false, own_worlds = false;
    std::string day;
    const OlderEdits *edits = nullptr;
    Admission admitted;
    uint64_t room = 0;
    std::vector<std::string> order;
};

// Empty when the boot may adopt the expansion made ahead, else why it may not. The stat-id store's generation is
// checked by ids::Store::commit, under its lock (adopt_ahead).
inline std::string ahead_refusal(const Ahead &a, const AheadNow &now)
{
    if (!a.boot || !a.ids)
        return "the expansion ahead did not finish";
    if (now.store_generation != a.store_generation)
        return "the registry changed after the expansion ahead started";
    if (now.box_worlds != a.box_worlds)
        return std::string("SWOFTY_BOX_WORLDS is ") + (now.box_worlds ? "on" : "off") + " at the boot and was " +
               (a.box_worlds ? "on" : "off") + " ahead";
    if (now.own_worlds != a.own_worlds)
        return std::string("SWOFTY_BOX_OWN_WORLDS is ") + (now.own_worlds ? "on" : "off") + " at the boot and was " +
               (a.own_worlds ? "on" : "off") + " ahead";
    if (now.day != a.day)
        return "the date changed after the expansion ahead started";
    if (!now.edits || !a.edits || *now.edits != *a.edits)
        return "the older addons' database edits changed after the expansion ahead started";
    if (!now.admitted.skip.empty())
        return "the room leaves " + now.admitted.refused.front().first + " out at the boot";
    Admission again = readmit(now.order, a.boot->addon_fresh, now.admitted, now.room);
    if (!again.skip.empty())
        return "the room leaves " + again.refused.front().first + " out once its rows are counted";
    return {};
}

// Puts the stat-id store made ahead in place of `live` when ahead_refusal finds nothing and `live` has not moved
// since the copy; empty when it did, else why not. Either way `live` is left whole.
inline std::string adopt_ahead(Ahead &a, const AheadNow &now, ids::Store &live)
{
    std::string why = ahead_refusal(a, now);
    if (why.empty() && !live.commit(std::move(*a.ids), a.ids_generation))
        why = "the stat-id store changed after the expansion ahead started";
    return why;
}

}
