#include "core/apply/content/boot.hpp"
#include "core/apply/content/boot_ahead.hpp"
#include "core/apply/ids.hpp"
#include "core/apply/scheduler.hpp"
#include "core/registry/table.hpp"
#include "core/registry/validate.hpp"

#include <chrono>
#include <cstring>
#include <fstream>
#include <iostream>
#include <sstream>
#include <thread>

// The core's boot apply of the content registries, outside the game: the same expansion appliers.cpp runs, over a
// game database file and a build's swofty/assets.json db_edits (the older addons' edits), with a fresh stat-id store.
// It writes the database the game would hold after the boot apply, as db_edits against the file (the records that
// differ from it), and a summary. With --plan worker or --plan here, the plan is counted through the scheduler's boot
// plan, on a worker or on this thread, as the core counts it. With --ahead, the boot expansion is made ahead on a
// worker first (boot_ahead.hpp) and adopted when its guards allow, else the boot expands again; --ahead-change
// registry|ids|box-worlds|day|edits changes what one guard compares after the expansion ahead, so the boot does not
// adopt it.

using namespace swofty_core;
using swofty::json::Value;
namespace content = apply::content;

static std::string slurp(const std::string &path)
{
    std::ifstream f(path, std::ios::binary);
    std::stringstream s;
    s << f.rdbuf();
    return s.str();
}

class PlanRows : public apply::Applier {
public:
    PlanRows(content::Expansion &plan, std::thread::id main) : plan_(plan), main_(main) {}
    const char *registry() const override { return "vehicle"; }
    registry::Applies applies() const override { return registry::Applies::boot; }
    bool counts_off_thread() const override { return true; }
    std::size_t database_keys(registry::Store &st) override
    {
        off_thread = std::this_thread::get_id() != main_;
        content::expand(st, plan_, content::planning_ids());
        std::size_t n = 0;
        for (auto &[reg, k] : plan_.fresh)
            n += k;
        return n;
    }
    bool off_thread = false;

private:
    content::Expansion &plan_;
    std::thread::id main_;
};

static std::string hex16(uint64_t k)
{
    char b[24];
    std::snprintf(b, sizeof b, "%016llx", (unsigned long long)k);
    return b;
}

