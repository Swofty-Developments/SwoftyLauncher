#pragma once

#include <string>

#include "safetyhook.hpp"

namespace runtime {

// A failed safetyhook install as one log clause: what went wrong and where.
std::string hook_error(const safetyhook::InlineHook::Error &e);
std::string hook_error(const safetyhook::MidHook::Error &e);

}
