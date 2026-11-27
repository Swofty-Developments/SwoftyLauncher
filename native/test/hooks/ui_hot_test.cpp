// Host test for the hot UI points lang.lookup and ui.vm.resolve (hooks/hot_table.hpp, hooks/ui_hot.hpp) and the
// view-model path rule ui::set_text/set_bool share with the core (frontend/vm_path.hpp): handler order and the
// text or node a handler supplies, the no-handler pass-through, the per-key table that runs the handlers once per
// key and epoch, the hot point's link (installed with the first subscriber, removed with the last), and the cost of
// the cached and no-subscriber paths. native/test/run_tests.sh ui_hot builds and runs it.
#include "core/frontend/vm_path.hpp"
#include "core/hooks/dispatch.hpp"
#include "core/hooks/hot_table.hpp"
#include "core/hooks/ui_hot.hpp"

#include <chrono>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

using namespace swofty_core::hooks;
namespace ev = swofty::ui::events;

static int fails;
static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static std::vector<std::string> trail;
static std::string seen_text, seen_key, seen_language;

struct Tag {
    std::string name, prefix, text;
    void *node = nullptr;
};

static int lang_handler(void *user, SwoftyHookCall *c)
{
    auto *tag = static_cast<Tag *>(user);
    auto *e = static_cast<ev::LangLookup *>(c->payload);
    trail.push_back(tag->name);
    seen_key = std::string(e->key());
    seen_language = std::string(e->language());
    seen_text = std::string(e->text());
    if (e->key().substr(0, tag->prefix.size()) == tag->prefix)
        e->set_text(tag->text);
    return 0;
}

static int vm_handler(void *user, SwoftyHookCall *c)
{
    auto *tag = static_cast<Tag *>(user);
    auto *e = static_cast<ev::VmResolve *>(c->payload);
    trail.push_back(tag->name);
    if (e->path().substr(0, tag->prefix.size()) == tag->prefix)
        e->node = tag->node;
    return 0;
}

struct Link final : GameLink {
    int installs = 0, uninstalls = 0;
    bool on = false;
    bool install(bool, const std::string &) override
    {
        installs++;
        return on = true;
    }
    void uninstall() override
    {
        uninstalls++;
        on = false;
    }
    bool installed() const override { return on; }
};

static std::string joined()
{
    std::string s;
    for (auto &t : trail)
        s += (s.empty() ? "" : " ") + t;
    trail.clear();
    return s;
}

static int redirect_handler(void *user, SwoftyHookCall *c)
{
    auto *tag = static_cast<Tag *>(user);
    auto *e = static_cast<ev::VmResolve *>(c->payload);
    trail.push_back(tag->name);
    if (e->path().substr(0, tag->prefix.size()) == tag->prefix)
        trail.push_back(std::to_string(int(e->use_node_of(tag->text))));
    return 0;
}

static void test_lang()
{
    Table t;
    Link link;
    t.declare_game(ui_hot::LANG_POINT, SWOFTY_POINT_MODIFY, ev::LangLookup::version, &link, true);
    t.arm();
    expect(!ui_hot::fire_lang(t, "lng_x", "eng", "Stock"), "lang.lookup: no handler leaves the text alone");
    expect(link.installs == 0, "lang.lookup: nothing installed without a subscriber");
    Tag low{"low", "lng_", "Low"}, high{"high", "lng_team", "High"}, other{"other", "lng_zzz", "Other"};
    uint32_t tok = 0, tok_high = 0;
    t.attach("a", ui_hot::LANG_POINT, SWOFTY_PHASE_BEFORE, 0, "", lang_handler, &low, &tok);
    expect(link.installs == 1 && link.on, "lang.lookup: the first subscriber installs the link");
    t.attach("b", ui_hot::LANG_POINT, SWOFTY_PHASE_BEFORE, 10, "", lang_handler, &high, &tok_high);
    t.attach("c", ui_hot::LANG_POINT, SWOFTY_PHASE_BEFORE, -5, "", lang_handler, &other, &tok);
    expect(link.installs == 1, "lang.lookup: a second subscriber does not install again");
    trail.clear();
    auto r = ui_hot::fire_lang(t, "lng_team_alpine", "eng", "Alpine");
    expect(r && *r == "Low", "lang.lookup: the last handler to run wins (the lowest priority runs last)");
    expect(joined() == "high low other", "lang.lookup: before handlers run by priority, highest first");
    expect(seen_key == "lng_zzz" || seen_key == "lng_team_alpine", "lang.lookup: the handler reads the key");
    seen_text.clear();
    ui_hot::fire_lang(t, "lng_team_alpine", "fre", "Alpine");
    expect(seen_language == "fre" && seen_text == "Low",
           "lang.lookup: the language is passed, and text shows the previous handler's text");
    auto none = ui_hot::fire_lang(t, "other_key", "eng", "Stock");
    expect(!none, "lang.lookup: a key no handler takes passes through");
    std::string huge(5000, 'x');
    Table t2;
    t2.declare_game(ui_hot::LANG_POINT, SWOFTY_POINT_MODIFY, ev::LangLookup::version, &link, true);
    Tag big{"big", "", huge};
    t2.attach("a", ui_hot::LANG_POINT, SWOFTY_PHASE_BEFORE, 0, "", lang_handler, &big, &tok);
    expect(!ui_hot::fire_lang(t2, "k", "eng", "S"), "lang.lookup: a text over the cap is refused and passes through");
    t.detach(tok_high);
    t.detach(tok);
    expect(link.uninstalls == 0, "lang.lookup: the link stays while a subscriber is left");
}

