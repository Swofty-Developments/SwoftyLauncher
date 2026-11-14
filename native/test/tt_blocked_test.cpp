#include "tt_blocked_logic.hpp"

#include <cstdio>
#include <string>

namespace b = runtime::tt_blocked;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    b::Session s = b::parse(R"({"tt_boards":true,"tt_nonce":"n","tt_blocked":{"reason":"couldn't reach the server"}})");
    expect(s.boards && s.blocked && s.reason == "couldn't reach the server" && !b::placed(s), "tt_blocked object is read with its reason");
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
               !b::blocks_command("back") && !b::blocks_command("ss_game_mode_select") && !b::blocks_command("entrant_list_selected") &&
               !b::blocks_command(""),
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
    expect(std::string(b::BODY).find("turn off Time Trial boards") != std::string::npos, "the popup says how to play without the boards");

    std::printf("%s\n", failed ? "FAILED" : "all ok");
    return failed ? 1 : 0;
}
