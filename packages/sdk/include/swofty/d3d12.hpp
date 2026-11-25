// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#if defined(_WIN32)
#include <windows.h>
#endif

// The game's shader resource views as they are made. The core owns ID3D12Device::CreateShaderResourceView (vtable
// slot 18) and hooks it once; an addon registers a callback here instead of patching the device's vtable itself. The
// core calls every callback right after the game's view is written, on the thread that made it (the game's load and
// render threads, several at once), with the view's resource, its description and the CPU descriptor handle it was
// written to. Typical use: find the view of one known texture (by size and format) to learn its descriptor, then swap
// what the shader reads through the addon's own path.
//
// A callback must return quickly and must not block on a lock the game's render threads may hold. A view the callback
// makes itself through the device does not call the callbacks again. Pointers in SrvCreated are the game's and valid
// for the call only; AddRef the resource to keep it. A core without the calls (older than the launcher this SDK came
// with) reads as unavailable.

namespace swofty::d3d12 {

// One view the game made. Fields past `size` (the bytes the running core filled) read as zero.
struct SrvCreated {
    uint32_t size;
    uint32_t format;              // DXGI_FORMAT of the resource (D3D12_RESOURCE_DESC::Format)
    void *device;                 // ID3D12Device *
    void *resource;               // ID3D12Resource *, never null
    const void *desc;             // const D3D12_SHADER_RESOURCE_VIEW_DESC *, as written (after the core's clamp); null
                                  // for the resource's default view
    uint64_t cpu_handle;          // D3D12_CPU_DESCRIPTOR_HANDLE::ptr the view was written to
    uint64_t width;               // D3D12_RESOURCE_DESC::Width
    uint32_t height;              // D3D12_RESOURCE_DESC::Height
    uint32_t dimension;           // D3D12_RESOURCE_DIMENSION
    uint16_t depth_or_array_size; // D3D12_RESOURCE_DESC::DepthOrArraySize
    uint16_t mip_levels;          // D3D12_RESOURCE_DESC::MipLevels
    uint32_t reserved;
};
static_assert(sizeof(SrvCreated) == 64, "SrvCreated crosses the core boundary: keep its layout");

using SrvFn = void (*)(void *user, const SrvCreated *view);

// ok: registered; the hook is live, or arms at the core's ready step (a registration from SwoftyAddonInit or the
// addon's on-load). not_live: registered, but the core could not hook the call on this system (see the [gfx] lines in
// loader.log); the callback never runs. full: 16 callbacks are registered already.
enum class Result { ok, not_live, unavailable, invalid, full };

namespace detail {

using OnSrvFn = int (*)(SrvFn fn, void *user, uint32_t *token);
using RemoveSrvFn = int (*)(uint32_t token);

template <class F> F core_fn(const char *name)
{
#if defined(_WIN32)
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    return core ? reinterpret_cast<F>(reinterpret_cast<void *>(GetProcAddress(core, name))) : nullptr;
#else
    (void)name;
    return nullptr;
#endif
}

inline Result result(int r) { return r == 0 ? Result::ok : r == 1 ? Result::not_live : r == -2 ? Result::full : Result::invalid; }

}

// The running core takes view callbacks.
inline bool available() { return detail::core_fn<detail::OnSrvFn>("SwoftyCoreD3d12OnSrv") != nullptr; }

// Calls fn(user, view) after every shader resource view the game makes, from now on. token, when given, names the
// registration for remove_srv_callback.
inline Result on_srv_created(SrvFn fn, void *user = nullptr, uint32_t *token = nullptr)
{
    if (!fn)
        return Result::invalid;
    auto f = detail::core_fn<detail::OnSrvFn>("SwoftyCoreD3d12OnSrv");
    if (!f)
        return Result::unavailable;
    return detail::result(f(fn, user, token));
}

// Stops a callback; a call already running on another thread may still finish after this returns.
inline Result remove_srv_callback(uint32_t token)
{
    auto f = detail::core_fn<detail::RemoveSrvFn>("SwoftyCoreD3d12RemoveSrv");
    if (!f)
        return Result::unavailable;
    return detail::result(f(token));
}

struct ListSubmission {
    uint32_t size, count;
    void *queue;        // ID3D12CommandQueue *
    const void *lists;  // ID3D12CommandList *const *
};

struct ListObserver {
    uint32_t size, reserved;
    void (*reset)(void *user, void *list);
    void (*barriers)(void *user, void *list, uint32_t count, const void *barriers); // const D3D12_RESOURCE_BARRIER *
    void (*texture_copy)(void *user, void *list, const void *dst, uint32_t x, uint32_t y, uint32_t z,
                         const void *src, const void *box); // D3D12_TEXTURE_COPY_LOCATION and D3D12_BOX
    void (*submit)(void *user, const ListSubmission *submission);
    void (*submitted)(void *user, void *queue);
};
static_assert(sizeof(ListSubmission) == 24 && sizeof(ListObserver) == 48);

// Register one process-lifetime observer in an early SwoftyAddonInit. Set size and leave reserved zero.
// Arguments are borrowed for the callback. See the SDK reference for callback order and threading.
inline Result observe_lists(const ListObserver &observer, void *user = nullptr)
{
    using RegisterFn = int (*)(const ListObserver *, void *);
    const auto reg = detail::core_fn<RegisterFn>("SwoftyCoreD3d12ObserveListsV1");
    return reg ? detail::result(reg(&observer, user)) : Result::unavailable;
}

using RecordComputeFn = void (*)(void *user, void *list);

// Inside a barrier or texture-copy callback, records compute on its active direct list and restores tracked bindings.
// Returns false and skips fn when there is no usable binding snapshot.
inline bool record_compute(void *list, RecordComputeFn fn, void *user = nullptr)
{
    using RecordFn = int (*)(void *, RecordComputeFn, void *);
    const auto record = detail::core_fn<RecordFn>("SwoftyCoreD3d12RecordComputeV1");
    return record && record(list, fn, user) == 1;
}

inline const char *name(Result r)
{
    return r == Result::ok            ? "ok"
           : r == Result::not_live    ? "not_live"
           : r == Result::unavailable ? "unavailable"
           : r == Result::full        ? "full"
                                      : "invalid";
}

}
