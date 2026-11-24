#pragma once
#include <algorithm>
#include <atomic>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "swofty_api.h"

// Overlay input: window messages from the game's window procedure are queued as events and folded
// into one snapshot per presented frame. One addon at a time can hold input focus; while it does, the
// core keeps mouse and keyboard messages from the game and draws a cursor.

namespace hud::input {

enum class Ev : uint8_t { move, down, up, wheel, chr, key_down, key_up };

struct Event {
    Ev kind;
    uint32_t a;
    float x, y;
};

struct PadRaw {
    bool connected = false;
    uint16_t buttons = 0;
    uint8_t lt = 0, rt = 0;
    int16_t lx = 0, ly = 0, rx = 0, ry = 0;
};

struct Snapshot {
    float x = -1, y = -1;
    float wheel = 0;
    uint32_t buttons = 0, pressed = 0, released = 0;
    std::vector<uint32_t> chars;
    uint8_t keys_down[32] = {};
    uint8_t keys_pressed[32] = {};
    uint8_t keys_typed[32] = {};
    uint32_t pad_connected = 0, pad_buttons = 0, pad_pressed = 0;
    float pad_lx = 0, pad_ly = 0, pad_rx = 0, pad_ry = 0, pad_lt = 0, pad_rt = 0;
    uint32_t pad_prev = 0;
    uint64_t pad_repeat_at = 0;
    uint32_t joy_connected = 0, joy_buttons = 0, joy_pressed = 0;
};

constexpr uint32_t PAD_DIRS = SWOFTY_PAD_UP | SWOFTY_PAD_DOWN | SWOFTY_PAD_LEFT | SWOFTY_PAD_RIGHT;
constexpr uint64_t PAD_REPEAT_DELAY = 400, PAD_REPEAT_EVERY = 90;

inline float stick(int16_t v, float deadzone)
{
    float f = v < 0 ? float(v) / 32768.0f : float(v) / 32767.0f;
    float a = f < 0 ? -f : f;
    if (a <= deadzone)
        return 0;
    float k = (a - deadzone) / (1 - deadzone);
    return f < 0 ? -k : k;
}

// Folds the pads polled this frame: buttons are ORed across pads, the sticks come from the first
// connected pad and the left stick past half way also holds the d-pad. A held direction presses again
// after 400 ms and then every 90 ms.
inline void fold_pads(Snapshot &s, const PadRaw *pads, int n, uint64_t now)
{
    uint32_t held = 0, connected = 0;
    bool sticks = false;
    s.pad_lx = s.pad_ly = s.pad_rx = s.pad_ry = s.pad_lt = s.pad_rt = 0;
    for (int i = 0; i < n; i++) {
        const PadRaw &p = pads[i];
        if (!p.connected)
            continue;
        connected |= 1u << i;
        held |= p.buttons;
        s.pad_lt = std::max(s.pad_lt, float(p.lt) / 255.0f);
        s.pad_rt = std::max(s.pad_rt, float(p.rt) / 255.0f);
        if (!sticks) {
            sticks = true;
            s.pad_lx = stick(p.lx, 0.24f);
            s.pad_ly = stick(p.ly, 0.24f);
            s.pad_rx = stick(p.rx, 0.27f);
            s.pad_ry = stick(p.ry, 0.27f);
        }
    }
    if (s.pad_ly > 0.5f)
        held |= SWOFTY_PAD_UP;
    if (s.pad_ly < -0.5f)
        held |= SWOFTY_PAD_DOWN;
    if (s.pad_lx < -0.5f)
        held |= SWOFTY_PAD_LEFT;
    if (s.pad_lx > 0.5f)
        held |= SWOFTY_PAD_RIGHT;
    s.pad_connected = connected;
    s.pad_buttons = held;
    s.pad_pressed = held & ~s.pad_prev;
    uint32_t dirs = held & PAD_DIRS;
    if (s.pad_pressed & PAD_DIRS)
        s.pad_repeat_at = now + PAD_REPEAT_DELAY;
    else if (dirs && now >= s.pad_repeat_at) {
        s.pad_pressed |= dirs;
        s.pad_repeat_at = now + PAD_REPEAT_EVERY;
    }
    s.pad_prev = held;
}

// DirectInput joystick states: DIJOYSTATE (80 bytes, 32 buttons) and DIJOYSTATE2 (272 bytes, 128 buttons) both put
// the buttons after six axes, two sliders and four POV hats, one byte each with 0x80 set while held.
constexpr uint32_t DIJOYSTATE_SIZE = 80, DIJOYSTATE2_SIZE = 272, DIJOY_BUTTONS_AT = 48;

// The first 32 buttons of a joystick state as a bitmask; false when size is not a joystick state.
inline bool joy_mask(const void *state, uint32_t size, uint32_t &mask)
{
    if (!state || (size != DIJOYSTATE_SIZE && size != DIJOYSTATE2_SIZE))
        return false;
    const uint8_t *b = static_cast<const uint8_t *>(state) + DIJOY_BUTTONS_AT;
    mask = 0;
    for (uint32_t i = 0; i < 32; i++)
        if (b[i] & 0x80)
            mask |= 1u << i;
    return true;
}

// The game controllers seen in the game's own DirectInput reads, written from the thread that reads them and folded
// on the render thread. A device keeps its slot while it is read; one unread for JOY_STALE_MS gives its slot up.
constexpr int JOY_SLOTS = 8;
constexpr uint64_t JOY_CONNECTED_MS = 2000, JOY_STALE_MS = 5000;

struct JoySlot {
    std::atomic<uintptr_t> dev{0};
    std::atomic<uint32_t> held{0};
    std::atomic<uint64_t> seen{0};
};

struct JoyRead {
    int slot = -1;
    bool fresh = false;
    uint32_t went_down = 0;
};

struct JoyDevices {
    JoySlot slots[JOY_SLOTS];
    std::atomic<uint32_t> downs{0};

