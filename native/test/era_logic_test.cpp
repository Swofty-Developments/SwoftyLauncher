// Host unit test for the era picker logic (era_logic.hpp): the box check, the default title and image,
// the box order, the Time Trial list and car limit, the press guard, the addon row's handle links and the DRS
// group swap. run_events_test.sh builds and runs it.
#include "../src/core/era_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace era = runtime::era;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static era::Box box(const char *list, const char *title = "", const char *image = "", int32_t order = 0)
{
    return era::Box{list, title, image, order, "test", 0, {}};
}

// A game setup mode object as Sess_Parse_GameSetupMode builds it: string fields are holders whose +0x10 is the label.
struct Mode {
    alignas(8) uint8_t mem[0x200] = {};
    era::DrsHolder own;
    explicit Mode(const char *group)
    {
        own.text = group;
        *reinterpret_cast<void **>(mem + era::MODE_DRS_GROUP) = &own;
    }
    void *field() const { return *reinterpret_cast<void *const *>(mem + era::MODE_DRS_GROUP); }
    std::string group() const { return era::holder_text(field()) ? era::holder_text(field()) : ""; }
};

static void drs_swap_tests()
{
    era::DrsSwaps swaps;
    Mode a("f1_2024"), b("f1_2024");
    era::DrsHolder *never = swaps.holder("swofty_drs_never_1a2b3c4d");
    expect(never && era::holder_text(never) == std::string("swofty_drs_never_1a2b3c4d") &&
               swaps.holder("swofty_drs_never_1a2b3c4d") == never,
           "one holder per label, at a stable address, its +0x10 the label");
    expect(!swaps.holder("") && !swaps.holder(std::string(64, 'x')), "an empty label or one past 63 characters has no holder");

    swaps.swap(a.mem, never);
    swaps.swap(b.mem, never);
    expect(a.group() == "swofty_drs_never_1a2b3c4d" && b.group() == "swofty_drs_never_1a2b3c4d" && swaps.active() == 2,
           "a press points every host mode's DRS group at the rule's group");

    era::DrsHolder *late = swaps.holder("swofty_drs_late_00ff00ff");
    swaps.swap(a.mem, late);
    expect(a.group() == "swofty_drs_late_00ff00ff" && swaps.active() == 2, "a second swap without a put back keeps the game's holder once");
    expect(swaps.put_back() == 2 && a.field() == &a.own && b.field() == &b.own && swaps.active() == 0,
           "put back restores the game's holder in every swapped mode");
    expect(swaps.put_back() == 0 && a.group() == "f1_2024", "a second put back changes nothing");

    swaps.swap(a.mem, never);
    era::DrsHolder other;
    other.text = "f2_2023";
    *reinterpret_cast<void **>(a.mem + era::MODE_DRS_GROUP) = &other;
    expect(swaps.put_back() == 0 && a.group() == "f2_2023", "put back leaves a field the game changed since the swap");

    swaps.swap(nullptr, never);
    swaps.swap(b.mem, nullptr);
    expect(swaps.active() == 0 && b.field() == &b.own, "no mode or no holder: nothing is swapped");
    expect(era::MODE_DRS_GROUP == 0x1e0 && era::MODE_NAME == 0xd8 && era::HOLDER_TEXT == 0x10,
           "GameSetupMode +0x1e0 DRS group and +0xd8 name holders, text at holder +0x10 (re/db structs/GameSetupMode.h)");
}

static void setup_guard_tests()
{
    era::DrsSwaps swaps;
    Mode a("f1_2024"), b("f1_2024");
    era::DrsHolder *never = swaps.holder("swofty_drs_never_1a2b3c4d");
    swaps.swap(a.mem, never);
    swaps.swap(b.mem, never);
    void *reused = reinterpret_cast<void *>(uintptr_t(0x5eed5eed5eed));
    *reinterpret_cast<void **>(a.mem + era::MODE_DRS_GROUP) = reused;
    *reinterpret_cast<void **>(b.mem + era::MODE_DRS_GROUP) = never;
    expect(swaps.forget() == 2 && swaps.active() == 0 && a.field() == reused && b.field() == never,
           "forget drops every swap and writes nothing into the modes, even one that still holds the rule's holder");
    expect(swaps.put_back() == 0 && b.field() == never, "a put back after forget has nothing to write back");
    expect(swaps.holder("swofty_drs_never_1a2b3c4d") == never, "forget keeps the holders at their addresses");
    swaps.swap(a.mem, never);
    expect(swaps.active() == 1 && a.field() == never, "a press after a rebuild swaps the new mode again");

    expect(era::availability_drop_line({}).empty(), "a rebuild with nothing kept logs nothing");
    std::string line = era::availability_drop_line({2, 3, true});
    expect(line.find("2 narrowed entrant setup mode(s)") != std::string::npos &&
               line.find("3 DRS group swap(s)") != std::string::npos && line.find("the Time Trial hold") != std::string::npos &&
               line.find("without writing them back") != std::string::npos,
           "the drop line names the narrowing, the DRS swaps and the Time Trial hold: " + line);
    std::string drs_only = era::availability_drop_line({0, 1, false});
    expect(drs_only.find("1 DRS group swap(s) without") != std::string::npos && drs_only.find("narrowed") == std::string::npos,
           "the drop line names only what was kept: " + drs_only);
}

// A VM node as the game constructs a reference node (0x14083c570): its own handle at +0x8 heads its holder list.
struct Node {
    alignas(8) uint8_t mem[0x30] = {};
    Node()
    {
        self() = {mem, nullptr, nullptr};
        holders() = &self();
        refs() = 2;
    }
    era::Handle &self() { return *reinterpret_cast<era::Handle *>(mem + 8); }
    void *&holders() { return *reinterpret_cast<void **>(mem + era::NODE_HOLDERS); }
    int16_t &refs() { return *reinterpret_cast<int16_t *>(mem + era::NODE_REFS); }
};

