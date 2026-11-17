#pragma once

#include "swofty_api.h"

namespace runtime {

// Fills the addon-facing entries of the loader's API table: binding resolution, mixin registration, per-object
// fields, settings, and every SDK table, in the order addons have always seen them filled.
void publish_addon_api(SwoftyApi *a);

}
