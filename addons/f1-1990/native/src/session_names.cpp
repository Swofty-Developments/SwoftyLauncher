// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - session names: the Friday shootout is Qualifying 1 and the Saturday qualifying Qualifying 2. Its own DLL.
//
// The game names a session by its type from fixed tables (long names 0x145736910, short names 0x14576e538, the
// timing tower's race_data.uidb) and a session group by its id (lng_sprintshootout / lng_qualifying). The 1990
// weekend runs the short or one-shot shootout (types 13, 14) on Friday and the short or one-shot qualifying (8, 9) on
// Saturday, so data/session-strings.json ("strings" per language) renames those keys and the two results headers.
// They are set through the core's runtime string overrides (API v8) while a 1990 career is loaded and cleared when it
// unloads. Built against an SDK below v8 the DLL only logs that the names stay stock.

#include <cstring>
#include <string>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#if SWOFTY_API_VERSION >= 8
#include <swofty/lang.hpp>
#endif

#include "career_1990.hpp"
#include "json_strings.hpp"

SWOFTY_SETTING(bool, session_names_1990, true);

namespace {

#if SWOFTY_API_VERSION >= 8
constexpr char kStringsFile[] = "data/session-strings.json";

struct Override {
    std::string language, key, text;
};

std::vector<Override> g_strings;
bool g_strings_read, g_strings_set;

void read_strings()
{
    std::string json;
    size_t error_at = 0;
    std::vector<f1990::StringLeaf> leaves;
    if (auto r = swofty::storage::read(kStringsFile, json); r != swofty::storage::Result::ok) {
        swofty::log("[f1-1990] sessions: read %s: %s", kStringsFile, swofty::storage::name(r));
        return;
    }
    if (!f1990::parse_string_leaves(json, leaves, &error_at)) {
        swofty::log("[f1-1990] sessions: %s is not an object of strings (byte %zu)", kStringsFile, error_at);
        return;
    }
    for (auto &l : leaves)
        if (l.path.size() == 3 && l.path[0] == "strings")
            g_strings.push_back({l.path[1], l.path[2], l.text});
}

void session_names(bool on)
{
    namespace lg = swofty::lang;
    if (on == g_strings_set || !lg::available())
        return;
    if (on && !g_strings_read) {
        g_strings_read = true;
        read_strings();
    }
    size_t failed = 0;
    for (auto &s : g_strings)
        failed += (on ? lg::set(s.key, s.text, s.language) : lg::clear(s.key, s.language)) != lg::Result::ok;
    g_strings_set = on;
    swofty::log("[f1-1990] sessions: %s %zu session name string(s) (%zu failed)", on ? "set" : "cleared",
                g_strings.size(), failed);
}

void on_career(const SwoftyEvent *ev, void *)
{
    bool yes = false;
    if (session_names_1990 && !std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED)) {
        void *career = swofty::career::entity();
        yes = career && f1990::is_1990_career(career);
    }
    session_names(yes);
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] sessions: subscribe %s failed; the session names stay stock", event);
}
#endif

}

SWOFTY_ON_LOAD
{
#if SWOFTY_API_VERSION >= 8
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
#else
    swofty::log("[f1-1990] sessions: built against SDK API v%d, without runtime string overrides; the session names "
                "stay stock",
                SWOFTY_API_VERSION);
#endif
}

SWOFTY_ADDON()
