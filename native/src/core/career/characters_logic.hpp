#pragma once

#include <string>

// The character overrides' rules (characters.cpp) over its swap and mute lists, which test/characters_logic_test.cpp
// pins: an entry acts only in careers of its group, the last swap of a character wins and a string without a language
// is for every language.
namespace career::characters {

// The character's replacement in careers of the active group: the last swap's, or empty.
template <class Swaps> std::string replacement_in(const Swaps &swaps, const std::string &active, const std::string &character)
{
    if (active.empty())
        return {};
    for (auto it = swaps.rbegin(); it != swaps.rend(); ++it)
        if (it->group == active && it->character == character)
            return it->replacement;
    return {};
}

// Whether the speaker's lines are muted in careers of the active group.
template <class Mutes> bool muted_in(const Mutes &mutes, const std::string &active, const char *speaker)
{
    if (active.empty() || !speaker)
        return false;
    for (auto &m : mutes)
        if (m.group == active && m.speaker == speaker)
            return true;
    return false;
}

// Whether any swap is for careers of group.
template <class Swaps> bool group_in(const Swaps &swaps, const std::string &group)
{
    for (auto &s : swaps)
        if (s.group == group)
            return true;
    return false;
}

inline std::string language_of(const char *language) { return !language || !*language ? "*" : language; }

}
