#include "core/testing/virtual_pad.hpp"

#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>

#include <atomic>
#include <cstring>
#include <mutex>

namespace {

struct XPad {
    uint16_t buttons;
    uint8_t lt, rt;
    int16_t lx, ly, rx, ry;
};
struct XState {
    DWORD packet;
    XPad pad;
};
struct XCaps {
    uint8_t type, subtype;
    uint16_t flags;
    XPad pad;
    uint16_t vib_l, vib_r;
};

std::once_flag once;
std::atomic<int> claims{0};
std::atomic<int> owner_claims[3] = {};
std::atomic<uint32_t> owners_seen{0};
std::atomic<uint64_t> reads_set{0}, reads_idle{0}, reads_real{0};
std::atomic<void *> caps_export{nullptr};
std::atomic<bool> hooked{false};
std::atomic<uint32_t> pad_word{0};
std::atomic<bool> pad_held{false};
std::atomic<DWORD> pad_packet{1};
safetyhook::InlineHook h_xget, h_xcaps;
std::string pad_why = "not set up";

DWORD WINAPI x_get_state(DWORD user, XState *s)
{
    DWORD r = h_xget.call<DWORD>(user, s);
    if (user != 0 || !s || claims.load() <= 0)
        return r;
    bool held = pad_held.load();
    (held ? reads_set : r != ERROR_SUCCESS ? reads_idle : reads_real).fetch_add(1, std::memory_order_relaxed);
    if (held || r != ERROR_SUCCESS) {
        uint32_t w = pad_word.load();
        std::memset(s, 0, sizeof *s);
        s->packet = pad_packet.load();
        s->pad.buttons = uint16_t(w);
        s->pad.lt = uint8_t(w >> 16);
        s->pad.rt = uint8_t(w >> 24);
    }
    return ERROR_SUCCESS;
}

DWORD WINAPI x_get_caps(DWORD user, DWORD flags, XCaps *c)
{
    DWORD r = h_xcaps.call<DWORD>(user, flags, c);
    if (user != 0 || !c || claims.load() <= 0 || r == ERROR_SUCCESS)
        return r;
    std::memset(c, 0, sizeof *c);
    c->type = 1;
    c->subtype = 1;
    c->pad.buttons = 0xf3ff;
    c->pad.lt = c->pad.rt = 0xff;
    return ERROR_SUCCESS;
}

void pad_setup()
{
    HMODULE x = LoadLibraryW(L"xinput1_4.dll");
    void *get = x ? reinterpret_cast<void *>(GetProcAddress(x, MAKEINTRESOURCEA(2))) : nullptr;
    void *caps = x ? reinterpret_cast<void *>(GetProcAddress(x, MAKEINTRESOURCEA(4))) : nullptr;
    caps_export.store(caps);
    if (!get || !caps) {
        pad_why = "xinput1_4.dll or its XInputGetState/XInputGetCapabilities exports were not found";
        return;
    }
    auto g = safetyhook::InlineHook::create(get, reinterpret_cast<void *>(x_get_state));
    auto c = safetyhook::InlineHook::create(caps, reinterpret_cast<void *>(x_get_caps));
    if (!g || !c) {
        pad_why = "hooking xinput1_4.dll's exports failed";
        return;
    }
    h_xget = std::move(*g);
    h_xcaps = std::move(*c);
    hooked.store(true, std::memory_order_release);
    pad_why.clear();
}

}

namespace runtime::pad {

void setup() { std::call_once(once, pad_setup); }

const std::string &why() { return pad_why; }

void claim(Owner who, bool on)
{
    auto i = size_t(who);
    if (on) {
        owners_seen.fetch_or(1u << i);
        owner_claims[i].fetch_add(1);
    } else if (owner_claims[i].fetch_sub(1) <= 0) {
        owner_claims[i].fetch_add(1);
        return;
    }
    claims.fetch_add(on ? 1 : -1);
}

void set(uint16_t buttons, uint8_t lt, uint8_t rt)
{
    pad_word = uint32_t(buttons) | uint32_t(lt) << 16 | uint32_t(rt) << 24;
    pad_held = buttons || lt || rt;
    pad_packet.fetch_add(1);
}

void release() { set(0); }

Stats stats()
{
    Stats s;
    for (size_t i = 0; i < 3; i++)
        if (owner_claims[i].load() > 0)
            s.owners |= 1u << i;
    s.seen = owners_seen.load();
    s.set = reads_set.load(std::memory_order_relaxed);
    s.idle = reads_idle.load(std::memory_order_relaxed);
    s.real = reads_real.load(std::memory_order_relaxed);
    return s;
}

uint32_t real_caps(uint32_t user, uint32_t flags, void *caps)
{
    using Fn = DWORD(WINAPI *)(DWORD, DWORD, void *);
    if (hooked.load(std::memory_order_acquire))
        return h_xcaps.call<DWORD>(user, flags, caps);
    void *fn = caps_export.load();
    if (!fn) {
        HMODULE x = GetModuleHandleW(L"xinput1_4.dll");
        fn = x ? reinterpret_cast<void *>(GetProcAddress(x, MAKEINTRESOURCEA(4))) : nullptr;
    }
    return fn ? reinterpret_cast<Fn>(fn)(user, flags, caps) : DWORD(ERROR_DEVICE_NOT_CONNECTED);
}

}
