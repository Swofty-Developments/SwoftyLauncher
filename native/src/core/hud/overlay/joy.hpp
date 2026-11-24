#pragma once

#include "core/hud/hud_input.hpp"

// Wheel and joystick buttons for the overlay's input. F1 25 reads wheels and joysticks through DirectInput only
// (IDirectInputDevice8::GetDeviceState into a DIJOYSTATE2 each frame), and XInput does not list them, so the core
// hooks GetDeviceState in dinput8.dll and reads the state each call returns to the game, after the game's call and
// without changing it.

namespace hud::overlay::joy {

// Hooks the device reads on a thread of its own the first time it is called; later calls do nothing.
void want();

hud::input::JoyDevices &devices();

}
