#pragma once
#include "core/render/postfx/resource_watch.hpp"

#include <windows.h>

#include <d3d12.h>

#include <atomic>

namespace render::postfx {

// The back buffers' render target views by descriptor address, and the latest views made, so a view
// made before its back buffer was named is still found. A list that binds one of them draws into that
// back buffer: the game sometimes ends the picture's list inside the HUD-less step and draws the rest
// of the picture in the next list, and that list closes the step with the back buffer bound. Read and
// written without the lock.
class BackBufferViews {
public:
    // The game named res a back buffer: the views made of it before then.
    void named(ID3D12Resource *res);
    void created(ID3D12Resource *res, D3D12_CPU_DESCRIPTOR_HANDLE at, const ResourceWatch &watched);
    // The back buffer whose view is at `at`; nullptr for any other view. Binds of anything else fall outside the
    // back buffers' views and return at the range check.
    ID3D12Resource *bound(SIZE_T at) const;

private:
    struct ViewAt {
        std::atomic<SIZE_T> at{0};
        std::atomic<ID3D12Resource *> res{nullptr};
    };

    void pin(SIZE_T at, ID3D12Resource *res);

    ViewAt back_views[32], recent_views[64];
    std::atomic<unsigned> recent_next{0};
    std::atomic<SIZE_T> views_lo{SIZE_T(-1)}, views_hi{0};
};

}