static void test_vm()
{
    Table t;
    Link link;
    t.declare_game(ui_hot::VM_POINT, SWOFTY_POINT_MODIFY, ev::VmResolve::version, &link, true);
    t.arm();
    int slot_a = 1, slot_b = 2;
    auto ok = [](void *) { return true; };
    expect(!ui_hot::fire_vm(t, "hud_progressive_x", ok), "ui.vm.resolve: no handler supplies no node");
    Tag one{"one", "hud_progressive_", "", &slot_a}, two{"two", "hud_progressive_team", "", &slot_b};
    uint32_t tok1 = 0, tok2 = 0;
    t.attach("a", ui_hot::VM_POINT, SWOFTY_PHASE_BEFORE, 0, "", vm_handler, &one, &tok1);
    t.attach("b", ui_hot::VM_POINT, SWOFTY_PHASE_BEFORE, -1, "", vm_handler, &two, &tok2);
    trail.clear();
    expect(ui_hot::fire_vm(t, "hud_progressive_title", ok) == &slot_a, "ui.vm.resolve: a handler supplies the node for its path");
    expect(joined() == "one two", "ui.vm.resolve: handlers run in priority order");
    expect(ui_hot::fire_vm(t, "hud_progressive_team_1", ok) == &slot_b, "ui.vm.resolve: the later handler's node wins");
    expect(!ui_hot::fire_vm(t, "career.hub.title", ok), "ui.vm.resolve: another path passes through");
    expect(!ui_hot::fire_vm(t, "hud_progressive_title", [](void *) { return false; }),
           "ui.vm.resolve: a node that does not validate is dropped, the game resolves its own");
    t.detach(tok1);
    t.detach(tok2);
    expect(link.uninstalls == 1 && !link.on, "ui.vm.resolve: the last subscriber leaving removes the link");

    Table r;
    Link rlink;
    r.declare_game(ui_hot::VM_POINT, SWOFTY_POINT_MODIFY, ev::VmResolve::version, &rlink, true);
    r.arm();
    int node_title = 3;
    std::vector<std::string> asked;
    auto node_of = [&](const char *p) -> void * {
        asked.push_back(p);
        return std::strcmp(p, "career.hub.title") == 0 ? &node_title : nullptr;
    };
    Tag to_title{"redir", "hud_progressive_title", "career.hub.title"}, to_gone{"gone", "hud_progressive_gone", "no.such.path"};
    uint32_t tr1 = 0, tr2 = 0;
    r.attach("a", ui_hot::VM_POINT, SWOFTY_PHASE_BEFORE, 0, "", redirect_handler, &to_title, &tr1);
    r.attach("b", ui_hot::VM_POINT, SWOFTY_PHASE_BEFORE, -1, "", redirect_handler, &to_gone, &tr2);
    trail.clear();
    expect(ui_hot::fire_vm(r, "hud_progressive_title", ok, node_of) == &node_title,
           "ui.vm.resolve: use_node_of takes the game's node for another path as the node");
    expect(joined() == "redir 0 gone" && asked == std::vector<std::string>{"career.hub.title"},
           "ui.vm.resolve: use_node_of reports ok and asks the core only for the path the handler names");
    asked.clear();
    expect(!ui_hot::fire_vm(r, "hud_progressive_gone", ok, node_of),
           "ui.vm.resolve: a path that resolves to no node supplies none");
    expect(joined() == "redir gone 2", "ui.vm.resolve: use_node_of returns unavailable for a path with no node");
    expect(!ui_hot::fire_vm(r, "hud_progressive_title", ok), "ui.vm.resolve: with no resolver use_node_of supplies none");
    joined();
    r.detach(tr1);
    r.detach(tr2);
}

