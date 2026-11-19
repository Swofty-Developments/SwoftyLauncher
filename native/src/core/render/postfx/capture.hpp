#pragma once
#include "core/render/postfx/gpu.hpp"
#include "core/render/postfx/pass.hpp"
#include "core/render/postfx/resource_watch.hpp"

#include <windows.h>

#include <d3d12.h>

#include <string>

namespace render::postfx {

// SWOFTY_POSTFX_CAPTURE: the picture before and after the passes, read back and written as a pair of PPM files,
// a few times after the passes first run and whenever swofty/postfx.capture appears.
class PictureCapture {
public:
    void configure(const std::string &dir, const std::wstring &file);
    const std::string &dir() const { return dir_; }
    // Remakes the readback buffers for a picture of desc's size.
    void remake(const GpuContext &gpu, const D3D12_RESOURCE_DESC &desc, RetireQueue &retired, unsigned frame_count);
    // Writes the pair whose slot the GPU has finished.
    void land(const Slot *slots, const D3D12_RESOURCE_DESC &desc, DXGI_FORMAT view);
    // Decides whether this run copies the picture.
    void arm(ULONGLONG now, unsigned ran_total);
    bool armed() const { return now_; }
    void copy(ID3D12GraphicsCommandList *l, ID3D12Resource *back, int which) const;
    void ran(int slot, ULONGLONG now);
    void first_run(ULONGLONG now) { next = now + 30000; }
    void signalled(int slot, UINT64 fence_value);

private:
    std::string dir_;
    std::wstring file;
    ID3D12Resource *rb[2] = {};
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    ULONGLONG next = 0, checked = 0;
    unsigned captures = 0;
    bool asked = false, pending = false, now_ = false;
    UINT64 fence = 0;
    int slot_ = -1;
};

// SWOFTY_POSTFX_CLIP: the next frames that run passes, scaled to 1280x720 and written as PPM files, when
// swofty/postfx.clip appears.
class ClipRecorder {
public:
    struct Entry {
        ID3D12Resource *rb = nullptr;
        int slot = -1;
        unsigned land = 0, index = 0;
        bool pending = false;
    };

    void configure(const std::string &dir, const std::wstring &file);
    // Starts a clip when its file appeared.
    void check_file();
    // A free readback entry for this frame's picture while a clip is being captured; none when the ring is
    // full, so the frame is not stalled and the capture tries again on the next one.
    Entry *begin(const GpuContext &gpu, const D3D12_RESOURCE_DESC &d, int k, DXGI_FORMAT view, RetireQueue &retired,
                 unsigned frame_count);
    void copy(ID3D12GraphicsCommandList *l, ID3D12Resource *back, const Entry &e) const;
    // Writes the frames whose readback the GPU has finished, a few frames after they were copied.
    void land(const Slot *slots, unsigned frame_count);

private:
    static constexpr UINT CLIP_W = 1280, CLIP_H = 720, CLIP_RING = 4, CLIP_DELAY = 6;

    bool busy() const;
    void start(unsigned frames);
    void write(const uint8_t *src, UINT pitch, unsigned index) const;

    Entry ring[CLIP_RING];
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    UINT w = 0, h = 0;
    DXGI_FORMAT view_ = DXGI_FORMAT_UNKNOWN;
    unsigned left = 0, issued = 0, written = 0;
    bool running = false;
    std::string dir;
    std::wstring file;
};

// swofty/postfx.burst: the next frames as the game hands them to Present, read back from the middle of the back
// buffer, logged as their mean colour and saturation and, with SWOFTY_POSTFX_BURST_DUMP, written as PPM files.
class BurstProbe {
public:
    void configure(const std::string &dump_dir) { dir = dump_dir; }
    bool active() const { return left != 0; }
    // Starts a burst when its file appeared and none runs.
    void check_file(const std::wstring &swofty_root);
    // Copies the back buffer in a batch that hands it from render target to Present.
    void record(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, const GpuContext &gpu,
                const ResourceWatch &watched, unsigned frame_count);
    void land(unsigned frame_count, DXGI_FORMAT view);

private:
    struct Probe {
        ID3D12Resource *rb = nullptr;
        unsigned frame = 0, land = 0;
        bool pending = false;
    };

    Probe probes[256];
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    UINT w = 0, h = 0;
    unsigned left = 0, at = 0;
    std::string dir;
};

}
