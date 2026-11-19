#include "core/render/badges/badge_images.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

namespace badge_images {
void trace(const char *message) { swofty::log("[badges] %s", message); }
}
