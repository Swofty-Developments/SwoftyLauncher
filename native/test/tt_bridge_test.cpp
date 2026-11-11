// Host-native unit test for the Time Trial boards bridge (native/src/core/tt_bridge_logic.hpp): board names, args decoding
// through checked memory, the XML extension, routing, the mailbox JSON, the pending table and the synthetic response.
// Built and run by native/test/run_tt_bridge_test.sh.

#include "tt_bridge_logic.hpp"

#include <cstdio>
#include <map>

namespace tb = runtime::tt_bridge;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Fake {
    std::map<uint64_t, uint8_t> bytes;
    void put(uint64_t at, const void *p, size_t n)
    {
        for (size_t i = 0; i < n; i++)
            bytes[at + i] = static_cast<const uint8_t *>(p)[i];
    }
    template <class T> void put(uint64_t at, T v) { put(at, &v, sizeof v); }
    void cstr(uint64_t at, const std::string &s) { put(at, s.c_str(), s.size() + 1); }
    void ne(uint64_t at, const std::string &s)
    {
        uint8_t h[9] = {};
        h[6] = uint8_t(s.size());
        h[7] = uint8_t(s.size() >> 8);
        put(at, h, 9);
        cstr(at + 9, s);
    }
    static bool read(void *ctx, uint64_t addr, void *out, size_t n)
    {
        auto &f = *static_cast<Fake *>(ctx);
        for (size_t i = 0; i < n; i++) {
            auto it = f.bytes.find(addr + i);
            if (it == f.bytes.end())
                return false;
            static_cast<uint8_t *>(out)[i] = it->second;
        }
        return true;
    }
    tb::Mem mem() { return {&Fake::read, this}; }
};

static std::string sample_xml(const char *eol, bool single_quotes)
{
    std::string q = single_quotes ? "'" : "\"";
    std::string out = std::string("<?xml version=") + q + "1.0" + q + "?>" + eol + "<leaderboards>" + eol;
    auto board = [&](int id, const char *name) {
        out += std::string("  <leaderboard version=") + q + "3" + q + " name=" + q + name + q + " sort=" + q + "ascending" + q + " id=" + q +
               std::to_string(id) + q + " name_pc=" + q + name + q + ">" + eol + "    <column type=" + q + "rating" + q + " name=" + q +
               "time" + q + " stat=" + q + "t" + q + "/>" + eol + "    <reset version=" + q + "1" + q + "/>" + eol + "  </leaderboard>" + eol;
    };
    board(10, "LBV0001D");
    board(11, "LBV0001W");
    board(12, "LBV0002D");
    board(13, "LBV0002W");
    out += std::string("  <leaderboard id=") + q + "40" + q + " name=" + q + "Other" + q + "/>" + eol;
    out += "</leaderboards>";
    out += eol;
    return out;
}

