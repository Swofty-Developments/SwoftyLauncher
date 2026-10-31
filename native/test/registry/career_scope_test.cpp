#include "../../src/core/registry/table.hpp"

#include <cstdio>
#include <string>

using namespace swofty_core::registry;

static int failures = 0;
#define CHECK(c, ...)                          \
    do {                                       \
        if (!(c)) {                            \
            std::printf("FAIL: " __VA_ARGS__); \
            std::printf("\n");                 \
            failures++;                        \
        }                                      \
    } while (0)

namespace {

Store &st = store();

std::string field(const char *registry, const char *id, const char *name)
{
    Record r = st.get(registry, id);
    if (r.status != Status::ok)
        return "(" + std::string(status_name(r.status)) + ")";
    return std::string(r.value[name].as_string());
}

void in_career(const char *group)
{
    Context c = st.context();
    c.career = group[0] ? std::string("career-") + group : std::string();
    c.data_group = group;
    st.set_context(c);
}

void patch(const char *addon, const char *registry, const char *id, const char *body, bool automatic)
{
    LayerSpec sp;
    sp.addon = addon;
    sp.registry = registry;
    sp.id = id;
    sp.automatic = automatic;
    sp.change.patch = swofty::json::parse(body);
    uint32_t handle = 0;
    CHECK(st.layer(sp, &handle) == 0, "%s layers %s", addon, id);
}

}

int main()
{
    st.declare("career.mode", swofty::json::parse(R"({"x-registry":"career.mode","x-applies":"boot","type":"object"})"));
    st.declare("career.content", swofty::json::parse(R"({"x-registry":"career.content","x-applies":"boot","type":"object"})"));
    st.declare("test.swap", swofty::json::parse(R"({"x-registry":"test.swap","x-applies":"read","type":"object"})"));
    st.declare("test.boot", swofty::json::parse(R"({"x-registry":"test.boot","x-applies":"boot","type":"object"})"));
    st.declare("career.calendar",
               swofty::json::parse(R"({"x-registry":"career.calendar","x-applies":"boot","x-data-group":true,"type":"object"})"));

    st.add("a.prog", "career.content", "prog", swofty::json::parse(R"({"data_group":"g_prog"})"), nullptr);
    st.add("b.classic", "career.mode", "cs", swofty::json::parse(R"({"data_group":"g_cs"})"), nullptr);
    st.add("e.both", "career.content", "e", swofty::json::parse(R"({"data_group":"g_e"})"), nullptr);

    st.add("x.base", "test.swap", "car", swofty::json::parse(R"({"v":"stock","a":"","b":"","c":"","d":"","s":""})"), nullptr);
    st.add("x.base", "test.boot", "car", swofty::json::parse(R"({"v":"stock"})"), nullptr);

    patch("a.prog", "test.swap", "x.base/car", R"({"a":"prog"})", true);
    patch("b.classic", "test.swap", "x.base/car", R"({"b":"classic"})", true);
    patch("c.plain", "test.swap", "x.base/car", R"({"c":"plain"})", true);
    patch("e.both", "test.swap", "x.base/car", R"({"d":"global"})", false);
    patch("a.prog", "test.boot", "x.base/car", R"({"v":"prog"})", true);

    CHECK(load_data_file("b.classic", "registry/layers.json",
                         R"({"test.swap":{"$select":[{"ids":"x.base/*","scope":"global","patch":{"s":"classic-global"}}]}})") == 0,
          "a select with scope global loads");
    CHECK(load_data_file("a.prog", "registry/layers.json",
                         R"({"test.swap":{"$select":[{"ids":"x.base/*","scope":"career","patch":{"s":"x"}}]}})") != 0,
          "a select with another scope is refused");

    st.add("a.prog", "career.calendar", "cal_prog", swofty::json::parse(R"({"data_group":"g_prog"})"), nullptr);
    st.add("b.classic", "career.calendar", "cal_cs", swofty::json::parse(R"({"data_group":"g_cs"})"), nullptr);
    st.add("x.base", "career.calendar", "cal_all", swofty::json::parse(R"({"seasons":3})"), nullptr);
    st.end_register();

    in_career("");
    CHECK(field("test.swap", "x.base/car", "a").empty(), "no career: the Progressive-style layer is off");
    CHECK(field("test.swap", "x.base/car", "b").empty(), "no career: the Classic-style layer is off");
    CHECK(field("test.swap", "x.base/car", "c") == "plain", "no career: an addon without a career mode keeps its layer");
    CHECK(field("test.swap", "x.base/car", "d") == "global", "no career: scope global keeps the layer of a career addon");
    CHECK(field("test.swap", "x.base/car", "s") == "classic-global", "no career: a select with scope global applies");
    CHECK(field("test.boot", "x.base/car", "v") == "prog", "a boot registry's layer is global");
    CHECK(field("career.calendar", "a.prog/cal_prog", "data_group") == "(no_record)", "no career: a scoped calendar is hidden");
    CHECK(field("career.calendar", "x.base/cal_all", "seasons") == "", "no career: a calendar with no data group is visible");
    CHECK(st.list("career.calendar", "").size() == 1, "no career: the calendar list holds the unscoped one only");

    in_career("g_prog");
    CHECK(field("test.swap", "x.base/car", "a") == "prog", "g_prog: Progressive's layer applies");
    CHECK(field("test.swap", "x.base/car", "b").empty(), "g_prog: Classic Career's layer does not");
    CHECK(field("test.swap", "x.base/car", "c") == "plain", "g_prog: the plain addon's layer applies");
    CHECK(field("test.swap", "x.base/car", "d") == "global", "g_prog: scope global applies");
    CHECK(field("test.swap", "x.base/car", "s") == "classic-global", "g_prog: Classic's select with scope global applies");
    CHECK(field("career.calendar", "a.prog/cal_prog", "data_group") == "g_prog", "g_prog: its own calendar is there");
    CHECK(field("career.calendar", "b.classic/cal_cs", "data_group") == "(no_record)", "g_prog: Classic's calendar is not");
    CHECK(st.list("career.calendar", "").size() == 2, "g_prog: its calendar and the unscoped one");

    in_career("g_cs");
    CHECK(field("test.swap", "x.base/car", "a").empty(), "g_cs: Progressive's layer does not apply");
    CHECK(field("test.swap", "x.base/car", "b") == "classic", "g_cs: Classic Career's layer applies");
    CHECK(field("career.calendar", "b.classic/cal_cs", "data_group") == "g_cs", "g_cs: its own calendar is there");
    CHECK(field("career.calendar", "a.prog/cal_prog", "data_group") == "(no_record)", "g_cs: Progressive's calendar is not");

    in_career("f1_career_2026");
    CHECK(field("test.swap", "x.base/car", "a").empty() && field("test.swap", "x.base/car", "b").empty(),
          "a stock career gets neither addon's layers");

    in_career("g_prog");
    CHECK(field("test.swap", "x.base/car", "a") == "prog", "going back into g_prog re-applies its layer");

    std::printf(failures ? "career scope: %d failure(s)\n" : "career scope: ok\n", failures);
    return failures ? 1 : 0;
}
