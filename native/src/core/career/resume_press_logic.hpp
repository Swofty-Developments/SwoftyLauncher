#pragma once

#include <cstdint>
#include <string>
#include <string_view>

#include "core/career/resume_list_logic.hpp"

// The state a list row's press checks before it calls PressSlot on the stock continue screen (resume_gate_guard.cpp reads the
// fields from the game, this file decides). The mode PressSlot loads the card's slot under comes from the career wizard
// state's data group (the group the pressed career tile wrote there), so the group has to be the career's own content group
// first. The stock continue screen's cards are Game_CareerContinue_ListSlots' first three saves; a row may name an older one.

namespace career::resume_list {

constexpr size_t CARD_COUNT = 3;
constexpr const char *STOCK_GROUP_25 = "f1_career_2025";
constexpr const char *STOCK_GROUP_26 = "f1_career_2026";

struct Card {
    bool read = false, has_save = false;
    uint64_t id = 0;
};

struct Facts {
    size_t row = 0;
    uint8_t pending = 0, busy = 0;
    Card stock[CARD_COUNT];
    Card card0;
    uint64_t target = 0;
    bool listed = false, mode_found = false;
    int64_t mode_stat = -1, want_stat = -1;
    std::string want_group, wizard_group, game_mode;
};

inline bool stock_group(std::string_view group) { return group == STOCK_GROUP_25 || group == STOCK_GROUP_26; }

inline std::string suffix_for(std::string_view group, std::string_view content_suffix)
{
    if (group == STOCK_GROUP_25)
        return "25";
    if (group == STOCK_GROUP_26)
        return "26";
    return std::string(content_suffix);
}

inline bool group_ok(std::string_view want, std::string_view current) { return !want.empty() && current == want; }

inline bool group_needs_write(std::string_view want, std::string_view current) { return !want.empty() && want != current; }

inline bool stock_has(const Card (&cards)[CARD_COUNT], uint64_t id)
{
    for (const Card &c : cards)
        if (c.read && c.has_save && c.id == id)
            return true;
    return false;
}

inline std::string group_text(std::string_view g) { return g.empty() ? "(none)" : std::string(g); }

inline std::string busy_reason(const Facts &f)
{
    if (f.pending != 0)
        return "an action is already pending (vm+0x1368 = " + std::to_string(f.pending) + ")";
    if (f.busy != 0)
        return "the screen is busy (vm+0x1380 = " + std::to_string(f.busy) + ")";
    return {};
}

inline std::string wizard_reason(const Facts &f)
{
    if (f.want_group.empty())
        return "the career's data group is unknown (the row and the careers index have none)";
    if (f.game_mode.empty())
        return "the row names no game mode";
    if (!group_ok(f.want_group, f.wizard_group))
        return "the wizard data group is " + group_text(f.wizard_group) + ", the career's is " + group_text(f.want_group);
    if (!f.mode_found)
        return "no game mode matches the wizard data group " + group_text(f.wizard_group);
    if (f.want_stat < 0)
        return "the row's game mode " + f.game_mode + " is not in the game's data";
    if (f.mode_stat != f.want_stat)
        return "the wizard resolves game mode statId " + std::to_string(f.mode_stat) + " for group " +
               group_text(f.wizard_group) + ", the row's career is " + f.game_mode + " (statId " + std::to_string(f.want_stat) +
               ")";
    return {};
}

inline std::string opened_refusal(const Facts &f)
{
    std::string r = busy_reason(f);
    if (!r.empty())
        return r;
    if (!f.listed && !stock_has(f.stock, f.target))
        return "slot id " + std::to_string(f.target) + " is neither a saved card on the continue screen nor a listed career";
    return {};
}

inline std::string press_refusal(const Facts &f)
{
    std::string r = busy_reason(f);
    if (!r.empty())
        return r;
    if (!f.card0.read || !f.card0.has_save)
        return "card 0 has no save, so PressSlot would start a new career (pending 3)";
    if (f.card0.id != f.target)
        return "card 0 holds slot id " + std::to_string(f.card0.id) + ", not " + std::to_string(f.target);
    return wizard_reason(f);
}

inline bool is_new_career_route(std::string_view route) { return route == NEW_CAREER_ROUTE; }

inline bool new_career_live(uint64_t set_at, uint64_t now) { return set_at != 0 && now - set_at <= NEW_CAREER_LIFE_MS; }

}