int main()
{
    // names
    auto b = tb::parse_board("LBV0605D");
    expect(b.ok && b.descriptor == 6 && b.track == 5 && b.weather == 'D' && b.addon(), "LBV0605D is an addon board, track 5, dry");
    b = tb::parse_board("LBV0012W");
    expect(b.ok && b.descriptor == 0 && b.track == 12 && b.weather == 'W' && !b.addon(), "LBV0012W is a stock board");
    expect(tb::parse_board("LBV0500D").descriptor == 5 && !tb::parse_board("LBV0500D").addon(), "descriptor 5 is stock");
    expect(tb::parse_board("LBV9907W").addon() && tb::parse_board("LBV9907W").track == 7, "descriptor 99 is an addon");
    expect(!tb::parse_board("LBV0605X").ok && !tb::parse_board("LBV06D").ok && !tb::parse_board("XBV0605D").ok &&
               !tb::parse_board("").ok && !tb::parse_board("LBV06a5D").ok && !tb::parse_board("LBV065D").ok,
           "malformed names do not parse");
    expect(tb::board_name(6, 5, 'D') == "LBV0605D" && tb::board_name(12, 3, 'W') == "LBV1203W", "names format back");

    // garage privilege result
    {
        uint8_t w[64] = {};
        expect(tb::garage_node_offset(w, sizeof w) == -1, "an all-zero control window does not match the garage leaderboard node");
        w[20] = 0x5d, w[21] = 0x2c, w[22] = 0x17, w[23] = 0x57;
        expect(tb::garage_node_offset(w, sizeof w) == 20, "the node id is found at its aligned offset");
        expect(tb::garage_node_offset(w, 23) == -1 && tb::garage_node_offset(w, 24) == 20, "a window that cuts the value short does not match");
        w[20] = 0;
        w[21] = 0x5d, w[22] = 0x2c, w[23] = 0x17, w[24] = 0x57;
        expect(tb::garage_node_offset(w, sizeof w) == -1, "an unaligned copy of the id does not match");
        expect(tb::garage_node_offset(nullptr, 64) == -1 && tb::garage_node_offset(w, 0) == -1 && tb::garage_node_offset(w, 3) == -1, "null and tiny windows never match");
        uint8_t other[16] = {0x5e, 0x2c, 0x17, 0x57, 0xff, 0xff, 0xff, 0xff};
        expect(tb::garage_node_offset(other, sizeof other) == -1, "another node id does not match");
    }
    expect(!tb::track_watch_on(true, false, false, false, false) && !tb::track_watch_on(true, true, true, false, false), "the track watch is off outside a session, whatever is on");
    expect(!tb::track_watch_on(true, true, true, true, false), "a live session that is not a Time Trial does not run the track watch");
    expect(tb::track_watch_on(true, false, false, true, true) && tb::track_watch_on(false, true, false, true, true) && tb::track_watch_on(false, false, true, true, true),
           "a live Time Trial runs the track watch for the bridge, the guards or the diagnostics");
    expect(!tb::track_watch_on(false, false, false, true, true), "a live Time Trial with nothing on does not run it");
    expect(!tb::track_watch_due(false, 10000, 0) && !tb::track_watch_due(true, 10400, 10000) && tb::track_watch_due(true, 10500, 10000) && tb::track_watch_due(true, 500, 0),
           "the watch reads the track twice a second only while on");
    expect(!tb::diag_tick_on(true, false, false) && !tb::diag_tick_on(true, true, false), "the diagnostics tick is off outside a live Time Trial, whatever is on");
    expect(tb::diag_tick_on(true, false, true) && tb::diag_tick_on(false, true, true), "a live Time Trial runs the diagnostics tick for the bridge or the diagnostics");
    expect(!tb::diag_tick_on(false, false, true), "a live Time Trial with nothing on does not run it");
    expect(!tb::diag_tick_due(false, 10000, 0) && !tb::diag_tick_due(true, 10499, 10000) && tb::diag_tick_due(true, 10500, 10000) && tb::diag_tick_due(true, 500, 0),
           "the diagnostics tick runs at most twice a second, and only while on");
    expect(!tb::diag_tick_due(tb::diag_tick_on(true, true, false), 99999, 0), "the diagnostics switch alone never makes the tick due outside a Time Trial");
    expect(tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2038, 0), "a panel with seven tabs is driven once");
    expect(!tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2038, 0x1000, 7), "the same panel is not driven twice");
    expect(!tb::garage_should_drive(false, true, 0x1000, 0x2000, 0x2038, 0) && !tb::garage_should_drive(true, false, 0x1000, 0x2000, 0x2038, 0),
           "never with the bridge off or for stock content");
    expect(!tb::garage_should_drive(true, true, 0, 0x2000, 0x2038, 0) && !tb::garage_should_drive(true, true, 0x1000, 0, 0, 0), "null panel or null list never");
    expect(!tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2000, 0) && !tb::garage_should_drive(true, true, 0x1000, 0x2038, 0x2000, 0), "an empty or inverted list never");
    expect(!tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2003, 0) && !tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2000 + 8 * 65, 0),
           "a misaligned or absurdly long list never");
    expect(tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2018, 0x1000, 1) && !tb::garage_should_drive(true, true, 0x1000, 0x2000, 0x2018, 0x1000, 3),
           "a panel is driven again only when its tab count changed");
    expect(tb::garage_feature_flags(0x5, true, true) == (0x5 | 0x2000000000ull) && tb::garage_feature_flags(0x5, false, true) == 0x5 && tb::garage_feature_flags(0x5, true, false) == 0x5,
           "the leaderboard feature bit is added only for our content with the bridge on");
    expect(tb::garage_feature_flags(0x2000000000ull, true, true) == 0x2000000000ull && tb::garage_feature_flags(0, true, true) == 0x2000000000ull, "adding the bit is idempotent");
    expect(tb::garage_block_byte(1, true, true) == 0 && tb::garage_block_byte(1, false, true) == 1 && tb::garage_block_byte(1, true, false) == 1 && tb::garage_block_byte(0, true, true) == 0,
           "the leaderboard block byte reads 0 only for our content with the bridge on");
    expect(tb::garage_append_tab(0, true, true) && !tb::garage_append_tab(1, true, true), "our content gets only the global garage leaderboard tab");
    expect(tb::garage_append_tab(0, true, false) && tb::garage_append_tab(1, true, false) && tb::garage_append_tab(1, false, true) && tb::garage_append_tab(1, false, false),
           "stock content and the bridge off keep both tabs");
    expect(tb::garage_should_rebuild(true, true, 0x1000, 0, 0x12, 1), "a built time trial panel is rebuilt once for our content");
    expect(!tb::garage_should_rebuild(true, true, 0x1000, 0x1000, 0x12, 1), "not when the builder already ran for it with our content");
    expect(!tb::garage_should_rebuild(false, true, 0x1000, 0, 0x12, 1) && !tb::garage_should_rebuild(true, false, 0x1000, 0, 0x12, 1), "never with the bridge off or for stock content");
    expect(!tb::garage_should_rebuild(true, true, 0x1000, 0, 0x10, 1) && !tb::garage_should_rebuild(true, true, 0, 0, 0x12, 1) && !tb::garage_should_rebuild(true, true, 0x1000, 0, 0x12, 0) &&
               !tb::garage_should_rebuild(true, true, 0x1000, 0, 0x12, 65),
           "never outside a time trial garage, for a null panel, an unbuilt panel or an absurd tab count");
    expect(tb::garage_gate_byte(0, true, true, 0x12) == 1 && tb::garage_gate_byte(1, true, true, 0x12) == 1, "the garage gate byte reads 1 for our content in a time trial garage");
    expect(tb::garage_gate_byte(0, false, true, 0x12) == 0 && tb::garage_gate_byte(0, true, false, 0x12) == 0 && tb::garage_gate_byte(0, true, true, 0x10) == 0 &&
               tb::garage_gate_byte(0, true, true, -1) == 0,
           "it is untouched with the bridge off, for stock content and in other session types");
    expect(tb::replay_capture_due(1000, 2500, 1500) && !tb::replay_capture_due(1000, 2499, 1500) && !tb::replay_capture_due(0, 9999, 1500), "a capture is due after its delay and never without a pending mark");
    {
        uint8_t h[16] = {0, 0, 0, 0, 0xa8, 0xca, 0x2f, 0xe3};
        expect(tb::replay_magic_offset(h, 16) == 4 && tb::replay_magic_offset(h, 7) == -1 && tb::replay_magic_offset(nullptr, 16) == -1, "the replay magic is found at its aligned offset");
        uint8_t z[16] = {};
        expect(tb::replay_magic_offset(z, 16) == -1 && tb::replay_magic_offset(z, 0) == -1, "a file without the magic is rejected");
    }
    expect(tb::replay_capture_name(1700000000, 87893) == "pb_1700000000_87893.replay", "the capture file name carries the time and the lap time");
    expect(tb::replay_capture_allowed(true, true, true, 1, 0x10) && !tb::replay_capture_allowed(false, true, true, 1, 0x10) && !tb::replay_capture_allowed(true, false, true, 1, 0x10) &&
               !tb::replay_capture_allowed(true, true, false, 1, 0x10) && !tb::replay_capture_allowed(true, true, true, 0, 0x10) && !tb::replay_capture_allowed(true, true, true, 150, 0x10) &&
               !tb::replay_capture_allowed(true, true, true, 1, 0),
           "the save starts only behind the switch, the bridge, a genuine replay system, a recording and a job queue, on stock content as on ours");
    expect(tb::replay_file_settled(100, 100, 3, 3) && !tb::replay_file_settled(100, 99, 3, 3) && !tb::replay_file_settled(0, 0, 9, 3) && !tb::replay_file_settled(100, 100, 2, 3),
           "a replay file is settled only when its size stopped changing");
    expect(tb::replay_lap_is_best(88505, 1, 0) && tb::replay_lap_is_best(88505, -1, 90000) && !tb::replay_lap_is_best(88505, 0, 0) && !tb::replay_lap_is_best(90000, 1, 88505) &&
               !tb::replay_lap_is_best(88505, 1, 88505) && !tb::replay_lap_is_best(0, 1, 0),
           "a lap marks a replay save only when valid and a new session best");
    {
        tb::ReplayManagerView m;
        m.vtable20 = 0x142000000ull, m.system40 = 0x317a560, m.lock118 = 0x32e0000, m.queue150 = 0x32e4000, m.flag6e1 = 1;
        expect(tb::replay_manager_plausible(m, 0x317a560, 0x140000000ull, 0x160000000ull, true, true, true), "a manager that links the system and has sane fields is plausible");
        tb::ReplayManagerView bad = m;
        bad.system40 = 0x1;
        expect(!tb::replay_manager_plausible(bad, 0x317a560, 0x140000000ull, 0x160000000ull, true, true, true), "not when +0x40 is not the system");
        bad = m, bad.flag6e1 = 150;
        expect(!tb::replay_manager_plausible(bad, 0x317a560, 0x140000000ull, 0x160000000ull, true, true, true), "not when the flag byte is not a bool (the GameReplaySystem itself reads 150 there)");
        bad = m, bad.vtable20 = 0x3f80000000000000ull;
        expect(!tb::replay_manager_plausible(bad, 0x317a560, 0x140000000ull, 0x160000000ull, true, true, true), "not when +0x20 is data instead of a vtable (the crash)");
        expect(!tb::replay_manager_plausible(m, 0x317a560, 0x140000000ull, 0x160000000ull, false, true, true) && !tb::replay_manager_plausible(m, 0, 0x140000000ull, 0x160000000ull, true, true, true) &&
                   !tb::replay_manager_plausible(m, 0x317a560, 0x140000000ull, 0x160000000ull, true, false, true) && !tb::replay_manager_plausible(m, 0x317a560, 0x140000000ull, 0x160000000ull, true, true, false),
               "unreadable pieces or no system are refused");
    }
    expect(tb::replay_ui_suppressed(5000, 4999) && !tb::replay_ui_suppressed(5000, 5000) && !tb::replay_ui_suppressed(0, 1), "the save messages are suppressed only inside our window");
    expect(!tb::replay_precheck_blocks(true, 0x10, 0x20, 0x30, true) && tb::replay_precheck_blocks(true, 0, 0x20, 0x30, true) && tb::replay_precheck_blocks(true, 0x10, 0, 0x30, true) &&
               tb::replay_precheck_blocks(true, 0x10, 0x20, 0, true) && !tb::replay_precheck_blocks(false, 0, 0, 0x30, true) && !tb::replay_precheck_blocks(false, 0, 0, 0, false),
           "the pre-check blocks a missing buffer or session but never an undecoded module list");
    expect(tb::garage_result(0, true, true) == 1, "a denied garage privilege becomes granted for our content");
    expect(tb::garage_result(0, false, true) == 0 && tb::garage_result(0, true, false) == 0, "it stays denied with the bridge off or for stock content");
    expect(tb::garage_result(1, true, true) == 1 && tb::garage_result(0xff, true, true) == 0xff && tb::garage_result(2, true, true) == 2,
           "a granted, unreadable or unknown value is left alone");

    // methods and routing
    expect(tb::method_of("GetTTLeaderboard") == tb::Method::get_tt && tb::method_of("Nope") == tb::Method::none &&
               tb::method_of("UpdateLeaderboardEntryBinaryData") == tb::Method::update_binary,
           "methods by name");
    auto addon = tb::parse_board("LBV0605D"), stock = tb::parse_board("LBV0105D");
    expect(tb::route_of(tb::Method::get_tt, addon, true) == tb::Route::mailbox, "addon read with the bridge on goes to the mailbox");
    expect(tb::route_of(tb::Method::add_entry, addon, true) == tb::Route::mailbox, "addon write with the bridge on goes to the mailbox");
    expect(tb::route_of(tb::Method::get_tt, stock, true) == tb::Route::pass, "a stock board passes untouched");
    expect(tb::route_of(tb::Method::add_entry, addon, false) == tb::Route::local && tb::route_of(tb::Method::update_binary, addon, false) == tb::Route::local,
           "addon writes with the bridge off are answered locally");
    expect(tb::route_of(tb::Method::get_binary, addon, false) == tb::Route::pass, "addon reads with the bridge off pass");
    expect(tb::route_of(tb::Method::none, addon, true) == tb::Route::pass, "an unknown method passes");
    expect(std::string(tb::local_body(tb::Method::add_entry)) == "{\"Rank\":0}" && std::string(tb::local_body(tb::Method::update_binary)) == "{}",
           "local answers");
    expect(tb::addon_content("swofty.swofty-2022/f1_2022") && !tb::addon_content("game/f1_2025") && !tb::addon_content(""), "guards treat non-game content as addon");

    // memory and strings: GetTTLeaderboard args through a NeStr pointer
    Fake f;
    uint64_t op = 0x10000, args = 0x20000;
    f.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    f.put<uint64_t>(args + 8, 0x50000);
    f.ne(0x50000, "LBV0605D");
    f.put<int32_t>(args + 0x30, 3);
    f.put<int32_t>(args + 0x34, 0);
    f.put<int64_t>(args + 0x38, 0);
    f.put<int64_t>(args + 0x40, 9);
    f.put<int32_t>(args + 0x48, 3);
    f.put<int32_t>(args + 0x4c, 1);
    auto a = tb::decode_args(f.mem(), op, tb::Method::get_tt);
    expect(a.ok && a.board_text == "LBV0605D" && a.form.form == tb::Form::ne_str && a.form.word == 0, "NeStr slot decodes and names its form");
    expect(a.values["LeaderboardID"].as_string() == "LBV0605D" && a.values["Version"].as_int() == 3 && a.values["ToIndex"].as_int() == 9 &&
               a.values["TopResultCount"].as_int() == 3 && a.values["CrossPlay"].as_int() == 1 && a.values["FromIndex"].as_int() == 0,
           "every GetTTLeaderboard key is read");

    // char* slot, second word
    Fake g;
    g.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    g.put<uint64_t>(args + 8 + 8, 0x60000);
    g.cstr(0x60000, "LBV0701W");
    g.put<int32_t>(args + 0x30, 1);
    auto a2 = tb::decode_args(g.mem(), op, tb::Method::get_tt);
    expect(a2.ok && a2.board.descriptor == 7 && a2.form.form == tb::Form::c_str && a2.form.word == 1, "a char* in the slot's second word decodes");

    // inline
    Fake h;
    h.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    h.cstr(args + 8, "LBV0606D");
    auto a3 = tb::decode_args(h.mem(), op, tb::Method::get_tt);
    expect(a3.ok && a3.form.form == tb::Form::inline_c && a3.board.track == 6, "an inline C string decodes");

    // unreadable and garbage
    Fake none;
    expect(!tb::decode_args(none.mem(), op, tb::Method::get_tt).ok, "unreadable args are not ok");
    Fake junk;
    junk.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    junk.put<uint64_t>(args + 8, 0x70000);
    junk.cstr(0x70000, "not a board");
    expect(!tb::decode_args(junk.mem(), op, tb::Method::get_tt).ok, "a string that is no board name is not ok");
    Fake nullp;
    nullp.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    nullp.put<uint64_t>(args + 8, 0);
    expect(!tb::decode_args(nullp.mem(), op, tb::Method::get_tt).ok, "a null string pointer is not ok");
    Fake binstr;
    binstr.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    binstr.put<uint64_t>(args + 8, 0x80000);
    uint8_t bad[] = {1, 2, 3, 4, 0, 0, 8, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8};
    binstr.put(0x80000, bad, sizeof bad);
    expect(!tb::decode_args(binstr.mem(), op, tb::Method::get_tt).ok, "non-printable characters are refused");

    // other methods' tables
    Fake m;
    m.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    m.put<uint64_t>(args + 0x48, 0x51000);
    m.ne(0x51000, "LBV0605D");
    m.put<uint64_t>(args + 8, 0x52000);
    m.ne(0x52000, "steam123");
    m.put<int32_t>(args + 0x30, 2);
    m.put<int32_t>(args + 0x70, 5);
    auto gb = tb::decode_args(m.mem(), op, tb::Method::get_binary);
    expect(gb.ok && gb.board_text == "LBV0605D" && gb.values["PlatformID"].as_string() == "steam123" && gb.values["Platform"].as_int() == 2 &&
               gb.values["Version"].as_int() == 5,
           "GetBinaryDataForUser reads LeaderboardID at +0x48 and PlatformID at +8");
    Fake u;
    u.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    u.put<uint64_t>(args + 0x10, 0x53000);
    u.ne(0x53000, "LBV0605D");
    u.put<int32_t>(args + 8, 4);
    auto up = tb::decode_args(u.mem(), op, tb::Method::update_binary);
    expect(up.ok && up.values["Version"].as_int() == 4 && !up.values["BinaryDataBase64"].is_string(), "UpdateLeaderboardEntryBinaryData has no binary key");
    Fake ad;
    ad.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    ad.put<uint64_t>(args + 8, 0x54000);
    ad.ne(0x54000, "LBV0605W");
    ad.put<int64_t>(args + 0x38, 91234);
    ad.put<int32_t>(args + 0xec, 1);
    auto ae = tb::decode_args(ad.mem(), op, tb::Method::add_entry);
    expect(ae.ok && ae.values["Score"].as_int() == 91234 && ae.values["CrossPlay"].as_int() == 1 && ae.board.weather == 'W', "AddLeaderboardEntry reads Score and CrossPlay");
    Fake fr;
    fr.put<uint64_t>(op + tb::ARGS_AT_OP, args);
    fr.put<uint64_t>(args + 8, 0x55000);
    fr.ne(0x55000, "LBV0605D");
    fr.put<uint64_t>(args + 0x38, 0x56000);
    fr.ne(0x56000, "a,b");
    fr.put<int32_t>(args + 0xa4, 1);
    auto fa = tb::decode_args(fr.mem(), op, tb::Method::get_friends_tt);
    expect(fa.ok && fa.values["CommaSeparatedFriends"].as_string() == "a,b" && fa.values["CrossPlay"].as_int() == 1, "GetFriendsTTLeaderboard reads the friends list and CrossPlay");

    // method at op
    Fake mo;
    mo.put<uint64_t>(op + tb::OP_METHOD, 0x57000);
    mo.cstr(0x57000, "GetTTLeaderboard");
    std::string name;
    expect(tb::method_at_op(mo.mem(), op, &name) == tb::Method::get_tt && name == "GetTTLeaderboard", "the method name at op+0x118 is read");
    Fake mo2;
    mo2.put<uint64_t>(op + tb::OP_METHOD, 0x57000);
    mo2.cstr(0x57000, "UpdateSession");
    expect(tb::method_at_op(mo2.mem(), op, &name) == tb::Method::none && name == "UpdateSession", "another method is named and not redirected");

    // XML
    for (const char *eol : {"\n", "\r\n"})
        for (bool sq : {false, true}) {
            std::string x = sample_xml(eol, sq);
            tb::XmlReport r;
            std::string y = tb::extend_xml(x, {6, 7, 5, 300}, {}, r);
            auto els = tb::leaderboard_elements(y);
            std::set<int64_t> ids;
            std::set<std::string> names;
            bool dup = false;
            for (auto &e : els) {
                dup |= !ids.insert(e.id_value).second || !names.insert(e.name).second;
            }
            std::string label = std::string(eol[1] ? "CRLF" : "LF") + (sq ? ", single quotes" : ", double quotes");
            expect(r.changed && r.added == 8 && els.size() == 5 + r.added, ("clones every descriptor, track and weather: " + label).c_str());
            expect(!dup && r.first_id == 41 && r.last_id == 40 + int64_t(r.added), ("ids are unique and above every stock id: " + label).c_str());
            expect(names.count("LBV0601D") && names.count("LBV0702W") && !names.count("LBV0501D") && !names.count("LBV30001D"), ("names follow the board format: " + label).c_str());
            expect(y.find("</leaderboards>") != std::string::npos && y.rfind("</leaderboards>") > y.rfind("LBV0702W") &&
                       y.compare(0, x.size() - 20, x, 0, x.size() - 20) == 0,
                   ("clones sit before the container close: " + label).c_str());
            std::string eolc = eol;
            expect(eolc != "\r\n" || (y.find("\n") != std::string::npos && y.find("\n\n") == std::string::npos && y.find("\r\r") == std::string::npos &&
                                      [&] { for (size_t i = 0; i < y.size(); i++) if (y[i] == '\n' && (i == 0 || y[i - 1] != '\r')) return false; return true; }()),
                   ("CRLF buffers stay CRLF: " + label).c_str());
            auto d = tb::leaderboard_elements(y);
            bool kids = true;
            for (auto &e : d)
                if (e.name.rfind("LBV06", 0) == 0) {
                    std::string body = y.substr(e.begin, e.end - e.begin);
                    kids = kids && body.find("name_pc=") != std::string::npos && body.find(e.name) != std::string::npos &&
                           body.find("rating") != std::string::npos && body.find("<reset") != std::string::npos && body.find("LBV0001D") == std::string::npos;
                }
            expect(kids, ("clones keep the children and replace every copy of the name: " + label).c_str());
        }

    {
        std::string x = "<leaderboards><leaderboard id=\"3\" name=\"LBV0001D\" sort=\"ascending\"/></leaderboards>";
        tb::XmlReport r;
        std::string y = tb::extend_xml(x, {6}, {}, r);
        auto els = tb::leaderboard_elements(y);
        expect(r.changed && els.size() == 3 && els[1].self_closing && els[1].name == "LBV0601D" && els[1].id_value == 4 && els[2].name == "LBV0601W",
               "a self-closing template is cloned self-closing");
        expect(y.find("</leaderboards>") == y.size() - 15, "the container close stays last");
    }
    {
        std::string x = "<leaderboards><leaderboard id=\"3\" name=\"LBV0001D\" sort=\"ascending\"/><leaderboard id=\"4\" name=\"LBV0205W\" sort=\"ascending\"/></leaderboards>";
        tb::XmlReport r;
        std::string y = tb::extend_xml(x, {}, {}, r, true);
        std::set<std::string> names;
        for (auto &e : tb::leaderboard_elements(y))
            names.insert(e.name);
        expect(r.changed && names.count("LBV0105D") && names.count("LBV0101W") && names.count("LBV0501D") && names.count("LBV0201D") && names.count("LBV0001W") &&
                   !names.count("LBV0601D") && r.added == 6 * 2 * 2 - 2,
               "a game descriptor gets the boards of every track the file names that it lacks");
        tb::XmlReport r2;
        expect(tb::extend_xml(y, {}, {}, r2, true) == y && !r2.changed, "filling the game descriptors twice adds nothing");
        tb::XmlReport r3;
        expect(tb::extend_xml(x, {}, {}, r3) == x && !r3.changed, "without the switch a game descriptor is never filled");
    }
    {
        tb::XmlReport r;
        std::string x = "<leaderboards><leaderboard name=\"Other\" id=\"1\"/></leaderboards>";
        expect(tb::extend_xml(x, {6}, {}, r) == x && !r.changed && !r.why.empty(), "a buffer with no LBV template is left alone and says why");
        std::string none = "<root/>";
        expect(tb::extend_xml(none, {6}, {}, r) == none && !r.changed, "a buffer with no leaderboard is left alone");
        std::string ok = sample_xml("\n", false);
        expect(tb::extend_xml(ok, {}, {}, r) == ok && !r.changed && r.why.find("no addon descriptor") == 0, "no assigned descriptors leaves it alone");
        expect(tb::extend_xml(ok, {1, 2, 5}, {}, r) == ok, "stock descriptors are never cloned");
        std::string once = tb::extend_xml(ok, {6}, {}, r);
        tb::XmlReport r2;
        expect(tb::extend_xml(once, {6}, {}, r2) == once && r2.existing == 4 && !r2.changed, "extending twice adds nothing");
        std::string odd = "<leaderboards>\n<leaderboards_extra/>\n<leaderboard\n  name = \"LBV0001D\"\n  id = \"7\"\n>\n<column type=\"rating\"/>\n</leaderboard>\n</leaderboards>\n";
        std::string y = tb::extend_xml(odd, {9}, {}, r);
        auto els = tb::leaderboard_elements(y);
        expect(r.changed && els.size() == 3 && els[1].id_value == 8 && els[1].name == "LBV0901D", "spaces around = and attributes on their own lines parse");
    }

    // custom tracks
    {
        namespace at = swofty_core::apply::tracks;
        at::Base base;
        auto row = [&](const char *id, const char *code, int stat, at::Kind kind, const char *addon, const char *name) {
            at::BaseTrack t;
            t.id = id;
            t.code = code;
            t.stat_id = stat;
            t.kind = kind;
            t.addon = addon;
            t.name = name;
            base.tracks.push_back(t);
        };
        row("game/bahrain", "bahrain", 1, at::Kind::stock, "game", "Bahrain");
        row("Swofty.Tracks/Ring", "ring", 7, at::Kind::record, "Swofty.Tracks", "The Ring");
        row("other.older/ring", "ring", 8, at::Kind::older, "other.older", "Old Ring");
        row("swofty.fill/china", "china", 2, at::Kind::record, "swofty.fill", "Filled China");
        auto unready = tb::board_track(base, "ring");
        expect(!unready.custom && !unready.known && unready.key == "ring", "a base that is not ready gives the lowercased code, not custom");
        base.ready = true;
        auto st = tb::board_track(base, "Bahrain");
        expect(st.known && !st.custom && st.key == "bahrain" && st.addon.empty() && st.kind.empty() && st.name == "Bahrain" && st.stat_id == 1,
               "a stock track keys by its code");
        auto cu = tb::board_track(base, "ring");
        expect(cu.known && cu.custom && cu.key == "swofty.tracks:ring" && cu.addon == "Swofty.Tracks" && cu.kind == "record" && cu.name == "The Ring" && cu.stat_id == 7,
               "a record track keys by addon id and code, lowercased");
        auto hint = tb::board_track(base, "ring", 8);
        expect(hint.key == "other.older:ring" && hint.kind == "older", "a stat id hint picks between rows that share a code; kind older keys the same way");
        expect(tb::board_track(base, "nowhere").key == "nowhere" && !tb::board_track(base, "nowhere").custom && !tb::board_track(base, "nowhere").known,
               "an unknown code keys by the lowercased code");
        expect(tb::board_track(base, "").key.empty() && !tb::board_track(base, "").custom, "an empty code gives an empty key");
        auto lb = tb::parse_board("LBV0007D");
        expect(tb::request_track(base, lb, "bahrain").key == "swofty.tracks:ring", "a request resolves the track from the board name's track number");
        expect(tb::request_track(base, tb::parse_board("LBV0001D"), "ring").key == "bahrain", "a stock track number wins over the session's track");
        expect(tb::request_track(base, tb::parse_board("LBV0055D"), "ring").key == "swofty.tracks:ring", "an unknown track number falls back to the session's track");
        expect(tb::request_track(base, tb::BoardName{}, "").key.empty(), "no board and no session track gives no key");
        expect(tb::ours(6, false) && tb::ours(0, true) && tb::ours(6, true) && !tb::ours(0, false) && !tb::ours(5, false), "ours: an addon descriptor or a custom track");
        expect(tb::ours_session("swofty.x/f1_2022", false) && tb::ours_session("game/f1_2025", true) && !tb::ours_session("game/f1_2025", false) && !tb::ours_session("", false),
               "guards use the same rule through the session's content");
        auto stock0 = tb::parse_board("LBV0001D");
        expect(tb::route_of(tb::Method::get_tt, stock0, true, true) == tb::Route::mailbox, "a stock descriptor on a custom track goes to the mailbox");
        expect(tb::route_of(tb::Method::add_entry, stock0, false, true) == tb::Route::local && tb::route_of(tb::Method::get_tt, stock0, false, true) == tb::Route::pass,
               "a stock descriptor on a custom track is answered locally for writes with the bridge off");
        expect(tb::route_of(tb::Method::get_tt, stock0, true, false) == tb::Route::pass && tb::route_of(tb::Method::get_tt, tb::BoardName{}, true, true) == tb::Route::pass,
               "a stock track on a stock descriptor and an unparsed board pass");
        auto filled = tb::request_track(base, tb::parse_board("LBV0002D"), "");
        expect(filled.custom && filled.key == "swofty.fill:china" && tb::route_of(tb::Method::get_tt, tb::parse_board("LBV0002D"), true, filled.custom) == tb::Route::mailbox,
               "the request for a dormant stock row a track fills is still ours");

        for (const char *eol : {"\n", "\r\n"}) {
            std::string x = sample_xml(eol, false);
            tb::XmlReport r;
            std::string y = tb::extend_xml(x, {6}, {7, 2, -1, 7}, r);
            auto els = tb::leaderboard_elements(y);
            std::set<std::string> names;
            std::set<int64_t> ids;
            bool dup = false;
            for (auto &e : els)
                dup |= !ids.insert(e.id_value).second || !names.insert(e.name).second;
            expect(r.changed && !dup && r.custom_tracks == 2 && r.stock_descriptors == 1, "custom stat ids are counted once, negatives dropped");
            expect(names.count("LBV0007D") && names.count("LBV0007W"), "a custom track gets boards under the stock descriptor 00");
            expect(names.count("LBV0607D") && names.count("LBV0607W") && names.count("LBV0601D") && names.count("LBV0602W"),
                   "a custom track gets boards under an addon descriptor next to the stock tracks");
            size_t count_2 = 0;
            for (auto &e : els)
                count_2 += e.name == "LBV0002D";
            expect(count_2 == 1 && r.existing == 2 && names.count("LBV0602D"), "a custom track on a dormant stock row adds no duplicate under the stock descriptor");
            expect(r.added == 4 + 2 + 2 && r.custom_added == 6 && r.first_id == 41 && r.last_id == 48, "added boards and ids are counted");
            tb::XmlReport r2;
            expect(tb::extend_xml(y, {6}, {7, 2}, r2) == y && !r2.changed && r2.added == 0, "extending with custom tracks twice adds nothing");
            tb::XmlReport r3;
            std::string z = tb::extend_xml(x, {}, {7}, r3);
            expect(r3.changed && r3.added == 2 && r3.descriptors == 0 && tb::leaderboard_elements(z).size() == 7, "custom tracks alone extend the stock descriptors");
            tb::XmlReport r4;
            expect(tb::extend_xml(x, {}, {}, r4) == x && !r4.changed, "no descriptors and no custom tracks leaves the buffer alone");
        }
    }

    // mailbox
    std::string req = tb::request_json(12, tb::Method::get_tt, a.values, "swofty.swofty-2022/f1_2022", "bahrain", a.board, a.board_text, "QUJD");
    auto parsed = swofty::json::parse(req);
    expect(parsed["seq"].as_int() == 12 && parsed["method"].as_string() == "GetTTLeaderboard" && parsed["args"]["LeaderboardID"].as_string() == "LBV0605D" &&
               parsed["board"]["content"].as_string() == "swofty.swofty-2022/f1_2022" && parsed["board"]["track"].as_string() == "bahrain" &&
               parsed["board"]["weather"].as_string() == "dry" && parsed["board"]["game_board"].as_string() == "LBV0605D" &&
               parsed["player"]["ghost_id"].as_string() == "QUJD",
           "the request has the contract's shape");
    expect(!swofty::json::parse(tb::request_json(1, tb::Method::get_tt, a.values, "c", "t", a.board, a.board_text, ""))["player"]["ghost_id"].is_string(), "no ghost id key when none is known");
    expect(tb::base64(reinterpret_cast<const uint8_t *>("ABC"), 3) == "QUJD" && tb::base64(reinterpret_cast<const uint8_t *>("AB"), 2) == "QUI=" &&
               tb::base64(reinterpret_cast<const uint8_t *>("A"), 1) == "QQ==",
           "base64 pads");
    auto ans = tb::parse_answer("{\"seq\":12,\"status\":200,\"body\":\"{\\\"Rank\\\":3}\"}", 12);
    expect(ans && ans->status == 200 && ans->body == "{\"Rank\":3}", "an answer parses");
    expect(!tb::parse_answer("{\"seq\":13,\"status\":200,\"body\":\"{}\"}", 12), "an answer for another seq is refused");
    expect(!tb::parse_answer("{\"seq\":12,\"status\":99,\"body\":\"{}\"}", 12) && !tb::parse_answer("{\"seq\":12,\"status\":200}", 12) &&
               !tb::parse_answer("nonsense", 12) && !tb::parse_answer("{\"seq\":12,\"status\":200,\"body\":{}}", 12),
           "bad statuses, missing and non-string bodies, and garbage are refused");

    // pending table
    tb::Table t(100);
    uint64_t s1 = t.add(0xaaa, tb::Method::get_tt, a, 1000, false);
    uint64_t s2 = t.add(0xbbb, tb::Method::add_entry, ae, 1000, true);
    expect(s1 == 100 && s2 == 101 && t.unsent().size() == 1 && t.waiting().size() == 0, "a local entry is already sent and answered");
    t.unsent()[0]->sent = true;
    expect(t.waiting().size() == 1, "a sent request waits");
    auto due = t.take_due(1500);
    expect(due.size() == 1 && due[0].op == 0xbbb && due[0].status == 200 && due[0].body == "{\"Rank\":0}" && due[0].why == tb::Completion::local, "the local answer completes at once");
    expect(t.take_due(1500).empty() && t.size() == 1, "nothing else is due before the answer or the deadline");
    expect(t.answer(s1, 200, "{}") && !t.answer(s1, 200, "{}") && !t.answer(999, 200, "{}"), "an answer is taken once, for a known seq");
    due = t.take_due(1600);
    expect(due.size() == 1 && due[0].why == tb::Completion::answered && due[0].status == 200 && t.size() == 0, "the answer completes the op");
    t.add(0xccc, tb::Method::get_tt, a, 2000, false);
    t.unsent()[0]->sent = true;
    expect(t.take_due(2000 + tb::TIMEOUT_MS - 1).empty(), "no timeout before 8 s");
    due = t.take_due(2000 + tb::TIMEOUT_MS);
    expect(due.size() == 1 && due[0].why == tb::Completion::timed_out && due[0].status == 504 && due[0].waited_ms == tb::TIMEOUT_MS &&
               swofty::json::parse(due[0].body)["Message"].is_string(),
           "an unanswered request times out at 8 s with a 504 error body");
    tb::Table full(1);
    for (size_t i = 0; i < tb::MAX_PENDING + 3; i++) {
        full.add(i + 1, tb::Method::get_tt, a, 0, false);
        full.unsent()[0]->sent = true;
    }
    auto drop = full.take_due(1);
    expect(drop.size() == 3 && drop[0].status == 503 && full.size() == tb::MAX_PENDING, "past the cap the oldest requests are failed at once");

    // response struct
    auto resp = std::make_unique<tb::Response>();
    resp->fill(200, "{\"Rank\":3}");
    uint32_t status;
    memcpy(&status, resp->raw + 0x9c, 4);
    uint64_t body_ptr;
    memcpy(&body_ptr, resp->raw + 0xa0, 8);
    auto *obj = reinterpret_cast<uint64_t *>(body_ptr);
    expect(resp->raw[0x88] == 0 && resp->raw[0x98] == 0 && status == 200, "the response's flags and status sit at +0x88, +0x98, +0x9c");
    expect(obj[0] == reinterpret_cast<uint64_t>(resp->data.c_str()) && obj[2] == 10 && std::string(reinterpret_cast<char *>(obj[0]), obj[2]) == "{\"Rank\":3}",
           "+0xa0 points at {data, ?, length}");
    resp->fill(504, "x");
    memcpy(&status, resp->raw + 0x9c, 4);
    expect(status == 504 && reinterpret_cast<uint64_t *>(resp->raw + 0xa0)[0] == body_ptr && static_cast<uint64_t *>(reinterpret_cast<void *>(body_ptr))[2] == 1,
           "a refill reuses the object and the new length");
    expect(sizeof(tb::Response::raw) == 0xb0, "the response is 0xb0 bytes");

    // online-state gate of the F1os method starts
    {
        std::vector<uint8_t> real;
        for (const char *p = "48 8B 8E 00 01 00 00 48 8B 49 40 48 8B 89 40 01 00 00 48 8B 89 98 01 00 00 48 85 C9 0F 84 EA 00 00 00 8B 91 C8 01 00 00 F7 D2 F6 C2 03 0F 85 D9 00 00 00 48 83 B9 C0 01 00 00 00 0F 84 CB 00 00 00 83 FF 02 0F 85 EF 00 00 00 83 F8 02 0F 85 F0 00 00 00 48 8B 05 1C 6B 0A 03 48 8B 80 E0 04 00 00 80 B8 90 00 02 00 00 0F 84 DF 00 00 00 48 8B 80 50 01 02 00 48 85 C0 0F 84 CF 00 00 00 80 38 00 0F 84 C6 00 00 00 80 7E 32 00"; *p;) {
            if (*p == ' ') { p++; continue; }
            real.push_back(uint8_t(std::stoi(std::string(p, 2), nullptr, 16)));
            p += 2;
        }
        expect(real.size() == tb::gate::TEMPLATE_LEN && tb::gate::pattern().ok, "the gate template parses and the real bytes are its length");
        std::vector<uint8_t> buf(0x3000, 0x90);
        auto put = [&](size_t at) { memcpy(buf.data() + at, real.data(), real.size()); };
        put(0x100);
        put(0x1800);
        auto matches = tb::gate::find_all(buf.data(), buf.size());
        expect(matches.size() == 2 && matches[0] == 0x100 && matches[1] == 0x1800, "the real GetTTLeaderboard gate bytes are found twice in a padded buffer");
        auto site = tb::gate::site_of(matches[0]);
        expect(site.hook == 0x100 + 25 && buf[site.hook] == 0x48 && buf[site.hook + 1] == 0x85 && buf[site.hook + 2] == 0xc9, "the hook site is the TEST RCX,RCX of the presence record");
        expect(buf[site.resume] == 0x80 && buf[site.resume + 1] == 0x7e && buf[site.resume + 2] == 0x32, "the resume site is the first compare after the gate (cmp byte [rsi+0x32], 0)");
        expect(tb::gate::session_global(buf.data(), matches[0]) == 0x100 + 83 + 7 + 0x030A6B1C, "the session block global is the rip-relative load inside the gate");
        expect(tb::gate::find_all(buf.data(), 100).empty(), "a buffer shorter than the template has no match");
        auto broken = buf;
        broken[0x100 + 33] ^= 0xff;
        broken[0x100 + 25] ^= 0xff;
        expect(tb::gate::find_all(broken.data(), broken.size()).size() == 1, "one changed fixed byte drops the match, a wildcard byte does not");
        auto lea = [&](size_t at, int64_t target) {
            buf[at] = 0x48; buf[at + 1] = 0x8d; buf[at + 2] = 0x05;
            int32_t rel = int32_t(target - int64_t(at) - 7);
            memcpy(buf.data() + at + 3, &rel, 4);
        };
        lea(0x900, 0x2400);
        lea(0x1f00, 0x2800);
        auto own = tb::gate::owners(buf.data(), buf.size(), matches, {0x2400, 0x2800, 0x2900, -1});
        expect(own.size() == 4 && own[0] == 0 && own[1] == 1 && own[2] == -1 && own[3] == -1, "each OnResult belongs to the function whose gate precedes its lea");
        lea(0x1900, 0x2400);
        own = tb::gate::owners(buf.data(), buf.size(), matches, {0x2400});
        expect(own[0] == -1, "an OnResult taken by two functions has no owner");
    }
    {
        tb::gate::State s;
        s.presence_ok = tb::gate::presence_ok(3, 7);
        s.s60 = 2;
        s.s64 = 0;
        s.session_ok = false;
        expect(tb::gate::first_failure(s) == tb::gate::Why::overlay, "with presence and S+0x60 right and S+0x64 wrong the first failure is 9, the overlay popup");
        s.s64 = 2;
        expect(tb::gate::first_failure(s) == tb::gate::Why::session, "then the missing session token");
        s.session_ok = true;
        expect(tb::gate::first_failure(s) == tb::gate::Why::none, "all four pass");
        s.s60 = 1;
        expect(tb::gate::first_failure(s) == tb::gate::Why::login, "S+0x60 first");
        s.presence_ok = tb::gate::presence_ok(1, 7);
        expect(tb::gate::first_failure(s) == tb::gate::Why::presence && !tb::gate::presence_ok(3, 0), "presence needs both flag bits and a user");
        expect(std::string(tb::gate::account_state_name(9)) == "eEAAccountSteamOverlayDisabled" && std::string(tb::gate::account_state_name(99)) == "unknown", "account state names");
        expect(tb::gate::account_error_quiet(true, 8) && tb::gate::account_error_quiet(true, 9) && tb::gate::account_error_quiet(true, 2),
               "with the bridge on the login error and overlay popups, and server not accessible, are not shown");
        expect(!tb::gate::account_error_quiet(true, 10),
               "a hostile session runs the game's handler, which sets the online-services error code the privilege check waits for");
        expect(!tb::gate::account_error_quiet(true, 4) && !tb::gate::account_error_quiet(true, 5) && !tb::gate::account_error_quiet(true, 7) && !tb::gate::account_error_quiet(true, 3) &&
                   !tb::gate::account_error_quiet(true, 6) && !tb::gate::account_error_quiet(true, 0) && !tb::gate::account_error_quiet(true, 1) && !tb::gate::account_error_quiet(true, 99),
               "the states Steam copies hit and every other state are left to the game");
        expect(!tb::gate::account_error_quiet(false, 8) && !tb::gate::account_error_quiet(false, 9), "with the bridge off nothing is quiet");
    }

    // log cap and kill switch
    expect(tb::say(0) == tb::Say::none && tb::say(1) == tb::Say::line && tb::say(6) == tb::Say::last && tb::say(7) == tb::Say::none, "log lines are capped");
    expect(tb::switch_off("0") && tb::switch_off("off") && !tb::switch_off("1") && !tb::switch_off("") && !tb::switch_off(nullptr) && !tb::switch_off("00"), "env switches parse");

    std::printf(failed ? "%d FAILED\n" : "all passed\n", failed);
    return failed ? 1 : 0;
}
