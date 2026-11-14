#pragma once

#include "json.hpp"

#include <cstddef>
#include <cstdint>
#include <string>

// Time Trial blocked for the session. swofty\tt.json's "tt_blocked" ({"reason"}, written by the wrapper when Time Trial
// boards is on, the player is signed in and no verified integrity module could be placed) blocks it, and so does
// "tt_module" ({"version", "dll"}, the module the wrapper placed) when that DLL is not in the process or nothing
// handles laps.sealing. Both count only with "tt_boards": true, the key that turns lap recording on.
namespace runtime::tt_blocked {

constexpr const char *TITLE = "Time Trial is unavailable";
constexpr const char *BODY = "The launcher could not load the Time Trial integrity module, so laps would not count. "
                             "Check your connection and start the game from the launcher again. To play Time Trial "
                             "without the boards, turn off Time Trial boards in the launcher.";
constexpr const char *CARD_DESCRIPTION = "Unavailable: the integrity module did not load";
constexpr const char *CONTAINER_COMMAND = "time_trial";
constexpr const char *DEFAULT_DLL = "swofty_integrity.dll";
constexpr uint64_t POPUP_GAP_MS = 1500;
constexpr uint64_t READ_MS = 2000;
constexpr size_t REASON_MAX = 200;
constexpr size_t DLL_MAX = 64;

struct Session {
    bool boards = false;
    bool blocked = false;
    std::string reason;
    std::string version;
    std::string dll;
};

inline Session parse(const std::string &text)
{
    Session s;
    json::Value doc;
    if (!json::parse(text, doc) || doc.type != json::Value::Obj)
        return s;
    s.boards = doc["tt_boards"].type == json::Value::Bool && doc["tt_boards"].b;
    const json::Value &b = doc["tt_blocked"];
    if (b.type == json::Value::Obj) {
        s.blocked = true;
        s.reason = b["reason"].str();
    } else if (b.type == json::Value::Str) {
        s.blocked = true;
        s.reason = b.s;
    } else if (b.type == json::Value::Bool) {
        s.blocked = b.b;
    }
    if (s.reason.size() > REASON_MAX)
        s.reason.resize(REASON_MAX);
    const json::Value &m = doc["tt_module"];
    if (m.type == json::Value::Obj) {
        s.version = m["version"].str();
        s.dll = m["dll"].str();
    }
    return s;
}

inline bool placed(const Session &s) { return !s.version.empty(); }

// The file name the placed module loads as. A "dll" that is missing, too long or not a plain file name is the
// integrity module's own name.
inline std::string module_dll(const Session &s)
{
    if (s.dll.empty() || s.dll.size() > DLL_MAX || s.dll.find_first_of("/\\:") != std::string::npos)
        return DEFAULT_DLL;
    return s.dll;
}

enum class Why { none, not_placed, not_loaded };

// loaded: whether the placed module's DLL is in the process and laps.sealing has a handler.
inline Why why_blocked(const Session &s, bool loaded)
{
    if (!s.boards)
        return Why::none;
    if (s.blocked)
        return Why::not_placed;
    if (placed(s) && !loaded)
        return Why::not_loaded;
    return Why::none;
}

inline const char *why_text(Why w)
{
    switch (w) {
    case Why::not_placed:
        return "the launcher placed no integrity module";
    case Why::not_loaded:
        return "the placed integrity module is not loaded or has no laps.sealing handler";
    default:
        return "not blocked";
    }
}

inline bool blocks_command(const std::string &command) { return command == CONTAINER_COMMAND; }

// One popup per press: presses within POPUP_GAP_MS of the last popup (the same press seen by two hooks) do not queue
// another.
class PopupGate {
  public:
    bool due(uint64_t now_ms)
    {
        if (shown_ && now_ms - last_ < POPUP_GAP_MS)
            return false;
        shown_ = true;
        last_ = now_ms;
        return true;
    }

  private:
    bool shown_ = false;
    uint64_t last_ = 0;
};

// The popup when the game enters Time Trial some other way while blocked: once per entry, again only after the game
// left Time Trial.
class EntryGate {
  public:
    bool step(bool in_time_trial)
    {
        bool entered = in_time_trial && !was_;
        was_ = in_time_trial;
        return entered;
    }

  private:
    bool was_ = false;
};

}
