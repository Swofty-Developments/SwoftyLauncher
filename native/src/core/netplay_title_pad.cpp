#include "runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstring>

#include "netplay_title_pad.hpp"

// The virtual pad's a through XInputGetState and XInputGetCapabilities in xinput1_4.dll (the game imports
// them by ordinals 2 and 4). A real pad passes through whenever the pad isn't pressing.

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

constexpr uint16_t PAD_A = 0x1000;

std::atomic<bool> armed{false}, down{false};
std::atomic<DWORD> packet{1};
safetyhook::InlineHook h_get, h_caps;

DWORD WINAPI get_state(DWORD user, XState *s)
{
    DWORD r = h_get.call<DWORD>(user, s);
    if (user != 0 || !s || !armed.load() || !down.load())
        return r;
    std::memset(s, 0, sizeof *s);
    s->packet = packet.load();
    s->pad.buttons = PAD_A;
    return ERROR_SUCCESS;
}

DWORD WINAPI get_caps(DWORD user, DWORD flags, XCaps *c)
{
    DWORD r = h_caps.call<DWORD>(user, flags, c);
    if (user != 0 || !c || !armed.load() || r == ERROR_SUCCESS)
        return r;
    std::memset(c, 0, sizeof *c);
    c->type = 1;
    c->subtype = 1;
    c->pad.buttons = 0xf3ff;
    c->pad.lt = c->pad.rt = 0xff;
    return ERROR_SUCCESS;
}

}

namespace runtime::title_pad {

bool install()
{
    HMODULE x = LoadLibraryW(L"xinput1_4.dll");
    void *get = x ? reinterpret_cast<void *>(GetProcAddress(x, MAKEINTRESOURCEA(2))) : nullptr;
    void *caps = x ? reinterpret_cast<void *>(GetProcAddress(x, MAKEINTRESOURCEA(4))) : nullptr;
    if (!get || !caps) {
        swofty::log("[netplay] room launch: xinput1_4.dll or its exports were not found; the title waits for a press");
        return false;
    }
    auto g = safetyhook::InlineHook::create(get, reinterpret_cast<void *>(get_state));
    auto c = safetyhook::InlineHook::create(caps, reinterpret_cast<void *>(get_caps));
    if (!g || !c) {
        swofty::log("[netplay] room launch: hooking xinput1_4.dll failed; the title waits for a press");
        return false;
    }
    h_get = std::move(*g);
    h_caps = std::move(*c);
    armed = true;
    return true;
}

void press(bool pressed)
{
    if (pressed != down.load()) {
        down = pressed;
        packet++;
    }
}

void disarm()
{
    down = false;
    armed = false;
}

}
