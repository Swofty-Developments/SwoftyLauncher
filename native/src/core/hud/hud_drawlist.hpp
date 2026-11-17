#pragma once

// The overlay's portable draw list: paint callbacks tessellate straight into triangles in framebuffer
// pixels, grouped into batches that share a texture and a clip rect. v1 calls keep their hard-edged
// geometry; v2 shapes carry a one-pixel anti-aliasing fringe.

#include "core/hud/drawlist/frame.hpp"
#include "core/hud/drawlist/tessellate.hpp"
#include "core/hud/drawlist/primitives.hpp"
#include "core/hud/drawlist/text.hpp"
#include "core/hud/drawlist/shapes.hpp"
#include "core/hud/drawlist/state.hpp"
#include "core/hud/drawlist/registry.hpp"
