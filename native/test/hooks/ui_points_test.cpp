// Host test for the UI game points ui.era.tiles, ui.home.tiles, ui.career_content.tiles, ui.car_category.tiles,
// ui.screen.built, ui.route.open and ui.track_select.build (hooks/ui_points.hpp): the payload fields, the before/after
// order, the RouteOpen cancel, the site link (installed with the first subscriber, removed with the last), the catalogue
// rows, the chosen tiles the content tile handlers edit, and the hide-only MFD page rules of hud.mfd.pages
// (frontend/mfd_pages_logic.hpp). native/test/run_tests.sh ui_points builds and runs it.
#include "core/boxes/era/box_table.hpp"
#include "core/frontend/chosen_tiles.hpp"
#include "core/frontend/mfd_pages_logic.hpp"
#include "core/hooks/catalogue.hpp"
#include "core/hooks/dispatch.hpp"
#include "core/hooks/track_rows.hpp"
#include "core/hooks/ui_names.hpp"
#include "core/hooks/ui_points.hpp"

#include <cstdio>
#include <cstring>
#include <map>
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
    up::fire_tracks(t, &vm);
    expect(!link.on, "track select: no subscriber, no detour");
    uint32_t tok = 0;
    t.attach("a", up::TRACK_SELECT_POINT, SWOFTY_PHASE_BEFORE, 0, "", track_handler, nullptr, &tok);
    uint32_t tok2 = 0;
    t.attach("b", up::TRACK_SELECT_POINT, SWOFTY_PHASE_AFTER, 0, "", track_handler, nullptr, &tok2);
    expect(link.on && t.live(up::TRACK_SELECT_POINT, nullptr) == SWOFTY_ASSET_OK,
           "track select: the first subscriber arms the link");
    trail.clear();
    seen_list = nullptr;
    up::fire_tracks(t, &vm);
    expect(seen_list == &vm && joined() == "track:before track:after", "track select: handlers get the screen's view model");
    t.detach(tok);
    t.detach(tok2);
    expect(!link.on, "track select: the last subscriber disarms the link");
}

static std::string idlist(const std::vector<frontend::tile_records::Tile> &v)
{
    std::string s;
    for (auto &t : v)
        s += (s.empty() ? "" : ",") + t.id;
    return s;
}

static frontend::tile_records::Tile tile_of(const char *id)
{
    frontend::tile_records::Tile t;
    t.id = id;
    t.title = std::string("title_") + id;
    t.route = std::string("route_") + id;
    return t;
}

static int edit_handler(void *, SwoftyHookCall *c)
{
    auto *e = static_cast<ev::HomeTiles *>(c->payload);
    auto tiles = e->tiles();
    trail.push_back("n=" + std::to_string(tiles.count()));
    std::string seen;
    for (auto item : tiles)
        seen += std::string(item.id) + "/" + std::string(item.place) + " ";
    trail.push_back(seen);
    e->remove(0);
    e->insert(1, {.id = "added", .title = "Added", .route = "added_route"});
    tiles.move(0, 1);
    return 0;
}

