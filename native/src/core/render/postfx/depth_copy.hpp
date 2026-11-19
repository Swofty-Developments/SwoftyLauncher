#pragma once
#include "core/render/postfx/gpu.hpp"
#include "core/render/postfx/resource_watch.hpp"

#include <windows.h>

#include <d3d12.h>

#include <atomic>
#include <mutex>

namespace render::postfx {

// The depth buffer as the game leaves each of its passes that write it. On track the passes run in the
// HUD's list, which never moves the depth buffer, so its state there is unknown; instead the core copies
// the buffer right before the game's own barrier that takes it out of DEPTH_WRITE, where its state is
// that barrier's StateBefore, exactly (record). The last copy of a frame holds the frame's finished
// depth. The copy rests in NON_PIXEL_SHADER_RESOURCE, and passes read it wherever the place they run
// leaves no readable depth of its own.
class DepthCopy {
public:
    // Whether copies are still made: passes ran without readable depth within the last FRAMES frames.
    bool wanted(unsigned frame_count) const { return frame_count <= until.load(std::memory_order_relaxed); }
    // Before the game records a barrier batch that takes its depth buffer out of DEPTH_WRITE: copies the
    // buffer, from the state the barrier names as its StateBefore, and puts it back in that state, so the
    // game's barrier then runs as it would have. Whole-resource, unsplit barriers in direct lists only; one
    // copy per batch.
    void record(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, const GpuContext &gpu,
                const ResourceWatch &watched, std::recursive_mutex &lock, RetireQueue &retired, const unsigned &frame_count);
    // A place without readable depth asks for the copy: copies go on being made, and the copy is returned when
    // the game wrote its depth buffer this frame or the one before.
    ID3D12Resource *fresh(unsigned frame_count);

private:
    static constexpr unsigned FRAMES = 120;

    // The copy for the game's depth buffer src, made or remade when the game's buffer changed. Under lock.
    ID3D12Resource *for_source(ID3D12Resource *src, const D3D12_RESOURCE_DESC &d, const GpuContext &gpu, RetireQueue &retired,
                               unsigned frame_count);

    std::atomic<ID3D12Resource *> copy{nullptr};
    std::atomic<unsigned> copied_at{0}; // frame_count when a copy was last recorded
    std::atomic<unsigned> until{0};     // copies are made until this frame: passes ran without readable depth
    ID3D12Resource *copy_of = nullptr;  // the game's buffer the copy matches, compared, not held; under lock
    D3D12_RESOURCE_DESC copy_desc{};
    bool logged = false, failed = false;
};

}
