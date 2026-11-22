#pragma once

#include <swofty_api.h>

// The lap host's input call (SwoftyLapHost version 4): the game controllers Windows lists and the virtual pad's
// reads. The device list is read on a thread of its own, at most every 5 seconds while someone asked in the last 30;
// input() copies the last read and never waits.
namespace input_devices {

int input(SwoftyLapInput *out);

}
