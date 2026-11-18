#include "rows_test.hpp"

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
        st.add_base("team.constructor", c.key,
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
          "pass is accepted, moves the store's generation and changes the expansion (" + code + ")");
}
