#include "core/hud/overlay/input.hpp"

#include "core/hud/overlay/joy.hpp"
#include "core/hud/overlay/overlay.hpp"
#include "core/hud/overlay/settings.hpp"
#include "core/hud/text_entry.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <windowsx.h>

#include <algorithm>

namespace runtime {
void crash_close_asked(const char *msg);
}

namespace hud::overlay {

void Input::poll_pads(ULONGLONG now)
{
    if (!xinput_tried) {
        xinput_tried = true;
        HMODULE m = GetModuleHandleW(L"xinput1_4.dll");
        if (!m)
            m = LoadLibraryW(L"xinput1_4.dll");
        xinput_get = m ? reinterpret_cast<XInputGetStateFn>(GetProcAddress(m, "XInputGetState")) : nullptr;
        swofty::log(xinput_get ? "[overlay] input: gamepads read through xinput1_4.dll!XInputGetState"
                               : "[overlay] input: xinput1_4.dll!XInputGetState is unavailable; no gamepad input");
    }
    if (!xinput_get)
        return;
    for (DWORD i = 0; i < 4; i++) {
        if (!pads[i].connected && now < pad_probe_at[i])
            continue;
        XINPUT_STATE st{};
        bool was = pads[i].connected;
        bool on = xinput_get(i, &st) == ERROR_SUCCESS;
        if (!on) {
            pads[i] = {};
            pad_probe_at[i] = now + 2000;
        } else {
            const XINPUT_GAMEPAD &g = st.Gamepad;
            pads[i] = {true, g.wButtons, g.bLeftTrigger, g.bRightTrigger, g.sThumbLX, g.sThumbLY, g.sThumbRX, g.sThumbRY};
        }
        if (on != was)
            swofty::log("[overlay] input: gamepad %lu %s", static_cast<unsigned long>(i), on ? "connected" : "disconnected");
    }
}

LRESULT CALLBACK Input::proc(HWND w, UINT msg, WPARAM wp, LPARAM lp)
{
    Input &in = overlay.input;
    using hud::input::Ev;
    bool held = in.focus_held.load(std::memory_order_relaxed);
    hud::input::Event e{};
    bool record = true, block = false;
    float x = float(GET_X_LPARAM(lp)), y = float(GET_Y_LPARAM(lp));
    switch (msg) {
    case WM_MOUSEMOVE:
        e = {Ev::move, 0, x, y};
        block = held;
        break;
    case WM_LBUTTONDOWN:
    case WM_LBUTTONDBLCLK:
        e = {Ev::down, SWOFTY_MOUSE_LEFT, x, y};
        block = held;
        break;
    case WM_RBUTTONDOWN:
    case WM_RBUTTONDBLCLK:
        e = {Ev::down, SWOFTY_MOUSE_RIGHT, x, y};
        block = held;
        break;
    case WM_MBUTTONDOWN:
    case WM_MBUTTONDBLCLK:
        e = {Ev::down, SWOFTY_MOUSE_MIDDLE, x, y};
        block = held;
        break;
    case WM_LBUTTONUP:
        e = {Ev::up, SWOFTY_MOUSE_LEFT, x, y};
        break;
    case WM_RBUTTONUP:
        e = {Ev::up, SWOFTY_MOUSE_RIGHT, x, y};
        break;
    case WM_MBUTTONUP:
        e = {Ev::up, SWOFTY_MOUSE_MIDDLE, x, y};
        break;
    case WM_MOUSEWHEEL:
        e = {Ev::wheel, 0, float(GET_WHEEL_DELTA_WPARAM(wp)) / WHEEL_DELTA, 0};
        block = held;
        break;
    case WM_CHAR:
        hud::text_entry::state.chr(GetTickCount64());
        e = {Ev::chr, in.utf16.push(uint32_t(wp)), 0, 0};
        record = e.a != 0;
        block = held;
        break;
    case WM_KEYDOWN:
        hud::text_entry::state.key(uint32_t(wp) & 0xff, GetTickCount64());
        e = {Ev::key_down, uint32_t(wp) & 0xff, 0, 0};
        block = held;
        break;
    case WM_KEYUP:
    case WM_SYSKEYUP:
        e = {Ev::key_up, uint32_t(wp) & 0xff, 0, 0};
        break;
    case WM_SYSKEYDOWN:
        e = {Ev::key_down, uint32_t(wp) & 0xff, 0, 0};
        break;
    case WM_SIZE:
        in.client_size.store(uint32_t(lp), std::memory_order_relaxed);
        record = false;
        break;
    case WM_CLOSE:
        runtime::crash_close_asked("WM_CLOSE");
        record = false;
        break;
    case WM_ENDSESSION:
        if (wp)
            runtime::crash_close_asked("WM_ENDSESSION");
        record = false;
        break;
    case WM_INPUT:
        record = false;
        if (held) {
            in.blocked_raw.fetch_add(1, std::memory_order_relaxed);
            return DefWindowProcW(w, msg, wp, lp);
        }
        break;
    default:
        record = false;
    }
    if (record) {
        std::lock_guard<std::mutex> g(in.lock);
        if (in.events.size() < 1024)
            in.events.push_back(e);
    }
    if (block) {
        in.blocked_msgs.fetch_add(1, std::memory_order_relaxed);
        return 0;
    }
    return CallWindowProcW(in.game_proc.load(), w, msg, wp, lp);
}

void Input::hook_window(HWND wnd)
{
    if (!wnd || game_wnd.load() || settings().killed("input"))
        return;
    WNDPROC prev = reinterpret_cast<WNDPROC>(GetWindowLongPtrW(wnd, GWLP_WNDPROC));
    if (!prev)
        return;
    game_proc.store(prev);
    RECT rc{};
    if (GetClientRect(wnd, &rc))
        client_size.store(uint32_t(rc.right & 0xffff) | uint32_t(rc.bottom & 0xffff) << 16);
    game_wnd.store(wnd);
    SetWindowLongPtrW(wnd, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(&proc));
    swofty::log("[overlay] input: window %p subclassed (procedure %p); addons can read the mouse and keyboard and "
                "take input focus",
                static_cast<void *>(wnd), reinterpret_cast<void *>(prev));
}

void Input::frame(const std::vector<hud::drawlist::Paintsub> &subs, float fb_w, float fb_h, ULONGLONG now)
{
    uint32_t cs = client_size.load(std::memory_order_relaxed);
    float sx = 1, sy = 1, cw = float(cs & 0xffff), ch = float(cs >> 16);
    if (cw > 0 && ch > 0) {
        sx = fb_w / cw;
        sy = fb_h / ch;
    }
    std::string expired;
    bool pads_wanted = now - read_ms.load(std::memory_order_relaxed) < 2000 && !settings().killed("pad");
    if (pads_wanted)
        poll_pads(now);
    {
        std::lock_guard<std::mutex> g(lock);
        hud::input::fold(snap, events, sx, sy);
        if (pads_wanted)
            hud::input::fold_pads(snap, pads, 4, now);
        else
            snap.pad_pressed = 0;
        hud::input::fold_joy(snap, joy::devices(), now);
        events.clear();
        for (const auto &s : subs)
            focus.painted(s.owner, now);
        std::string owner = focus.owner;
        if (focus.expire(now, FOCUS_TIMEOUT_MS))
            expired = owner;
        focus_held.store(focus.held());
        frame_snap = snap;
        frame_focus = focus;
    }
    if (!expired.empty())
        swofty::log("[overlay] input: %s held focus but has not painted for %llu ms; focus released", expired.c_str(),
                    static_cast<unsigned long long>(FOCUS_TIMEOUT_MS));
    if (now - blocked_reported >= 60000) {
        unsigned m = blocked_msgs.exchange(0), r = blocked_raw.exchange(0);
        if (m || r)
            swofty::log("[overlay] input: last %llu s: %u window message(s) and %u raw input message(s) kept from the "
                        "game while an addon held focus",
                        static_cast<unsigned long long>(blocked_reported ? (now - blocked_reported) / 1000 : 60), m, r);
        blocked_reported = now;
    }
}

void Input::draw_cursor(hud::drawlist::Frame &f)
{
    if (!frame_focus.held() || frame_snap.x < 0)
        return;
    hud::drawlist::reset_state(f);
    float s = std::max(0.75f, hud::drawlist::ui_scale(f)), x = frame_snap.x, y = frame_snap.y;
    const float m[6] = {s, 0, 0, s, x, y};
    hud::drawlist::push_transform(f, m);
    const float tip[] = {0, 0, 0, 17, 12.5f, 12.5f};
    const float tail[] = {3.2f, 13.5f, 6.4f, 12.2f, 10.2f, 20.5f, 7.0f, 21.8f};
    hud::drawlist::polygon(f, tail, 4, 0xffffffffu);
    hud::drawlist::polygon(f, tip, 3, 0xffffffffu);
    hud::drawlist::polyline(f, tip, 3, 1.2f, 0x000000ffu, SWOFTY_POLYLINE_CLOSED);
    hud::drawlist::pop_transform(f);
}

void Input::view(const std::string &owner, SwoftyInput &full)
{
    read_ms.store(GetTickCount64(), std::memory_order_relaxed);
    joy::want();
    hud::input::view(frame_snap, frame_focus, owner, full, runtime::typing(nullptr));
}

bool Input::take_focus(const std::string &who, bool on, std::string &before)
{
    std::lock_guard<std::mutex> g(lock);
    before = focus.owner;
    bool changed = focus.take(who, on, GetTickCount64());
    focus_held.store(focus.held());
    return changed;
}

}
