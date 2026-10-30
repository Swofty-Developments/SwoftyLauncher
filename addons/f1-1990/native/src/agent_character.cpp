// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the agent's character ref shows Flavio (names, job, message avatar) in 1990 careers
// only. Its own DLL, loaded at the early stage so it sees the game's first character list build.
//
// The UI keeps one VM node per entrants.characters row under character_list, filled by
// Game_EntrantsUi_BuildCharacterRef (stat_id, filename, full/first/last name, job_role, and the
// message_avatar / message_avatar_race textures message_<name>); the subtitle box, emails and
// contacts bind those nodes. Game_EntrantsUi_OnCharacterListLoaded, the GameCharacterListLoaded
// handler, clears character_list and builds it from the game's character vector. While the loaded
// career's data group is f1_career_1990, the builder wrap gives the elena row a stand-in record:
// the flavio row (stat_id 105, names lng_f1990_agent_*) under elena's stat_id. The handler wrap
// keeps the character vector, and when a career load or unload changes the 1990 state the DLL runs
// the handler again with it, so the elena node is rebuilt from the stand-in or from her own row.

#include <swofty/mixin.hpp>

#ifdef SWOFTY_API_CHARACTERS
// With a core that has the characters library the core owns the ref hooks: the swap is registered
// for the 1990 data group and the core rebuilds the character list on career load and unload.
#include <swofty/characters.hpp>

#include "career_group.hpp"

SWOFTY_ON_LOAD
{
    namespace ch = swofty::characters;
    ch::Result r = ch::swap(f1990::kCareerGroup1990, "elena", "flavio");
    swofty::log("[f1-1990] agent: elena shows flavio in %s careers through the core's characters library (%s)",
                f1990::kCareerGroup1990, ch::name(r));
}
#else
#include <atomic>
#include <cstring>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "career_1990.hpp"
#include "character_ref.hpp"

namespace {

using f1990::at;

constexpr char kStockAgent[] = "elena";
constexpr char kAgent1990[] = "flavio";

std::atomic<const void *> g_vector{nullptr};
std::atomic<bool> g_1990{false};
std::atomic<uint32_t> g_stand_ins, g_builds, g_missing_logged;

size_t rows(const void *vector)
{
    if (!vector || !swofty::mem::readable(vector, 0x18))
        return 0;
    auto *b = at<const uint8_t *>(vector, 0x08), *e = at<const uint8_t *>(vector, 0x10);
    return b && e > b ? size_t(e - b) / f1990::kCharacterSize : 0;
}

void rebuild(const char *why)
{
    const void *vector = g_vector.load();
    if (!rows(vector)) {
        swofty::log("[f1-1990] agent: %s, but the game has not built the character list yet; the agent ref is left "
                    "as it is",
                    why);
        return;
    }
    uint8_t message[0x18] = {};
    std::memcpy(message + 0x10, &vector, sizeof vector);
    uint32_t before = g_stand_ins.load();
    swofty::game::Game_EntrantsUi_OnCharacterListLoaded(nullptr, message);
    swofty::log("[f1-1990] agent: %s; character list rebuilt, the elena ref shows %s", why,
                g_stand_ins.load() != before ? "flavio" : "elena");
}

void on_career(const SwoftyEvent *ev, void *)
{
    bool yes = false;
    if (!std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED)) {
        void *career = swofty::career::entity();
        yes = career && f1990::is_1990_career(career);
    }
    if (g_1990.exchange(yes) != yes)
        rebuild(yes ? "1990 career loaded" : "1990 career left");
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    swofty::events::Result r = swofty::events::subscribe(event, on_career, nullptr, &token);
    if (r != swofty::events::Result::ok)
        swofty::log("[f1-1990] agent: subscribe %s: %s; the agent ref stays elena", event, swofty::events::name(r));
}

}

SWOFTY_WRAP(Game_EntrantsUi_OnCharacterListLoaded, int64_t, void *ctx, const void *message)
{
    const void *vector = message ? at<const void *>(message, 0x10) : nullptr;
    if (vector)
        g_vector = vector;
    if (g_builds++ < 8)
        swofty::log("[f1-1990] agent: character list built from %p (%zu rows, 1990 career %s)", vector, rows(vector),
                    g_1990.load() ? "loaded" : "not loaded");
    return orig(ctx, message);
}

SWOFTY_WRAP(Game_EntrantsUi_BuildCharacterRef, void *, void *out, const uint8_t *character, void *node)
{
    if (g_1990.load() && character) {
        const char *name = f1990::character_name(character);
        if (name && !std::strcmp(name, kStockAgent)) {
            if (const uint8_t *flavio = f1990::find_character(g_vector.load(), kAgent1990)) {
                uint8_t stand_in[f1990::kCharacterSize];
                f1990::stand_in_character(stand_in, character, flavio);
                g_stand_ins++;
                return orig(out, stand_in, node);
            }
            if (!g_missing_logged++)
                swofty::log("[f1-1990] agent: no flavio row in the character list; the elena ref stays elena");
        }
    }
    return orig(out, character, node);
}

SWOFTY_ON_LOAD
{
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
}
#endif

SWOFTY_ADDON()
