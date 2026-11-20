#pragma once

#include <string>

namespace runtime {

enum Stage { STAGE_WINDOW = 1 };

// Runs fn at every loader stage; a handler registered after the window stage gets it once its setup is done.
void on_stage(void (*fn)(int stage));
void when_code_ready(const char *what, void (*fn)());
bool code_ready();

// Gives the window stage to the handlers registered after it (a setup deferred on a packed build).
void stage_catch_up();

// Whether a runtime asset feature is on: stable ones always, experimental ones when assets.json lists them.
bool enabled(const char *feature, const char *what);
std::string load_stage();
// The loader has given the window stage.
bool window_reached();

}
