#include "core/hud/overlay/joy.hpp"

#include "core/hud/overlay/settings.hpp"
#include "core/runtime/hook_errors.hpp"

#include <swofty/mixin.hpp>

#include <safetyhook.hpp>

#include <atomic>
#include <mutex>
#include <string>
#include <thread>

namespace hud::overlay::joy {
namespace {

constexpr GUID IID_DI8W = {0xBF798031, 0x483A, 0x4DA2, {0xAA, 0x99, 0x5D, 0x64, 0xED, 0x36, 0x97, 0x00}};
constexpr GUID IID_DI8A = {0xBF798030, 0x483A, 0x4DA2, {0xAA, 0x99, 0x5D, 0x64, 0xED, 0x36, 0x97, 0x00}};
constexpr GUID SYS_KEYBOARD = {0x6F1D2B61, 0xD5A0, 0x11CF, {0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00}};
constexpr DWORD DI_VERSION = 0x0800;
constexpr int VT_RELEASE = 2, VT_CREATE_DEVICE = 3, VT_GET_DEVICE_STATE = 9;
constexpr int HOOKS = 2;
constexpr uint32_t SEEN_LOGS = 16, PRESS_LOGS = 12;

using CreateFn = HRESULT(WINAPI *)(HINSTANCE, DWORD, const GUID *, void **, void *);
using CreateDeviceFn = HRESULT(WINAPI *)(void *, const GUID *, void **, void *);
using ReleaseFn = ULONG(WINAPI *)(void *);
using GetStateFn = HRESULT(WINAPI *)(void *, DWORD, void *);

// Never destroyed: the game's input thread may still read through the hooks while the process ends.
struct State {
    hud::input::JoyDevices devices;
    safetyhook::InlineHook hooks[HOOKS];
    std::atomic<GetStateFn> original[HOOKS] = {};
    std::atomic<uint32_t> seen_logs{0}, press_logs{0};
};

State &state()
{
    static State *s = new State;
    return *s;
}

std::once_flag started;
thread_local int depth = 0;

void *vtable_slot(void *com, int i) { return (*static_cast<void ***>(com))[i]; }

void release(void *com) { reinterpret_cast<ReleaseFn>(vtable_slot(com, VT_RELEASE))(com); }

void record(void *dev, DWORD size, void *data)
{
    uint32_t mask = 0;
    if (!hud::input::joy_mask(data, size, mask))
        return;
    State &st = state();
    hud::input::JoyRead r = st.devices.read(reinterpret_cast<uintptr_t>(dev), mask, GetTickCount64());
    if (r.fresh && st.seen_logs.fetch_add(1, std::memory_order_relaxed) < SEEN_LOGS)
        swofty::log("[overlay] input: game controller %p read by the game through DirectInput (%s), slot %d", dev,
                    size == hud::input::DIJOYSTATE2_SIZE ? "DIJOYSTATE2" : "DIJOYSTATE", r.slot);
    if (r.went_down && st.press_logs.fetch_add(1, std::memory_order_relaxed) < PRESS_LOGS)
        swofty::log("[overlay] input: game controller slot %d buttons 0x%08x went down", r.slot, r.went_down);
}

template <int I> HRESULT WINAPI get_state(void *self, DWORD size, void *data)
{
    int outer = depth++;
    HRESULT hr = state().original[I].load(std::memory_order_acquire)(self, size, data);
    depth--;
    if (outer == 0 && SUCCEEDED(hr))
        record(self, size, data);
    return hr;
}

constexpr GetStateFn detours[HOOKS] = {&get_state<0>, &get_state<1>};

// GetDeviceState of a keyboard device made from our own IDirectInput8: dinput8 gives every device type one
// IDirectInputDevice8 vtable (Windows and Wine alike), so this is the function the game's wheel reads call.
void *device_read_fn(CreateFn create, const GUID &iid, const char *which)
{
    void *di = nullptr;
    HRESULT hr = create(GetModuleHandleW(nullptr), DI_VERSION, &iid, &di, nullptr);
    if (FAILED(hr) || !di) {
        swofty::log("[overlay] input: DirectInput8Create(%s) failed (0x%08lx)", which, static_cast<unsigned long>(hr));
        return nullptr;
    }
    void *dev = nullptr;
    hr = reinterpret_cast<CreateDeviceFn>(vtable_slot(di, VT_CREATE_DEVICE))(di, &SYS_KEYBOARD, &dev, nullptr);
    void *fn = nullptr;
    if (SUCCEEDED(hr) && dev) {
        fn = vtable_slot(dev, VT_GET_DEVICE_STATE);
        release(dev);
    } else {
        swofty::log("[overlay] input: IDirectInput8%s::CreateDevice(keyboard) failed (0x%08lx)", which,
                    static_cast<unsigned long>(hr));
    }
    release(di);
    return fn;
}

std::string module_of(void *fn, uintptr_t &offset)
{
    HMODULE mod = nullptr;
    char path[MAX_PATH] = {};
    offset = 0;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            static_cast<LPCWSTR>(fn), &mod) ||
        !GetModuleFileNameA(mod, path, sizeof path))
        return "?";
    offset = reinterpret_cast<uintptr_t>(fn) - reinterpret_cast<uintptr_t>(mod);
    const char *base = path;
    for (const char *c = path; *c; c++)
        if (*c == '\\' || *c == '/')
            base = c + 1;
    return base;
}

void setup()
{
    HMODULE m = GetModuleHandleW(L"dinput8.dll");
    auto create = m ? reinterpret_cast<CreateFn>(reinterpret_cast<void *>(GetProcAddress(m, "DirectInput8Create"))) : nullptr;
    if (!create) {
        swofty::log("[overlay] input: dinput8.dll is not loaded; no wheel or joystick buttons");
        return;
    }
    State &st = state();
    void *fns[HOOKS] = {device_read_fn(create, IID_DI8W, "W"), device_read_fn(create, IID_DI8A, "A")};
    const char *names[HOOKS] = {"W", "A"};
    for (int i = 0; i < HOOKS; i++) {
        if (!fns[i] || (i > 0 && fns[i] == fns[0]))
            continue;
        auto h =
            safetyhook::InlineHook::create(fns[i], reinterpret_cast<void *>(detours[i]), safetyhook::InlineHook::StartDisabled);
        if (!h) {
            swofty::log("[overlay] input: hooking IDirectInputDevice8%s::GetDeviceState failed: %s", names[i],
                        runtime::hook_error(h.error()).c_str());
            continue;
        }
        st.original[i].store(h->original<GetStateFn>(), std::memory_order_release);
        if (auto on = h->enable(); !on) {
            swofty::log("[overlay] input: enabling the IDirectInputDevice8%s::GetDeviceState hook failed: %s", names[i],
                        runtime::hook_error(on.error()).c_str());
            continue;
        }
        st.hooks[i] = std::move(*h);
        uintptr_t offset = 0;
        std::string mod = module_of(fns[i], offset);
        swofty::log("[overlay] input: IDirectInputDevice8%s::GetDeviceState hooked at %s+0x%llx; addons read wheel and "
                    "joystick buttons from the game's own reads",
                    names[i], mod.c_str(), static_cast<unsigned long long>(offset));
    }
}

}

void want()
{
    std::call_once(started, [] {
        if (settings().killed("joy")) {
            swofty::log("[overlay] input: wheel and joystick buttons are off (overlay kill list names joy)");
            return;
        }
        std::thread(setup).detach();
    });
}

hud::input::JoyDevices &devices() { return state().devices; }

}
