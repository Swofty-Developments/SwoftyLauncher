#pragma once
// How the overlay and the core's D3D12 hooks arm and draw, as decisions without Windows calls so the host tests run
// them: the frame generation and present layer modules they look for, when the present and device hooks arm, how a
// swapchain's present queue is learned, the game's hardware settings they read, the gate a swapchain draws through,
// and when a back buffer can take the overlay.
#include "core/hud/arm_policy/arm.hpp"
#include "core/hud/arm_policy/draw_target.hpp"
#include "core/hud/arm_policy/game_settings.hpp"
#include "core/hud/arm_policy/gates.hpp"
#include "core/hud/arm_policy/modules.hpp"
#include "core/hud/arm_policy/present_queue.hpp"
