// Host test for the UI game points ui.era.tiles, ui.home.tiles, ui.screen.built, ui.route.open and
// ui.track_select.build (hooks/ui_points.hpp): the payload fields, the before/after order, the RouteOpen cancel, the
// site link (installed with the first subscriber, removed with the last), the catalogue rows, the two content tile
// points staying unfired, and the hide-only MFD page rules of hud.mfd.pages (frontend/mfd_pages_logic.hpp).
// native/test/run_tests.sh ui_points builds and runs it.
#include "core/frontend/mfd_pages_logic.hpp"
#include "core/hooks/catalogue.hpp"
#include "core/hooks/dispatch.hpp"
#include "core/hooks/ui_points.hpp"

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

using namespace swofty_core::hooks;
namespace ev = swofty::ui::events;
namespace up = swofty_core::hooks::ui_points;

static int fails;
static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static std::vector<std::string> trail, lines;
static void *seen_list, *seen_view;
static std::string seen_screen, seen_route;
static int cancel_route;

struct Tag {
    std::string name;
};

static int list_handler(void *user, SwoftyHookCall *c)
{
    trail.push_back(static_cast<Tag *>(user)->name + (c->phase == SWOFTY_PHASE_BEFORE ? ":before" : ":after"));
    auto *e = static_cast<ev::EraTiles *>(c->payload);
    seen_list = e->list;
    return 0;
}

static int screen_handler(void *, SwoftyHookCall *c)
{
    auto *e = static_cast<ev::ScreenBuilt *>(c->payload);
    seen_screen = std::string(e->screen());
    seen_view = e->view;
    return 0;
}

static int route_handler(void *user, SwoftyHookCall *c)
{
    auto *e = static_cast<ev::RouteOpen *>(c->payload);
    seen_route = std::string(e->route()) + "->" + std::string(e->screen());
    trail.push_back(static_cast<Tag *>(user)->name + (c->phase == SWOFTY_PHASE_REPLACE ? ":replace" : ":other"));
    return c->phase == SWOFTY_PHASE_REPLACE && cancel_route ? 1 : 0;
}

struct Fixture {
    Table t;
    up::SiteLink era, home, screen, route;
    Fixture()
        : era(up::ERA_POINT, "site_FeTile_EraTilesBuilt", {}, log), home(up::HOME_POINT, "site_FeHome_AfterCareerCall", {}, log),
          screen(up::SCREEN_POINT, "site_Ui_Job_ScreenDone", {}, log), route(up::ROUTE_POINT, "Vm_Run_Route", {}, log)
    {
        t.declare_core();
        t.declare_game(up::ERA_POINT, SWOFTY_POINT_MODIFY, ev::EraTiles::version, &era);
        t.declare_game(up::HOME_POINT, SWOFTY_POINT_MODIFY, ev::HomeTiles::version, &home);
        t.declare_game(up::SCREEN_POINT, SWOFTY_POINT_NOTIFY, ev::ScreenBuilt::version, &screen);
        t.declare_game(up::ROUTE_POINT, SWOFTY_POINT_CANCEL, ev::RouteOpen::version, &route);
        t.arm();
    }
    static void log(const std::string &s) { lines.push_back(s); }
};

static std::string joined()
{
    std::string s;
    for (auto &t : trail)
        s += (s.empty() ? "" : " ") + t;
    trail.clear();
    return s;
}