    // Records one read of dev. A device's first read sets its held buttons without pressing them, so a button
    // already held when the core starts watching is not a press.
    JoyRead read(uintptr_t dev, uint32_t mask, uint64_t now)
    {
        JoyRead r;
        for (int i = 0; i < JOY_SLOTS; i++)
            if (slots[i].dev.load(std::memory_order_acquire) == dev) {
                r.slot = i;
                break;
            }
        if (r.slot < 0) {
            for (int i = 0; i < JOY_SLOTS && r.slot < 0; i++) {
                uintptr_t cur = slots[i].dev.load(std::memory_order_acquire);
                bool free = cur == 0 || now - slots[i].seen.load(std::memory_order_relaxed) >= JOY_STALE_MS;
                if (free && slots[i].dev.compare_exchange_strong(cur, dev, std::memory_order_acq_rel)) {
                    r.slot = i;
                    r.fresh = true;
                }
            }
            if (r.slot < 0)
                return r;
        }
        JoySlot &s = slots[r.slot];
        uint32_t prev = s.held.exchange(mask, std::memory_order_relaxed);
        s.seen.store(now, std::memory_order_relaxed);
        if (!r.fresh) {
            r.went_down = mask & ~prev;
            if (r.went_down)
                downs.fetch_or(r.went_down, std::memory_order_relaxed);
        }
        return r;
    }
};

// Folds the controllers into s: a bit per slot read in the last JOY_CONNECTED_MS, their held buttons ORed, and every
// button that went down since the last fold, so a press shorter than a frame still shows once.
inline void fold_joy(Snapshot &s, JoyDevices &d, uint64_t now)
{
    uint32_t connected = 0, held = 0;
    for (int i = 0; i < JOY_SLOTS; i++) {
        const JoySlot &j = d.slots[i];
        if (!j.dev.load(std::memory_order_acquire) || now - j.seen.load(std::memory_order_relaxed) >= JOY_CONNECTED_MS)
            continue;
        connected |= 1u << i;
        held |= j.held.load(std::memory_order_relaxed);
    }
    s.joy_connected = connected;
    s.joy_buttons = held;
    s.joy_pressed = d.downs.exchange(0, std::memory_order_relaxed);
}

inline void set_bit(uint8_t *bits, uint32_t vk, bool on)
{
    if (vk >= 256)
        return;
    if (on)
        bits[vk >> 3] |= uint8_t(1u << (vk & 7));
    else
        bits[vk >> 3] &= uint8_t(~(1u << (vk & 7)));
}

inline bool bit(const uint8_t *bits, uint32_t vk) { return vk < 256 && (bits[vk >> 3] >> (vk & 7) & 1); }

// Folds the frame's events into s: edges and text start empty, held buttons and keys carry over, and
// positions arrive in client pixels and leave in framebuffer pixels.
inline void fold(Snapshot &s, const std::vector<Event> &events, float sx, float sy)
{
    s.wheel = 0;
    s.pressed = s.released = 0;
    s.chars.clear();
    memset(s.keys_pressed, 0, sizeof s.keys_pressed);
    memset(s.keys_typed, 0, sizeof s.keys_typed);
    for (const Event &e : events) {
        switch (e.kind) {
        case Ev::move:
            s.x = e.x * sx;
            s.y = e.y * sy;
            break;
        case Ev::down:
            s.x = e.x * sx;
            s.y = e.y * sy;
            s.buttons |= e.a;
            s.pressed |= e.a;
            break;
        case Ev::up:
            s.x = e.x * sx;
            s.y = e.y * sy;
            s.buttons &= ~e.a;
            s.released |= e.a;
            break;
        case Ev::wheel:
            s.wheel += e.x;
            break;
        case Ev::chr:
            if (e.a >= 0x20 && e.a != 0x7f && s.chars.size() < 16)
                s.chars.push_back(e.a);
            break;
        case Ev::key_down:
            if (!bit(s.keys_down, e.a))
                set_bit(s.keys_pressed, e.a, true);
            set_bit(s.keys_down, e.a, true);
            set_bit(s.keys_typed, e.a, true);
            break;
        case Ev::key_up:
            set_bit(s.keys_down, e.a, false);
            break;
        }
    }
}

// Joins UTF-16 surrogate pairs from WM_CHAR into code points; returns 0 while half a pair is pending.
struct Utf16 {
    uint32_t high = 0;