static void test_tile_tables()
{
    namespace ct = frontend::chosen_tiles;
    namespace tr = frontend::tile_records;
    std::vector<tr::Tile> tiles{tile_of("a"), tile_of("b"), tile_of("c")};
    ct::Chosen chosen{tiles, 4, tr::Place::car_category};
    ct::ChosenTable table(chosen);
    const SwoftyTileList *l = table.table();
    expect(l->size == sizeof(SwoftyTileList) && l->version == SWOFTY_TILE_LIST_VERSION && l->self && l->count(l->self) == 3,
           "tile table: count is the chosen tiles");
    SwoftyTileView v{};
    expect(l->get(l->self, 1, &v) == SWOFTY_ASSET_OK && !std::strcmp(v.id, "b") && !std::strcmp(v.title, "title_b") &&
               !std::strcmp(v.route, "route_b") && !std::strcmp(v.place, "home") && !v.literal,
           "tile table: get fills id, title, route, image and place");
    expect(l->get(l->self, 3, &v) == SWOFTY_ASSET_INVALID && l->get(l->self, -1, &v) == SWOFTY_ASSET_INVALID &&
               l->get(l->self, 0, nullptr) == SWOFTY_ASSET_INVALID,
           "tile table: get past the end, below zero or with no out is invalid");
    expect(l->remove(l->self, 3) == SWOFTY_ASSET_INVALID && l->remove(l->self, -1) == SWOFTY_ASSET_INVALID &&
               idlist(tiles) == "a,b,c",
           "tile table: remove out of range changes nothing");
    expect(l->remove(l->self, 0) == SWOFTY_ASSET_OK && idlist(tiles) == "b,c", "tile table: remove drops the tile");
    expect(l->move(l->self, 0, 1) == SWOFTY_ASSET_OK && idlist(tiles) == "c,b", "tile table: move reorders");
    expect(l->move(l->self, 0, 2) == SWOFTY_ASSET_INVALID && l->move(l->self, 5, 0) == SWOFTY_ASSET_INVALID &&
               idlist(tiles) == "c,b",
           "tile table: move out of range changes nothing");
    SwoftyTileSpec spec{"d", "D", "route_d", "#arr[x]", 1};
    expect(l->insert(l->self, 1, &spec) == SWOFTY_ASSET_OK && idlist(tiles) == "c,d,b" && tiles[1].literal &&
               tiles[1].image == "#arr[x]" && tiles[1].place == tr::Place::car_category,
           "tile table: insert adds a tile of the list's place");
    expect(l->insert(l->self, 1, &spec) == SWOFTY_ASSET_INVALID, "tile table: a repeated id is refused");
    SwoftyTileSpec e1{"e", "E", "route_e", "", 0};
    expect(l->insert(l->self, 9, &e1) == SWOFTY_ASSET_INVALID && l->insert(l->self, -1, &e1) == SWOFTY_ASSET_INVALID &&
               l->insert(l->self, 0, nullptr) == SWOFTY_ASSET_INVALID,
           "tile table: insert out of range or with no spec is invalid");
    SwoftyTileSpec no_id{"", "E", "route_e", "", 0}, no_title{"e", "", "route_e", "", 0}, no_route{"e", "E", "", "", 0};
    expect(l->insert(l->self, 0, &no_id) == SWOFTY_ASSET_INVALID && l->insert(l->self, 0, &no_title) == SWOFTY_ASSET_INVALID &&
               l->insert(l->self, 0, &no_route) == SWOFTY_ASSET_INVALID,
           "tile table: a spec needs an id, a title and, here, a route");
    expect(l->insert(l->self, 3, &e1) == SWOFTY_ASSET_OK && tiles.size() == 4, "tile table: insert up to the cap");
    SwoftyTileSpec f1{"f", "F", "route_f", "", 0};
    expect(l->insert(l->self, 0, &f1) == SWOFTY_ASSET_INVALID && idlist(tiles) == "c,d,b,e",
           "tile table: insert past the cap is refused");
    chosen.need_route = false;
    chosen.cap = 5;
    expect(l->insert(l->self, 0, &no_route) == SWOFTY_ASSET_INVALID, "tile table: a repeated id stays refused without a route");
    SwoftyTileSpec g1{"g", "G", "", "", 0};
    expect(l->insert(l->self, 0, &g1) == SWOFTY_ASSET_OK, "tile table: a place that needs no route takes a spec without one");

    std::vector<tr::Tile> over{tile_of("a"), tile_of("b")};
    ct::Chosen small{over, 2};
    ct::ChosenTable small_table(small);
    SwoftyTileSpec h1{"h", "H", "route_h", "", 0};
    small_table.table()->insert(small_table.table()->self, 0, &h1);
    expect(idlist(over) == "a,b", "tile table: a full list takes no insert, so settle has nothing to drop");
    over.push_back(tile_of("a"));
    expect(ct::settle(over, 2) == 1 && idlist(over) == "a,b", "tile table: settle still drops what a handler left over");

    SwoftyTileList none{sizeof(SwoftyTileList), SWOFTY_TILE_LIST_VERSION, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr};
    swofty::ui::TileList empty(&none);
    expect(empty.count() == 0 && !empty.get(0) && empty.remove(0) == swofty::Result::unavailable &&
               empty.insert(0, {.id = "x", .title = "X"}) == swofty::Result::unavailable,
           "tile list helper: a table with no functions is unavailable, not a crash");
}

