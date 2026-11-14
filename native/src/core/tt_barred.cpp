#include "runtime.hpp"
#include "tt_barred.hpp"
#include "tt_barred_logic.hpp"
#include "apply/moments.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <atomic>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

// The red line naming the barred addons while the game is in Time Trial (tt_barred_logic.hpp). The flow tick reads the
// game mode the apply mode watch keeps and re-reads swofty\tt.json every READ_MS while in Time Trial, since the wrapper
// may write the list after the game started; the overlay frame paints the line. SWOFTY_TT_BARRED=0 turns it off.
namespace runtime::tt_barred {
namespace {

constexpr ULONGLONG READ_MS = 2000;

std::atomic<int> off{-1};
std::atomic<bool> visible{false};
std::mutex line_lock;
std::string line;
std::mutex view_lock;
main_menu::HomeView view;
Gate gate;
ULONGLONG last_read = 0;
bool was_tt = false;

bool is_off()
{
    if (off.load() < 0) {
        char v[8] = {};
        off = GetEnvironmentVariableA("SWOFTY_TT_BARRED", v, sizeof v) && v[0] == '0';
    }
    return off.load() > 0;
}

std::string read_line()
{
    return line_of(names_of(read_file(swofty_root() + L"tt.json", size_t(1) << 20)));
}

void keep(std::string next)
{
    std::lock_guard<std::mutex> g(line_lock);
    if (next == line)
        return;
    swofty::log("[tt-barred] %s", next.empty() ? "no barred addon is enabled" : next.c_str());
    line = std::move(next);
}

}

bool wants_overlay()
{
    if (is_off())
        return false;
    keep(read_line());
    std::lock_guard<std::mutex> g(line_lock);
    return !line.empty();
}

bool drawing()
{
    return visible.load(std::memory_order_relaxed);
}

void screen(const char *name)
{
    if (!name || is_off())
        return;
    std::lock_guard<std::mutex> g(view_lock);
    view.screen(name);
}

void tick()
{
    if (is_off())
        return;
    static bool watching = false;
    if (!std::exchange(watching, true))
        swofty_core::apply::moments::mode_watch();
    ULONGLONG now = GetTickCount64();
    bool tt = swofty_core::apply::moments::mode_kind() == swofty_core::apply::facts::ModeKind::time_trial;
    if (tt && (!was_tt || now - last_read >= READ_MS)) {
        last_read = now;
        keep(read_line());
    }
    was_tt = tt;
    bool home;
    {
        std::lock_guard<std::mutex> g(view_lock);
        home = view.home;
    }
    bool any;
    {
        std::lock_guard<std::mutex> g(line_lock);
        any = !line.empty();
    }
    bool shown = gate.step(tt, home, now) && any;
    if (visible.exchange(shown) != shown)
        swofty::log("[tt-barred] warning %s", shown ? "shown: the game is in Time Trial" : "hidden");
}

void paint(hud::Frame &f, uint64_t)
{
    if (!visible.load(std::memory_order_relaxed))
        return;
    std::string text;
    {
        std::lock_guard<std::mutex> g(line_lock);
        text = line;
    }
    if (text.empty() || f.width <= 0 || f.height <= 0)
        return;
    hud::reset_state(f);
    const float W = f.width, H = f.height, k = H / 1080.0f;
    hud::TextStyle s;
    s.px = 26 * k;
    s.rgba = 0xff3b30ffu;
    s.outline_rgba = 0x000000e0u;
    s.outline_px = 2 * k;
    float w = 0, h = 0;
    hud::measure(s, text, &w, &h);
    const float room = W * 0.92f;
    if (w > room && w > 0) {
        s.px *= room / w;
        s.outline_px *= room / w;
        hud::measure(s, text, &w, &h);
    }
    const float pad_x = 18 * k, pad_y = 10 * k, top = 16 * k;
    const float x = (W - w) * 0.5f;
    hud::rect_rounded(f, x - pad_x, top, w + 2 * pad_x, h + 2 * pad_y, 8 * k, 0x0b0b0fd0u);
    hud::rect_rounded_outline(f, x - pad_x, top, w + 2 * pad_x, h + 2 * pad_y, 8 * k, 2 * k, 0xff3b30c0u);
    hud::text(f, x, top + pad_y, s, text);
}

}
