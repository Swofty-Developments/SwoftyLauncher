#include "core/diagnostics/perf.hpp"
#include "core/hud/arm_policy.hpp"
#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12/subscribers.hpp"
#include "core/render/d3d12/swapchains.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <mutex>
#include <vector>

namespace render::d3d12 {

namespace {

constinit Swapchains swapchains;

// The swapchain set: the swapchain's Present, ResizeBuffers and SetColorSpace1, the DXGI factory's swapchain creations,
// and the module each creation's code is in.
struct SwapchainHooks : runtime::loader::HookSet<> {
    safetyhook::InlineHook present, present1, resize, resize1, color_space, create, create_hwnd, create_core, create_comp;
    HMODULE impl_create{}, impl_hwnd{}, impl_core{}, impl_comp{};
};

SwapchainHooks hooks;
thread_local int creating, presenting;
std::atomic<bool> in_present;

HRESULT STDMETHODCALLTYPE create_hook(IDXGIFactory *f, IUnknown *dev, DXGI_SWAP_CHAIN_DESC *d, IDXGISwapChain **out)
{
    creating++;
    HRESULT hr = hooks.create.call<HRESULT>(f, dev, d, out);
    if (--creating == 0 && SUCCEEDED(hr) && out && *out && d)
        swapchains.created("IDXGIFactory::CreateSwapChain", dev, *out, d->BufferDesc.Width, d->BufferDesc.Height,
                           d->BufferDesc.Format, d->BufferCount, hooks.impl_create);
    return hr;
}

HRESULT STDMETHODCALLTYPE create_hwnd_hook(IDXGIFactory2 *f, IUnknown *dev, HWND wnd, const DXGI_SWAP_CHAIN_DESC1 *d,
                                           const DXGI_SWAP_CHAIN_FULLSCREEN_DESC *fs, IDXGIOutput *o, IDXGISwapChain1 **out)
{
    creating++;
    HRESULT hr = hooks.create_hwnd.call<HRESULT>(f, dev, wnd, d, fs, o, out);
    if (--creating == 0 && SUCCEEDED(hr) && out && *out && d)
        swapchains.created("CreateSwapChainForHwnd", dev, *out, d->Width, d->Height, d->Format, d->BufferCount, hooks.impl_hwnd);
    return hr;
}

HRESULT STDMETHODCALLTYPE create_core_hook(IDXGIFactory2 *f, IUnknown *dev, IUnknown *wnd, const DXGI_SWAP_CHAIN_DESC1 *d,
                                           IDXGIOutput *o, IDXGISwapChain1 **out)
{
    creating++;
    HRESULT hr = hooks.create_core.call<HRESULT>(f, dev, wnd, d, o, out);
    if (--creating == 0 && SUCCEEDED(hr) && out && *out && d)
        swapchains.created("CreateSwapChainForCoreWindow", dev, *out, d->Width, d->Height, d->Format, d->BufferCount,
                           hooks.impl_core);
    return hr;
}

HRESULT STDMETHODCALLTYPE create_comp_hook(IDXGIFactory2 *f, IUnknown *dev, const DXGI_SWAP_CHAIN_DESC1 *d, IDXGIOutput *o,
                                           IDXGISwapChain1 **out)
{
    creating++;
    HRESULT hr = hooks.create_comp.call<HRESULT>(f, dev, d, o, out);
    if (--creating == 0 && SUCCEEDED(hr) && out && *out && d)
        swapchains.created("CreateSwapChainForComposition", dev, *out, d->Width, d->Height, d->Format, d->BufferCount,
                           hooks.impl_comp);
    return hr;
}

// The frame generation module a Present came from, remembered for the last caller.
class FgCaller {
public:
    std::string of(void *caller)
    {
        if (caller == last)
            return last_fg;
        std::string name = module_of(caller);
        last = caller;
        last_fg = hud::arm_policy::fg_module(name) && !hud::arm_policy::streamline_hook(name) ? name : std::string();
        return last_fg;
    }

private:
    void *last = nullptr;
    std::string last_fg;
};

constinit FgCaller fg_caller;

struct PresentSeen {
    void *sc, *caller;
    DWORD tid;
    const char *how;
};

// One line for each new swapchain, caller, thread and entry point that presents, and a rate each minute.
class PresentLog {
public:
    void count(IDXGISwapChain3 *sc, void *caller, const char *how);
    uint64_t frames() const { return frames_.load(std::memory_order_relaxed); }

private:
    std::mutex lock;
    std::vector<PresentSeen> seen;
    ULONGLONG since = 0;
    unsigned presents = 0;
    std::atomic<uint64_t> frames_{0};
};

constinit PresentLog present_log;

void PresentLog::count(IDXGISwapChain3 *sc, void *caller, const char *how)
{
    DWORD tid = GetCurrentThreadId();
    ULONGLONG now = GetTickCount64();
    std::lock_guard<std::mutex> g(lock);
    presents++;
    frames_.fetch_add(1, std::memory_order_relaxed);
    if (!since)
        since = now;
    bool fresh = seen.size() < 32 &&
                 std::none_of(
                     seen.begin(), seen.end(),
                     [&](const PresentSeen &p) { return p.sc == sc && p.caller == caller && p.tid == tid && p.how == how; });
    if (fresh) {
        seen.push_back({sc, caller, tid, how});
        swofty::log("[d3d12] %s on swapchain %p from %s (%p) on thread %lu", how, static_cast<void *>(sc),
                    module_of(caller).c_str(), caller, tid);
    }
    if (now - since >= 60000) {
        swofty::log("[d3d12] last %llu s: %u present(s), %.1f per second", static_cast<unsigned long long>((now - since) / 1000),
                    presents, presents * 1000.0 / double(now - since));
        since = now;
        presents = 0;
    }
}

// Present and Present1 both close a frame. One that runs inside the other on the same thread, as DXVK's Present
// calls its Present1, is the same frame and is dispatched once, by the outer call.
template <class Original>
HRESULT dispatch_present(IDXGISwapChain3 *sc, UINT sync, UINT flags, void *caller, const char *how, Original original)
{
    if (!hud::arm_policy::dispatches_present(presenting))
        return original();
    uint64_t entry = perf::stamp();
    presenting++;
    if (!(flags & DXGI_PRESENT_TEST))
        present_log.count(sc, caller, how);
    if (!(flags & DXGI_PRESENT_TEST) && subscribers.presents.n.load() && !in_present.exchange(true)) {
        std::string pfg = fg_caller.of(caller);
        Swapchain c;
        bool known = swapchains.at_present(sc, pfg, c);
        if (!known)
            known = swapchains.learn(sc, caller, pfg, c);
        for (int i = 0, n = subscribers.presents.n.load(); i < n; i++)
            subscribers.presents.fns[i](sc, known ? &c : nullptr, caller);
        in_present.store(false);
    }
    uint64_t before = perf::stamp();
    HRESULT hr = original();
    if (!(flags & DXGI_PRESENT_TEST))
        perf::presented(sc, sync, flags, hr, entry, before, perf::stamp());
    for (int i = 0, n = subscribers.presenteds.n.load(); i < n; i++)
        subscribers.presenteds.fns[i](sc, hr);
    presenting--;
    return hr;
}

HRESULT STDMETHODCALLTYPE present_hook(IDXGISwapChain3 *sc, UINT sync, UINT flags)
{
    return dispatch_present(sc, sync, flags, __builtin_return_address(0), "Present",
                            [&] { return hooks.present.call<HRESULT>(sc, sync, flags); });
}

HRESULT STDMETHODCALLTYPE present1_hook(IDXGISwapChain3 *sc, UINT sync, UINT flags, const DXGI_PRESENT_PARAMETERS *params)
{
    return dispatch_present(sc, sync, flags, __builtin_return_address(0), "Present1",
                            [&] { return hooks.present1.call<HRESULT>(sc, sync, flags, params); });
}

HRESULT STDMETHODCALLTYPE resize_hook(IDXGISwapChain3 *sc, UINT count, UINT w, UINT h, DXGI_FORMAT fmt, UINT flags)
{
    for (int i = 0, n = subscribers.resizes.n.load(); i < n; i++)
        subscribers.resizes.fns[i](sc);
    return hooks.resize.call<HRESULT>(sc, count, w, h, fmt, flags);
}

HRESULT STDMETHODCALLTYPE resize1_hook(IDXGISwapChain3 *sc, UINT count, UINT w, UINT h, DXGI_FORMAT fmt, UINT flags,
                                       const UINT *mask, IUnknown *const *queues)
{
    for (int i = 0, n = subscribers.resizes.n.load(); i < n; i++)
        subscribers.resizes.fns[i](sc);
    if (queues && count)
        swapchains.per_buffer_queues(sc);
    return hooks.resize1.call<HRESULT>(sc, count, w, h, fmt, flags, mask, queues);
}

HRESULT STDMETHODCALLTYPE color_space_hook(IDXGISwapChain3 *sc, DXGI_COLOR_SPACE_TYPE cs)
{
    HRESULT hr = hooks.color_space.call<HRESULT>(sc, cs);
    swapchains.set_color_space(sc, cs, hr);
    return hr;
}

bool hook_factory(void **vt)
{
    struct Want {
        int slot;
        safetyhook::InlineHook *h;
        void *detour;
        HMODULE *impl;
        const char *what;
        bool required;
    } wants[] = {
        {15, &hooks.create_hwnd, reinterpret_cast<void *>(&create_hwnd_hook), &hooks.impl_hwnd, "CreateSwapChainForHwnd", true},
        {10, &hooks.create, reinterpret_cast<void *>(&create_hook), &hooks.impl_create, "CreateSwapChain", false},
        {16, &hooks.create_core, reinterpret_cast<void *>(&create_core_hook), &hooks.impl_core, "CreateSwapChainForCoreWindow",
         false},
        {24, &hooks.create_comp, reinterpret_cast<void *>(&create_comp_hook), &hooks.impl_comp, "CreateSwapChainForComposition",
         false},
    };
    std::vector<void *> done;
    std::string list;
    for (auto &w : wants) {
        void *target = vt[w.slot];
        if (std::find(done.begin(), done.end(), target) != done.end()) {
            swofty::log("[d3d12] IDXGIFactory %s shares its code with another hooked slot; not hooked twice", w.what);
            continue;
        }
        module_of(target, w.impl);
        if (!install(*w.h, target, w.detour, w.what)) {
            if (w.required)
                return false;
            continue;
        }
        done.push_back(target);
        char b[64];
        snprintf(b, sizeof b, "%s%s (vt+0x%x)", list.empty() ? "" : ", ", w.what, unsigned(w.slot * 8));
        list += b;
    }
    swofty::log("[d3d12] swapchain creation hooked on the DXGI factory (%s, in %s)", list.c_str(), module_of(vt[15]).c_str());
    return true;
}

using CreateFactoryFn = HRESULT(WINAPI *)(REFIID, void **);

}

bool learning(void *sc) { return swapchains.learning(sc); }

uint64_t frames_presented() { return present_log.frames(); }

bool arm_swapchain()
{
    if (!hooks.gate.offer())
        return hooks.live();
    HMODULE dxgi = LoadLibraryA("dxgi.dll");
    auto create_factory = dxgi ? reinterpret_cast<CreateFactoryFn>(GetProcAddress(dxgi, "CreateDXGIFactory1")) : nullptr;
    if (!create_factory) {
        swofty::log("[d3d12] dxgi.dll!CreateDXGIFactory1 is unavailable; the swapchain hooks are not armed");
        return settle(hooks, false, "dxgi.dll!CreateDXGIFactory1 is unavailable");
    }
    WNDCLASSEXW wc{};
    wc.cbSize = sizeof wc;
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"SwoftyD3D12Probe";
    RegisterClassExW(&wc);
    HWND wnd =
        CreateWindowExW(0, wc.lpszClassName, L"", WS_OVERLAPPEDWINDOW, 0, 0, 64, 64, nullptr, nullptr, wc.hInstance, nullptr);
    if (!wnd) {
        swofty::log("[d3d12] could not make a probe window to read the swapchain vtable");
        UnregisterClassW(wc.lpszClassName, wc.hInstance);
        return settle(hooks, false, "no probe window to read the swapchain vtable");
    }
    ID3D12Device *dev = probe_device();
    ID3D12CommandQueue *q = nullptr;
    IDXGIFactory4 *factory = nullptr;
    IDXGISwapChain1 *swap1 = nullptr;
    IDXGISwapChain3 *swap = nullptr;
    bool ok = false;
    do {
        if (!dev)
            break;
        D3D12_COMMAND_QUEUE_DESC qd{};
        qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        if (FAILED(dev->CreateCommandQueue(&qd, IID_PPV_ARGS(&q))))
            break;
        if (FAILED(create_factory(IID_PPV_ARGS(&factory))))
            break;
        DXGI_SWAP_CHAIN_DESC1 sd{};
        sd.BufferCount = 2;
        sd.Width = 64;
        sd.Height = 64;
        sd.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        sd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        sd.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
        sd.SampleDesc.Count = 1;
        if (FAILED(factory->CreateSwapChainForHwnd(q, wnd, &sd, nullptr, nullptr, &swap1)))
            break;
        if (FAILED(swap1->QueryInterface(IID_PPV_ARGS(&swap))))
            break;
        void **sc_vt = *reinterpret_cast<void ***>(swap);
        void **f_vt = *reinterpret_cast<void ***>(factory);
        ok = hook_factory(f_vt) &&
             install(hooks.present, sc_vt[8], reinterpret_cast<void *>(&present_hook), "IDXGISwapChain::Present");
        if (!ok)
            break;
        bool present1 = sc_vt[22] != sc_vt[8] &&
                        install(hooks.present1, sc_vt[22], reinterpret_cast<void *>(&present1_hook), "IDXGISwapChain1::Present1");
        install(hooks.resize, sc_vt[13], reinterpret_cast<void *>(&resize_hook), "IDXGISwapChain::ResizeBuffers");
        install(hooks.resize1, sc_vt[39], reinterpret_cast<void *>(&resize1_hook), "IDXGISwapChain3::ResizeBuffers1");
        install(hooks.color_space, sc_vt[38], reinterpret_cast<void *>(&color_space_hook), "IDXGISwapChain3::SetColorSpace1");
        swofty::log("[d3d12] swapchain hooks armed in %s: Present (vt+0x40), %sResizeBuffers (vt+0x68), "
                    "SetColorSpace1 (vt+0x130), ResizeBuffers1 (vt+0x138); only swapchains created after this point "
                    "are known",
                    module_of(sc_vt[8]).c_str(), present1 ? "Present1 (vt+0xb0), " : "");
    } while (false);
    release(swap);
    release(swap1);
    release(factory);
    release(q);
    release(dev);
    DestroyWindow(wnd);
    UnregisterClassW(wc.lpszClassName, wc.hInstance);
    return settle(hooks, ok);
}

}
