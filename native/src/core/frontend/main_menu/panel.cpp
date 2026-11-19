#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/runtime/json.hpp"

#include <atomic>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <utility>

namespace frontend::main_menu {

namespace {

struct PadSample {
    bool connected = false;
    uint32_t packet = 0;
    uint16_t buttons = 0;
};

PadSample sample_pad(void *xinput_get)
{
    using Get_t = DWORD(WINAPI *)(DWORD, void *);
    auto get = reinterpret_cast<Get_t>(xinput_get);
    uint8_t buf[16] = {};
    PadSample s;
    s.connected = get && get(0, buf) == 0;
    std::memcpy(&s.packet, buf, 4);
    std::memcpy(&s.buttons, buf + 4, 2);
    return s;
}

void *xinput_get_state()
{
    HMODULE m = LoadLibraryA("xinput1_4.dll");
    if (!m)
        m = LoadLibraryA("xinput1_3.dll");
    return m ? reinterpret_cast<void *>(GetProcAddress(m, "XInputGetState")) : nullptr;
}

bool any_key_down()
{
    for (int vk = 8; vk < 255; vk++)
        if (GetAsyncKeyState(vk) & 0x8000)
            return true;
    return false;
}

void open_video(const State &s)
{
    std::string body = open_request_json(s.video_url);
    bool ok = runtime::write_file(runtime::under_swofty("open_url.json"), body);
    swofty::log("[main-menu] watch pressed: %s open_url.json for %s", ok ? "wrote" : "could not write", s.video_url.c_str());
}

std::string host_path(const std::string &p)
{
    if (!p.empty() && p[0] == '/') {
        std::string w = "Z:" + p;
        for (auto &c : w)
            if (c == '/')
                c = '\\';
        return w;
    }
    if (!p.empty() && p.find(':') == std::string::npos) {
        std::string rel = "swofty:" + p;
        for (auto &c : rel)
            if (c == '/')
                c = '\\';
        return rel;
    }
    return p;
}

}

bool BindSampler::pressed_edge(bool *on_pad)
{
    uint64_t now = GetTickCount64();
    if (!std::exchange(xinput_tried, true))
        xinput_get = xinput_get_state();
    PadSample p = sample_pad(xinput_get);
    if (p.connected && p.packet != last_packet)
        pad_at = now;
    last_packet = p.packet;
    bool key_active = GetForegroundWindow() != nullptr && any_key_down();
    if (key_active)
        key_at = now;
    if (pad_at != key_at)
        shown_pad = p.connected && pad_at > key_at;
    *on_pad = shown_pad;
    bool y = p.connected && (p.buttons & WATCH_PAD_MASK);
    bool key = (GetAsyncKeyState(WATCH_KEY_VK) & 0x8000) != 0 && GetForegroundWindow() != nullptr;
    bool down = y || key;
    bool edge = down && !was;
    was = down;
    return edge;
}

void BindSampler::run()
{
    for (;;) {
        Sleep(100);
        if (GetTickCount64() - wanted_at.load() > 500)
            continue;
        uint64_t t0 = GetTickCount64();
        bool on_pad = false;
        if (pressed_edge(&on_pad))
            pressed = true;
        pad = on_pad;
        uint64_t took = GetTickCount64() - t0;
        if (took > 50 && slow++ % 100 == 0)
            swofty::log("[main-menu] the video card's pad and key sample took %llu ms on its own thread (slow sample %llu)",
                        static_cast<unsigned long long>(took), static_cast<unsigned long long>(slow));
    }
}

bool BindSampler::poll(uint64_t now)
{
    wanted_at = now;
    if (!started.exchange(true))
        std::thread([this] { run(); }).detach();
    return pressed.exchange(false);
}

void Panel::set_offline_text(long long online)
{
    std::string text = offline_text(online);
    if (online == offline_shown)
        return;
    if (ui_vm_set_path("ui_menu_data.swofty_offline_text", 0, text.c_str())) {
        offline_shown = online;
        swofty::log("[main-menu] Launched with panel text set: %s",
                    online_line(online).empty() ? "no online line" : online_line(online).c_str());
    } else {
        if (offline_fails++ % 60 == 0)
            swofty::log("[main-menu] could not set ui_menu_data.swofty_offline_text yet (try %d)", offline_fails);
    }
}

void Panel::read_state(bool force)
{
    uint64_t now = GetTickCount64();
    std::lock_guard<std::mutex> g(state_lock);
    if (!force && state_read_at && now - state_read_at < 1000)
        return;
    state_read_at = now;
    auto next = parse_state(runtime::read_file(runtime::under_swofty("home_menu.json"), size_t(1) << 20));
    if (const char *override_path = std::getenv("SWOFTY_HOME_VIDEO_JSON"); override_path && *override_path) {
        json::Value v;
        if (json::parse(runtime::read_file(runtime::wide(override_path), size_t(1) << 20), v)) {
            next.loaded = true;
            next.video_title = v["title"].str();
            next.video_url = v["url"].str();
            next.video_thumbnail = v["thumbnail"].str();
        }
    }
    if (next.loaded != state.loaded || next.signed_in != state.signed_in || next.video_url != state.video_url)
        swofty::log("[main-menu] state: %s, %s, video %s", next.loaded ? version_line(next).c_str() : "no file",
                    discord_line(next).c_str(), has_video(next) ? "yes" : "none");
    state = next;
    state_loaded = next.loaded;
    set_offline_text(next.online);
}

State Panel::snapshot()
{
    std::lock_guard<std::mutex> g(state_lock);
    return state;
}

bool Panel::account(std::string &name)
{
    if (!state_loaded.load())
        return false;
    std::lock_guard<std::mutex> g(state_lock);
    if (!state.signed_in || state.name.empty())
        return false;
    name = state.name;
    return true;
}

bool Panel::due()
{
    return panel_due(menu.patched.load(), menu.home_open.load() && !runtime::in_session_now(), state_loaded.load(),
                     panel_disabled.load(), panel_prepared.load());
}

void Panel::want_strip(int visible) { strip_wanted = visible; }

void Panel::home_tick()
{
    uint64_t t0 = GetTickCount64();
    read_state();
    uint64_t t1 = GetTickCount64(), t2 = t1, t3 = t1;
    if (menu.patched.load() && !panel_disabled.load() && std::strcmp(env("SWOFTY_MAIN_MENU_PANEL"), "0") != 0) {
        prepare();
        t2 = GetTickCount64();
        poll_bind();
        t3 = GetTickCount64();
    }
    if (t3 - t0 > 100 && (slow_ticks++ < 10 || slow_ticks % 50 == 0))
        swofty::log("[main-menu] slow home tick: state read %llu ms, panel prepare %llu ms, video bind %llu ms",
                    static_cast<unsigned long long>(t1 - t0), static_cast<unsigned long long>(t2 - t1),
                    static_cast<unsigned long long>(t3 - t2));
}

void Panel::sync_strip()
{
    int want = strip_wanted.load();
    if (want >= 0 && want != strip_set.load() && menu.patched.load()) {
        int v = want;
        bool ok = ui_vm_set_path("ui_menu_data.swofty_strip_visible", 1, &v);
        if (ok) {
            strip_set = want;
            swofty::log("[main-menu] stock top-right strip %s (ui_menu_data.swofty_strip_visible = %d)",
                        want ? "shown" : "hidden", want);
        } else if (strip_tries++ % 120 == 0) {
            swofty::log("[main-menu] could not set ui_menu_data.swofty_strip_visible yet (try %d)", strip_tries);
        }
    }
}

void Panel::prepare()
{
    uint64_t now = GetTickCount64();
    State s;
    {
        std::lock_guard<std::mutex> g(state_lock);
        s = state;
    }
    if (!s.loaded)
        return;
    std::string key = s.version + "|" + s.avatar + "|" + s.video_thumbnail + "|" + (s.signed_in ? "1" : "0") + "|" + s.name;
    if (key == prepared_key && now - prepared_at < 60000)
        return;
    prepared_key = key;
    prepared_at = now;
    auto p = std::make_shared<Prepared>();
    p->s = s;
    uint32_t w = 0, h = 0;
    if (has_avatar(s))
        p->avatar_slot = runtime::overlay_image("core.main_menu\navatar", host_path(s.avatar), &w, &h);
    if (has_video(s)) {
        p->video_slot = runtime::overlay_image("core.main_menu\nvideo", host_path(s.video_thumbnail), &w, &h);
        p->video_w = w;
        p->video_h = h;
    }
    p->version = version_line(s);
    p->discord = discord_line(s);
    {
        std::lock_guard<std::mutex> g(prepared_lock);
        prepared = p;
    }
    panel_prepared = true;
    swofty::log("[main-menu] panel prepared on the flow thread: avatar slot %d, video slot %d", p->avatar_slot, p->video_slot);
}

void Panel::poll_bind()
{
    uint64_t now = GetTickCount64();
    if (now - bind_polled_at < 100)
        return;
    bind_polled_at = now;
    std::shared_ptr<const Prepared> p;
    {
        std::lock_guard<std::mutex> g(prepared_lock);
        p = prepared;
    }
    if (!p || p->s.video_url.empty())
        return;
    if (bind.poll(now))
        open_video(p->s);
}

}
