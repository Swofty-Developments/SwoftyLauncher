#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "core/boxes/era_logic.hpp"

// Pure logic of the LAN Car Category addon boxes (car_category.cpp): the press commands, the option value
// name decoding, the value match and the handle release. No game calls, so the unit test runs on the host.

namespace runtime::car_category {

using era::djb2;
using era::Handle;

constexpr size_t STOCK_BOXES = 2;
// The launcher's UI build gives the screen this many addon box slots (SLOTS in packages/core/src/games/f1_25/ui/car_category.rs).
constexpr size_t MAX_ADDED = 4;
constexpr size_t VALUE_NAME_MAX = 128;

constexpr uint32_t BUTTON_1 = djb2("button_1");
constexpr uint32_t BUTTON_2 = djb2("button_2");
constexpr uint32_t BACK = djb2("back_pressed");
static_assert(BUTTON_1 == 0x324cce31 && BUTTON_2 == 0x324cce32 && BACK == 0x493fe92b);

inline uint32_t button_hash(size_t n)
{
    std::string s = "button_" + std::to_string(n);
    return djb2(s.c_str());
}

// How many ui.tile records the screen can still show once `used` addon slots hold era boxes: era boxes come first.
inline size_t records_room(size_t used) { return used >= MAX_ADDED ? 0 : MAX_ADDED - used; }

// A slice as the string FillTile resolves into the tile's image node.
inline std::string slice_path(const std::string &array, const std::string &name)
{
    return "#overrides." + array + "[" + name + "]";
}

// The added box a press command names: button_<stock + 1 + i> is added box i. -1 for anything else.
inline int added_box_of(uint32_t hash, size_t added)
{
    for (size_t i = 0; i < added && i < MAX_ADDED; ++i)
        if (hash == button_hash(STOCK_BOXES + 1 + i))
            return int(i);
    return -1;
}

// An option value's name as the game decodes it: byte i = key[(i - 1) & 3] ^ src[i] for i < len - 1, the
// last byte is the terminator. Empty for a length outside 2..VALUE_NAME_MAX.
inline std::string decode_name(const uint8_t *src, size_t len, const uint8_t key[4])
{
    if (!src || len < 2 || len > VALUE_NAME_MAX)
        return {};
    std::string out;
    for (size_t i = 0; i + 1 < len; ++i) {
        char c = char(key[(i - 1) & 3] ^ src[i]);
        if (!c)
            break;
        out.push_back(c);
    }
    return out;
}

struct Value {
    std::string name, display;
};

// The value a box selects: the one whose game name is the box's entrant list. -1 when none is.
inline int value_of(const std::vector<Value> &values, const std::string &entrant_list)
{
    for (size_t i = 0; i < values.size(); ++i)
        if (values[i].name == entrant_list)
            return int(i);
    return -1;
}

struct Scenario {
    uint8_t stat_id;
    std::string name;
};

// The scenario a category value plays: the one with the value's game name, as setupFromUnranked looks it up.
// -1 when none has it.
inline int scenario_of(const std::vector<Scenario> &scenarios, const std::string &value)
{
    for (const auto &s : scenarios)
        if (s.name == value)
            return s.stat_id;
    return -1;
}

// Whether this game decides the session's scenario: setupFromUnranked and Game_Network_SetScenario act on a
// session role below 2 (offline or host); a client (2) takes the host's.
inline bool decides_scenario(int role) { return role >= 0 && role < 2; }

using era::unlink;

}
