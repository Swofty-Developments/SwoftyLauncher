// Host unit test for ui.option (core/apply/content/option_tags.hpp): the stock option and context tag reads, the
// records seeded from them, and the set operations a layered record turns into, over a hand-made options_system and,
// when a stock game database path is given, over the real one.
#include "core/apply/content/option_tags.hpp"

#include <cstdio>
#include <fstream>
#include <iterator>
#include <map>
#include <string>

namespace content = swofty_core::apply::content;
namespace ot = swofty_core::apply::content::option_tags;
namespace gvdb = swofty_core::apply::content::gvdb;
using swofty::json::Value;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

struct Db {
    std::map<std::string, std::string> keys;

    void option(const std::string &folder, int idx, const std::string &name,
                const std::vector<std::pair<std::string, std::string>> &rows)
    {
        std::string at = folder + ".option_" + std::to_string(idx);
        keys[at + "._metadata_"] = name;
        keys[at + "._additional_tags_instancecount_"] = std::to_string(rows.size());
        for (std::size_t i = 0; i < rows.size(); i++) {
            keys[at + ".additional_tags_" + std::to_string(i) + "._metadata_"] = rows[i].first;
            keys[at + ".additional_tags_" + std::to_string(i) + ".tags"] = rows[i].second;
        }
    }

    void tags(int folder, const std::string &kind, const std::string &word, const std::vector<std::string> &values)
    {
        std::string at = std::string(ot::CONTEXT) + ".tag_folder_" + std::to_string(folder);
        keys[at + "._metadata_"] = kind;
        keys[at + "._" + word + "_instancecount_"] = std::to_string(values.size());
        for (std::size_t i = 0; i < values.size(); i++)
            keys[at + "." + word + "_" + std::to_string(i) + "._metadata_"] = values[i];
    }

    ot::Read read() const
    {
        return [this](const std::string &p) -> std::optional<std::string> {
            auto it = keys.find(p);
            if (it == keys.end())
                return std::nullopt;
            return it->second;
        };
    }

    content::rows::Base base() const
    {
        return [this](uint64_t key) -> std::optional<content::rows::Rec> {
            for (auto &[path, text] : keys)
                if (gvdb::fnv64(path) == key)
                    return content::rows::Rec{18, text};
            return std::nullopt;
        };
    }
};

static Db fixture()
{
    Db db;
    std::string g = "options_system.options.game";
    db.keys[g + "._options_folder_instancecount_"] = "1";
    std::string f = g + ".options_folder_0";
    db.keys[f + "._option_instancecount_"] = "3";
    db.option(
        f, 0, "parc_ferme_rules",
        {{"game_state", "game_state,FrontEnd,PreSession"}, {"game_setup_mode", "game_setup_mode,gamesetup_f1,gamesetup_f2"}});
    db.option(f, 1, "drift_assist", {{"game_setup_mode", "game_setup_mode"}});
    db.option(f, 2, "formation_lap", {{"game_state", "game_state,FrontEnd"}});
    db.keys["options_system.context._tag_folder_instancecount_"] = "2";
    db.tags(0, "game_state", "game_state", {"FrontEnd", "PreSession", "InSession"});
    db.tags(1, "game_setup_mode", "game_setup_mode_tag", {"gamesetup_f1", "gamesetup_f2", "gamesetup_f1_1990"});
    return db;
}

static bool has(const std::vector<std::string> &lines, const std::string &needle)
{
    for (auto &l : lines)
        if (l.find(needle) != std::string::npos)
            return true;
    return false;
}

static std::string tags_of(content::rows::Engine &e, const std::string &option_row)
{
    return e.text("options_system.options.game.options_folder_0." + option_row + ".tags").value_or("<none>");
}

static void test_stock()
{
    Db db = fixture();
    auto all = ot::stock_options(db.read());
    check(all.size() == 3 && all[0].name == "parc_ferme_rules" && all[0].section == "game" && all[0].rows.size() == 2,
          "the stock read finds the options of a section and their tag rows");
    check(all[0].rows[1].kind == "game_setup_mode" &&
              all[0].rows[1].tags == std::vector<std::string>{"gamesetup_f1", "gamesetup_f2"},
          "a row's first item is its kind, the rest its tags");
    check(all[1].rows.size() == 1 && all[1].rows[0].tags.empty(), "a row with only its kind holds no tags");
    auto ctx = ot::context_tags(db.read());
    check(ctx.size() == 6 && ctx["gamesetup_f1_1990"] == "game_setup_mode" && ctx["PreSession"] == "game_state",
          "the context lists every tag value with its folder");
    Value rec = ot::stock_record(all[0]);
    check(rec["id"].as_string() == "parc_ferme_rules" && rec["restriction_tags"].size() == 4 &&
              rec["restriction_tags"][2].as_string() == "gamesetup_f1",
          "the seeded record is the option's flat tag list in row order");
}