static void test_table()
{
    HotTable<const char *> hot;
    int builds = 0, resets = 0;
    size_t reset_entries = 0, reset_supplied = 0;
    auto reset = [&](const HotStats &s) {
        resets++;
        reset_entries = s.entries;
        reset_supplied = s.supplied;
    };
    auto build_for = [&](const char *text) {
        return [&, text]() -> std::pair<const char *, bool> {
            builds++;
            return {text, text != nullptr};
        };
    };
    const char *a = hot.resolve("k1", 1, build_for("A"), reset);
    const char *a2 = hot.resolve("k1", 1, build_for("B"), reset);
    const char *n = hot.resolve("k2", 1, build_for(nullptr), reset);
    const char *n2 = hot.resolve("k2", 1, build_for("C"), reset);
    expect(!strcmp(a, "A") && !strcmp(a2, "A") && !n && !n2 && builds == 2,
           "hot table: the handlers run once per key and epoch, and a no-answer is cached too");
    hot.resolve("k1", 2, build_for("D"), reset);
    expect(builds == 3 && resets == 1 && reset_entries == 2 && reset_supplied == 1,
           "hot table: a new epoch (content, language, view model) drops the table and rebuilds from the handlers");
    hot.clear();
    hot.resolve("k1", 2, build_for("E"), reset);
    expect(builds == 4 && hot.stats().entries == 1, "hot table: clear starts over");
}

static void test_path()
{
    frontend::vm_path::Split s;
    expect(frontend::vm_path::split("ui_menu_data.swofty.m1", s) && s.parent == "ui_menu_data.swofty" && s.leaf == "m1",
           "vm path: the last dot splits the leaf from the parent node path");
    expect(frontend::vm_path::split("view_model_data.career.hub.title", s) && s.parent == "view_model_data.career.hub" &&
               s.leaf == "title",
           "vm path: a deep path keeps every earlier dot in the parent");
    expect(!frontend::vm_path::split("title", s) && !frontend::vm_path::split(".title", s) && !frontend::vm_path::split("a.", s),
           "vm path: no parent, no leaf and a bare leaf are refused");
    expect(frontend::vm_path::settable("a.b") && !frontend::vm_path::settable("a. b") && !frontend::vm_path::settable("a[0].b") &&
               !frontend::vm_path::settable("#a.b"),
           "vm path: spaces, indexes and a '#' prefix are refused for a write");
}

static void test_cost()
{
    HotTable<const char *> hot;
    const char *keys[] = {"lng_team_alpine", "lng_team_ferrari", "lng_driver_max", "lng_track_monza", "lng_hud_speed", "lng_x"};
    for (auto k : keys)
        hot.resolve(k, 7, [] { return std::pair<const char *, bool>{nullptr, false}; });
    constexpr int N = 4000000;
    volatile size_t sink = 0;
    auto t0 = std::chrono::steady_clock::now();
    for (int i = 0; i < N; i++)
        sink = sink + (hot.resolve(keys[i % 6], 7, [] { return std::pair<const char *, bool>{nullptr, false}; }) != nullptr);
    auto t1 = std::chrono::steady_clock::now();
    Table none;
    none.declare(ui_hot::LANG_POINT, SWOFTY_POINT_MODIFY, 1, false, true);
    for (int i = 0; i < N; i++)
        sink = sink + none.handlers(ui_hot::LANG_POINT);
    auto t2 = std::chrono::steady_clock::now();
    double cached = std::chrono::duration<double, std::nano>(t1 - t0).count() / N;
    double subscribed_check = std::chrono::duration<double, std::nano>(t2 - t1).count() / N;
    printf("info: cached hot lookup %.1f ns, handlers() check %.1f ns\n", cached, subscribed_check);
    expect(cached < 500, "cost: a cached lookup stays under 500 ns");
}

int main()
{
    test_lang();
    test_vm();
    test_table();
    test_path();
    test_cost();
    printf(fails ? "ui_hot: %d FAILED\n" : "ui_hot: ALL PASSED\n", fails);
    return fails ? 1 : 0;
}