    uint32_t push(uint32_t unit)
    {
        if (unit >= 0xd800 && unit < 0xdc00) {
            high = unit;
            return 0;
        }
        if (unit >= 0xdc00 && unit < 0xe000) {
            uint32_t cp = high ? 0x10000 + ((high - 0xd800) << 10) + (unit - 0xdc00) : 0xfffd;
            high = 0;
            return cp;
        }
        high = 0;
        return unit;
    }
};

// Which addon holds focus. A holder that has not painted for timeout_ms loses it.
struct Focus {
    std::string owner;
    uint64_t last_paint = 0;

    bool take(const std::string &who, bool on, uint64_t now)
    {
        if (on) {
            owner = who;
            last_paint = now;
            return true;
        }
        if (owner == who) {
            owner.clear();
            return true;
        }
        return false;
    }

    void painted(const std::string &who, uint64_t now)
    {
        if (!owner.empty() && owner == who)
            last_paint = now;
    }

    bool expire(uint64_t now, uint64_t timeout_ms)
    {
        if (owner.empty() || now - last_paint < timeout_ms)
            return false;
        owner.clear();
        return true;
    }

    bool held() const { return !owner.empty(); }
};

// The snapshot an addon sees: the focus holder gets everything; while nobody holds focus every addon
// gets the cursor, mouse buttons, key edges (no keys while the game has a text box focused), pads and game
// controllers; while another addon holds it, an addon gets nothing. The wheel and typed text go only to the holder.
inline void view(const Snapshot &s, const Focus &f, const std::string &who, SwoftyInput &out, bool typing = false)
{
    uint32_t size = out.size;
    memset(&out, 0, sizeof out);
    out.size = size;
    bool mine = f.held() && f.owner == who;
    out.focused = mine ? 1 : 0;
    out.x = out.y = -1;
    if (f.held() && !mine)
        return;
    out.x = s.x;
    out.y = s.y;
    out.buttons = s.buttons;
    out.pressed = s.pressed;
    out.released = s.released;
    if (!typing || mine) {
        memcpy(out.keys_down, s.keys_down, sizeof out.keys_down);
        memcpy(out.keys_pressed, s.keys_pressed, sizeof out.keys_pressed);
        memcpy(out.keys_typed, s.keys_typed, sizeof out.keys_typed);
    }
    out.pad_connected = s.pad_connected;
    out.pad_buttons = s.pad_buttons;
    out.pad_pressed = s.pad_pressed;
    out.pad_lx = s.pad_lx;
    out.pad_ly = s.pad_ly;
    out.pad_rx = s.pad_rx;
    out.pad_ry = s.pad_ry;
    out.pad_lt = s.pad_lt;
    out.pad_rt = s.pad_rt;
    out.joy_connected = s.joy_connected;
    out.joy_buttons = s.joy_buttons;
    out.joy_pressed = s.joy_pressed;
    if (!mine)
        return;
    out.wheel = s.wheel;
    out.char_count = uint32_t(s.chars.size());
    for (size_t i = 0; i < s.chars.size(); i++)
        out.chars[i] = s.chars[i];
}

}
