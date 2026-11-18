#pragma once
#include "core/render/postfx/descriptor_logic.hpp"
#include "core/render/postfx/effect_logic.hpp"
#include "core/render/postfx/texture_logic.hpp"
#include "core/render/postfx/view_logic.hpp"

#include <windows.h>

#include <d3d12.h>

#include <cstdint>
#include <string>
#include <vector>

namespace render::postfx {

// A pass's descriptor table: t0 the picture, t1 depth, t2..t9 its inputs, t10 its data (DATA_AT), u0 its
// output (TABLE - 1). DATA_MAX bytes of data at most a pass, DATA_ALIGN apart in a slot's upload buffer.
inline constexpr UINT FRAMES = 8, MAX_PASSES = 64, CONSTANTS = CONSTANT_COUNT, BUDGET_FRAMES = 120, INPUTS = 8,
                      DATA_AT = 2 + INPUTS, TABLE = DATA_AT + 2, DATA_MAX = 64 * 1024, DATA_ALIGN = 256;
inline constexpr UINT MIP_STEPS = 512;
inline constexpr D3D12_RESOURCE_STATES HOME = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
inline constexpr D3D12_RESOURCE_STATES READ_STATES =
    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;

struct LiveOption {
    UINT slot;
    std::string option;
};

struct RenderPass {
    std::string addon, addon_id, name, shader, enabled;
    std::vector<uint8_t> dxil;
    float options[SLOTS] = {};
    std::vector<LiveOption> live;
    int inputs[INPUTS] = {IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE};
    UINT n_inputs = 0;
    int output = -1;
    UINT threads[2] = {8, 8};
    When when;
    double budget_ms = 1.5;
    ID3D12PipelineState *pso = nullptr;
    bool off = false, on = true, ruled_off = false;
    const char *why = nullptr;
    double ms_sum = 0, ms_peak = 0;
    unsigned ms_frames = 0, over = 0;
    int inst = -1;
    std::string effect, label;
};

struct PassTexture {
    std::string name, file;
    double scale = 1;
    UINT width = 0, height = 0;
    DXGI_FORMAT format = DXGI_FORMAT_R16G16B16A16_FLOAT;
    bool persistent = false;
    uint32_t mips = 1;
    ID3D12Resource *res = nullptr, *upload = nullptr;
    UINT w = 0, h = 0;
    std::string pixels;
    bool clear = false, uploaded = false;

    UINT levels() const { return file.empty() ? mip_levels(w, h, mips) : 1; }
    // Persistent textures and images are shared by every slot; the rest are each slot's own.
    bool shared() const { return persistent || !file.empty(); }
};

struct AddonToggle {
    std::string addon_id, addon, option, key;
    int vk = 0;
    bool down = false, off = false;
};

// One frame in flight: its own list for running between the game's lists, its fence, the passes it ran
// (for their GPU times) and the descriptors it last wrote.
struct Slot {
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12Fence *fence = nullptr;
    UINT64 fence_value = 0;
    bool pending = false;
    ULONGLONG armed_at = 0;
    UINT passes = 0;
    int pass_of[MAX_PASSES] = {};
    ID3D12Resource *depth = nullptr;
    Written<ID3D12Resource, DXGI_FORMAT> views[MAX_PASSES * TABLE];
    unsigned views_made = 0;
    // The passes' data this slot ran with: an upload buffer, mapped while it lives.
    ID3D12Resource *data = nullptr;
    uint8_t *data_at = nullptr;
    UINT data_cap = 0;

    bool gpu_done() const { return !pending && fence->GetCompletedValue() >= fence_value; }
};

}
