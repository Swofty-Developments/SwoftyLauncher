#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/runtime/json.hpp"

#include <algorithm>
#include <atomic>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <utility>
#include <vector>

namespace runtime {
int overlay_image(const std::string &id, const std::string &path, uint32_t *width, uint32_t *height);
}

namespace runtime::main_menu {

namespace {

namespace logic = ::runtime::main_menu;

constexpr uint64_t PANEL_DELAY_MS = 450, PANEL_FADE_MS = 250;

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

void open_video(const logic::State &s)
{
    std::string body = logic::open_request_json(s.video_url);
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

void fade_panel(hud::Frame &f, float x, float y, float w, float h, float fade_left, float fade_right, float k)
{
    const int slices = 48;
    auto smooth = [](float t) {
        t = t < 0 ? 0 : t > 1 ? 1 : t;
        return t * t * (3 - 2 * t);
    };
    for (int i = 0; i < slices; i++) {
        float t0 = float(i) / slices, t1 = float(i + 1) / slices, tm = (t0 + t1) * 0.5f;
        float a = (fade_left > 0 && tm < fade_left ? smooth(tm / fade_left) : 1.0f) *
                  (fade_right > 0 && tm > 1 - fade_right ? smooth((1 - tm) / fade_right) : 1.0f);
        uint32_t fill = 0x0b0d1200u | uint32_t(a * 0xe4);
        uint32_t line = 0xffffff00u | uint32_t(a * 0x50);
        float sx = x + t0 * w, sw = (t1 - t0) * w + 0.6f;
        hud::rect(f, sx, y, sw, h, fill);
        hud::rect(f, sx, y, sw, 1.3f * k, line);
        hud::rect(f, sx, y + h - 1.3f * k, sw, 1.3f * k, line);
    }
}

void key_glyph(hud::Frame &f, float x, float y, float h, float k, const std::string &key, bool pad_style)
{
    hud::TextStyle st;
    st.px = h * 0.62f;
    st.rgba = 0xf3f5f9ffu;
    float w = 0, hh = 0;
    hud::measure(st, key, &w, &hh);
    float bw = pad_style ? std::max(h * 1.5f, w + 18 * k) : std::max(h, w + 14 * k);
    float radius = pad_style ? h * 0.5f : 6 * k;
    hud::rect_rounded(f, x, y, bw, h, radius, 0xffffff1cu);
    hud::rect_rounded_outline(f, x, y, bw, h, radius, 1.4f * k, 0xf3f5f9d8u);
    hud::text(f, x + (bw - w) * 0.5f, y + (h - st.px) * 0.5f, st, key);
}

void image_rounded(hud::Frame &f, int slot, float x, float y, float w, float h, float r, float u0, float v0, float u1, float v1)
{
    if (slot < 0 || w <= 0 || h <= 0 || hud::clipped_out(f))
        return;
    std::vector<hud::Pt> pts;
    hud::rounded_local(pts, x, y, w, h, r, f.xform.scale());
    if (pts.size() < 3 || !hud::reserve(f, pts.size() * 3, slot + 1))
        return;
    uint32_t col = hud::pack(0xffffffff, f.opacity);
    auto uv = [&](const hud::Pt &p, float *u, float *v) {
        *u = u0 + (p.x - x) / w * (u1 - u0);
        *v = v0 + (p.y - y) / h * (v1 - v0);
    };
    hud::Pt c{x + w * 0.5f, y + h * 0.5f};
    for (size_t i = 0; i < pts.size(); i++) {
        const hud::Pt &a = pts[i], &b = pts[(i + 1) % pts.size()];
        float cu, cv, au, av, bu, bv;
        uv(c, &cu, &cv);
        uv(a, &au, &av);
        uv(b, &bu, &bv);
        hud::vtx(f, c.x, c.y, col, cu, cv, hud::IMAGE);
        hud::vtx(f, a.x, a.y, col, au, av, hud::IMAGE);
        hud::vtx(f, b.x, b.y, col, bu, bv, hud::IMAGE);
    }
}

std::vector<std::string> wrap_title(const hud::TextStyle &style, const std::string &title, float max_w, size_t max_lines)
{
    std::vector<std::string> lines;
    std::string line;
    size_t i = 0;
    auto width = [&](const std::string &t) {
        float w = 0, h = 0;
        hud::measure(style, t, &w, &h);
        return w;
    };
    while (i < title.size() && lines.size() < max_lines) {
        size_t j = title.find(' ', i);
        std::string word = title.substr(i, j == std::string::npos ? std::string::npos : j - i);
        std::string trial = line.empty() ? word : line + " " + word;
        if (width(trial) <= max_w || line.empty()) {
            line = trial;
            i = j == std::string::npos ? title.size() : j + 1;
        } else {
            lines.push_back(line);
            line.clear();
        }
    }
    if (!line.empty() && lines.size() < max_lines)
        lines.push_back(line);
    bool cut = i < title.size();
    if (!lines.empty()) {
        std::string &last = lines.back();
        if (cut)
            last += "...";
        while (width(last) > max_w && last.size() > 4) {
            last.erase(last.size() - (cut ? 4 : 1), cut ? 4 : 1);
            if (cut)
                last += "...";
        }
    }
    return lines;
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
    bool y = p.connected && (p.buttons & logic::WATCH_PAD_MASK);
    bool key = (GetAsyncKeyState(logic::WATCH_KEY_VK) & 0x8000) != 0 && GetForegroundWindow() != nullptr;
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
    std::string text = logic::offline_text(online);
    if (online == offline_shown)
        return;
    if (ui_vm_set_path("ui_menu_data.swofty_offline_text", 0, text.c_str())) {
        offline_shown = online;
        swofty::log("[main-menu] Launched with panel text set: %s", logic::online_line(online).empty() ? "no online line" : logic::online_line(online).c_str());
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
    auto next = logic::parse_state(runtime::read_file(runtime::under_swofty("home_menu.json"), size_t(1) << 20));
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
        swofty::log("[main-menu] state: %s, %s, video %s", next.loaded ? logic::version_line(next).c_str() : "no file",
                    logic::discord_line(next).c_str(), logic::has_video(next) ? "yes" : "none");
    state = next;
    state_loaded = next.loaded;
    set_offline_text(next.online);
}

logic::State Panel::snapshot()
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
    return logic::panel_due(menu.patched.load(), menu.home_open.load() && !runtime::in_session_now(), state_loaded.load(),
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
        swofty::log("[main-menu] slow home tick: state read %llu ms, panel prepare %llu ms, video bind %llu ms", static_cast<unsigned long long>(t1 - t0),
                    static_cast<unsigned long long>(t2 - t1), static_cast<unsigned long long>(t3 - t2));
}

void Panel::sync_strip()
{
    int want = strip_wanted.load();
    if (want >= 0 && want != strip_set.load() && menu.patched.load()) {
        int v = want;
        bool ok = ui_vm_set_path("ui_menu_data.swofty_strip_visible", 1, &v);
        if (ok) {
            strip_set = want;
            swofty::log("[main-menu] stock top-right strip %s (ui_menu_data.swofty_strip_visible = %d)", want ? "shown" : "hidden", want);
        } else if (strip_tries++ % 120 == 0) {
            swofty::log("[main-menu] could not set ui_menu_data.swofty_strip_visible yet (try %d)", strip_tries);
        }
    }
}

void Panel::prepare()
{
    uint64_t now = GetTickCount64();
    logic::State s;
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
    if (logic::has_avatar(s))
        p->avatar_slot = runtime::overlay_image("core.main_menu\navatar", host_path(s.avatar), &w, &h);
    if (logic::has_video(s)) {
        p->video_slot = runtime::overlay_image("core.main_menu\nvideo", host_path(s.video_thumbnail), &w, &h);
        p->video_w = w;
        p->video_h = h;
    }
    p->version = logic::version_line(s);
    p->discord = logic::discord_line(s);
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

void Panel::paint(hud::Frame &f, uint64_t now)
{
    (void)now;
    if (!std::exchange(told_first, true))
        swofty::log("[main-menu] panel: paint called on a %.0fx%.0f frame (patched %d, home open %d, state %d, prepared %d, disabled %d)",
                    double(f.width), double(f.height), int(menu.patched.load()), int(menu.home_open.load()), int(state_loaded.load()),
                    int(panel_prepared.load()), int(panel_disabled.load()));
    menu.navigation.paint_cover(f);
    if (!due()) {
        if (menu.home_open.load() && !std::exchange(told_blocked, true))
            swofty::log("[main-menu] panel: not due on an open home screen (patched %d, state %d, prepared %d, disabled %d)",
                        int(menu.patched.load()), int(state_loaded.load()), int(panel_prepared.load()), int(panel_disabled.load()));
        return;
    }
    std::shared_ptr<const Prepared> p;
    {
        std::lock_guard<std::mutex> g(prepared_lock);
        p = prepared;
    }
    if (!p) {
        if (!std::exchange(told_noprep, true))
            swofty::log("[main-menu] panel: due but the prepared snapshot is empty");
        return;
    }
    size_t verts_before = f.verts.size();
    LARGE_INTEGER t0, t1, freq;
    QueryPerformanceCounter(&t0);
    hud::reset_state(f);
    uint64_t shown_for = GetTickCount64() - menu.home_opened_at.load();
    float fade = shown_for < PANEL_DELAY_MS ? 0.0f : std::min(1.0f, float(shown_for - PANEL_DELAY_MS) / float(PANEL_FADE_MS));
    if (fade <= 0.0f)
        return;
    f.opacity = fade;
    float W = f.width, H = f.height;
    if (!std::exchange(told_drawn, true))
        swofty::log("[main-menu] panel: first draw on a %.0fx%.0f frame; video card %s, avatar %s", double(W), double(H),
                    p->video_slot >= 0 ? "yes" : "no", p->avatar_slot >= 0 ? "yes" : "no");
    const float k = logic::panel_scale(W, H);
    const float band_top = logic::ui_band(W, H).top;
    auto anchor = logic::scene_to_px(logic::CARD_RIGHT_X, logic::CARD_BOTTOM_Y, W, H);
    const float right = anchor.x;
    {
        hud::TextStyle name_style;
        name_style.px = 26 * k;
        name_style.rgba = 0xf3f5f9ffu;
        std::string who = p->s.signed_in ? (p->s.name.empty() ? std::string("Discord") : p->s.name) : std::string("Not signed in");
        float tmp = 0, nw = 0;
        hud::measure(name_style, who, &nw, &tmp);
        nw = nw * 1.06f + 4 * k;
        float avatar = 44 * k, pad = 10 * k, gap = 12 * k;
        float block_h = name_style.px * 1.3f;
        float ph = avatar + 2 * pad;
        float pw = std::max(190 * k, pad + avatar + gap + 3 * k + 9 * k + nw + 20 * k);
        float px = right - pw, py = band_top + 44 * k;
        fade_panel(f, px - 110 * k, py, pw + 110 * k, ph, 0.45f, 0.04f, k);
        float ax = px + pad, ay = py + pad;
        if (p->avatar_slot >= 0) {
            image_rounded(f, p->avatar_slot, ax, ay, avatar, avatar, avatar * 0.5f, 0, 0, 1, 1);
        } else if (p->s.signed_in) {
            hud::rect_rounded(f, ax, ay, avatar, avatar, avatar * 0.5f, 0x5865f2ffu);
            hud::TextStyle initial;
            initial.px = 24 * k;
            initial.rgba = 0xffffffffu;
            std::string letter = p->s.name.empty() ? "D" : p->s.name.substr(0, 1);
            float lw = 0;
            hud::measure(initial, letter, &lw, &tmp);
            hud::text(f, ax + (avatar - lw) * 0.5f, ay + (avatar - initial.px) * 0.5f, initial, letter);
        } else {
            hud::rect_rounded(f, ax, ay, avatar, avatar, avatar * 0.5f, 0x3a3f4bffu);
            hud::rect_rounded(f, ax + avatar * 0.34f, ay + avatar * 0.18f, avatar * 0.32f, avatar * 0.32f, avatar * 0.16f, 0xb9bfcdffu);
            hud::rect_rounded(f, ax + avatar * 0.2f, ay + avatar * 0.56f, avatar * 0.6f, avatar * 0.26f, avatar * 0.13f, 0xb9bfcdffu);
        }
        float bx = ax + avatar + gap, by = py + (ph - block_h) * 0.5f;
        hud::rect_rounded(f, bx, by, 3 * k, block_h, 1.5f * k, 0xe10600ffu);
        hud::text(f, bx + 12 * k, by + (block_h - name_style.px) * 0.5f, name_style, who);
    }
    if (p->video_slot >= 0 && p->video_w && p->video_h) {
        hud::TextStyle cap_style;
        cap_style.px = 16 * k;
        cap_style.rgba = 0xa9afbfffu;
        cap_style.tracking = 16 * k * 0.12f;
        hud::TextStyle title_style;
        title_style.px = 24 * k;
        title_style.rgba = 0xf3f5f9ffu;
        hud::TextStyle watch_style;
        watch_style.px = 20 * k;
        watch_style.rgba = 0xd5d9e3ffu;
        const std::string caption = "LATEST VIDEO";
        const bool pad_dev = bind.on_pad();
        const std::string key = logic::button_name(pad_dev);
        float tmp_h = 0, cap_w = 0, watch_w = 0, key_w = 0;
        hud::TextStyle key_probe;
        key_probe.px = 26 * k * 0.62f;
        hud::measure(cap_style, caption, &cap_w, &tmp_h);
        hud::measure(watch_style, "Watch", &watch_w, &tmp_h);
        hud::measure(key_probe, key, &key_w, &tmp_h);
        const float row_h = 26 * k;
        const float glyph_w = pad_dev ? std::max(row_h * 1.5f, key_w + 18 * k) : std::max(row_h, key_w + 14 * k);
        auto plan0 = logic::plan_card(k, 0);
        auto title_lines = wrap_title(title_style, p->s.video_title, plan0.text_max, 2);
        float title_w = 0;
        for (auto &line : title_lines) {
            float w = 0;
            hud::measure(title_style, line, &w, &tmp_h);
            title_w = std::max(title_w, w);
        }
        float text_w = std::max({cap_w, title_w, glyph_w + 10 * k + watch_w}) * 1.06f + 4 * k;
        auto card = logic::plan_card(k, text_w);
        float cx = right - card.width, cy = anchor.y - card.height;
        fade_panel(f, cx - 110 * k, cy, card.width + 110 * k, card.height, 0.45f, 0.04f, k);
        float tx = cx + card.pad + 12 * k;
        float cap_h = cap_style.px * 1.25f, title_h = title_style.px * 1.2f * float(title_lines.size());
        float block_h = cap_h + 6 * k + title_h + 8 * k + row_h;
        float ty = cy + (card.height - block_h) * 0.5f;
        hud::rect_rounded(f, cx + card.pad, ty, 3 * k, block_h, 1.5f * k, 0xe10600ffu);
        hud::text(f, tx, ty, cap_style, caption);
        float yy = ty + cap_h + 6 * k;
        for (auto &line : title_lines) {
            hud::text(f, tx, yy, title_style, line);
            yy += title_style.px * 1.2f;
        }
        yy += 8 * k;
        key_glyph(f, tx, yy, row_h, k, key, pad_dev);
        hud::text(f, tx + glyph_w + 10 * k, yy + (row_h - watch_style.px) * 0.5f, watch_style, "Watch");
        float want = card.thumb_w / card.thumb_h, have = float(p->video_w) / float(p->video_h);
        float v0 = 0, v1 = 1, u0 = 0, u1 = 1;
        if (have < want) {
            float keep = have / want;
            v0 = (1 - keep) * 0.5f;
            v1 = 1 - v0;
        } else {
            float keep = want / have;
            u0 = (1 - keep) * 0.5f;
            u1 = 1 - u0;
        }
        image_rounded(f, p->video_slot, cx + card.width - card.pad - card.thumb_w, cy + card.pad, card.thumb_w, card.thumb_h,
                      8 * k, u0, v0, u1, v1);
    }
    overlay_drawn = true;
    if (!std::exchange(told_verts, true))
        swofty::log("[main-menu] panel: first draw added %zu vertices and %zu batch(es) (frame total %zu / %zu)",
                    f.verts.size() - verts_before, f.batches.size(), f.verts.size(), f.batches.size());
    QueryPerformanceCounter(&t1);
    QueryPerformanceFrequency(&freq);
    double ms = double(t1.QuadPart - t0.QuadPart) * 1000.0 / double(freq.QuadPart);
    if (logic::panel_too_slow(ms) && !panel_disabled.exchange(true))
        swofty::log("[main-menu] panel: one draw took %.2f ms (budget %.1f ms); the panel is disabled for this session", ms,
                    logic::PANEL_BUDGET_MS);
}

}
