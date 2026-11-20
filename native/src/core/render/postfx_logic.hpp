#pragma once

// The parts of the post-process passes that are plain logic: which views a pass runs in, the key
// names a toggle option holds, texture formats and sizes, option values from settings text, and
// which addon's passes take the bytes an addon's code gives them.

#include "core/render/postfx/active_logic.hpp"
#include "core/render/postfx/addon_data_logic.hpp"
#include "core/render/postfx/descriptor_logic.hpp"
#include "core/render/postfx/effect_logic.hpp"
#include "core/render/postfx/frame_logic.hpp"
#include "core/render/postfx/option_logic.hpp"
#include "core/render/postfx/plan_logic.hpp"
#include "core/render/postfx/rule_logic.hpp"
#include "core/render/postfx/signal_logic.hpp"
#include "core/render/postfx/texture_logic.hpp"
#include "core/render/postfx/view_logic.hpp"