static void test_tile_event()
{
    namespace ct = frontend::chosen_tiles;
    namespace tr = frontend::tile_records;
    Table t;
    up::SiteLink home(up::HOME_POINT, "site_FeHome_AfterCareerCall", {}, Fixture::log);
    t.declare_core();
    t.declare_game(up::HOME_POINT, SWOFTY_POINT_MODIFY, ev::HomeTiles::version, &home);
    t.arm();
    uint32_t tok = 0;
    t.attach("a", up::HOME_POINT, SWOFTY_PHASE_AFTER, 0, "", edit_handler, nullptr, &tok);
    std::vector<tr::Tile> tiles{tile_of("s1"), tile_of("s2"), tile_of("s3")};
    ct::Chosen chosen{tiles, 3, tr::Place::home};
    ct::ChosenTable table(chosen);
    trail.clear();
    seen_list = nullptr;
    up::fire_list<ev::HomeTiles>(t, &chosen, table.table());
    expect(ev::HomeTiles::version == 2 && sizeof(ev::HomeTiles) == 24, "tile event: payload version 2, 24 bytes");
    expect(trail.size() == 2 && trail[0] == "n=3" && trail[1] == "s1/home s2/home s3/home ",
           "tile event: a handler counts and iterates the chosen tiles");
    expect(idlist(tiles) == "added,s2,s3" || idlist(tiles) == "s2,added,s3",
           "tile event: the handler's remove, insert and move reach the chosen list: " + idlist(tiles));
    expect(ct::settle(tiles, chosen.cap) == 0 && tiles.size() == 3, "tile event: settle keeps an edited list inside the cap");

    ev::HomeTiles v1{};
    v1.size = 16;
    v1.payload_version = 1;
    v1.tiles_table = table.table();
    expect(!v1.tiles() && v1.remove(0) == swofty::Result::unavailable,
           "tile event: a version 1 payload of 16 bytes has no table, even with a stale pointer");

    std::vector<runtime::era::Box> boxes(4);
    const char *names[] = {"a", "b", "c", "d"};
    for (size_t i = 0; i < boxes.size(); i++) {
        boxes[i].entrant_list = names[i];
        boxes[i].title = std::string("lng_") + names[i];
        boxes[i].order = int(i);
    }
    boxes[1].only = runtime::era::ONLY_TIME_TRIAL;
    boxes[2].time_trial = "c_tt";
    auto era_ids = [&] {
        std::string r;
        for (auto &b : boxes)
            r += b.entrant_list + ",";
        return r;
    };
    {
        runtime::era::BoxTable gp(boxes, false);
        swofty::ui::TileList l(gp.table());
        auto b0 = l.get(1);
        expect(l.count() == 3 && b0 && b0->id == "c" && b0->title == "lng_c" && b0->route == "c" && b0->place == "era" &&
                   !b0->literal && b0->image == "#overrides.home_screen.tiles_solo_6x6[c]",
               "era table: Grand Prix lists the boxes it shows with id, title, route and image");
        expect(l.get(3) == std::nullopt && l.remove(3) == swofty::Result::invalid && l.move(0, 3) == swofty::Result::invalid &&
                   l.insert(0, {.id = "x", .title = "X"}) == swofty::Result::unavailable,
               "era table: out of range is invalid and insert is not offered");
        expect(l.move(0, 2) == swofty::Result::ok && era_ids() == "c,b,d,a,",
               "era table: move reorders the shown boxes and the hidden box keeps its slot: " + era_ids());
        expect(l.remove(1) == swofty::Result::ok && era_ids() == "c,b,a," && l.count() == 2,
               "era table: remove drops the box from the core's list: " + era_ids());
    }
    {
        runtime::era::BoxTable tt(boxes, true);
        swofty::ui::TileList l(tt.table());
        auto b = l.get(0);
        expect(l.count() == 3 && b && b->route == "c_tt" && b->id == "c",
               "era table: Time Trial shows the Time Trial boxes and routes a box to its Time Trial list");
    }

    uint8_t gp_vm[0x940] = {};
    uint8_t *gp_rows = new uint8_t[0x128 * 5];
    uintptr_t b0p = uintptr_t(gp_rows), e0p = b0p + 0x128 * 5;
    std::memcpy(gp_vm + 0x750, &b0p, 8);
    std::memcpy(gp_vm + 0x758, &e0p, 8);
    namespace trk = swofty_core::hooks::track_rows;
    trk::Table gp_table(gp_vm, trk::Screen::grand_prix);
    ev::TrackSelectBuild gp_ev{};
    gp_ev.size = sizeof gp_ev;
    gp_ev.payload_version = 2;
    gp_ev.tracks_table = gp_table.table();
    expect(gp_ev.tracks().count() == 5 && !gp_ev.tracks().get(0),
           "track table: Grand Prix counts the 0x128-byte rows and offers no get");
    e0p = b0p;
    std::memcpy(gp_vm + 0x758, &e0p, 8);
    expect(gp_ev.tracks().count() == 0, "track table: a cleared row vector counts 0");
    delete[] gp_rows;

    uint8_t st_vm[0xb30] = {}, st_obj[0xa0] = {};
    uint8_t *st_rows = new uint8_t[0x168 * 3];
    uintptr_t sb = uintptr_t(st_rows), se = sb + 0x168 * 3, owner = uintptr_t(st_obj);
    std::memcpy(st_vm + 0xb28, &owner, 8);
    std::memcpy(st_obj + 0x90, &sb, 8);
    std::memcpy(st_obj + 0x98, &se, 8);
    trk::Table st_table(st_vm, trk::Screen::settings);
    swofty::ui::TrackList st_list(st_table.table());
    expect(st_list.count() == 3 && !st_list.get(0), "track table: settings counts the 0x168-byte rows of its owner object");
    uint8_t blank[0xb30] = {};
    trk::Table none_table(blank, trk::Screen::settings);
    expect(swofty::ui::TrackList(none_table.table()).count() == 0 && trk::rows_of(nullptr, trk::Screen::grand_prix) == 0,
           "track table: no owner or no vm counts 0");
    delete[] st_rows;

    SwoftyTrackList none{sizeof(SwoftyTrackList), SWOFTY_TRACK_LIST_VERSION, nullptr, nullptr, nullptr, nullptr, nullptr};
    ev::TrackSelectBuild e{};
    e.size = sizeof e;
    e.payload_version = 2;
    e.tracks_table = &none;
    expect(e.tracks().count() == 0 && !e.tracks().get(0) && sizeof e == 24, "track table: read-only, no functions on this core");
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
    for (const char *id : {up::ERA_POINT, up::HOME_POINT, up::CAREER_CONTENT_POINT, up::CAR_CATEGORY_POINT, up::SCREEN_POINT,
                           up::ROUTE_POINT, up::TRACK_SELECT_POINT, "hud.mfd.pages"}) {
        auto *e = row(id);
        expect(e && e->fired == cat::Fired::game, std::string(id) + ": catalogue row is Fired::game");
    }
    Table t;
    t.declare_core();
    expect(t.live(up::TRACK_SELECT_POINT, nullptr) == SWOFTY_ASSET_UNAVAILABLE,
           "track select: unavailable before its game link is declared");
    expect(t.live(up::ROUTE_POINT, nullptr) == SWOFTY_ASSET_UNAVAILABLE, "route: unavailable before its game link is declared");
    expect(row(up::ROUTE_POINT)->kind == SWOFTY_POINT_CANCEL && row(up::SCREEN_POINT)->kind == SWOFTY_POINT_NOTIFY,
           "kinds: ui.route.open cancels, ui.screen.built notifies");
}