static void test_lists()
{
    Fixture f;
    int vector_slot = 0;
    up::fire_list<ev::EraTiles>(f.t, &vector_slot);
    expect(f.era.installed() == false, "era: nothing installed without a subscriber");
    Tag a{"a"}, b{"b"};
    uint32_t ta = 0, tb = 0;
    f.t.attach("a", up::ERA_POINT, SWOFTY_PHASE_AFTER, 0, "", list_handler, &a, &ta);
    f.t.attach("b", up::ERA_POINT, SWOFTY_PHASE_BEFORE, 0, "", list_handler, &b, &tb);
    expect(f.era.installed(), "era: the first subscriber arms the link");
    expect(f.t.live(up::ERA_POINT, nullptr) == SWOFTY_ASSET_OK, "era: the point reports live once armed");
    trail.clear();
    seen_list = nullptr;
    up::fire_list<ev::EraTiles>(f.t, &vector_slot);
    expect(joined() == "b:before a:after", "era: before handlers run, then after handlers");
    expect(seen_list == &vector_slot, "era: the payload carries the list pointer");
    f.t.detach(ta);
    expect(f.era.installed(), "era: the link stays while a subscriber is left");
    f.t.detach(tb);
    expect(!f.era.installed(), "era: the last subscriber disarms the link");
    int home_list = 0;
    Tag h{"h"};
    uint32_t th = 0;
    f.t.attach("a", up::HOME_POINT, SWOFTY_PHASE_AFTER, 0, "", list_handler, &h, &th);
    seen_list = nullptr;
    up::fire_list<ev::HomeTiles>(f.t, &home_list);
    expect(seen_list == &home_list && joined() == "h:after", "home: the after handler sees the tile list");
    expect(f.t.live(up::HOME_POINT, nullptr) == SWOFTY_ASSET_OK, "home: live once subscribed");
}

static void test_screen()
{
    Fixture f;
    uint32_t tok = 0;
    f.t.attach("a", up::SCREEN_POINT, SWOFTY_PHASE_AFTER, 0, "", screen_handler, nullptr, &tok);
    int node = 0;
    up::fire_screen(f.t, "swofty_demo", &node);
    expect(seen_screen == "swofty_demo" && seen_view == &node, "screen: the payload carries the screen name and its root node");
    up::fire_screen(f.t, "other", nullptr);
    expect(seen_screen == "other" && !seen_view, "screen: a screen with no view model node gives a null view");
}

static void test_route()
{
    Fixture f;
    Tag low{"low"}, high{"high"};
    uint32_t t1 = 0, t2 = 0;
    f.t.attach("a", up::ROUTE_POINT, SWOFTY_PHASE_BEFORE, 0, "", route_handler, &low, &t1);
    expect(up::fire_route(f.t, "open_settings", "mod.settings/settings"), "route: a before handler does not cancel");
    expect(seen_route == "open_settings->mod.settings/settings", "route: the payload carries the route and its screen");
    trail.clear();
    cancel_route = 1;
    f.t.attach("b", up::ROUTE_POINT, SWOFTY_PHASE_REPLACE, 0, "", route_handler, &high, &t2);
    trail.clear();
    expect(!up::fire_route(f.t, "open_settings", ""), "route: a handler that takes the route cancels it");
    expect(joined() == "low:other high:replace", "route: before runs ahead of the replace handler");
    cancel_route = 0;
    expect(up::fire_route(f.t, "open_settings", ""), "route: a replace handler that declines lets the route run");
}

static int track_handler(void *, SwoftyHookCall *c)
{
    auto *e = static_cast<ev::TrackSelectBuild *>(c->payload);
    seen_list = e->list;
    trail.push_back(c->phase == SWOFTY_PHASE_BEFORE ? "track:before" : "track:after");
    return 0;
}

static void test_track_select()
{
    struct Link final : GameLink {
        bool on = false;
        bool install(bool, const std::string &) override { return on = true; }
        void uninstall() override { on = false; }
        bool installed() const override { return on; }
    } link;
    Table t;
    t.declare_core();
    t.declare_game(up::TRACK_SELECT_POINT, SWOFTY_POINT_MODIFY, ev::TrackSelectBuild::version, &link);
    t.arm();
    int vm = 0;
    up::fire_list<ev::TrackSelectBuild>(t, &vm);
    expect(!link.on, "track select: no subscriber, no detour");
    uint32_t tok = 0;
    t.attach("a", up::TRACK_SELECT_POINT, SWOFTY_PHASE_BEFORE, 0, "", track_handler, nullptr, &tok);
    uint32_t tok2 = 0;
    t.attach("b", up::TRACK_SELECT_POINT, SWOFTY_PHASE_AFTER, 0, "", track_handler, nullptr, &tok2);
    expect(link.on && t.live(up::TRACK_SELECT_POINT, nullptr) == SWOFTY_ASSET_OK,
           "track select: the first subscriber arms the link");
    trail.clear();
    seen_list = nullptr;
    up::fire_list<ev::TrackSelectBuild>(t, &vm);
    expect(seen_list == &vm && joined() == "track:before track:after", "track select: handlers get the screen's view model");
    t.detach(tok);
    t.detach(tok2);
    expect(!link.on, "track select: the last subscriber disarms the link");
}

