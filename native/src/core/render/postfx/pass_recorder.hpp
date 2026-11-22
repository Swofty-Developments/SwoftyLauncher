#pragma once
#include "core/render/postfx/addon_data_logic.hpp"
#include "core/render/postfx/pass.hpp"

#include <windows.h>

#include <d3d12.h>

#include <atomic>
#include <mutex>
#include <vector>

namespace render::postfx {

class Pipeline;

// Records a frame's passes into a slot: into the game's list at the place the frame's lists leave for them, or into
// the slot's own list submitted between the game's lists. Each slot has its own fence, timestamp queries and data
// buffer.
class PassRecorder {
public:
    // Where a frame's passes are recorded: into the game's list at its HUD-less step, or into a slot's own
    // list between the game's lists. The states are the resources' states at that point of the frame.
    struct Where {
        ID3D12GraphicsCommandList *list;
        bool game;
        ID3D12Resource *back;
        D3D12_RESOURCE_STATES back_state;
        ID3D12Resource *hudless;
        D3D12_RESOURCE_STATES hudless_state;
        ID3D12Resource *depth;
        D3D12_RESOURCE_STATES depth_state;
        const char *path;
    };

    explicit PassRecorder(Pipeline &owner) : pipeline(owner) {}

    Slot slots[FRAMES];
    UINT slot_at = 0;
    ID3D12QueryHeap *queries = nullptr;
    ID3D12Resource *query_readback = nullptr;
    double ticks_per_ms = 0;
    bool warned_busy = false, hudless_logged = false, depth_logged = false;
    // The bytes each addon's code gave one of its passes (SwoftyCorePostfxSetData). Every frame a pass reads the
    // latest as t10, from its slot's own upload buffer.
    std::mutex data_lock;
    DataStore pass_data;
    std::atomic<bool> any_data{false};
    bool data_logged = false;

    // The device comes from the game's resource, never from the queue: a queue can be Streamline's proxy,
    // whose device makes proxy command lists that the hooked ExecuteCommandLists cannot take.
    bool init(ID3D12Resource *res);
    // The GPU clock rate comes from the first queue the passes run on.
    void read_frequency(ID3D12CommandQueue *q);
    int pick_slot();
    // Records this frame's passes at w into slot k's tables. Returns false when nothing was recorded.
    bool record(const Where &w, int k);
    // The list of the core's own for a frame whose picture list ends with the back buffer still a render
    // target and draws no HUD, for one whose HUD-less step another list closed, or at Present. True when it ran.
    bool run_between(ID3D12CommandQueue *q, ID3D12Resource *back, D3D12_RESOURCE_STATES state, ID3D12Resource *hudless,
                     D3D12_RESOURCE_STATES hudless_state, const char *path);

private:
    void pack_data(Slot &s, const std::vector<int> &live, std::vector<UINT> &offsets, std::vector<UINT> &sizes);
    void read_times(Slot &s);

    Pipeline &pipeline;
};

}