static frontend::tile_records::Tile tile(const char *id)
{
    frontend::tile_records::Tile t;
    t.id = id;
    return t;
}

static std::string ids(const std::vector<frontend::tile_records::Tile> &v)
{
    std::string s;
    for (auto &t : v)
        s += (s.empty() ? "" : ",") + t.id;
    return s;
}

static int drop_first_handler(void *, SwoftyHookCall *c)
{
    auto *e = static_cast<ev::CareerContentTiles *>(c->payload);
    seen_list = e->list;
    auto *chosen = static_cast<frontend::chosen_tiles::Chosen *>(e->list);
    chosen->tiles.erase(chosen->tiles.begin());
    chosen->tiles.push_back(tile("late"));
    chosen->tiles.push_back(tile("late"));
    return 0;
}

static void test_chosen()
{
    Table t;
    up::SiteLink career(up::CAREER_CONTENT_POINT, "career content select", {}, Fixture::log),
        car(up::CAR_CATEGORY_POINT, "car category tab", {}, Fixture::log);
    t.declare_core();
    t.declare_game(up::CAREER_CONTENT_POINT, SWOFTY_POINT_MODIFY, ev::CareerContentTiles::version, &career);
    t.declare_game(up::CAR_CATEGORY_POINT, SWOFTY_POINT_MODIFY, ev::CarCategoryTiles::version, &car);
    t.arm();
    std::vector<frontend::tile_records::Tile> tiles{tile("a"), tile("b")};
    frontend::chosen_tiles::Chosen chosen{tiles, 3};
    up::fire_list<ev::CareerContentTiles>(t, &chosen);
    expect(ids(tiles) == "a,b" && !career.installed(), "content: nothing runs and nothing changes without a subscriber");
    uint32_t tok = 0;
    t.attach("a", up::CAREER_CONTENT_POINT, SWOFTY_PHASE_BEFORE, 0, "", drop_first_handler, nullptr, &tok);
    expect(career.installed() && !car.installed(), "content: the first subscriber arms only its own link");
    seen_list = nullptr;
    up::fire_list<ev::CareerContentTiles>(t, &chosen);
    expect(seen_list == &chosen, "content: the payload carries the chosen tiles");
    expect(ids(tiles) == "b,late,late", "content: the handler's edit is in the list before the add step");
    size_t dropped = frontend::chosen_tiles::settle(tiles, chosen.cap);
    expect(dropped == 1 && ids(tiles) == "b,late", "content: the add step drops a repeated id");
    std::vector<frontend::tile_records::Tile> many{tile("1"), tile("2"), tile("3"), tile("4"), tile("5")};
    expect(frontend::chosen_tiles::settle(many, 2) == 3 && ids(many) == "1,2",
           "content: the add step keeps within the slots left");
    std::vector<frontend::tile_records::Tile> carcat{tile("x")};
    frontend::chosen_tiles::Chosen cc{carcat, 4};
    up::fire_list<ev::CarCategoryTiles>(t, &cc);
    expect(ids(carcat) == "x", "car category: a content subscriber does not see the car category point");
}

