#pragma once
#include "core/hud/hud_image.hpp"

namespace hud {

// The built-in font atlas on its way to the GPU: decoded by the first texture sync that asks for it, handed out
// until that sync has uploaded it, then dropped. A decode that fails is not tried again.
class FontUpload {
public:
    template <class Decode> const img::Pixels *pending(Decode decode)
    {
        if (!tried_) {
            tried_ = true;
            decode(pixels_);
        }
        return pixels_.rgba.empty() ? nullptr : &pixels_;
    }

    void done() { pixels_ = {}; }

private:
    img::Pixels pixels_;
    bool tried_ = false;
};

}
