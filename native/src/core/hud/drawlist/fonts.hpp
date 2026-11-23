#pragma once
#include <cstdint>

namespace hud::drawlist {

// A glyph of a loaded font, in em units relative to the pen on the baseline, and its rect in an atlas page.
struct FaceGlyph {
    float x0 = 0, y0 = 0, x1 = 0, y1 = 0;
    float u0 = 0, v0 = 0, u1 = 0, v1 = 0;
    int slot = -1;
};

// A font an addon loaded. Glyph indices come from index(); 0 is a codepoint the font lacks. glyph() rasterises
// its glyph into an atlas page the first time it is asked for.
class Face {
public:
    virtual ~Face() = default;
    virtual uint32_t index(uint32_t cp) const = 0;
    virtual float advance(uint32_t glyph) const = 0;
    virtual float kern(uint32_t left, uint32_t right) const = 0;
    virtual FaceGlyph glyph(uint32_t glyph) = 0;

    float baseline = 0.75f;
    float atlas_em = 64, spread = 8;
    uint64_t serial = 0;
};

}
