#include "runtime.hpp"

#include "netplay_room.hpp"
#include "room_cover.hpp"
#include "room_cover_logic.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <mutex>

namespace runtime::room_cover {

namespace {

std::mutex lock;
Cover cover;
std::atomic<bool> armed{false};
uint64_t polled_at = 0;
Signal last = Signal::waiting;
float title_w = 0, title_h = 0, sub_w = 0, sub_h = 0, measured_for = 0;

constexpr uint32_t BACKGROUND = 0x0b0d12ffu;
constexpr uint32_t TITLE_RGBA = 0xf2f4f8ffu;
constexpr uint32_t SUB_RGBA = 0xaab0bcffu;

hud::TextStyle title_style(float h)
{
    hud::TextStyle s;
    s.px = h * 0.1f;
    s.rgba = TITLE_RGBA;
    s.tracking = s.px * 0.12f;
    return s;
}

hud::TextStyle sub_style(float h)
{
    hud::TextStyle s;
    s.px = h * 0.05f;
    s.rgba = SUB_RGBA;
    s.tracking = s.px * 0.2f;
    return s;
}

}

bool setup()
{
    char env[1024] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_DIRECT_START", env, sizeof env);
    std::string value = n && n < sizeof env ? std::string(env) : std::string(), text;
    if (value.empty())
        text = read_file(swofty_root() + L"direct_start.json", size_t(1) << 16);
    else if (value[0] == '@')
        text = read_file(wide(value.c_str() + 1), size_t(1) << 16);
    if (!request_is_room(value, text))
        return false;
    std::lock_guard<std::mutex> g(lock);
    cover.arm();
    armed.store(true);
    return true;
}

bool active()
{
    return armed.load();
}

void paint(hud::Frame &f, uint64_t now)
{
    if (!armed.load())
        return;
    std::lock_guard<std::mutex> g(lock);
    if (now - polled_at >= 100) {
        polled_at = now;
        auto sig = netplay_room::cover_signal();
        last = sig == netplay_room::CoverSignal::failed  ? Signal::failed
               : sig == netplay_room::CoverSignal::shown ? Signal::shown
                                                         : Signal::waiting;
    }
    Event ev = Event::none;
    float alpha = cover.frame(now, last, &ev);
    if (ev == Event::went_up)
        swofty::log("[cover] room launch cover up (%.0fx%.0f)", double(f.width), double(f.height));
    else if (ev == Event::came_off)
        swofty::log("[cover] room launch cover off: %s", why_name(cover.why()));
    if (!cover.drawing()) {
        armed.store(false);
        return;
    }
    if (alpha <= 0)
        return;
    hud::reset_state(f);
    f.opacity = alpha;
    hud::rect(f, 0, 0, f.width, f.height, BACKGROUND);
    if (measured_for != f.height) {
        measured_for = f.height;
        hud::measure(title_style(f.height), "SWOFTY LAUNCHER", &title_w, &title_h);
        hud::measure(sub_style(f.height), "Multiplayer", &sub_w, &sub_h);
    }
    float gap = f.height * 0.02f, top = (f.height - (title_h + gap + sub_h)) * 0.5f;
    hud::text(f, (f.width - title_w) * 0.5f, top, title_style(f.height), "SWOFTY LAUNCHER");
    hud::text(f, (f.width - sub_w) * 0.5f, top + title_h + gap, sub_style(f.height), "Multiplayer");
}

}