static Node &node_of(uint8_t *mem)
{
    return *reinterpret_cast<Node *>(mem);
}

struct Set {
    era::Handle list, selected;
};

// The handle part of FeTile_Clear_EraTileSet_Thunk (0x14bd31910): unlink from the holder list, release.
static void game_unlink(era::Handle &h)
{
    if (!h.node)
        return;
    auto *next = static_cast<era::Handle *>(h.next), *prev = static_cast<era::Handle *>(h.prev);
    if (prev)
        prev->next = next;
    if (next)
        next->prev = prev;
    if (node_of(h.node).holders() == &h)
        node_of(h.node).holders() = next;
    node_of(h.node).refs()--;
    h = {};
}

// The holder walk of the game's node teardown (0x1442b3e30): every holder but the node's own is cut loose
// and releases the node it holds.
static void game_teardown(Node &n)
{
    for (void *p = n.holders(); p;) {
        auto *h = static_cast<era::Handle *>(p);
        p = h->next;
        h->next = h->prev = nullptr;
        if (h == &n.self() || !h->node)
            continue;
        if (node_of(h->node).holders() == h)
            node_of(h->node).holders() = nullptr;
        node_of(h->node).refs()--;
        h->node = nullptr;
    }
}

static std::vector<era::Handle *> chain(Node &n)
{
    std::vector<era::Handle *> out;
    for (void *p = n.holders(); p && out.size() < 16; p = static_cast<era::Handle *>(p)->next)
        out.push_back(static_cast<era::Handle *>(p));
    return out;
}

static bool consistent(Node &n)
{
    void *prev = nullptr;
    for (era::Handle *h : chain(n)) {
        if (h->prev != prev || h->node != n.mem)
            return false;
        prev = h;
    }
    return chain(n).size() < 16;
}

static bool holds(Node &n, const era::Handle &h)
{
    for (era::Handle *p : chain(n))
        if (p == &h)
            return true;
    return false;
}

static void continue_tests()
{
    std::vector<uint16_t> saved{518, 517, 514, 520, 512, 519, 518, 513, 513, 516, 520, 511, 510, 519, 514, 515, 515, 511, 517,
                                512, 516, 510};
    std::vector<era::ListTeams> lists{
        {"f1_2025", {0, 1, 2, 3, 4, 5, 6, 7, 8, 9}},
        {"cs_f1_2017", {521, 522, 523, 524, 525, 526, 527, 528, 529, 530}},
        {"cs_f1_2016", {510, 511, 512, 513, 514, 515, 516, 517, 518, 519, 520, era::NO_TEAM}},
    };
    era::ContinuePick p = era::continue_list(lists, saved, "", "swofty_f1");
    expect(p.index == 2 && p.line.find("loading cs_f1_2016, which holds all 11 teams") != std::string::npos,
           "Continue with an empty list loads the list holding every team of the saved entrants: " + p.line);
    p = era::continue_list(lists, saved, "cs_f1_2017", "swofty_f1");
    expect(p.index == 2, "the teams win over a pressed box whose list doesn't hold them");
    std::vector<era::ListTeams> twice{lists[2], {"cs_f1_2016_copy", lists[2].teams}};
    p = era::continue_list(twice, saved, "cs_f1_2016_copy", "swofty_f1");
    expect(p.index == 1, "of two lists holding the teams, the pressed box's is loaded");
    std::vector<uint16_t> moved{646, 652, 559, 572, 600, 601, 602, 603, 604, 605, 606};
    p = era::continue_list(lists, moved, "cs_f1_2017", "swofty_f1");
    expect(p.index == 1 && p.line.find("the box pressed last") != std::string::npos,
           "teams no list holds fall back to the pressed box's list: " + p.line);
    p = era::continue_list(lists, moved, "", "swofty_f1");
    expect(p.index < 0 && p.line.find("no box was pressed") != std::string::npos, "nothing to load is said: " + p.line);
    std::string stop = era::continue_stop("", p, true);
    expect(stop.find("Grand Prix load stopped: ") == 0 && stop.find("646") != std::string::npos &&
               stop.find("start a new Grand Prix") != std::string::npos,
           "no list and no press stops the load with the teams it looked for: " + stop);
    expect(era::continue_stop("", p, false).empty(), "a list the game set is never stopped for");
    p = era::continue_list(lists, {}, "", "swofty_f1");
    expect(p.index < 0, "no saved entrants match no list");
    p = era::continue_list(lists, {era::NO_TEAM, 0}, "", "f1");
    expect(p.index < 0, "unresolved team ids match no list");

    namespace trk = swofty_core::apply::tracks;
    trk::Base base;
    expect(era::track_missing(base, "game/bahrain24").empty(), "a base not built yet stops nothing");
    base.ready = true;
    trk::BaseTrack bahrain;
    bahrain.id = "game/bahrain";
    bahrain.code = "bahrain";
    bahrain.stat_id = 3;
    trk::BaseTrack hockenheim;
    hockenheim.id = "mod.hockenheim/hockenheim";
    hockenheim.code = "hockenheim";
    hockenheim.stat_id = 43;
    hockenheim.kind = trk::Kind::older;
    base.tracks = {bahrain, hockenheim};
    base.dormant = {"game/bahrain24", "game/sochi"};
    expect(era::track_missing(base, "game/bahrain").empty(), "a stock track of the base loads");
    expect(era::track_missing(base, "game/hockenheim").empty(), "a track made for 1.5 found by its code loads");
    expect(era::track_missing(base, "mod.hockenheim/hockenheim").empty(), "a track found by its id loads");
    expect(era::track_missing(base, "").empty(), "an unread track stops nothing");
    std::string gone = era::track_missing(base, "game/bahrain24");
    expect(gone.find("game/bahrain24 is not a track of this build") != std::string::npos &&
               gone.find("dormant row with no track archive") != std::string::npos,
           "a saved weekend at a dormant row whose addon is off is named: " + gone);
    trk::BaseTrack sepang;
    sepang.id = "mod.bahrain24/bahrain24";
    sepang.code = "bahrain24";
    sepang.kind = trk::Kind::record;
    base.tracks.push_back(sepang);
    expect(era::track_missing(base, "game/bahrain24").empty(), "the same row filled by its track record loads");
    base.tracks.pop_back();
    era::ContinuePick fine;
    fine.index = 0;
    stop = era::continue_stop(gone, fine, false);
    expect(stop.find("Grand Prix load stopped: the saved weekend's track game/bahrain24") == 0,
           "a missing track stops the load even when the list is set: " + stop);
    stop = era::continue_stop(gone, p, true);
    expect(stop.find("bahrain24") != std::string::npos && stop.find("no list holds") != std::string::npos,
           "both reasons are named together: " + stop);
}

