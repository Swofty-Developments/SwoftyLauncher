#pragma once

// The per-scope setup screen layout's game hooks (setup_layout_hooks.cpp).

namespace swofty_core::setup_layout_hooks {

// Finds the three game sites and places the hooks; with nothing to show (no layout file, no probe) it does nothing.
void arm();
// Enters or leaves the layout for the current facts. Called at every populate and before every ApplyTuning.
void refresh();

}
