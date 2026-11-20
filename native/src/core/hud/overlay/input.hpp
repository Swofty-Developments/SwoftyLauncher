#pragma once
#include <windows.h>

#include "core/hud/hud_drawlist.hpp"
#include "core/hud/hud_input.hpp"

#include <xinput.h>

#include <atomic>
#include <mutex>
#include <string>
#include <vector>

namespace hud::overlay {

constexpr ULONGLONG FOCUS_TIMEOUT_MS = 2000;

// The core subclasses the window the drawn swapchain presents to. Mouse, wheel, character and key
// messages are queued for the overlay; while an addon holds input focus the button-down, wheel,
// character, key-down and raw input messages do not reach the game (ups always do, so no key sticks).
class Input {
public:
    void hook_window(HWND wnd);
    void frame(const std::vector<hud::drawlist::Paintsub> &subs, float fb_w, float fb_h, ULONGLONG now);
    void draw_cursor(hud::drawlist::Frame &f);
    void view(const std::string &owner, SwoftyInput &full);
    bool take_focus(const std::string &who, bool on, std::string &before);
    bool window_hooked() const { return game_wnd.load() != nullptr; }

private:
    static LRESULT CALLBACK proc(HWND w, UINT msg, WPARAM wp, LPARAM lp);
    void poll_pads(ULONGLONG now);

    std::mutex lock;
    std::vector<hud::input::Event> events;
    hud::input::Snapshot snap, frame_snap;
    hud::input::Focus focus, frame_focus;
    std::atomic<bool> focus_held{false};
    std::atomic<WNDPROC> game_proc{nullptr};
    std::atomic<HWND> game_wnd{nullptr};
    std::atomic<uint32_t> client_size{0};
    std::atomic<unsigned> blocked_msgs{0}, blocked_raw{0};
    ULONGLONG blocked_reported = 0;
    hud::input::Utf16 utf16;
    std::atomic<ULONGLONG> read_ms{0};

    using XInputGetStateFn = DWORD(WINAPI *)(DWORD, XINPUT_STATE *);
    XInputGetStateFn xinput_get = nullptr;
    bool xinput_tried = false;
    hud::input::PadRaw pads[4];
    ULONGLONG pad_probe_at[4] = {};
};

}