static void test_mfd_pages()
{
    namespace mfd = frontend::mfd_pages;
    using Page = swofty::ui::events::MfdPage;
    Page pages[mfd::CAPACITY];
    mfd::fill(pages);
    expect(mfd::CAPACITY == 8 && mfd::STOCK_COUNT == 6 && !std::strcmp(pages[0].name, "car_settings") &&
               !std::strcmp(pages[5].name, "mods") && !pages[6].name,
           "mfd: the payload lists the six stock pages in capacity 8");
    mfd::Outcome o = mfd::read(pages, mfd::STOCK_COUNT);
    expect(o.mask == 0 && !o.reordered && !o.resized && o.unknown == 0, "mfd: a handler that does nothing hides nothing");
    pages[2].hide = 1;
    pages[4].hide = 7;
    o = mfd::read(pages, mfd::STOCK_COUNT);
    expect(o.mask == ((1u << 2) | (1u << 4)) && !o.reordered, "mfd: hide flags become a mask by page index");
    std::swap(pages[0], pages[2]);
    o = mfd::read(pages, mfd::STOCK_COUNT);
    expect(o.reordered && o.mask == ((1u << 2) | (1u << 4)),
           "mfd: a reorder is flagged and the hide flags still go by page name");
    mfd::fill(pages);
    pages[5].hide = 1;
    o = mfd::read(pages, 9);
    expect(o.resized && o.mask == (1u << 5), "mfd: a count past the stock six is refused and entries stay the stock six");
    o = mfd::read(pages, 3);
    expect(o.resized && o.mask == 0, "mfd: a shortened count does not hide the pages cut off");
    mfd::fill(pages);
    pages[1].name = "swofty_addons_panel";
    pages[1].hide = 1;
    o = mfd::read(pages, mfd::STOCK_COUNT);
    expect(o.unknown == 1 && o.mask == 0, "mfd: an unknown page name is ignored");
    expect(mfd::enabled_path("car_damage") == "view_model_data.hud.content.mfd_complete.mfd.car_damage_enabled",
           "mfd: the _enabled leaf path");
    ev::MfdPages e{};
    e.pages = pages;
    e.count = 6;
    expect(e.page(0) == pages && e.page(5) == pages + 5 && !e.page(6) && !e.page(-1), "mfd: page() stays inside count");
}

static void test_catalogue()
{
    namespace cat = swofty_core::hooks::catalogue;
    auto row = [](const char *id) -> const cat::Event * {
        for (auto &e : cat::EVENTS)
            if (!std::strcmp(e.id, id))
                return &e;
        return nullptr;
    };
    for (const char *id :
         {up::ERA_POINT, up::HOME_POINT, up::SCREEN_POINT, up::ROUTE_POINT, up::TRACK_SELECT_POINT, "hud.mfd.pages"}) {
        auto *e = row(id);
        expect(e && e->fired == cat::Fired::game, std::string(id) + ": catalogue row is Fired::game");
    }
    for (const char *id : {"ui.career_content.tiles", "ui.car_category.tiles"}) {
        auto *e = row(id);
        expect(e && e->fired == cat::Fired::no, std::string(id) + ": catalogue row stays Fired::no");
    }
    Table t;
    t.declare_core();
    expect(t.live(up::TRACK_SELECT_POINT, nullptr) == SWOFTY_ASSET_UNAVAILABLE,
           "track select: unavailable before its game link is declared");
    expect(t.live(up::ROUTE_POINT, nullptr) == SWOFTY_ASSET_UNAVAILABLE, "route: unavailable before its game link is declared");
    expect(row(up::ROUTE_POINT)->kind == SWOFTY_POINT_CANCEL && row(up::SCREEN_POINT)->kind == SWOFTY_POINT_NOTIFY,
           "kinds: ui.route.open cancels, ui.screen.built notifies");
}

int main()
{
    test_lists();
    test_screen();
    test_route();
    test_track_select();
    test_mfd_pages();
    test_catalogue();
    printf("%s\n", fails ? "FAILED" : "all ok");
    return fails ? 1 : 0;
}