static void test_add_and_remove()
{
    Db db = fixture();
    content::rows::Engine e(db.base());
    ot::Result r = ot::apply_wants(e, {{"swofty.f1-1990",
                                        "swofty.f1-1990/parc",
                                        "game/parc_ferme_rules",
                                        {"FrontEnd", "PreSession", "gamesetup_f1", "gamesetup_f2", "gamesetup_f1_1990"},
                                        true}});
    check(r.refused == 0 && r.options == 1 && r.added == 1 && r.removed == 0, "one tag added: " + std::to_string(r.added));
    check(tags_of(e, "option_0.additional_tags_1") == "game_setup_mode,gamesetup_f1,gamesetup_f2,gamesetup_f1_1990",
          "the tag is appended to the option's row of its kind, in the kind's own row");
    check(tags_of(e, "option_0.additional_tags_0") == "game_state,FrontEnd,PreSession", "the other row is untouched");
    check(e.writes().size() == 1 && e.writes()[0].owner == "swofty.f1-1990" && e.writes()[0].registry == "ui.option",
          "the write is attributed to the addon");

    ot::Result later = ot::apply_wants(
        e, {{"swofty.later", "game/parc_ferme_rules", "parc_ferme_rules", {"FrontEnd", "PreSession", "gamesetup_f1"}, true}});
    check(later.refused == 0 && later.removed == 2 && later.added == 0, "a later layer removes tags");
    check(tags_of(e, "option_0.additional_tags_1") == "game_setup_mode,gamesetup_f1",
          "the removed tags are gone, the kind stays");

    ot::Result again = ot::apply_wants(
        e, {{"swofty.later", "game/parc_ferme_rules", "parc_ferme_rules", {"FrontEnd", "PreSession", "gamesetup_f1"}, true}});
    check(again.options == 0 && again.added == 0 && again.removed == 0, "the same list again changes nothing");

    ot::Result extra = ot::apply_wants(e, {{"a.b", "a.b/more", "formation_lap", {"PreSession", "InSession"}, false}});
    check(extra.added == 2 && tags_of(e, "option_2.additional_tags_0") == "game_state,FrontEnd,PreSession,InSession",
          "an addon's own record adds its tags and removes none");
    ot::Result none = ot::apply_wants(e, {{"a.b", "a.b/less", "formation_lap", {}, false}});
    check(none.removed == 0 && none.added == 0, "an addon record with no tags removes nothing");
}

static void test_refusals()
{
    Db db = fixture();
    content::rows::Engine e(db.base());
    ot::Result r = ot::apply_wants(e, {{"a.b", "a.b/x", "game/parc_ferme_rule", {"FrontEnd"}, false}});
    check(r.refused == 1 && r.options == 0 && has(r.lines, "no option \"game/parc_ferme_rule\"") &&
              has(r.lines, "similar: parc_ferme_rules") && has(r.lines, "cannot declare a new option"),
          "an unknown option is refused with the nearest names");
    r = ot::apply_wants(e, {{"a.b", "a.b/x", "career/parc_ferme_rules", {"FrontEnd"}, false}});
    check(r.refused == 1 && has(r.lines, "is in the game section, not career"), "the wrong section is refused");
    r = ot::apply_wants(e, {{"a.b", "a.b/x", "parc_ferme_rules", {"PreSession", "gamesetup_nope"}, false}});
    check(r.refused == 1 && has(r.lines, "\"gamesetup_nope\" is not a value of any options_system.context folder") &&
              has(r.lines, "gamesetup_f1_1990") &&
              tags_of(e, "option_0.additional_tags_1") == "game_setup_mode,gamesetup_f1,gamesetup_f2",
          "an unknown tag refuses the whole record and writes nothing");
    r = ot::apply_wants(e, {{"a.b", "a.b/x", "formation_lap", {"gamesetup_f1"}, false}});
    check(r.refused == 1 && has(r.lines, "has no game_setup_mode tag row") && has(r.lines, "(it has game_state)"),
          "a tag whose kind the option has no row for is refused");
    check(e.writes().empty(), "nothing was written by any refusal");
}

static void test_real(const char *path)
{
    std::ifstream in(path, std::ios::binary);
    std::string bytes((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    gvdb::File file;
    if (!file.load(std::move(bytes))) {
        check(false, std::string("the stock database loads: ") + path);
        return;
    }
    ot::Read read = [&file](const std::string &p) -> std::optional<std::string> {
        auto r = file.get(gvdb::fnv64(p));
        if (!r)
            return std::nullopt;
        return r->text;
    };
    auto all = ot::stock_options(read);
    auto ctx = ot::context_tags(read);
    auto parc = std::find_if(all.begin(), all.end(), [](const ot::StockOption &o) { return o.name == "parc_ferme_rules"; });
    check(all.size() == 212, "the stock database holds 212 options in four sections, found " + std::to_string(all.size()));
    check(parc != all.end() && parc->section == "game", "parc_ferme_rules is a game section option");
    auto kinds = std::map<std::string, int>();
    for (auto &o : all)
        for (auto &r : o.rows)
            for (auto &t : r.tags)
                if (r.kind == "game_setup_mode")
                    kinds[t]++;
    check(kinds["gamesetup_f1"] > 20 && kinds["gamesetup_f2"] > 20 && kinds["gamesetup_classic"] > 0 &&
              kinds["gamesetup_f1w"] > 20,
          "the stock game_setup_mode tags on options are gamesetup_f1, f2, f2_21, f1_esports, f1w, classic and multiclass");
    check(ctx["gamesetup_f1"] == "game_setup_mode" && ctx["gamesetup_multiclass"] == "game_setup_mode" &&
              ctx["FrontEnd"] == "game_state" && ctx["Unranked"] == "multiplayer_context" && ctx["tv_pod"] == "selected_camera",
          "every tag value of an option row is a context value, with its folder as the row kind");
    std::size_t unknown = 0;
    for (auto &o : all)
        for (auto &r : o.rows)
            for (auto &t : r.tags)
                unknown += !ctx.count(t);
    check(unknown == 56, "56 stock tag entries name only export_for_unranked_templates, a folder without values");
    std::printf("      parc_ferme_rules tags: %s\n", parc == all.end() ? "-" : ot::join(ot::flat_tags(*parc)).c_str());
}

int main(int argc, char **argv)
{
    test_stock();
    test_add_and_remove();
    test_refusals();
    if (argc > 1)
        test_real(argv[1]);
    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
