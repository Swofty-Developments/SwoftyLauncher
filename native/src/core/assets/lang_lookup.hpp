#pragma once

#include <optional>
#include <string>

#include "core/hooks/dispatch.hpp"

namespace runtime {

swofty_core::hooks::GameLink *lang_lookup_link();
std::optional<std::string> lang_lookup_fire(const char *key, const char *language, const char *stock);
uint64_t lang_lookup_epoch();

}
