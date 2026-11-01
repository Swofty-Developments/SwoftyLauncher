#pragma once

// The virtual pad's a for a game the launcher started for a room (netplay_drive.cpp's title skip). This is
// the XInput hook of #683; the shared virtual pad in core (virtual_pad.hpp, #686) takes its place by
// implementing these calls.

namespace runtime::title_pad {

// Hooks xinput1_4.dll's XInputGetState and XInputGetCapabilities. False, with a log line, when it cannot.
bool install();

// Holds the pad's a (true) or releases it (false). A real pad passes through whenever this is released.
void press(bool down);

// Ends the pad: the hooks pass everything through, including the capabilities of a missing pad.
void disarm();

}
