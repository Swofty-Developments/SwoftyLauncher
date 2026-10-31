#include "../../src/core/career_content_logic.hpp"
#include "../../src/core/registry/older.hpp"
#include "../../src/core/registry/table.hpp"

#include <cstdio>
#include <string>

using namespace swofty_core::registry;
namespace older = swofty_core::registry::older;

static int failures = 0;

static void check(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok" : "FAIL", what);
    if (!ok)
        failures++;
}

static int layer_patch(Store &st, const char *addon, const char *registry, const std::string &id, const char *json)
{
    LayerSpec spec;
    spec.addon = addon;
    spec.registry = registry;
    spec.id = id;
    spec.scope = Scope::global;
    spec.change.is_patch = true;
    spec.change.patch = swofty::json::parse(json);
    spec.source = "older_test";
    uint32_t handle = 0;
    return st.layer(spec, &handle);
}

int main()
{
    Store &st = store();
    older::declare(st);
    for (const char *r : {older::CONTENT, older::HUB_TAB, older::WIZARD, older::CHARACTER, older::STRING, older::OPTION})
        check(st.declared(r), r);

    {
        older::ContentDesc d;
        d.id = "f1_1990";
        d.title = "F1 1990";
        d.description = "The 1990 season";
        d.data_group = "f1_1990";
        d.suffix = "_1990";
        d.mode = "f1_1990";
        d.entitlement = "ent";
        d.badge = 7;
        d.car_group = 4;
        d.careers = 2;
        check(older::put(st, "ex.addon", older::CONTENT, d.id, older::content_record(d), "test") == SWOFTY_ASSET_OK,
              "career.content: the 1.5 call adds a record under the caller's id");
        older::ContentDesc shown = d;
        older::content_shown(older::effective(st, older::CONTENT, "ex.addon/f1_1990"), shown);
        check(shown.title == d.title && shown.description == d.description && shown.badge == 7 && shown.car_group == 4 &&
                  shown.entitlement == "ent" && shown.careers == 2,
              "career.content: with no layer the effective record is what the call gave");
        check(layer_patch(st, "other.addon", older::CONTENT, "ex.addon/f1_1990", R"({"title":"Retitled","badge":9})") ==
                  SWOFTY_ASSET_OK,
              "career.content: another addon layers the record");
        shown = d;
        older::content_shown(older::effective(st, older::CONTENT, "ex.addon/f1_1990"), shown);
        check(shown.title == "Retitled" && shown.badge == 9 && shown.description == d.description,
              "career.content: the 1.5 behaviour reads the layered record");
        check(older::drop(st, "ex.addon", older::CONTENT, d.id) == SWOFTY_ASSET_OK, "career.content: drop");
        check(older::effective(st, older::CONTENT, "ex.addon/f1_1990").is_null(), "career.content: dropped record is gone");
    }

    {
        older::ContentDesc call;
        call.id = "f1_1990";
        call.title = "lng_f1_1990_title";
        call.description = "lng_f1_1990_desc";
        call.data_group = "f1_career_1990";
        call.suffix = "90";
        call.mode = "driver_career_1990";
        call.model = runtime::content::MODEL_2025;
        call.car_group = 2;
        call.badge = 1;
        call.entitlement = "ent";
        call.careers = runtime::content::CAREER_ALL;
        swofty::json::Value rec = older::content_record(call);
        check(rec["model"].as_string() == "game/f1_career_2025",
              "career.content: a 2025-model call records game/f1_career_2025");
        older::ContentDesc back;
        std::string why;
        check(older::content_from("ex.addon/f1_1990", rec, swofty::json::Value(), back, why) && why.empty(),
              "career.content: the record a call gives reads back as a descriptor");
        check(back.id == call.id && back.title == call.title && back.description == call.description &&
                  back.data_group == call.data_group && back.suffix == call.suffix && back.mode == call.mode &&
                  back.model == call.model && back.car_group == call.car_group && back.badge == call.badge &&
                  back.entitlement == call.entitlement && back.careers == call.careers,
              "career.content: a record with the call's fields gives the call's descriptor, field for field");
    }

    {
        swofty::json::Value open = swofty::json::parse(
            R"({"type":"object","additionalProperties":true,"x-registry":"career.mode","x-version":"1.0","x-applies":"boot"})");
        check(st.declare(older::MODE, open, true) == SWOFTY_ASSET_OK, "career.mode declared for the data-file cases");
        st.set_order({"ex.data", "other.addon"});
        const char *file = R"({
            "career.mode": {
                "f1_2012": { "data_group": "f1_career_2012", "save_container": "driver_career_2012",
                             "title": "lng_f1_2012_title" }
            },
            "career.content": {
                "f1_2012": { "mode": "ex.data/f1_2012", "suffix": "12", "careers": "both", "badge": 3 },
                "f1_2012_short": { "mode": "f1_2012", "data_group": "f1_career_2012b", "suffix": "12B",
                                   "title": { "literal": "F1 2012" }, "description": "lng_f1_2012b_desc",
                                   "model": "game/f1_career_2025", "save_container": "driver_career_2012b" },
                "f1-1990": { "mode": null, "tile": null, "model": "game/f1_career_2026", "calendar": null,
                             "after_last": "market", "title": "lng_f1_1990_title", "description": "lng_f1_1990_desc",
                             "data_group": "f1_career_1990", "suffix": "90", "save_container": "driver_career_1990",
                             "car_group": 2, "badge": 0, "entitlement": "", "careers": "driver" },
                "bare": { "data_group": "f1_career_bare" },
                "lost_mode": { "mode": "ex.data/nothing", "suffix": "LM" },
                "odd_model": { "data_group": "f1_career_odd", "suffix": "OD", "save_container": "odd", "title": "t",
                               "model": "other.addon/f1_career_odd" }
            }
        })";
        check(load_data_file("ex.data", "registry/career.json", file) == SWOFTY_ASSET_OK,
              "career.content: a data file adds career.mode and career.content records");
        auto from = [&](const char *id, older::ContentDesc &d, std::string &why) {
            swofty::json::Value rec = older::effective(st, older::CONTENT, id);
            std::string mode = older::content_mode_id(id, rec);
            return older::content_from(id, rec, mode.empty() ? swofty::json::Value() : older::effective(st, older::MODE, mode),
                                       d, why);
        };
        auto passes = [](const older::ContentDesc &d) {
            runtime::content::Desc c{d.id,    d.title,     d.description, d.data_group, d.suffix, d.mode,
                                     d.entitlement, d.model, d.car_group, d.badge,     d.careers};
            return runtime::content::check(c, [](const char *, const std::string &) { return false; });
        };
        bool listed = false;
        for (auto &id : older::in_load_order(st, older::CONTENT))
            listed = listed || id == "ex.data/f1_2012";
        check(listed, "career.content: a data-file record is listed with the call's records");
        older::ContentDesc d;
        std::string why;
        check(from("ex.data/f1_2012", d, why), "career.content: a data-file record with a mode makes a tile");
        check(d.id == "f1_2012" && d.data_group == "f1_career_2012" && d.mode == "driver_career_2012" &&
                  d.title == "lng_f1_2012_title" && d.description == "lng_f1_2012_title" && d.suffix == "12" &&
                  d.model == runtime::content::MODEL_2026 && d.careers == runtime::content::CAREER_ALL && d.badge == 3 &&
                  d.car_group == -1,
              "career.content: data group, save container and title come from the career.mode record, the description "
              "from the title, the model defaults to 2026");
        check(passes(d).empty(), "career.content: the data-file descriptor passes the call's check");
        check(from("ex.data/f1_2012_short", d, why) && d.title == "F1 2012" && d.data_group == "f1_career_2012b" &&
                  d.mode == "driver_career_2012b" && d.model == runtime::content::MODEL_2025 &&
                  d.description == "lng_f1_2012b_desc" && passes(d).empty(),
              "career.content: a bare mode key names the record's own addon's mode; the record's fields win over it");
        check(from("ex.data/f1-1990", d, why) && d.id == "f1-1990" && d.car_group == 2 && d.mode == "driver_career_1990" &&
                  passes(d).empty(),
              "career.content: the full schema example makes a tile with no mode record");
        check(!from("ex.data/bare", d, why) && why.find("save_container") != std::string::npos,
              "career.content: a record with only a data group makes no tile and says what is missing");
        check(!from("ex.data/lost_mode", d, why) && why.find("ex.data/nothing is no career.mode record") != std::string::npos,
              "career.content: a mode that names no career.mode record is named");
        check(!from("ex.data/odd_model", d, why) && why.find("model other.addon/f1_career_odd") != std::string::npos,
              "career.content: a model other than the two stock careers makes no tile");
        check(layer_patch(st, "other.addon", older::CONTENT, "ex.data/f1_2012", R"({"title":"lng_retitled","badge":9})") ==
                  SWOFTY_ASSET_OK,
              "career.content: another addon layers a data-file record");
        check(from("ex.data/f1_2012", d, why) && d.title == "lng_retitled" && d.description == "lng_retitled" &&
                  d.badge == 9,
              "career.content: the tile reads the layered data-file record");
    }

    {
        older::HubTab t;
        t.addon = "ex.addon";
        t.name = "progressive";
        t.title = "Progressive";
        t.group = "prog";
        t.options = {"a", "b"};
        older::put(st, t.addon, older::HUB_TAB, t.name, older::hub_record(t), "test");
        older::HubTab back = older::hub_from("ex.addon/progressive", older::effective(st, older::HUB_TAB, "ex.addon/progressive"));
        check(back.addon == t.addon && back.name == t.name && back.title == t.title && back.group == t.group &&
                  back.options == t.options,
              "hub.tab: the record reads back field for field");
        layer_patch(st, "other.addon", older::HUB_TAB, "ex.addon/progressive", R"({"title":"Renamed"})");
        back = older::hub_from("ex.addon/progressive", older::effective(st, older::HUB_TAB, "ex.addon/progressive"));
        check(back.title == "Renamed" && back.group == "prog", "hub.tab: a layer retitles the tab");
    }

    {
        std::vector<older::WizardEntry> given = {{older::W_START, "g", "", "", 0, 0},
                                                 {older::W_TILE_ROUTE, "g", "x", "y", 0, 0},
                                                 {older::W_STEP, "g", "", "", 3, 5},
                                                 {older::W_OPTION, "g", "", "", 1, 2},
                                                 {older::W_CONFIRM_ROUTE, "g", "p", "q", 0, 0},
                                                 {older::W_TAB, "g", "tab", "", 0, 0}};
        older::put(st, "ex.addon", older::WIZARD, "g", older::wizard_record("g", given), "test");
        auto back = older::wizard_entries(older::effective(st, older::WIZARD, "ex.addon/g"));
        bool same = back.size() == given.size();
        for (std::size_t i = 0; same && i < back.size(); i++)
            same = back[i].kind == given[i].kind && back[i].from == given[i].from && back[i].to == given[i].to &&
                   back[i].a == given[i].a && back[i].b == given[i].b;
        check(same, "career.wizard: every entry reads back in order");
        layer_patch(st, "other.addon", older::WIZARD, "ex.addon/g", R"({"starts_in_f1":false})");
        back = older::wizard_entries(older::effective(st, older::WIZARD, "ex.addon/g"));
        check(back.size() == given.size() - 1 && back[0].kind == older::W_TILE_ROUTE, "career.wizard: a layer turns the start off");
        older::put(st, "ex.addon", older::WIZARD, "g", older::wizard_record("g", {}), "test");
        check(older::wizard_entries(older::effective(st, older::WIZARD, "ex.addon/g")).empty(),
              "career.wizard: a rewrite replaces the addon's state");
    }

    {
        older::CharacterState s;
        s.group = "g";
        s.swaps = {{"a", "b"}, {"c", "d"}};
        s.mutes = {"m"};
        s.strings = {{"en", {"k", "v"}}, {"fr", {"k2", "v2"}}};
        older::put(st, "ex.addon", older::CHARACTER, "g", older::character_record(s), "test");
        older::CharacterState back = older::character_from(older::effective(st, older::CHARACTER, "ex.addon/g"));
        check(back.group == s.group && back.swaps == s.swaps && back.mutes == s.mutes && back.strings.size() == 2,
              "career.character: swaps, mutes and strings read back");
        layer_patch(st, "other.addon", older::CHARACTER, "ex.addon/g", R"({"mute":["m","n"]})");
        back = older::character_from(older::effective(st, older::CHARACTER, "ex.addon/g"));
        check(back.mutes.size() == 2 && back.swaps == s.swaps, "career.character: a layer extends the mute list");
    }

    {
        check(older::string_layer(st, "ex.addon", "KEY_A", swofty::json::parse(R"({"en":"hello"})")) == SWOFTY_ASSET_OK,
              "lang.string: lang::set writes the caller's layer");
        auto texts = older::string_texts(older::effective(st, older::STRING, "game/KEY_A"));
        check(texts.size() == 1 && texts[0].first == "en" && texts[0].second == "hello",
              "lang.string: the effective record carries the text");
        older::string_layer(st, "ex.addon", "KEY_A", swofty::json::Value());
        check(older::string_texts(older::effective(st, older::STRING, "game/KEY_A")).empty(),
              "lang.string: clearing the layer restores the game's text");
        bool listed = false;
        for (auto &id : older::string_ids(st))
            listed = listed || id == "game/KEY_A";
        check(listed, "lang.string: a key once set stays listed");
    }

    {
        std::vector<older::OptionState> states = {{"Off", false}, {"On", true}};
        older::put(st, "ex.addon", older::OPTION, "follow", older::option_record("toggle", states, 1), "test");
        const swofty::json::Value rec = older::effective(st, older::OPTION, "ex.addon/follow");
        check(older::option_current(rec, states, 1) == 1, "addon.option: with no layer the player's choice stands");
        layer_patch(st, "other.addon", older::OPTION, "ex.addon/follow", R"({"value":false})");
        check(older::option_current(older::effective(st, older::OPTION, "ex.addon/follow"), states, 1) == 0,
              "addon.option: a layered value picks the state");
    }

    std::printf(failures ? "older test: %d FAILED\n" : "older test: ALL PASSED\n", failures);
    return failures ? 1 : 0;
}
