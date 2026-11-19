// Host-native unit test for the addon settings' model (native/src/core/runtime/settings_logic.hpp): the requests.json
// text, the JSON of each state value, the option checks, the value an addon's read gets, the command hash and the
// read memo.
// Built and run by native/test/run_tests.sh settings_logic.

#include "core/runtime/settings_logic.hpp"

#include <cstdio>
#include <string>

namespace sm = runtime::settings_model;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

static json::Value value(const char *text)
{
    json::Value v;
    json::parse(text, v);
    return v;
}

static sm::Option option(const char *id, uint32_t type, std::initializer_list<const char *> values, size_t current)
{
    sm::Option o{id, type, {}, current, false, {}, {}};
    for (const char *v : values)
        o.states.push_back({value(v), std::string("text ") + v, std::string("cmd_") + id});
    return o;
}

static void json_text()
{
    CHECK(sm::quote("plain") == "\"plain\"");
    CHECK(sm::quote("a\"b\\c\n\x01") == "\"a\\\"b\\\\c\\u000a\\u0001\"");
    CHECK(sm::json_of(value("true")) == "true");
    CHECK(sm::json_of(value("false")) == "false");
    CHECK(sm::json_of(value("42")) == "42");
    CHECK(sm::json_of(value("-7")) == "-7");
    CHECK(sm::json_of(value("0.1")) == "0.1");
    CHECK(sm::json_of(value("2.5")) == "2.5");
    CHECK(sm::json_of(value("1e20")) == "1e+20");
    CHECK(sm::json_of(value("\"hard\"")) == "\"hard\"");
    CHECK(sm::json_of(value("null")) == "null");
    CHECK(sm::object({}) == "{}");
    CHECK(sm::object({"\"a\": 1", "\"b\": 2"}) == "{ \"a\": 1, \"b\": 2 }");
}

static void requests()
{
    std::vector<sm::AddonSettings> addons;
    CHECK(sm::requests_text(addons, "Default", 0, false) ==
          "{\n  \"schema\": 1,\n  \"profile\": \"Default\",\n  \"seq\": 0,\n  \"options\": {},\n  \"enabled\": {},\n"
          "  \"relaunch\": false\n}\n");
    sm::AddonSettings a{"example.hud", {}, {}, {}};
    a.options.push_back(option("mode", SWOFTY_SETTING_CHOICE, {"\"off\"", "\"compact\"", "\"full\""}, 2));
    a.options.push_back(option("scale", SWOFTY_SETTING_SLIDER, {"0.5", "1", "1.25"}, 2));
    a.options.push_back(option("show", SWOFTY_SETTING_TOGGLE, {"false", "true"}, 1));
    a.options[0].requested = a.options[1].requested = true;
    sm::AddonSettings b{"example.audio", {}, {}, {}};
    b.enabled = 0;
    sm::AddonSettings c{"example.cars", {}, {}, {}};
    c.options.push_back(option("pack", SWOFTY_SETTING_CHOICE, {"\"2024\""}, 0));
    c.enabled = 1;
    addons = {a, b, c};
    CHECK(sm::requests_text(addons, "League \"A\"", 17, true) ==
          "{\n  \"schema\": 1,\n  \"profile\": \"League \\\"A\\\"\",\n  \"seq\": 17,\n"
          "  \"options\": { \"example.hud\": { \"mode\": \"full\", \"scale\": 1.25 } },\n"
          "  \"enabled\": { \"example.audio\": false, \"example.cars\": true },\n  \"relaunch\": true\n}\n");
}

static void options()
{
    CHECK(!sm::invalid(option("show", SWOFTY_SETTING_TOGGLE, {"false", "true"}, 0)));
    CHECK(!sm::invalid(option("scale", SWOFTY_SETTING_SLIDER, {"0.5", "2"}, 1)));
    CHECK(!sm::invalid(option("mode", SWOFTY_SETTING_CHOICE, {"\"a\""}, 0)));
    CHECK(std::string(sm::invalid(option("x", 0, {"true"}, 0))) == "unknown type");
    CHECK(std::string(sm::invalid(option("x", SWOFTY_SETTING_TOGGLE, {"true"}, 1))) == "current is not a state index");
    CHECK(std::string(sm::invalid(option("x", SWOFTY_SETTING_TOGGLE, {}, 0))) == "current is not a state index");
    CHECK(std::string(sm::invalid(option("x", SWOFTY_SETTING_TOGGLE, {"true", "1"}, 0))) ==
          "a state value does not fit the type");
    CHECK(std::string(sm::invalid(option("x", SWOFTY_SETTING_CHOICE, {"\"a\"", "2"}, 0))) ==
          "a state value does not fit the type");
    CHECK(std::string(sm::type_names[SWOFTY_SETTING_SLIDER]) == "slider");

    sm::Option t = option("show", SWOFTY_SETTING_TOGGLE, {"false", "true"}, 0);
    SwoftySettingValue v = sm::value_of(t, 1);
    CHECK(v.type == SWOFTY_SETTING_TOGGLE && v.toggle == 1 && v.slider == 0 && !v.choice);
    sm::Option s = option("scale", SWOFTY_SETTING_SLIDER, {"0.5", "1.25"}, 0);
    v = sm::value_of(s, 1);
    CHECK(v.type == SWOFTY_SETTING_SLIDER && v.slider == 1.25 && !v.toggle && !v.choice);
    sm::Option c = option("mode", SWOFTY_SETTING_CHOICE, {"\"off\"", "\"full\""}, 0);
    v = sm::value_of(c, 1);
    CHECK(v.type == SWOFTY_SETTING_CHOICE && std::string(v.choice) == "full");
}

static void commands_and_memo()
{
    CHECK(sm::djb2("") == 5381u);
    CHECK(sm::djb2("a") == 5381u * 33 + 'a');
    CHECK(sm::djb2("swofty_hud_mode_full") == sm::djb2(std::string("swofty_hud_mode_full")));
    CHECK(sm::djb2("ab") != sm::djb2("ba"));

    sm::Memo m;
    size_t state = 9;
    CHECK(!m.at(1, state) && state == 0);
    m.keep(5, 2);
    CHECK(m.at(5, state) && state == 2);
    CHECK(!m.at(6, state));
    CHECK(!m.at(0, state));
    m.keep(4, 1);
    CHECK(m.at(5, state) && state == 2);
    m.keep(7, 3);
    CHECK(m.at(7, state) && state == 3);
    sm::Memo copy = m;
    CHECK(copy.at(7, state) && state == 3);
}

int main()
{
    json_text();
    requests();
    options();
    commands_and_memo();
    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("settings_logic: all checks passed\n");
    return 0;
}