static void test_names()
{
    namespace un = swofty_core::hooks::ui_names;
    std::vector<std::string> routes{"mod.settings/open_settings", "other/open_page"};
    std::vector<std::string> screens{"other/page", "mod.settings/settings", "mod.settings/page"};
    std::map<std::string, std::string> opens{{"mod.settings/open_settings", "mod.settings/settings"},
                                             {"other/open_page", "page"}};
    auto opens_of = [&](const std::string &id) { return opens[id]; };
    expect(un::screen_of_route("open_settings", routes, screens, opens_of) == "mod.settings/settings",
           "route screen: the bare name the game fires finds its record and reports the screen id");
    expect(un::screen_of_route("mod.settings/open_settings", routes, screens, opens_of) == "mod.settings/settings",
           "route screen: a record id is accepted as well");
    expect(un::screen_of_route("career_hub", routes, screens, opens_of).empty(), "route screen: a stock route has no screen");
    expect(un::screen_of_route("open_page", routes, screens, opens_of) == "other/page",
           "route screen: a bare opens is the screen record of the route's own addon");
    opens["other/open_page"] = "gone";
    expect(un::screen_of_route("open_page", routes, screens, opens_of) == "gone",
           "route screen: an unknown screen is reported as written");
    expect(un::built_screen("root_x", "settings", screens) == "mod.settings/settings",
           "built screen: an addon screen reports its ui.screen record id");
    expect(un::built_screen("MainMenu", "", screens) == "MainMenu", "built screen: a stock screen keeps its root name");
    expect(un::built_screen("root_x", "launcher_page", screens) == "root_x",
           "built screen: a screen with no record keeps its root name");
}

int main()
{
    test_names();
    test_chosen();
    test_tile_tables();
    test_tile_event();
    test_lists();
    test_screen();
    test_route();
    test_track_select();
    test_mfd_pages();
    test_catalogue();
    printf("%s\n", fails ? "FAILED" : "all ok");
    return fails ? 1 : 0;
}
