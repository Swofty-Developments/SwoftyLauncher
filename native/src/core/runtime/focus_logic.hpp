#pragma once

#include <cstdint>

// The game has the player's input when the foreground window belongs to the game's process and is not minimized.
// GetAsyncKeyState and XInputGetState report the keyboard and pads whatever window has focus, so a key or button read
// through them counts only while this holds; runtime::game_in_front() feeds it on Windows.

namespace runtime::focus {

inline bool game_focused(uint32_t foreground_pid, uint32_t own_pid, bool minimized)
{
    return foreground_pid != 0 && foreground_pid == own_pid && !minimized;
}

}