struct GridSetup {
    alignas(8) uint8_t mem[0x300] = {};
    explicit GridSetup(int32_t limit) { set(limit); }
    void set(int32_t v) { *reinterpret_cast<int32_t *>(mem + era::ENTRANT_SETUP_MAX_ENTRANTS) = v; }
    int32_t get() const { return *reinterpret_cast<const int32_t *>(mem + era::ENTRANT_SETUP_MAX_ENTRANTS); }
};

static void grid_limit_tests()
{
    GridSetup press(20);
    era::GridLimit pressed;
    std::string line = pressed.raise(press.mem, 24);
    expect(press.get() == 24 && !line.empty() && line.find("20 -> 24") != std::string::npos &&
               line.find("clamped") == std::string::npos,
           "a box press on a 24-car list raises max_entrants from 20 to 24");
    pressed.restore();
    expect(press.get() == 20, "the put-back restores max_entrants to 20");
    pressed.restore();
    expect(press.get() == 20, "a second put-back changes nothing");

    GridSetup small(20);
    era::GridLimit none;
    expect(none.raise(small.mem, 18).empty() && small.get() == 20, "a list within the limit leaves max_entrants alone");
    expect(none.raise(small.mem, 20).empty() && small.get() == 20, "a list equal to the limit leaves max_entrants alone");
    expect(none.raise(small.mem, 0).empty() && none.raise(nullptr, 24).empty(), "an unread car count or setup mode changes nothing");
    GridSetup unset(0);
    expect(none.raise(unset.mem, 24).empty() && unset.get() == 0, "a max_entrants of 0 is left alone");

    GridSetup clamp(20);
    era::GridLimit clamped;
    line = clamped.raise(clamp.mem, 40);
    expect(clamp.get() == 25 && line.find("clamped to 25 cars") != std::string::npos,
           "a 40-car list clamps max_entrants to 25, one short of the 26-slot table, and says so");
    clamped.restore();
    expect(clamp.get() == 20, "the clamped limit is restored too");
    GridSetup at_table(26);
    expect(none.raise(at_table.mem, 40).empty() && at_table.get() == 26, "a limit already at the table size is not lowered or logged");

    GridSetup moved(20);
    era::GridLimit stale;
    stale.raise(moved.mem, 24);
    moved.set(22);
    stale.restore();
    expect(moved.get() == 22, "a limit someone else changed since the raise is not overwritten on put-back");

    GridSetup career(20);
    era::GridLimit box, cont;
    box.raise(career.mem, 24);
    expect(cont.raise(career.mem, 24).empty() && career.get() == 24, "the Continue path finds a limit a box press already raised");
    cont.restore();
    expect(career.get() == 24, "...and its put-back leaves the press's raise to the press's put-back");
    box.restore();
    expect(career.get() == 20, "the press's put-back restores it");

    GridSetup resume(20);
    era::GridLimit load;
    line = load.raise(resume.mem, 25);
    expect(resume.get() == 25 && line.find("clamped") == std::string::npos,
           "the Continue path (no box press) raises max_entrants to a 25-car list's count");
    line = load.raise(resume.mem, 26);
    expect(resume.get() == 25 && line.find("clamped to 25 cars") != std::string::npos,
           "a 26-car list is clamped to 25 so the safety car still fits");
    line = load.raise(resume.mem, 24);
    expect(resume.get() == 24 && line.find("20 -> 24") != std::string::npos, "a new raise first puts the earlier one back");
    load.restore();
    expect(resume.get() == 20, "the Continue path's put-back restores max_entrants to 20");
}

