#pragma once

#include "swofty_api.h"

namespace runtime::loader {

// The loader's API table, as SwoftyCoreAttach received it; the core fills its addon-facing entries.
inline SwoftyApi *api = nullptr;

}
