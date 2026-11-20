#pragma once
#include "core/hud/arm_policy/game_settings.hpp"

#include <cstdint>
#include <string>

namespace hud::hardware_settings {

std::wstring path();
hud::arm_policy::FgSetting frame_gen(const std::wstring &path);

// The game's hardware settings file, looked at again at most every two seconds. Its path is found on the first look
// that finds one.
class Poll {
public:
    bool due(uint64_t now)
    {
        if (checked && now - checked < 2000)
            return false;
        checked = now;
        if (file.empty())
            file = path();
        return true;
    }
    const std::wstring &file_path() const { return file; }

private:
    std::wstring file;
    uint64_t checked = 0;
};

}
