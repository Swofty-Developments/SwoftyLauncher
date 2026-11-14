#include "tt_barred_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace b = runtime::tt_barred;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    auto names = b::names_of(
        R"({"tt_boards":true,"tt_barred":[{"id":"kizikafuka.ai-drive","name":"AI Drive"},{"id":"apoc.race-broadcast","name":"Race Broadcast"}]})");
    expect(names == std::vector<std::string>{"AI Drive", "Race Broadcast"}, "names come in tt.json's order");
    expect(b::line_of(names) == "Not allowed in Time Trial, laps won't count: AI Drive, Race Broadcast",
           "the line names every barred addon");
    expect(b::line_of({}).empty(), "no names, no line");

    expect(b::names_of(R"({"tt_barred":[{"id":"a.b","name":""},{"id":"a.b"},"Plain",{"name":"  Plain "},{},7]})") ==
               std::vector<std::string>{"a.b", "Plain"},
           "a blank name falls back to the id, strings count, repeats and junk are left out");
    expect(b::names_of(R"({"tt_boards":true})").empty(), "no tt_barred key, no names");
    expect(b::names_of(R"({"tt_barred":{"id":"a.b"}})").empty(), "tt_barred that is not a list is ignored");
    expect(b::names_of("").empty() && b::names_of("not json").empty(), "a missing or broken file has no names");

    std::string many = R"({"tt_barred":[)";
    for (int i = 0; i < 20; i++)
        many += std::string(i ? "," : "") + "\"n" + std::to_string(i) + "\"";
    many += "]}";
    expect(b::names_of(many).size() == b::MAX_NAMES, "at most MAX_NAMES names");

    std::string longer(100, 'x');
    auto cut = b::names_of(R"({"tt_barred":[")" + longer + R"("]})");
    expect(cut.size() == 1 && cut[0].size() == b::MAX_NAME + 3 && cut[0].ends_with("..."), "a long name is cut");
    expect(b::clean("a\tb\n\nc") == "a b c", "control characters read as single spaces");

    b::Gate g;
    expect(!g.step(true, false, 1000), "not shown the moment Time Trial starts");
    expect(!g.step(true, false, 1000 + b::SHOW_AFTER_MS - 1), "not shown before SHOW_AFTER_MS");
    expect(g.step(true, false, 1000 + b::SHOW_AFTER_MS), "shown once Time Trial held SHOW_AFTER_MS");
    expect(g.step(false, false, 2000), "a blank mode read does not hide it at once");
    expect(g.step(true, false, 2200), "Time Trial again before HIDE_AFTER_MS keeps it");
    expect(g.step(false, false, 3000) && g.step(false, false, 3000 + b::HIDE_AFTER_MS - 1), "kept until HIDE_AFTER_MS out of it");
    expect(!g.step(false, false, 3000 + b::HIDE_AFTER_MS), "hidden after HIDE_AFTER_MS out of Time Trial");

    b::Gate h;
    h.step(true, false, 10);
    expect(h.step(true, false, 10 + b::SHOW_AFTER_MS), "shown in Time Trial");
    expect(!h.step(true, true, 10 + b::SHOW_AFTER_MS + 1), "the main menu hides it at once, whatever the mode read says");
    expect(!h.step(true, false, 10 + b::SHOW_AFTER_MS + 2), "leaving the main menu starts the wait again");
    expect(h.step(true, false, 10 + 2 * b::SHOW_AFTER_MS + 2), "and it shows after SHOW_AFTER_MS");

    b::Gate z;
    expect(!z.step(true, false, 0) && z.step(true, false, b::SHOW_AFTER_MS), "a clock at zero still counts");

    runtime::main_menu::HomeView v;
    v.screen("fe_home");
    expect(v.home, "fe_home is the main menu");
    v.screen("fe_planes");
    expect(v.home, "an overlay screen leaves the main menu answer");
    v.screen("time_trial_select");
    expect(!v.home, "another full screen is not the main menu");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