int main(int argc, char **argv)
{
    std::string regs, gvdb_path, out_path, edits_path, summary_path, room_text, plan_how, ahead_change;
    bool box_worlds = false, ahead_on = false;
    std::vector<std::pair<std::string, std::string>> files;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        auto next = [&]() -> std::string { return i + 1 < argc ? argv[++i] : ""; };
        if (a == "--registries")
            regs = next();
        else if (a == "--db")
            gvdb_path = next();
        else if (a == "--edits")
            edits_path = next();
        else if (a == "--out")
            out_path = next();
        else if (a == "--summary")
            summary_path = next();
        else if (a == "--room")
            room_text = next();
        else if (a == "--box-worlds")
            box_worlds = true;
        else if (a == "--plan")
            plan_how = next();
        else if (a == "--ahead")
            ahead_on = true;
        else if (a == "--ahead-change")
            ahead_on = true, ahead_change = next();
        else if (i + 1 < argc) {
            files.push_back({a, argv[i + 1]});
            i++;
        }
    }
    if (regs.empty() || gvdb_path.empty() || out_path.empty() || files.empty()) {
        std::cerr << "usage: rows_tool --registries DIR --db GVDB --out OUT.json [--edits assets.json] [--summary S.json] "
                     "[--room N] [--box-worlds] [--plan here|worker] [--ahead] "
                     "[--ahead-change registry|ids|box-worlds|day|edits] <addon> <data file>...\n";
        return 2;
    }
    content::gvdb::File file;
    if (!file.load(slurp(gvdb_path))) {
        std::cerr << "cannot read " << gvdb_path << "\n";
        return 2;
    }
    std::unordered_map<uint64_t, content::rows::Rec> edits;
    if (!edits_path.empty()) {
        Value doc = swofty::json::parse(slurp(edits_path));
        const Value &list = doc["db_edits"];
        for (std::size_t i = 0; i < list.size(); i++) {
            const Value &e = list[i];
            uint64_t key = e["key"].is_string() ? content::gvdb::fnv64(e["key"].as_string())
                                                : std::strtoull(std::string(e["hash"].as_string()).c_str(), nullptr, 16);
            int tag = e["type"].is_int() ? int(e["type"].as_int()) : -1;
            if (tag < 0)
                if (auto f = file.get(key))
                    tag = f->tag;
            edits[key] = {tag, std::string(e["value"].as_string())};
        }
    }
    content::rows::Base base = [&](uint64_t key) -> std::optional<content::rows::Rec> {
        if (auto it = edits.find(key); it != edits.end())
            return it->second;
        if (auto r = file.get(key))
            return content::rows::Rec{r->tag, r->text};
        return std::nullopt;
    };

    registry::reset_store();
    auto &st = registry::store();
    std::vector<std::string> order;
    for (auto &[addon, path] : files)
        if (std::find(order.begin(), order.end(), addon) == order.end())
            order.push_back(addon);
    st.set_order(order);
    st.set_validator(&registry::schema_validator());
    st.set_log([](const std::string &l) { std::cerr << l << "\n"; });
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"engine_sound",
                             "vehicle.package",
                             "vehicle",
                             "livery",
                             "content.package",
                             "era.box",
                             "team.identity",
                             "team.constructor",
                             "team.pit_times",
                             "team.logo",
                             "helmet",
                             "suit",
                             "gloves",
                             "driver.head",
                             "driver.look",
                             "driver.person",
                             "driver.identity",
                             "driver.stats",
                             "wearable.season_set",
                             "rules.drs"}) {
        std::string schema = slurp(regs + "/" + name + "/1.0.json");
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int bad = 0;
    for (auto &[addon, path] : files)
        if (registry::load_data_file(addon, path, slurp(path)) != SWOFTY_ASSET_OK) {
            std::cerr << "data file " << path << ": some records refused\n";
            bad++;
        }
    st.end_register();
    for (auto &r : st.refusals()) {
        std::cerr << "refused: " << r.addon << " " << r.registry << " " << r.id << " " << r.reason << ": " << r.message << "\n";
        bad++;
    }

    content::Expansion plan(base);
    plan.box_worlds = box_worlds;
    bool plan_off_thread = false;
    if (plan_how.empty())
        content::expand(st, plan, content::planning_ids());
    else {
        apply::clear_appliers();
        auto &rows = apply::make<PlanRows>(plan, std::this_thread::get_id());
        apply::Scheduler sched(st);
        sched.start_plan(plan_how == "worker");
        sched.planned_keys();
        plan_off_thread = rows.off_thread;
        apply::clear_appliers();
    }

    auto text = [&](const std::string &path) -> std::optional<std::string> {
        auto r = base(content::gvdb::fnv64(path));
        if (!r)
            return std::nullopt;
        return r->text;
    };
    std::map<std::string, content::PoolScan> scans;
    Value pools = Value::object();
    for (auto &scan : content::scan_all_pools(text)) {
        pools[scan.registry] = Value::array({Value(scan.lo), Value(scan.hi)});
        scans[scan.registry] = scan;
    }
    Value clashes = Value::array();
    for (auto &p : content::pins(st)) {
        auto s = scans.find(p.registry);
        if (s != scans.end() && s->second.used.count(p.stat_id))
            clashes.push_back(Value(p.registry + " " + p.id + " " + std::to_string(p.stat_id) + " " + s->second.used[p.stat_id]));
    }
    auto &ids = apply::ids::store();
    ids.clear();
    uint64_t room = room_text.empty() ? UINT64_MAX : std::strtoull(room_text.c_str(), nullptr, 10);
    const std::string day = "test";
    using Clock = std::chrono::steady_clock;
    auto ms_since = [](Clock::time_point t) { return std::chrono::duration<double, std::milli>(Clock::now() - t).count(); };

    std::vector<std::string> logs, boot_lines;
    content::Admission adm;
    std::unique_ptr<content::Expansion> boot_ptr;
    std::string ahead_how;
    double ahead_ms = 0, boot_ms = 0;
    if (ahead_on) {
        content::Ahead ahead;
        auto copy = std::make_shared<content::OlderEdits>(edits);
        if (ahead_change == "edits")
            (*copy)[content::gvdb::fnv64("swofty.ahead.test.only_ahead")] = content::rows::Rec{0, "ahead"};
        ahead.edits = copy;
        ahead.box_worlds = ahead_change == "box-worlds" ? !box_worlds : box_worlds;
        ahead.day = ahead_change == "day" ? std::string("ahead") : day;
        std::thread worker([&] {
            auto t = Clock::now();
            content::boot_ahead(st, ids, content::base_over(file, ahead.edits), ahead);
            ahead_ms = ms_since(t);
        });
        worker.join();
        if (ahead_change == "registry")
            st.set_context(st.context());
        else if (ahead_change == "ids")
            ids.saved();
        else if (!ahead_change.empty() && ahead_change != "edits" && ahead_change != "box-worlds" && ahead_change != "day") {
            std::cerr << "unknown --ahead-change " << ahead_change << "\n";
            return 2;
        }
        auto t = Clock::now();
        content::AheadNow now;
        now.store_generation = st.generation();
        now.box_worlds = box_worlds;
        now.day = day;
        now.edits = &edits;
        now.order = st.order();
        now.room = room;
        now.admitted = content::admit(now.order, plan.addon_fresh, room);
        ahead_how = content::adopt_ahead(ahead, now, ids);
        if (ahead_how.empty()) {
            logs = std::move(ahead.logs);
            boot_lines = std::move(ahead.lines);
            boot_ptr = std::move(ahead.boot);
        }
        boot_ms = ms_since(t);
    }
    if (!boot_ptr) {
        auto t = Clock::now();
        content::LogLine log = [&](const std::string &l) { logs.push_back(l); };
        content::boot_ids(st, ids, base, day, box_worlds, boot_lines, log);
        content::StatIds stat = content::boot_stat_ids(ids, day, log);
        adm = content::admit(st.order(), plan.addon_fresh, room);
        boot_ptr = std::make_unique<content::Expansion>(base);
        boot_ptr->box_worlds = box_worlds;
        content::expand(st, *boot_ptr, stat, adm.skip);
        for (;;) {
            content::Admission again = content::readmit(st.order(), boot_ptr->addon_fresh, adm, room);
            if (again.skip.size() == adm.skip.size())
                break;
            adm = std::move(again);
            boot_ptr = std::make_unique<content::Expansion>(base);
            boot_ptr->box_worlds = box_worlds;
            content::expand(st, *boot_ptr, stat, adm.skip);
        }
        boot_ms += ms_since(t);
    }
    content::Expansion &boot = *boot_ptr;

    std::map<uint64_t, std::pair<int, std::string>> db;
    for (auto &[k, r] : edits)
        db[k] = {r.tag, r.text};
    std::size_t untyped = 0;
    Value staged = Value::array();
    for (auto &w : boot.engine.writes()) {
        if (!*content::tag_name(w.tag))
            untyped++;
        db[w.key] = {w.tag, w.text};
        staged.push_back(Value::object({{"addon", Value(w.owner)},
                                        {"registry", Value(w.registry)},
                                        {"path", Value(w.path)},
                                        {"type", Value(content::tag_name(w.tag))},
                                        {"value", Value(w.text)}}));
    }
    Value out = Value::array();
    for (auto &[k, v] : db) {
        auto f = file.get(k);
        if (f && f->tag == v.first && f->text == v.second)
            continue;
        out.push_back(Value::object({{"hash", Value(hex16(k))}, {"type", Value(v.first)}, {"value", Value(v.second)}}));
    }
    std::ofstream(out_path) << swofty::json::dump(Value::object({{"db_edits", out}}), 0) << "\n";

    Value summary = Value::object();
    auto counts = [](const std::map<std::string, std::size_t> &m) {
        Value o = Value::object();
        for (auto &[k, v] : m)
            o[k] = Value(int64_t(v));
        return o;
    };
    summary["plan_fresh"] = counts(plan.fresh);
    summary["plan_off_thread"] = Value(plan_off_thread);
    summary["boot_fresh"] = counts(boot.fresh);
    summary["addon_fresh"] = counts(plan.addon_fresh);
    Value recs = Value::object(), refused = Value::object(), lines = Value::object();
    for (auto &[k, v] : boot.records)
        recs[k] = Value(v);
    for (auto &[k, v] : boot.refused)
        refused[k] = Value(v);
    for (auto &[k, v] : boot.lines) {
        Value a = Value::array();
        for (auto &l : v)
            a.push_back(Value(l));
        lines[k] = a;
    }
    summary["records"] = recs;
    summary["refused"] = refused;
    summary["room_refused"] = Value::array();
    for (auto &[a, k] : adm.refused)
        summary["room_refused"].push_back(Value::object({{"addon", Value(a)}, {"keys", Value(int64_t(k))}}));
    summary["pools"] = pools;
    summary["pin_clashes"] = clashes;
    Value log_lines = Value::array(), boot_line_list = Value::array();
    for (auto &l : logs)
        log_lines.push_back(Value(l));
    for (auto &l : boot_lines)
        boot_line_list.push_back(Value(l));
    summary["id_lines"] = log_lines;
    summary["boot_lines"] = boot_line_list;
    summary["ids"] = swofty::json::parse(ids.dump());
    summary["ahead"] = ahead_on ? Value(ahead_how.empty() ? std::string("adopted") : ahead_how) : Value();
    summary["boot_ms"] = Value(boot_ms);
    summary["ahead_ms"] = Value(ahead_ms);
    summary["staged"] = Value(int64_t(staged.size()));
    summary["untyped"] = Value(int64_t(untyped));
    summary["lines"] = lines;
    if (!summary_path.empty()) {
        Value full = summary;
        full["staged_records"] = staged;
        std::ofstream(summary_path) << swofty::json::dump(full, 1) << "\n";
    }
    int refused_n = 0;
    for (auto &[k, v] : boot.refused)
        refused_n += v;
    std::cout << staged.size() << " records staged, " << out.size() << " db_edits after the boot apply; plan "
              << swofty::json::dump(summary["plan_fresh"]) << ", boot " << swofty::json::dump(summary["boot_fresh"]) << "; "
              << refused_n << " record(s) refused, " << adm.refused.size() << " addon(s) left out for room"
              << (ahead_on ? "; ahead: " + (ahead_how.empty() ? std::string("adopted") : ahead_how) : std::string()) << "\n";
    return bad || refused_n || untyped ? 1 : 0;
}
