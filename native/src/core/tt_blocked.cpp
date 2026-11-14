#include "runtime.hpp"
#include "tt_blocked.hpp"
#include "tt_blocked_logic.hpp"
#include "main_menu.hpp"
#include "main_menu_logic.hpp"
#include "apply/moments.hpp"
#include "hooks/hooks.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <atomic>
#include <mutex>
#include <string>

// Time Trial blocked for the session (tt_blocked_logic.hpp). The state is read from swofty\tt.json at most every
// READ_MS. The main menu's Time Trial card (main_menu.cpp card_action) and the activity container's time_trial command,
// which F1 World > Play > Time Trial raises, are refused with the game's own message popup; the card's description says
// it is unavailable. When the game reaches Time Trial another way (the activity container's tabs), the popup shows once
// on entry. Whether a placed module loaded is looked at from the first main menu on, when the loader has loaded every
// addon. SWOFTY_TT_BLOCKED=0 turns it off.
namespace runtime::tt_blocked {
namespace {

std::atomic<int> off{-1};
std::atomic<bool> home_seen{false};
std::mutex lock;
Session session;
bool have = false;
ULONGLONG read_at = 0;
Why told = Why::none;
bool told_once = false;
PopupGate popups;
EntryGate entries;
ULONGLONG ticked_at = 0;

bool is_off()
{
    if (off.load() < 0) {
        char v[8] = {};
        off = GetEnvironmentVariableA("SWOFTY_TT_BLOCKED", v, sizeof v) && v[0] == '0';
    }
    return off.load() > 0;
}

bool module_loaded(const Session &s)
{
    if (!home_seen.load())
        return true;
    if (!GetModuleHandleW(runtime::wide(module_dll(s)).c_str()))
        return false;
    return swofty_core::hooks::core().handlers("laps.sealing") > 0;
}

Why current()
{
    std::lock_guard<std::mutex> g(lock);
    ULONGLONG now = GetTickCount64();
    if (!have || now - read_at >= READ_MS) {
        session = parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
        have = true;
        read_at = now;
    }
    Why w = why_blocked(session, placed(session) && module_loaded(session));
    if (w != told || !told_once) {
        told_once = true;
        told = w;
        if (w == Why::none)
            swofty::log("[tt-blocked] Time Trial is not blocked (boards %s, module %s)", session.boards ? "on" : "off",
                        placed(session) ? session.version.c_str() : "not placed");
        else
            swofty::log("[tt-blocked] Time Trial is blocked: %s%s%s", why_text(w), session.reason.empty() ? "" : "; the launcher said: ",
                        session.reason.c_str());
    }
    return w;
}

}

void screen(const char *name)
{
    if (name && !home_seen.load() && runtime::main_menu::classify_screen(name) == runtime::main_menu::Screen::home)
        home_seen = true;
}

bool active()
{
    return !is_off() && current() != Why::none;
}

bool refuse(const char *where)
{
    if (!active())
        return false;
    bool show;
    {
        std::lock_guard<std::mutex> g(lock);
        show = popups.due(GetTickCount64());
    }
    if (!show) {
        swofty::log("[tt-blocked] %s refused; the popup is already up", where);
        return true;
    }
    int r = runtime::popup_show(TITLE, BODY, "swofty_core (Time Trial)");
    swofty::log("[tt-blocked] %s refused; popup %s", where, r == 0 ? "queued" : "not available");
    return true;
}

bool refuses_command(const std::string &command)
{
    return blocks_command(command) && refuse("F1 World > Play > Time Trial (activity container command time_trial)");
}

const char *card_description()
{
    return active() ? CARD_DESCRIPTION : nullptr;
}

void tick()
{
    if (is_off())
        return;
    ULONGLONG now = GetTickCount64();
    if (now - ticked_at < 500)
        return;
    ticked_at = now;
    if (!active()) {
        entries.step(false);
        return;
    }
    static bool armed = false;
    if (!armed) {
        armed = true;
        swofty_core::apply::moments::mode_watch();
        runtime::main_menu::arm_container();
    }
    bool tt = swofty_core::apply::moments::mode_kind() == swofty_core::apply::facts::ModeKind::time_trial;
    if (entries.step(tt))
        refuse("Time Trial entered another way");
}

}
