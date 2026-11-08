#pragma once

#include <atomic>
#include <cstdint>
#include <string_view>

namespace text_entry {

constexpr uint64_t IDLE_MS = 20000;
constexpr uint32_t KEY_ENTER = 0x0d, KEY_ESCAPE = 0x1b;

inline bool ieq_contains(std::string_view hay, std::string_view needle)
{
    if (needle.size() > hay.size())
        return false;
    for (size_t i = 0; i + needle.size() <= hay.size(); i++) {
        size_t j = 0;
        for (; j < needle.size(); j++) {
            char c = hay[i + j];
            if (c >= 'A' && c <= 'Z')
                c = char(c + 32);
            if (c != needle[j])
                break;
        }
        if (j == needle.size())
            return true;
    }
    return false;
}

inline bool is_entry_message(std::string_view id)
{
    for (std::string_view key : {"entry", "text_input", "name_input", "rename", "keyboard", "enter_name", "search"})
        if (ieq_contains(id, key))
            return true;
    return false;
}

struct State {
    std::atomic<uint64_t> until{0};
    std::atomic<bool> logged{false};

    void begin(uint64_t now)
    {
        until.store(now + IDLE_MS);
        logged.store(false);
    }

    bool active(uint64_t now) const { return now < until.load(); }

    void key(uint32_t vk, uint64_t now)
    {
        if (!active(now))
            return;
        until.store(vk == KEY_ENTER || vk == KEY_ESCAPE ? 0 : now + IDLE_MS);
    }

    void chr(uint64_t now)
    {
        if (active(now))
            until.store(now + IDLE_MS);
    }

    bool first_suppression() { return !logged.exchange(true); }
};

inline State state;

}
