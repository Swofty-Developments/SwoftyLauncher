#pragma once

#include <cstdint>
#include <string>

// The virtual pad: XInputGetState and XInputGetCapabilities in xinput1_4.dll (the game imports them by
// ordinal 2 and 4) report a connected pad in slot 0 while someone has claimed it (the test agent, the title
// skip, a plugin through the session host). Buttons that are set replace a real pad's state while they are held;
// otherwise a real pad passes through. stats() counts the slot 0 reads it answered, for the lap file's input record.
namespace runtime::pad {

enum class Owner : uint8_t { title, agent, plugin };

struct Stats {
    uint32_t owners = 0, seen = 0;
    uint64_t set = 0, idle = 0, real = 0;
};

void setup();
const std::string &why();
void claim(Owner who, bool on);
void set(uint16_t buttons, uint8_t lt = 0, uint8_t rt = 0);
void release();
Stats stats();
// XInputGetCapabilities as the game would see it without the virtual pad: the original export, past the hook.
// ERROR_DEVICE_NOT_CONNECTED (1167) when xinput1_4.dll or the export is missing.
uint32_t real_caps(uint32_t user, uint32_t flags, void *caps);

constexpr uint16_t A = 0x1000;

}
