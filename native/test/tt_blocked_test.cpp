// Host unit test for the Time Trial block (core/frontend/tt_blocked_logic.hpp): reading swofty\tt.json, when Time Trial
// is blocked, the read done once off the game thread, the refused container command and the popup gates.
// native/test/run_tests.sh tt_blocked builds and runs it.
#include "core/frontend/tt_blocked_logic.hpp"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <functional>
#include <string>
#include <thread>

namespace b = frontend::tt_blocked;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

using Clock = std::chrono::steady_clock;

static double ms_since(Clock::time_point t) { return std::chrono::duration<double, std::milli>(Clock::now() - t).count(); }

// A drive that stalls: tt.json takes STALL_MS to read. The game thread asks why() every millisecond for longer than the
// old two-second re-read period; no call may wait on the read, the read runs once and never on the asking thread.
static void stalled_drive()
{
    constexpr int STALL_MS = 600;
    constexpr int ASK_MS = 2300;
    std::atomic<int> reads{0};
    std::atomic<bool> on_asker{false};
    const std::thread::id asker = std::this_thread::get_id();
    b::Reading r([&] {
        reads++;
        on_asker = std::this_thread::get_id() == asker;
        std::this_thread::sleep_for(std::chrono::milliseconds(STALL_MS));
        return std::string(R"({"tt_boards":true,"tt_blocked":{"reason":"couldn't reach the server"}})");
    });
    auto loaded = [](const b::Session &) { return true; };
    Clock::time_point start = Clock::now();
    double slowest = 0;
    int before = 0, after = 0, wrong = 0;
    while (ms_since(start) < ASK_MS) {
        Clock::time_point t = Clock::now();
        b::Why w = r.why(loaded);
        slowest = std::max(slowest, ms_since(t));
        if (!r.done()) {
            before++;
            wrong += w != b::Why::none;
        } else {
            after++;
            wrong += w != b::Why::not_placed;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    std::printf("      stalled read: %d asks before it was done, %d after, slowest ask %.2f ms, %d read(s)\n", before, after,
                slowest, reads.load());
    expect(slowest < 50, "no ask waits on a stalled tt.json read");
    expect(before > 0 && after > 0 && !wrong, "not blocked while the read stalls, blocked once it is done");
    expect(reads == 1, "tt.json is read once, however long the game asks");
    expect(!on_asker, "tt.json is read on a thread of its own, not the asking thread");
    expect(r.session().reason == "couldn't reach the server", "the read's session is kept");
}

static b::Why ask_once(b::Reading &r, const std::function<bool(const b::Session &)> &loaded)
{
    Clock::time_point start = Clock::now();
    while (ms_since(start) < 2000) {
        b::Why w = r.why(loaded);
        if (r.done())
            return w;
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    return b::Why::none;
}

static void readings()
{
    int asked = 0;
    auto counted = [&](bool v) {
        return [&asked, v](const b::Session &) {
            asked++;
            return v;
        };
    };
    b::Reading placed(
        [] { return std::string(R"({"tt_boards":true,"tt_module":{"version":"v1","dll":"swofty_integrity.dll"}})"); });
    expect(ask_once(placed, counted(false)) == b::Why::not_loaded && asked > 0, "a placed module that did not load blocks");
    expect(placed.why(counted(true)) == b::Why::none, "the module loading later lifts the block without another read");

    asked = 0;
    b::Reading off([] { return std::string(R"({"tt_boards":false,"tt_module":{"version":"v1"}})"); });
    ask_once(off, counted(false));
    b::Reading missing([] { return std::string(); });
    expect(ask_once(missing, counted(false)) == b::Why::none && missing.done(), "no tt.json does not block");
    b::Reading boards_only([] { return std::string(R"({"tt_boards":true})"); });
    expect(ask_once(boards_only, counted(false)) == b::Why::none, "boards on with no module key does not block");
    expect(asked == 1, "the module is looked for only when one was placed");

    b::Reading late([] {
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
        return std::string(R"({"tt_boards":true,"tt_blocked":{"reason":"timeout"}})");
    });
    b::EntryGate e;
    auto loaded = [](const b::Session &) { return true; };
    bool shown = e.step(late.why(loaded) != b::Why::none);
    expect(!shown, "in Time Trial before the read is done: no popup yet");
    ask_once(late, loaded);
    expect(late.why(loaded) != b::Why::none && e.step(true), "the read done while in Time Trial: the popup shows once");
    expect(!e.step(late.why(loaded) != b::Why::none), "and not again while the game stays there");
}

int main()
{
    b::Session s = b::parse(R"({"tt_boards":true,"tt_nonce":"n","tt_blocked":{"reason":"couldn't reach the server"}})");
    expect(s.boards && s.blocked && s.reason == "couldn't reach the server" && !b::placed(s),
           "tt_blocked object is read with its reason");
    expect(b::why_blocked(s, true) == b::Why::not_placed, "tt_blocked blocks even when a module is loaded");

    s = b::parse(R"({"tt_blocked":{"reason":"timeout"}})");
    expect(b::why_blocked(s, false) == b::Why::none, "tt_blocked without tt_boards does not block");
    s = b::parse(R"({"tt_boards":false,"tt_blocked":{"reason":"timeout"}})");
    expect(b::why_blocked(s, false) == b::Why::none, "tt_blocked with tt_boards false does not block");

    s = b::parse(R"({"tt_boards":true,"tt_module":{"version":"2026.11.10.1","dll":"swofty_integrity.dll"}})");
    expect(b::placed(s) && s.version == "2026.11.10.1" && b::module_dll(s) == "swofty_integrity.dll", "tt_module is read");
    expect(b::why_blocked(s, true) == b::Why::none, "a placed module that loaded does not block");
    expect(b::why_blocked(s, false) == b::Why::not_loaded, "a placed module that did not load blocks");

    s = b::parse(R"({"tt_boards":true})");
    expect(b::why_blocked(s, false) == b::Why::none, "boards on with no module key does not block (an older wrapper)");
    s = b::parse(R"({"tt_boards":true,"tt_bridge":true,"tt_ghost":true})");
    expect(b::why_blocked(s, false) == b::Why::none, "the other session keys do not block");
    expect(b::why_blocked(b::parse(""), false) == b::Why::none, "no tt.json does not block");
    expect(b::why_blocked(b::parse("not json"), false) == b::Why::none, "a broken tt.json does not block");
    expect(b::why_blocked(b::parse("[1,2]"), false) == b::Why::none, "a tt.json that is no object does not block");

    s = b::parse(R"({"tt_boards":true,"tt_blocked":"timeout"})");
    expect(s.blocked && s.reason == "timeout", "tt_blocked as a string is a reason");
    s = b::parse(R"({"tt_boards":true,"tt_blocked":false})");
    expect(!s.blocked, "tt_blocked false does not block");
    s = b::parse(R"({"tt_boards":true,"tt_blocked":null})");
    expect(!s.blocked, "tt_blocked null does not block");
    s = b::parse(std::string(R"({"tt_boards":true,"tt_blocked":{"reason":")") + std::string(500, 'x') + "\"}}");
    expect(s.blocked && s.reason.size() == b::REASON_MAX, "a long reason is cut");

    s = b::parse(R"({"tt_boards":true,"tt_module":{"version":"v1","dll":"..\\evil.dll"}})");
    expect(b::module_dll(s) == b::DEFAULT_DLL, "a dll with a path is the default name");
    s = b::parse(R"({"tt_boards":true,"tt_module":{"version":"v1"}})");
    expect(b::module_dll(s) == b::DEFAULT_DLL, "a missing dll is the default name");
    s = b::parse(R"({"tt_boards":true,"tt_module":{"version":"v1","dll":"other.dll"}})");
    expect(b::module_dll(s) == "other.dll", "a plain dll name is used");

    expect(b::blocks_command("time_trial"), "the container's time_trial command is refused");
    expect(!b::blocks_command("solo_gp") && !b::blocks_command("grand_prix_new") && !b::blocks_command("grand_prix_resume") &&
               !b::blocks_command("back") && !b::blocks_command("ss_game_mode_select") &&
               !b::blocks_command("entrant_list_selected") && !b::blocks_command(""),
           "other container commands pass");

    b::PopupGate p;
    expect(p.due(10000), "the first press shows the popup");
    expect(!p.due(10000 + b::POPUP_GAP_MS - 1), "the same press seen by a second hook does not show another");
    expect(p.due(10000 + b::POPUP_GAP_MS), "a later press shows it again");

    b::EntryGate e;
    expect(!e.step(false), "outside Time Trial nothing shows");
    expect(e.step(true), "entering Time Trial shows the popup");
    expect(!e.step(true), "staying in Time Trial does not show it again");
    expect(!e.step(false) && e.step(true), "leaving and entering again shows it again");

    expect(std::string(b::TITLE) == "Time Trial is unavailable", "the popup title");
    expect(std::string(b::BODY).find("turn off Time Trial boards") != std::string::npos,
           "the popup says how to play without the boards");

    stalled_drive();
    readings();

    std::printf("%s\n", failed ? "FAILED" : "all ok");
    return failed ? 1 : 0;
}
