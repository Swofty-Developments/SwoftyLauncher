#pragma once

#include <cstdint>
#include <string>

// The title screen without input, for the test agent and the direct start (direct_start.cpp). StateIntro
// waits on its sync point; once the title's view is up the core sets both bool nodes, and the game sends its
// own request, which loads the profile and engages the title.
namespace runtime::title {

void setup();
const std::string &why();
void want(bool on);
bool wanted();
bool engaged();
std::string state();
uintptr_t node_set_bool();

}
