#include "core/render/d3d12/swapchains.hpp"

#include "core/hud/arm_policy.hpp"
#include "core/render/d3d12/device_hooks.hpp"
#include "core/render/d3d12/subscribers.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstdio>

namespace render::d3d12 {

namespace {

std::string self_module()
{
    static std::string name = module_of(reinterpret_cast<const void *>(&self_module));
    return name;
}

using SlGetNativeInterface = int(__cdecl *)(void *proxy, void **native);

}

bool Swapchains::listed(const std::vector<void *> &v, void *sc) { return std::find(v.begin(), v.end(), sc) != v.end(); }

void Swapchains::store(Swapchain &c)
{
    void *sc = c.sc;
    std::lock_guard<std::mutex> g(lock);
    auto pend = std::find_if(pending_color_space.begin(), pending_color_space.end(),
                             [&](const std::pair<void *, UINT> &x) { return x.first == sc; });
    if (pend != pending_color_space.end()) {
        c.color_space = pend->second;
        c.color_space_set = true;
        pending_color_space.erase(pend);
    }
    auto it = std::find_if(chains.begin(), chains.end(), [&](const Swapchain &x) { return x.sc == sc; });
    if (it != chains.end())
        *it = c;
    else
        chains.push_back(c);
}

void Swapchains::created(const char *how, IUnknown *device, void *sc, UINT w, UINT h, DXGI_FORMAT fmt, UINT buffers, HMODULE impl)
{
    void *frames[48];
    USHORT n = RtlCaptureStackBackTrace(0, 48, frames, nullptr);
    std::vector<std::string> mods;
    for (USHORT i = 0; i < n; i++)
        mods.push_back(module_of(frames[i]));
    char impl_path[MAX_PATH] = {};
    DWORD impl_n = impl ? GetModuleFileNameA(impl, impl_path, MAX_PATH) : 0;
    std::string impl_name(hud::arm_policy::base_name(std::string_view(impl_path, impl_n)));
    std::string by = hud::arm_policy::chain(mods, self_module(), impl_name);
    ID3D12CommandQueue *q = nullptr;
    if (!device || FAILED(device->QueryInterface(IID_PPV_ARGS(&q))) || !q) {
        swofty::log("[d3d12] swapchain %p (%s, by %s) was not created on a D3D12 command queue; ignored", sc, how, by.c_str());
        return;
    }
    D3D12_COMMAND_QUEUE_DESC qd = q->GetDesc();
    q->Release();
    Swapchain c;
    c.sc = sc;
    c.queue = q;
    c.how = how;
    c.by = by;
    c.fg = hud::arm_policy::fg_in(mods);
    c.streamline = hud::arm_policy::streamline_in(mods);
    // FidelityFX's loader is on the stack whatever the settings, so it does not stand in for a non-direct queue.
    if (qd.Type != D3D12_COMMAND_LIST_TYPE_DIRECT && (c.fg.empty() || hud::arm_policy::ffx_loader(c.fg)))
        c.fg = "a non-direct present queue";
    store(c);
    swofty::log("[d3d12] swapchain %p created through %s on queue %p (type %d, %ux%u, format %d, %u buffer(s)); "
                "created by %s%s%s",
                sc, how, static_cast<void *>(q), int(qd.Type), w, h, int(fmt), buffers, by.c_str(),
                c.fg.empty() ? (c.streamline ? "; through Streamline's frame generation hook" : "") : "; frame generation: ",
                c.fg.c_str());
}

bool Swapchains::learn(IDXGISwapChain3 *sc, void *caller, const std::string &pfg, Swapchain &out)
{
    if (listed(refused, sc))
        return false;
    DWORD tid = GetCurrentThreadId();
    ID3D12CommandQueue *q = nullptr;
    HRESULT hr = sc->GetDevice(IID_PPV_ARGS(&q));
    const char *via = "IDXGISwapChain::GetDevice at Present";
    std::string native_note;
    if (FAILED(hr) || !q) {
        q = nullptr;
        HMODULE sl = GetModuleHandleW(L"sl.interposer.dll");
        auto get_native = sl ? reinterpret_cast<SlGetNativeInterface>(GetProcAddress(sl, "slGetNativeInterface")) : nullptr;
        if (!sl)
            native_note = "sl.interposer.dll is not loaded";
        else if (!get_native)
            native_note = "sl.interposer.dll has no slGetNativeInterface";
        else {
            void *native = nullptr;
            int sr = get_native(sc, &native);
            if (!hud::arm_policy::streamline_result_ok(sr) || !native) {
                char buf[64];
                snprintf(buf, sizeof buf, "slGetNativeInterface returned %d", sr);
                native_note = buf;
            } else {
                HRESULT nhr = static_cast<IDXGISwapChain *>(native)->GetDevice(IID_PPV_ARGS(&q));
                if (FAILED(nhr) || !q) {
                    q = nullptr;
                    char buf[96];
                    snprintf(buf, sizeof buf, "native swapchain %p GetDevice failed: 0x%08lx", native,
                             static_cast<unsigned long>(nhr));
                    native_note = buf;
                } else
                    via = "slGetNativeInterface + IDXGISwapChain::GetDevice at Present";
            }
        }
    }
    D3D12_COMMAND_QUEUE_DESC qd{};
    hud::arm_policy::ExecSeen exec{};
    if (q) {
        qd = q->GetDesc();
        q->Release();
    } else {
        bool told = listed(waiting, sc);
        if (!told && waiting.size() < 16)
            waiting.push_back(sc);
        using hud::arm_policy::UnreadQueue;
        UnreadQueue step = hud::arm_policy::unread_queue_step(exec_hooked(), unread_arm_asked, false);
        if (step == UnreadQueue::Arm) {
            unread_arm_asked = true;
            for (int i = 0, k = subscribers.unread_queues.n.load(); i < k; i++)
                if (subscribers.unread_queues.fns[i](sc, caller))
                    break;
            step = hud::arm_policy::unread_queue_step(exec_hooked(), unread_arm_asked, false);
        }
        if (step == UnreadQueue::Refuse) {
            refused.push_back(sc);
            swofty::log("[d3d12] swapchain %p was made before the creation hooks and its present queue cannot be read; "
                        "ignored (GetDevice: 0x%08lx; %s)",
                        static_cast<void *>(sc), static_cast<unsigned long>(hr), native_note.c_str());
            return false;
        }
        queue_watch.want();
        bool seen = queue_watch.recent_direct(tid, exec);
        if (hud::arm_policy::unread_queue_step(true, unread_arm_asked, seen) == UnreadQueue::Wait) {
            if (!told)
                swofty::log("[d3d12] swapchain %p was made before the creation hooks and its present queue cannot be "
                            "read (GetDevice: 0x%08lx; %s); no direct command queue has executed command lists yet, "
                            "so the overlay waits",
                            static_cast<void *>(sc), static_cast<unsigned long>(hr), native_note.c_str());
            return false;
        }
        q = static_cast<ID3D12CommandQueue *>(exec.queue);
        qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        via = exec.tid == tid ? "ID3D12CommandQueue::ExecuteCommandLists on the presenting thread"
                              : "ID3D12CommandQueue::ExecuteCommandLists, most recent direct queue";
    }
    std::string by = module_of(caller);
    out = Swapchain();
    out.sc = sc;
    out.queue = q;
    out.how = via;
    out.by = by;
    out.fg = hud::arm_policy::learned_fg(qd.Type == D3D12_COMMAND_LIST_TYPE_DIRECT, pfg);
    out.streamline = hud::arm_policy::streamline_caller(by);
    out.armed_late = exec.queue && unread_arm_asked;
    store(out);
    if (exec.queue)
        swofty::log("[d3d12] swapchain %p was made before the creation hooks; its present queue %p is the direct queue "
                    "that executed command lists on thread %lu %llu ms before Present on thread %lu; presented from "
                    "%s%s%s",
                    static_cast<void *>(sc), static_cast<void *>(q), static_cast<unsigned long>(exec.tid),
                    static_cast<unsigned long long>(GetTickCount64() - exec.tick), static_cast<unsigned long>(tid), by.c_str(),
                    out.streamline ? "; through Streamline" : "",
                    out.armed_late ? "; the device hooks armed for it after the game named its back buffers, so the "
                                     "overlay draws on it at Present"
                                   : "");
    else
        swofty::log("[d3d12] swapchain %p was made before the creation hooks; its queue %p (type %d) is read from the "
                    "swapchain; presented from %s%s",
                    static_cast<void *>(sc), static_cast<void *>(q), int(qd.Type), by.c_str(),
                    out.streamline ? "; through Streamline" : "");
    return true;
}

bool Swapchains::learning(void *sc) const { return listed(waiting, sc) && !listed(refused, sc); }

bool Swapchains::at_present(IDXGISwapChain3 *sc, const std::string &pfg, Swapchain &out)
{
    std::lock_guard<std::mutex> g(lock);
    auto it = std::find_if(chains.begin(), chains.end(), [&](const Swapchain &x) { return x.sc == sc; });
    if (it == chains.end())
        return false;
    if (it->fg.empty() && !pfg.empty())
        it->fg = pfg + " (calls Present)";
    out = *it;
    return true;
}

void Swapchains::per_buffer_queues(IDXGISwapChain3 *sc)
{
    std::lock_guard<std::mutex> g(lock);
    for (auto &c : chains)
        if (c.sc == sc && c.fg.empty()) {
            c.fg = "ResizeBuffers1 with per-buffer present queues";
            swofty::log("[d3d12] swapchain %p now presents through per-buffer queues; features leave it alone",
                        static_cast<void *>(sc));
        }
}

void Swapchains::set_color_space(IDXGISwapChain3 *sc, DXGI_COLOR_SPACE_TYPE cs, HRESULT hr)
{
    std::lock_guard<std::mutex> g(lock);
    auto it = std::find_if(chains.begin(), chains.end(), [&](const Swapchain &x) { return x.sc == sc; });
    if (it == chains.end()) {
        auto pend = std::find_if(pending_color_space.begin(), pending_color_space.end(),
                                 [&](const std::pair<void *, UINT> &x) { return x.first == sc; });
        if (SUCCEEDED(hr)) {
            if (pend != pending_color_space.end())
                pend->second = UINT(cs);
            else if (pending_color_space.size() < 16)
                pending_color_space.push_back({sc, UINT(cs)});
        }
        swofty::log("[d3d12] SetColorSpace1(%d) on swapchain %p before its creation was recorded -> 0x%08lx", int(cs),
                    static_cast<void *>(sc), static_cast<unsigned long>(hr));
    } else if (SUCCEEDED(hr) && (!it->color_space_set || it->color_space != UINT(cs))) {
        it->color_space = UINT(cs);
        it->color_space_set = true;
        swofty::log("[d3d12] swapchain %p colour space %d", static_cast<void *>(sc), int(cs));
    } else if (FAILED(hr)) {
        swofty::log("[d3d12] SetColorSpace1(%d) on swapchain %p failed: 0x%08lx", int(cs), static_cast<void *>(sc),
                    static_cast<unsigned long>(hr));
    }
}

}
