#include "rows_test.hpp"

#include "core/apply/content/boot_ahead.hpp"

namespace {

std::string team_code(registry::Store &st, Db &db)
{
    content::Expansion x(db.base());
    content::expand(st, x, content::planning_ids());
    for (int i = 2; i < 4; i++) {
        std::string row = "entrants.teams.team_" + std::to_string(i);
        if (written(x.engine, row + "._metadata_") == "late_team")
            return written(x.engine, row + ".display_name_code").value_or("none");
    }
    return "no row";
}

}

void late_write_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.a", "mod.b"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"team.identity", "team.constructor"}) {
        std::string schema = slurp(regs + "/" + name + (std::string(name) == "team.identity" ? "/1.1.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int r = registry::load_data_file("mod.a", "registry/teams.json", R"j({ "team.identity": {
        "late_team": { "donor": "game/mercedes", "names": { "code": "LTE" }, "livery_name": "late", "stat_id": 700 } } })j");
    registry::LayerSpec spec;
    spec.addon = "mod.b";
    spec.registry = "team.identity";
    spec.id = "mod.a/late_team";
    spec.change.patch = Value::object({{"names", Value::object({{"code", Value("LYR")}})}});
    uint32_t handle = 0;
    int layered = st.layer(spec, &handle);
    st.end_register();
    Db db;
    stock_team_rows(db);
    content::Read read = [&](const std::string &path) -> std::optional<std::string> {
        auto rec = db.base()(content::gvdb::fnv64(path));
        return rec ? std::optional<std::string>(rec->text) : std::nullopt;
    };
    for (auto &c : content::CONSTRUCTOR_SEEDS)
        st.add_base(
            "team.constructor", c.key,
            Value::object({{"name", Value(std::string(c.name))}, {"country", Value(std::string(c.country))}, {"team", Value()}}));
    for (auto &s : content::stock_teams(read))
        st.add_base("team.identity", s.label, s.record);
    st.stock_loaded("team.identity", true);
    check(r == SWOFTY_ASSET_OK && layered == SWOFTY_ASSET_OK && team_code(st, db) == "LYR",
          "a layer made in the register pass changes a boot registry's record in the expansion");

    registry::LayerSpec again = spec;
    again.change.patch = Value::object({{"names", Value::object({{"code", Value("NEW")}})}});
    check(st.layer(again, &handle) == SWOFTY_ASSET_LATE && team_code(st, db) == "LYR",
          "after the register pass a new layer on a boot registry is refused as late");

    uint64_t before = st.generation();
    int cleared = st.clear_layer("mod.b", "team.identity", "mod.a/late_team", registry::Scope::global);
    std::string code = team_code(st, db);
    check(cleared == SWOFTY_ASSET_OK && st.generation() != before && code == "LTE",
          "today's behaviour, documented in docs/design/native-runtime.md: clear_layer on a boot registry after the register "
          "pass is accepted, moves the store's generation and changes the expansion (" +
              code + ")");
}

namespace {

bool same_writes(const rows::Engine &a, const rows::Engine &b)
{
    auto &x = a.writes();
    auto &y = b.writes();
    if (x.size() != y.size())
        return false;
    for (std::size_t i = 0; i < x.size(); i++)
        if (x[i].key != y[i].key || x[i].tag != y[i].tag || x[i].text != y[i].text || x[i].owner != y[i].owner ||
            x[i].path != y[i].path)
            return false;
    return true;
}

}

void ahead_tests(const std::string &regs)
{
    registry::reset_store();
    auto &st = registry::store();
    st.set_order({"mod.a", "mod.b"});
    st.set_validator(&registry::schema_validator());
    const SwoftyRegistry *t = registry::table();
    for (const char *name : {"team.identity", "team.constructor"}) {
        std::string schema = slurp(regs + "/" + name + (std::string(name) == "team.identity" ? "/1.1.json" : "/1.0.json"));
        t->declare(name, SwoftyText{schema.data(), schema.size()});
    }
    int a_ok = registry::load_data_file("mod.a", "registry/teams.json", R"j({ "team.identity": {
        "alpha": { "donor": "game/mercedes", "names": { "code": "ALP" }, "livery_name": "alpha" } } })j");
    int b_ok = registry::load_data_file("mod.b", "registry/teams.json", R"j({ "team.identity": {
        "beta": { "donor": "game/tororosso", "names": { "code": "BET" }, "livery_name": "beta", "stat_id": 40 } } })j");
    st.end_register();
    Db db;
    stock_team_rows(db);
    content::Read read = [&](const std::string &path) -> std::optional<std::string> {
        auto rec = db.base()(content::gvdb::fnv64(path));
        return rec ? std::optional<std::string>(rec->text) : std::nullopt;
    };
    for (auto &c : content::CONSTRUCTOR_SEEDS)
        st.add_base(
            "team.constructor", c.key,
            Value::object({{"name", Value(std::string(c.name))}, {"country", Value(std::string(c.country))}, {"team", Value()}}));
    for (auto &s : content::stock_teams(read))
        st.add_base("team.identity", s.label, s.record);
    st.stock_loaded("team.identity", true);

    apply::ids::Store today_ids;
    std::vector<std::string> today_logs, today_lines;
    content::LogLine log = [&](const std::string &l) { today_logs.push_back(l); };
    content::boot_ids(st, today_ids, db.base(), "d", false, today_lines, log);
    content::Expansion today(db.base());
    content::expand(st, today, content::boot_stat_ids(today_ids, "d", log));

    content::OlderEdits none;
    auto make = [&](apply::ids::Store &live) {
        auto a = std::make_unique<content::Ahead>();
        a->edits = std::make_shared<const content::OlderEdits>();
        a->day = "d";
        content::boot_ahead(st, live, db.base(), *a);
        return a;
    };
    auto now_of = [&](const content::Ahead &a) {
        content::AheadNow n;
        n.store_generation = st.generation();
        n.day = "d";
        n.edits = &none;
        n.order = st.order();
        n.room = UINT64_MAX;
        n.admitted = content::admit(n.order, a.boot->addon_fresh, n.room);
        return n;
    };

    apply::ids::Store live;
    auto a = make(live);
    std::string why = content::adopt_ahead(*a, now_of(*a), live);
    check(a_ok == SWOFTY_ASSET_OK && b_ok == SWOFTY_ASSET_OK && why.empty() && !today_logs.empty() && a->logs == today_logs &&
              a->lines == today_lines && live.dump() == today_ids.dump() && same_writes(a->boot->engine, today.engine) &&
              !today.engine.writes().empty(),
          "the boot adopts the expansion made ahead: the same writes, stat ids, held lines (" +
              std::to_string(today_logs.size()) + ") and pool lines as the boot's own path (" + why + ")");

    struct Case {
        const char *what, *reason;
        std::function<void(content::AheadNow &, apply::ids::Store &)> change;
    };
    content::OlderEdits other{{content::gvdb::fnv64("entrants.teams.team_0.display_name_code"), rows::Rec{2, "OLD"}}};
    std::vector<Case> cases = {
        {"the registry moved", "the registry changed",
         [&](content::AheadNow &n, apply::ids::Store &) {
             st.set_context(st.context());
             n.store_generation = st.generation();
         }},
        {"the stat-id store moved", "the stat-id store changed", [&](content::AheadNow &, apply::ids::Store &l) { l.saved(); }},
        {"SWOFTY_BOX_WORLDS differs", "SWOFTY_BOX_WORLDS is on at the boot",
         [&](content::AheadNow &n, apply::ids::Store &) { n.box_worlds = true; }},
        {"the date moved", "the date changed", [&](content::AheadNow &n, apply::ids::Store &) { n.day = "e"; }},
        {"the older edits differ", "older addons' database edits changed",
         [&](content::AheadNow &n, apply::ids::Store &) { n.edits = &other; }},
        {"the room leaves an addon out at the boot", "out at the boot",
         [&](content::AheadNow &n, apply::ids::Store &) {
             n.room = 0;
             n.admitted = content::admit(n.order, today.addon_fresh, n.room);
         }},
        {"the room leaves an addon out once its rows are counted", "out once its rows are counted",
         [&](content::AheadNow &n, apply::ids::Store &) {
             n.room = 0;
             n.admitted = content::Admission{};
         }},
    };
    for (auto &c : cases) {
        apply::ids::Store l;
        auto x = make(l);
        content::AheadNow n = now_of(*x);
        c.change(n, l);
        std::string kept = l.dump();
        std::string got = content::adopt_ahead(*x, n, l);
        check(got.find(c.reason) != std::string::npos && l.dump() == kept,
              std::string("the boot does not adopt the expansion made ahead when ") + c.what +
                  ", and the stat-id store stays whole: " + got);
    }
    content::Ahead unfinished;
    content::AheadNow n;
    check(content::ahead_refusal(unfinished, n) == "the expansion ahead did not finish",
          "an expansion ahead that failed or never ran is not adopted");
}