int main()
{
    std::vector<era::Box> have;

    era::Box a = box("cs_f1_2016", "lng_cs_f1_2016", "", 2016);
    expect(era::check(a, have).empty(), "a box with a list, a title and an order is accepted");
    expect(era::image_of(a) == "#overrides.home_screen.tiles_solo_6x6[cs_f1_2016]", "an empty image is the list's tiles_solo_6x6 entry");
    a.seq = have.size();
    have.push_back(a);

    era::Box untitled = box("cs_f1_2017");
    expect(era::check(untitled, have).empty() && untitled.title == "lng_cs_f1_2017", "an empty title is lng_<entrant list>");

    era::Box dup = box("cs_f1_2016", "lng_other");
    expect(!era::check(dup, have).empty(), "a second box for the same entrant list is refused");

    for (const char *bad : {"", "has space", "dots.in.name", "slash/name", "0123456789012345678901234567890123456789012345678901234567890123"}) {
        era::Box b = box(bad);
        expect(!era::check(b, have).empty(), std::string("entrant list '") + bad + "' is refused");
    }

    era::Box image = box("cs_f1_2018", "lng_cs_f1_2018", "#overrides.home_screen.tiles_solo_6x6[f1]");
    expect(era::check(image, have).empty() && era::image_of(image) == "#overrides.home_screen.tiles_solo_6x6[f1]",
           "a given image is kept");

    era::Box nul = box("cs_f1_2019");
    nul.title = std::string("lng_a\0b", 7);
    expect(!era::check(nul, have).empty(), "a title with a NUL byte is refused");

    std::vector<era::Box> full;
    for (size_t i = 0; i < era::MAX_BOXES; i++)
        full.push_back(box(("l" + std::to_string(i)).c_str()));
    era::Box over = box("one_more");
    expect(!era::check(over, full).empty(), "a box past the 64th is refused");

    std::vector<era::Box> mixed = {box("y2018", "", "", 2018), box("y2016", "", "", 2016), box("late_a"), box("y2017", "", "", 2017),
                                   box("late_b")};
    for (size_t i = 0; i < mixed.size(); i++)
        mixed[i].seq = i;
    std::vector<era::Box> sorted = era::ordered(mixed);
    std::string got;
    for (const auto &b : sorted)
        got += b.entrant_list + " ";
    expect(got == "late_a late_b y2016 y2017 y2018 ", "boxes sort by order, then by registration (" + got + ")");

    expect(era::press_refusal(true, true, 13, 11, 2).empty(), "a press with the last build's stock and addon tiles goes through");
    expect(!era::press_refusal(false, true, 13, 11, 2).empty(), "a press from another view model is refused");
    expect(!era::press_refusal(true, false, 13, 11, 2).empty(), "a press without the addon nodes is refused");
    expect(!era::press_refusal(true, true, 11, 11, 0).empty(), "a press with no addon boxes is refused");
    expect(!era::press_refusal(true, true, 12, 11, 2).empty(), "a press with fewer tiles than the last build is refused");

    {
        Node a_list, a_sel, b_list, b_sel;
        era::Handle a_ui{}, b_ui{};
        era::link(a_ui, a_list.mem);
        era::link(b_ui, b_list.mem);
        int16_t a0 = a_list.refs(), as0 = a_sel.refs(), b0 = b_list.refs(), bs0 = b_sel.refs();
        Set set{};
        int clears = 0;
        auto clear = [&](Set &s) {
            game_unlink(s.list);
            game_unlink(s.selected);
            clears++;
        };
        auto build = [&](Node &list, Node &sel) {
            era::relink(set, list.mem, clear);
            era::link(set.selected, sel.mem);
        };

        build(a_list, a_sel);
        expect(!clears && a_list.holders() == &set.list && a_sel.holders() == &set.selected && consistent(a_list) &&
                   consistent(a_sel) && a_list.refs() == a0 + 1,
               "the first build links the set at the head of A's holder lists");

        build(b_list, b_sel);
        expect(clears == 1, "a build for view model B clears the set that A's nodes still hold");
        expect(!holds(a_list, set.list) && !holds(a_sel, set.selected) && consistent(a_list) && consistent(a_sel) &&
                   chain(a_list).size() == 2 && a_list.refs() == a0 && a_sel.refs() == as0,
               "A's nodes no longer reference the set and have their references back");
        expect(b_list.holders() == &set.list && b_sel.holders() == &set.selected && consistent(b_list) &&
                   consistent(b_sel) && b_list.refs() == b0 + 1 && b_sel.refs() == bs0 + 1,
               "B's nodes hold the set once, with consistent holder lists");

        game_teardown(a_list);
        game_teardown(a_sel);
        expect(set.list.node == b_list.mem && set.selected.node == b_sel.mem && b_sel.self().node == b_sel.mem &&
                   consistent(b_list) && consistent(b_sel) && b_list.refs() == b0 + 1 && b_sel.refs() == bs0 + 1,
               "tearing A down leaves B's nodes and the set alone");

        build(b_list, b_sel);
        expect(clears == 2 && chain(b_list).size() == 3 && consistent(b_list) && b_list.refs() == b0 + 1,
               "a rebuild of B links the set once");

        game_teardown(b_list);
        game_teardown(b_sel);
        expect(!set.list.node && !set.list.next && !set.selected.node && !set.selected.prev,
               "tearing B down unlinks the set and releases its nodes");
        Node c_list, c_sel;
        build(c_list, c_sel);
        expect(clears == 2 && consistent(c_list) && consistent(c_sel) && c_sel.holders() == &set.selected,
               "a build after the game tore the set's nodes down links them without a clear");

        Node old_a, old_b;
        Set old{};
        era::link(old.selected, old_a.mem);
        old = {};
        era::link(old.selected, old_b.mem);
        int16_t ob = old_b.refs();
        game_teardown(old_a);
        expect(old_b.refs() < ob && !old_b.self().node,
               "1.5.4's reset left A's holder list running into B's, so tearing A down released B's node");
    }

    {
        std::vector<era::Box> none;
        era::Box tt = box("bdx_2012", "lng_bdx_2012", "", 2012);
        tt.time_trial = "bdx_2012_tt";
        expect(era::check(tt, none).empty() && tt.time_trial == "bdx_2012_tt", "a box with a Time Trial list is accepted");
        expect(era::list_for(tt, true) == "bdx_2012_tt" && era::list_for(tt, false) == "bdx_2012",
               "the Time Trial picker uses the box's Time Trial list, the Grand Prix picker its own");
        era::Box plain = box("cs_f1_2016");
        expect(era::check(plain, none).empty() && era::list_for(plain, true) == "cs_f1_2016",
               "a box without a Time Trial list uses its own list in Time Trial");
        era::Box same = box("cs_f1_2017");
        same.time_trial = "cs_f1_2017";
        expect(era::check(same, none).empty() && same.time_trial.empty(), "a Time Trial list equal to the box's list is dropped");
        era::Box bad = box("cs_f1_2018");
        bad.time_trial = "has space";
        expect(!era::check(bad, none).empty(), "a Time Trial list that is not a plain name is refused");
        std::vector<era::Box> shared = {tt};
        era::Box other = box("bdx_2012_alt");
        other.time_trial = "bdx_2012_tt";
        expect(era::check(other, shared).empty(), "two boxes may share a Time Trial list");

        expect(era::TT_MAX_CARS == 23, "Time Trial holds 23 cars of a list (26 slots, 3 the game adds)");
        expect(era::time_trial_refusal(plain, 22).empty() && era::time_trial_refusal(plain, 23).empty(),
               "a list of 22 or 23 cars is offered in Time Trial");
        std::string why = era::time_trial_refusal(plain, 24);
        expect(why.find("cs_f1_2016 holds 24 cars") != std::string::npos && why.find("at most 23") != std::string::npos &&
                   why.find("Give the box a Time Trial entrant list") != std::string::npos,
               "a 24-car list without a Time Trial list is refused, naming the fix (" + why + ")");
        why = era::time_trial_refusal(tt, 24);
        expect(why.find("bdx_2012_tt holds 24 cars") != std::string::npos && why.find("Give the box") == std::string::npos,
               "an oversized Time Trial list is refused by its own name (" + why + ")");
        expect(era::time_trial_refusal(plain, -1).empty(), "an unread car count refuses nothing");

        expect(era::entrant_count(0x1000, 0x1000 + 24 * 0x88) == 24, "24 entrant records of 0x88 bytes count 24");
        expect(era::entrant_count(0x1000, 0x1000) == 0, "an empty list counts 0");
        expect(era::entrant_count(0, 0x88) < 0 && era::entrant_count(0x2000, 0x1000) < 0 &&
                   era::entrant_count(0x1000, 0x1000 + 0x89) < 0 && era::entrant_count(0x1000, 0x1000 + 65 * 0x88) < 0,
               "a null, reversed, misaligned or over-long entrant vector is unread");

        std::vector<era::ListCars> host = {{"cs_f1_2016", 22}, {"cs_f1_2017", 20}, {"bdx_s5_2012", 24}, {"unread", -1}};
        expect(!era::oversized(23) && era::oversized(24) && !era::oversized(-1), "past 23 cars is oversized, an unread count is not");
        auto keep = era::time_trial_keep(host);
        expect(keep == std::vector<size_t>{0, 1, 3},
               "a stock press in Time Trial keeps swofty_f1's lists of at most 23 cars and the unread ones, and drops the 24-car list");
        std::vector<era::ListCars> fine = {{"cs_f1_2016", 22}, {"cs_f1_2017", 20}};
        expect(era::time_trial_keep(fine).size() == fine.size(), "lists that all fit are all kept");
        std::vector<era::ListCars> big = {{"a", 24}, {"b", 30}};
        expect(era::time_trial_keep(big).empty(), "lists that all overflow keep nothing");
        expect(era::time_trial_narrows(big, era::time_trial_keep(big)),
               "lists that all overflow still narrow swofty_f1, to no list at all, so a stock Time Trial takes none of their teams");
        std::vector<era::ListCars> only_2012 = {{"bdx_s5_2012", 24}};
        expect(era::time_trial_narrows(only_2012, era::time_trial_keep(only_2012)) && era::time_trial_keep(only_2012).empty(),
               "F1 2012 Season alone (swofty_f1 offers only bdx_s5_2012, 24 cars) leaves swofty_f1 offering nothing in Time Trial");
        expect(era::time_trial_narrows(host, keep) && !era::time_trial_narrows(fine, era::time_trial_keep(fine)) &&
                   !era::time_trial_narrows({}, {}),
               "a mix narrows to the lists that fit; lists that all fit, or none at all, are left as they are");

        std::vector<era::ListCars> esm = {{"bdx_s5_2012", 24}, {"cs_f1_2016", 22}, {"f1_2025", 20}, {"tt_list", 12}};
        expect(era::time_trial_fallback(esm, {"f1_2025"}) == 2, "an oversized session list falls back to the box pressed");
        expect(era::time_trial_fallback(esm, {"bdx_s5_2012", "tt_list"}) == 3,
               "an oversized box pressed falls back to its Time Trial list");
        expect(era::time_trial_fallback(esm, {"missing"}) == 1, "with no preferred list, the first list that fits");
        expect(era::time_trial_fallback(esm, {}) == 1, "with no box pressed, the first list that fits");
        std::vector<era::ListCars> none_fit = {{"bdx_s5_2012", 24}, {"unread", -1}};
        expect(era::time_trial_fallback(none_fit, {"unread"}) == -1, "an unread list is never a fallback, nor is an oversized one");

        std::string swap = era::time_trial_swap("bdx_s5_2012", 24, "swofty_f1", "cs_f1_2016", 0);
        expect(swap.find("bdx_s5_2012 (setup mode swofty_f1) holds 24 cars") != std::string::npos &&
                   swap.find("at most 23") != std::string::npos && swap.find("loading cs_f1_2016 instead") != std::string::npos &&
                   swap.find("max_entrants") == std::string::npos,
               "the swap line names the list, its setup mode, its count and the list loaded instead (" + swap + ")");
        swap = era::time_trial_swap("bdx_s5_2012", 24, "bdx_2012_mode", "", 24);
        expect(swap.find("offers no list of at most 23 cars") != std::string::npos &&
                   swap.find("max_entrants 24 -> 23 until the session ends") != std::string::npos,
               "with nothing to load instead, the line says the session draws at most 23 cars (" + swap + ")");

        expect(era::time_trial_limit(24) == 23 && era::time_trial_limit(-1) == 23 && era::time_trial_limit(0) == 23,
               "an entrant setup mode allowing more than 23 cars, or any number, is held to 23 for a Time Trial session");
        expect(era::time_trial_limit(23) == 23 && era::time_trial_limit(20) == 20 && era::time_trial_limit(5) == 5,
               "a limit of at most 23 is left as it is");

        uintptr_t b = 0x10000;
        expect(era::session_vehicles(b, b + 27 * 8) == 27 && era::session_vehicles(b, b + 25 * 8) == 25 &&
                   era::session_vehicles(b, b) == 0 && era::session_vehicles(b + 8, b) == 0,
               "the session's vehicle count is its pointer vector's length, 0 when empty or reversed");
        expect(era::vehicles_end_within_table(b, b + 27 * 8) == b + 26 * 8,
               "27 vehicles (a 24-car list plus the game's 3) are handed over as the first 26, the table's size");
        expect(era::vehicles_end_within_table(b, b + 26 * 8) == b + 26 * 8 && era::vehicles_end_within_table(b, b + 25 * 8) == b + 25 * 8,
               "26 vehicles or fewer are handed over as they are");

        auto refs = [](uint32_t first, size_t n) {
            std::vector<uint32_t> r;
            for (size_t i = 0; i < n; ++i)
                r.push_back((first + uint32_t(i)) << 16);
            return r;
        };
        era::TtList custom{"f1_custom_team", true, false, refs(1, 11)};
        era::TtList f1_2025{"f1_2025", false, false, refs(1, 10)};
        auto stock = era::time_trial_budget({custom, f1_2025});
        expect(stock.dropped.empty() && stock.cars == 11 && stock.cars + era::TT_OWN_ENTRIES == 15,
               "a stock F1 2025 Time Trial draws 11 distinct cars (f1_2025's 10 are among f1_custom_team's 11): 15 vehicles");
        era::TtList bdx{"bdx_s5_2012", false, true, refs(700, 12)};
        auto with_2012 = era::time_trial_budget({custom, f1_2025, bdx});
        expect(with_2012.dropped == std::vector<size_t>{2} && with_2012.cars == 11,
               "F1 2012 Season's 12 cars would make 27 vehicles: its list is left out and the session keeps 15");
        era::TtList a{"ex_a", false, true, refs(800, 10)}, bb{"ex_b", false, true, refs(820, 10)};
        auto two = era::time_trial_budget({custom, a, bb});
        expect(two.dropped == std::vector<size_t>{2} && two.cars == 21,
               "two kept boxes of f1_2025's leaderboard (15 + 10 + 10): the first fits (25 vehicles), the second is left out");
        era::TtList wide{"ex_23", false, true, refs(900, 12)};
        wide.refs.resize(23, wide.refs.back());
        expect(era::distinct_cars(wide.refs) == 12 && era::time_trial_budget({custom, wide}).dropped == std::vector<size_t>{1},
               "a 23-car list over 12 cars passes the 23-car check but makes 27 vehicles: it is left out");
        era::TtList other_mode{"ex_mode_list", false, false, refs(950, 12)};
        auto foreign = era::time_trial_budget({custom, other_mode});
        expect(foreign.dropped == std::vector<size_t>{1} && foreign.cars == 11,
               "another addon's setup mode on the Time Trial mode with a 12-car list of the leaderboard is left out too");
        era::TtList stock_extra{"stock_extra", false, false, refs(400, 5)}, addon_small{"ex_small", false, true, refs(420, 8)};
        auto order = era::time_trial_budget({custom, addon_small, stock_extra});
        expect(order.dropped == std::vector<size_t>{1} && order.cars == 16,
               "stock lists join before addon lists, so the addon list is the one left out (11 + 5, then 8 would pass 22)");
        era::TtList big_selected{"ex_selected", true, true, refs(1000, 24)};
        auto sel = era::time_trial_budget({big_selected, f1_2025});
        expect(sel.dropped == std::vector<size_t>{1} && sel.cars == 24,
               "the selected list always stays; the others are left out when it fills the table by itself");
        era::TtList empty_refs{"ex_empty", false, true, {0x3fff0000u, 0x3fff0000u}};
        expect(era::time_trial_budget({custom, empty_refs}).dropped.empty() && era::distinct_cars(empty_refs.refs) == 0,
               "entrants without a vehicle ref (0x3fff) take no slot");
        std::string bl = era::time_trial_budget_line({custom, f1_2025, bdx}, with_2012, 0);
        expect(bl == "the Time Trial session's list f1_custom_team (leaderboard 0, f1) draws 11 cars from 2 list(s) of its leaderboard; "
                     "with the game's 4 entries 15 of the 26-slot vehicle table; left out until the session unloads: bdx_s5_2012 (12 cars)",
               "the budget line names the list, leaderboard, cars, vehicles and what it left out (" + bl + ")");
        {
            alignas(8) uint8_t slots[4 * era::OVERRIDE_SLOT] = {};
            uint8_t meta[4] = {0, 3, 0, 5};
            int base = 0, f1_custom_team = 0, other = 0;
            slots[1 * era::OVERRIDE_SLOT] = 9;
            *reinterpret_cast<void **>(slots + 1 * era::OVERRIDE_SLOT + 8) = &f1_custom_team;
            slots[3 * era::OVERRIDE_SLOT] = 12;
            *reinterpret_cast<void **>(slots + 3 * era::OVERRIDE_SLOT + 8) = &other;
            slots[0] = 7;
            *reinterpret_cast<void **>(slots + 8) = &base;
            expect(era::override_for(9, slots, meta, 4).to == &f1_custom_team && era::override_for(12, slots, meta, 4).to == &other,
                   "an entrant setup mode with an override row is swapped for its override, as slot 0x1d8 does");
            expect(!era::override_for(7, slots, meta, 4).found && !era::override_for(8, slots, meta, 4).found,
                   "a slot whose metadata byte is 0 is empty, so stale key bytes there match nothing; no row, no swap");
            expect(!era::override_for(9, nullptr, meta, 4).found &&
                       !era::override_for(9, slots, meta, era::MAX_OVERRIDE_SLOTS + 1).found,
                   "an unread or over-long override map swaps nothing");
            *reinterpret_cast<void **>(slots + 3 * era::OVERRIDE_SLOT + 8) = nullptr;
            auto null_row = era::override_for(12, slots, meta, 4);
            expect(null_row.found && !null_row.to,
                   "a row whose override is null is still installed, as the game does: that mode slot is left empty");
        }
        expect(era::ref_key(0x02a5001fu) == era::ref_key(0x02a5ff1fu) && era::ref_key(0x02a5001fu) != era::ref_key(0x02a60020u),
               "vehicle refs are the same car when byte +0 and the u16 +2 match, whatever byte +1 holds");
        era::TtList pool{"entrants system pools", false, false, refs(1, 3)};
        pool.fixed = true;
        pool.refs.push_back(60u << 16);
        auto with_pool = era::time_trial_budget({custom, pool, bdx});
        expect(with_pool.cars == 12 && with_pool.dropped == std::vector<size_t>{2},
               "entrants system pool refs always count (11 + 1 new) and never leave; the 12-car list still does");
        std::string pl = era::time_trial_budget_line({custom, pool, bdx}, with_pool, 3);
        expect(pl.find("from 1 list(s) of its leaderboard and the entrants system pools (4)") != std::string::npos &&
                   pl.find("16 of the 26-slot") != std::string::npos,
               "the budget line names the pools and their cars (" + pl + ")");
        {
            std::vector<uint8_t> pool(3001 * era::POOL_SLOT, 0);
            uint32_t high = 3000u << 16, low = 5u << 16;
            *reinterpret_cast<uint32_t **>(pool.data() + 3000 * era::POOL_SLOT + 8) = &high;
            *reinterpret_cast<uint32_t **>(pool.data() + 5 * era::POOL_SLOT + 8) = &low;
            auto read = [](uintptr_t a, auto &v) {
                std::memcpy(&v, reinterpret_cast<const void *>(a), sizeof(v));
                return true;
            };
            auto b0 = reinterpret_cast<uintptr_t>(pool.data());
            auto got = era::pool_refs_from(b0, b0 + pool.size(), read);
            expect(got.refs == std::vector<uint32_t>{low, high} && !got.capped,
                   "a pool is as long as its highest vehicle stat id + 1: an addon car at id 3000 is counted");
            std::vector<uint8_t> huge((era::MAX_POOL + 10) * era::POOL_SLOT, 0);
            uint32_t past = 1u << 16;
            *reinterpret_cast<uint32_t **>(huge.data() + (era::MAX_POOL + 5) * era::POOL_SLOT + 8) = &past;
            auto hb = reinterpret_cast<uintptr_t>(huge.data());
            auto capped = era::pool_refs_from(hb, hb + huge.size(), read);
            expect(capped.capped && capped.refs.empty(), "a pool past 8192 slots is counted to 8192 and reported as capped");
            expect(era::pool_refs_from(b0, b0 + 0x18, read).refs.empty() && era::pool_refs_from(0, 0x100, read).refs.empty(),
                   "a misaligned or null pool counts nothing");
        }
        {
            era::SessionTrace bt;
            bt.where = "Sess_Setup_VehicleComponents";
            bt.budget = 15;
            bt.vehicles = 27;
            expect(era::budget_mismatch(bt) ==
                       "Time Trial fill at Sess_Setup_VehicleComponents made 27 vehicles where the budget expected 15",
                   "a fill that made more vehicles than the budget expected logs a line");
            bt.vehicles = 15;
            expect(era::budget_mismatch(bt).empty(), "a fill that matches the budget logs nothing more");
            bt.vehicles = -1;
            bt.gm_session = 27;
            bt.where = "StateLoadSession Exiting";
            expect(!era::budget_mismatch(bt).empty(), "before the vehicle setup the descriptor's participants are compared");
            bt.gm_session = 0;
            expect(era::budget_mismatch(bt).empty(), "an empty descriptor (no fill yet) is not a mismatch");
            bt.budget = -1;
            bt.gm_session = 27;
            expect(era::budget_mismatch(bt).empty(), "outside a budgeted Time Trial nothing is compared");

            int64_t previous = 25;
            bool any = false;
            for (auto [where, seen] : {std::pair<const char *, int64_t>{"StateLoadSession Entering", 25},
                                       {"StateLoadSession Updating", 25},
                                       {"StateLoadSession Exiting", 15},
                                       {"StateSession Entering", 15}}) {
                era::SessionTrace st;
                st.where = where;
                st.budget = 15;
                st.gm_session = seen;
                st.previous = previous;
                any = any || !era::budget_mismatch(st).empty();
                previous = seen;
            }
            expect(!any, "a Time Trial after a 2012 GP (participants 25, 25, then the fill's 15, 15) logs no mismatch: "
                         "the GP's 25 before the fill are not compared");
            era::SessionTrace early;
            early.where = "StateLoadSession Entering";
            early.budget = 15;
            early.gm_session = 27;
            early.previous = 25;
            expect(!era::budget_mismatch(early).empty(), "a count that changed before Entering (a fill there) is compared");
            early.where = "StateLoadSession Exiting";
            early.gm_session = 25;
            early.previous = 25;
            expect(!era::budget_mismatch(early).empty(), "from Exiting on, any count is compared, changed or not");
            expect(era::leaderboard_text(0) == "0, f1" && era::leaderboard_text(1) == "1, classic" &&
                       era::leaderboard_text(4) == "4" && era::leaderboard_text(40) == "40",
                   "stock leaderboard ids print with their names (f1 is 0); others print as the id");
        }
        expect(era::vehicle_flags(era::TAKES_LIST, 0) == era::TAKES_LIST &&
                   era::vehicle_flags(0, era::TAKES_POOL_A) == era::TAKES_POOL_A && era::vehicle_flags(0, 0) == 0,
               "the fill's human record reads the player or the second player flag word, so both are taken together");
        expect(era::cut_recordable(15, 16, false) && !era::cut_recordable(16, 16, false) && era::cut_recordable(16, 16, true),
               "a cut needs a free restore slot, or an entry already holding that mode; with the stack full it is skipped");
        expect(era::LIST_LEADERBOARD == 0xe0 && era::LIST_LEADERBOARD_SET == 0xf0 && era::ENTRANT_VEHICLE == 0x5c &&
                   era::TT_ROOM == 22,
               "a list's leaderboard is +0xe0 (engaged +0xf0), an entrant's vehicle ref +0x5c, and 22 cars fit beside the 4 entries");

        expect(era::trace_flow("StateLoadSession", 1) && era::trace_flow("StateLoadSession", 2) &&
                   era::trace_flow("StateLoadSession", 3) && era::trace_flow("StateSession", 1),
               "the session trace runs at StateLoadSession Entering, Updating and Exiting and at StateSession Entering");
        expect(era::trace_flow("StateSession", 3) && era::trace_flow("StateUnloadSession", 1) &&
                   era::trace_flow("StateUnloadSession", 2) && era::trace_flow("StateUnloadSession", 3),
               "a restart or retry is traced too: StateSession Exiting and every StateUnloadSession step");
        expect(!era::trace_flow("StateLoadSession", 0) && !era::trace_flow("StateSession", 2) &&
                   !era::trace_flow("StateUnloadSession", 0) && !era::trace_flow("#957880c6", 1) && !era::trace_flow("", 1),
               "no trace when a state leaves, while a session runs or on menu flows");
        expect(era::time_trial_cut_off("StateUnloadSession", 3) && !era::time_trial_cut_off("StateUnloadSession", 1) &&
                   !era::time_trial_cut_off("StateUnloadSession", 2) && !era::time_trial_cut_off("StateSession", 3) &&
                   !era::time_trial_cut_off("StateLoadSession", 3),
               "the Time Trial cut comes off at StateUnloadSession Exiting, after the onStateEnter keep check, not at Entering");
        uintptr_t d = 0x200000;
        expect(era::participant_count(d, d + 27 * era::PARTICIPANT) == 27 && era::participant_count(d, d) == 0,
               "a descriptor's participants count in 0x6780-byte steps");
        expect(era::participant_count(0, 0x6780) < 0 && era::participant_count(d, d + 0x6781) < 0 &&
                   era::participant_count(d + 8, d) < 0 && era::participant_count(d, d + 65 * era::PARTICIPANT) < 0,
               "a null, misaligned, reversed or over-long participant vector is unread");
        era::SessionTrace tr;
        tr.where = "StateLoadSession Exiting";
        tr.mode = "time_trial";
        tr.entrants = 3;
        tr.teams = {700, 701, 1};
        tr.list = "f1_custom_team";
        tr.list_cars = 22;
        tr.setup_mode = "f1";
        tr.gm_session = 27;
        tr.module_session = 27;
        std::string tl = era::session_trace_line(tr);
        expect(tl == "session trace StateLoadSession Exiting (time_trial): entrants 3 (teams 700,701,1), list f1_custom_team "
                     "(22 cars) of setup mode f1; participants: game manager 27, game module 27 (next -); session vehicles -",
               "the trace line names the step, mode, entrants with their teams, list, setup mode and participant counts (" +
                   tl + ")");
        expect(era::session_trace_line(era::SessionTrace{}).find("(?): entrants -, list ? (- cars)") != std::string::npos,
               "unread values print as - and ?");
        expect(era::MGR_SESSION_BEGIN == 0x2e0 && era::ENTRANT_TEAM == 0x5a && era::DESC_PARTICIPANTS_BEGIN == 0xad8 &&
                   era::GM_SESSION == 0x1a8 && era::GM_MODULE == 0x140,
               "the trace reads EntrantsManager +0x2e0, an entrant's team at +0x5a and descriptor participants at +0xad8");

        for (const char *m : {"time_trial", "solo_testing"})
            expect(era::time_trial_mode(m), std::string(m) + " is a Time Trial picker");
        for (const char *m : {"grand_prix_23", "grand_prix_splitscreen", "", "my_team_career_25"})
            expect(!era::time_trial_mode(m), std::string("'") + m + "' is not a Time Trial picker");
    }

    expect(era::ADDON_PRESS == 0x8fb68dffu && era::SELECT_PRESS == 0x2b0357au && era::ADDON_TILES == 0xf2614e2bu,
           "djb2 of addon_select_pressed, select_pressed and addon_tiles");

    drs_swap_tests();
    continue_tests();
    setup_guard_tests();
    grid_limit_tests();

    printf("%s\n", fails ? "era_logic_test: FAILED" : "era_logic_test: all passed");
    return fails ? 1 : 0;
}
