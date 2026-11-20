#pragma once
#include "core/render/d3d12_hooks.hpp"

#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace render::d3d12 {

// The game's swapchains the core knows: recorded at creation, or learned at their first Present when they
// were made before the creation hooks.
class Swapchains {
public:
    void created(const char *how, IUnknown *device, void *sc, UINT w, UINT h, DXGI_FORMAT fmt, UINT buffers, HMODULE impl);
    bool learn(IDXGISwapChain3 *sc, void *caller, const std::string &pfg, Swapchain &out);
    bool learning(void *sc) const;
    bool at_present(IDXGISwapChain3 *sc, const std::string &pfg, Swapchain &out);
    void per_buffer_queues(IDXGISwapChain3 *sc);
    void set_color_space(IDXGISwapChain3 *sc, DXGI_COLOR_SPACE_TYPE cs, HRESULT hr);

private:
    void store(Swapchain &c);
    static bool listed(const std::vector<void *> &v, void *sc);

    std::mutex lock;
    std::vector<Swapchain> chains;
    std::vector<std::pair<void *, UINT>> pending_color_space;
    std::vector<void *> refused, waiting;
    bool unread_arm_asked = false;
};

}
