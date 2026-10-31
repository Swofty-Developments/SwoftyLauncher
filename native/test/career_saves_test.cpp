// Host unit test for career saves: the index model (career_saves_logic.hpp) and the SDK's career::saves calls
// over the in-memory fake table (fake_saves.hpp). run_events_test.sh builds and runs it.
#include "../src/core/career_saves_logic.hpp"

#include <swofty/fake_saves.hpp>

#include <cstdio>

namespace cs = runtime::career_saves;
using swofty::json::Value;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static void index_model()
{
    cs::Index ix;
    expect(ix.load(""), "an empty file loads as an empty index");
    ix.set_text("m:1", "mode", "driver_career");
    ix.set_text("m:1", "driver", "Max");
    ix.set_text("m:1", "driver", "");
    ix.set_int("m:1", "season", 2016);
    ix.need("m:1", {"a.b", "^1.0.0", "its calendar"});
    ix.need("m:1", {"a.b", "", "again"});
    ix.need("m:1", {"c.d", "", ""});
    ix.saved("m:1", 1700);
    std::string text = ix.to_json();
    cs::Index back;
    expect(back.load(text) && back.size() == 1, "the index reads back");
    const Value *c = back.find("m:1");
    expect(c && (*c)["driver"].as_string() == "Max" && (*c)["season"].as_int() == 2016, "fields survive; an empty value is not written");
    expect(c && (*c)["addons"].size() == 2 && (*c)["addons"][0]["need"].as_string() == "^1.0.0" &&
               (*c)["addons"][0]["why"].as_string() == "again",
           "a repeated need updates the addon's row and keeps the range");
    expect(c && (*c)["last_saved"].as_int() == 1700 && (*c)["status"].as_string() == "present", "saved stamps the time");
    expect(cs::Index().load("{\"careers\":[{\"career\":\"x\",\"profile\":\"p\",\"match\":{\"driver\":1}},{\"nope\":1}]}"),
           "the launcher's shape loads");
    cs::Index launcher;
    launcher.load("{\"careers\":[{\"career\":\"x\",\"profile\":\"p\",\"match\":{\"driver\":1}},{\"nope\":1}]}");
    launcher.set_text("x", "mode", "my_team");
    expect(launcher.size() == 1 && (*launcher.find("x"))["profile"].as_string() == "p" &&
               (*launcher.find("x"))["match"]["driver"].as_int() == 1,
           "fields the core does not own stay, and a record without an id is dropped");
    expect(!cs::Index().load("garbage"), "garbage is refused");
}

static void missing_and_back()
{
    cs::Index ix;
    ix.touch("a");
    ix.touch("b");
    ix.touch("c");
    auto gone = ix.reconcile({"a", "c"});
    expect(gone.size() == 1 && gone[0] == "b", "reconcile names the career the load did not find");
    expect(ix.reconcile({"a", "c"}).empty(), "a career already missing is not named again");
    expect(ix.missing_ids() == std::vector<std::string>{"b"}, "missing_ids lists it");
    ix.reconcile({"a", "b", "c"});
    expect(ix.missing_ids().empty(), "a career that comes back is present again");
    ix.reconcile({"a"});
    Value only = swofty::json::parse(ix.only_json(ix.missing_ids()));
    expect(only["careers"].size() == 2, "only_json holds the missing records");
}

static void data_files()
{
    expect(cs::safe_name("f1:7/../x") == "f1%3a7%2f..%2fx", "a career id becomes one safe folder name");
    expect(cs::data_file("m:1", "a.b") == "careers/data/m%3a1/a.b.json", "data file path");
    expect(cs::addon_id("swofty.progressive") && !cs::addon_id("../x") && !cs::addon_id("a/b") && !cs::addon_id(""), "addon ids");
    expect(!cs::plain_id("a/b") && cs::plain_id("driver_career:7731"), "career ids");
    std::string wrapped = cs::wrap_data(3, swofty::json::parse("{\"k\":[1,2]}"));
    uint32_t v = 0;
    std::string value;
    expect(cs::unwrap_data(wrapped, v, value) && v == 3 && value == "{\"k\":[1,2]}", "data round-trips with its version");
    expect(!cs::unwrap_data("{\"data\":1}", v, value) && !cs::unwrap_data("nope", v, value), "a file without a version is refused");
    expect(cs::label_of("g", "Max", "RB", 2016) == "g: Max, RB, 2016", "label");
}

static void sdk_over_fake()
{
    using namespace swofty;
    expect(!career::saves::available() && career::saves::index().empty() && career::saves::current().empty(),
           "without a table nothing is available");
    expect(career::saves::need("a.b") == career::Result::unavailable, "need is unavailable without a table");
    {
        fake::Saves saves;
        saves.install();
        expect(career::saves::available(), "the fake table is available");
        expect(career::saves::current().empty(), "no career loaded");
        saves.load("m:1", "driver_career", "g1");
        expect(career::saves::current() == "m:1", "current is the loaded career");
        expect(career::saves::need("a.b", "^1.2.0", "its calendar") == career::Result::ok, "need");
        career::saves::Description d;
        d.driver = "Max";
        d.season = 2016;
        d.round = 4;
        expect(career::saves::describe(d) == career::Result::ok, "describe");
        auto e = career::saves::find("m:1");
        expect(e && e->driver == "Max" && e->season == 2016 && e->round == 4 && e->content == "g1" && e->mode == "driver_career",
               "the index entry has what was described");
        expect(e && !e->addons.empty() && e->addons[0].id == "a.b" && e->addons[0].range == "^1.2.0" &&
                   e->addons[0].why == "its calendar",
               "the entry lists the need");
        auto mine = career::saves::data("a.b");
        expect(!mine.get("m:1"), "no data yet");
        expect(mine.set_current(json::parse("{\"x\":1}"), 2) == career::Result::ok, "set_current");
        auto got = mine.get_current();
        expect(got && got->version == 2 && got->value["x"].as_int() == 1, "get returns the version and the value");
        expect(career::saves::data("c.d").get("m:1") == std::nullopt, "another addon's data is separate");
        expect(mine.set("zzz", json::parse("1")) == career::Result::refused, "data for a career the index does not know is refused");
        expect(mine.set("m:1", json::parse("{}")) == career::Result::ok && mine.get("m:1")->version == 1, "the default version is 1");
        saves.save();
        expect(career::saves::find("m:1")->last_saved == saves.clock(), "save stamps last_saved");
        expect(career::saves::missing().empty(), "nothing missing yet");
        saves.drop("m:1");
        auto m = career::saves::missing();
        expect(m.size() == 1 && m[0].career == "m:1" && m[0].status == "missing", "a dropped career is listed missing");
        expect(mine.remove("m:1") == career::Result::ok && !mine.get("m:1"), "remove");
        std::string big(5000, 'x');
        saves.load("m:2");
        expect(career::saves::describe({.label = big}) == career::Result::ok && career::saves::find("m:2")->label == big,
               "a long index grows the buffer");
    }
    expect(!career::saves::available(), "the table goes with the fake");
}

int main()
{
    index_model();
    missing_and_back();
    data_files();
    sdk_over_fake();
    printf("career saves: %d failed\n", fails);
    return fails ? 1 : 0;
}
