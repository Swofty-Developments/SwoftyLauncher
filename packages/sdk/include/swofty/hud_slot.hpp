// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>

#ifdef _WIN32
#include <windows.h>

#include <atomic>
#include <string>

#include <swofty/lang.hpp>
#endif

// An addon's own entry in the game's HUD editor. A `hud.slot` record in one of the addon's
// operations.hud files makes the entry, its selection box and its saved placement; this header
// reads the placement the game applies, live while the player edits it, so a native overlay
// draws at the slot's rect. Describe the slot here with the same id, rect and anchor as the record.

extern "C" {

typedef struct SwoftyHudSlotState {
    uint32_t size;
    int32_t found;
    int32_t enabled;
    int32_t editor_open;
    float x, y;
    float scale, opacity;
    int32_t has_box;
    float box[4];
    uint32_t element_key;
    char key[96];
} SwoftyHudSlotState;
}

namespace swofty::hud_slot {

enum class Anchor { top_left, top, top_right, left, centre, right, bottom_left, bottom, bottom_right };

struct Slot {
    const char *id;
    const char *name;
    float x, y, w, h;
    Anchor anchor = Anchor::centre;
};

struct Placement {
    float x = 0, y = 0, w = 0, h = 0;
    float scale = 1, opacity = 1;
    bool visible = true;
    bool editor_open = false;
    bool live = false;
};

enum class Result { ok, unavailable, no_slot };

inline constexpr float kUnitsW = 19.2f, kUnitsH = 10.8f;

inline void anchor_fraction(Anchor a, float &fx, float &fy)
{
    static constexpr float X[] = {0.0f, 0.5f, 1.0f, 0.0f, 0.5f, 1.0f, 0.0f, 0.5f, 1.0f};
    static constexpr float Y[] = {0.0f, 0.0f, 0.0f, 0.5f, 0.5f, 0.5f, 1.0f, 1.0f, 1.0f};
    int i = int(a);
    fx = X[i];
    fy = Y[i];
}

inline void default_offset(const Slot &s, float &ux, float &uy)
{
    float fx, fy;
    anchor_fraction(s.anchor, fx, fy);
    ux = (s.x + s.w / 2 - fx) * kUnitsW;
    uy = (fy - (s.y + s.h / 2)) * kUnitsH;
}

inline Placement place(const Slot &s, const SwoftyHudSlotState &st, float fb_w, float fb_h)
{
    Placement p;
    p.editor_open = st.editor_open != 0;
    p.live = st.found != 0;
    float ux, uy, scale = 1;
    default_offset(s, ux, uy);
    if (st.found) {
        ux = st.x;
        uy = st.y;
        scale = st.scale > 0 ? st.scale : 1;
        p.opacity = st.opacity;
        p.visible = st.enabled != 0;
    }
    p.scale = scale;
    float fx, fy;
    anchor_fraction(s.anchor, fx, fy);
    float cx = fx * kUnitsW - kUnitsW / 2 + ux;
    float cy = kUnitsH / 2 - fy * kUnitsH + uy;
    float hw = s.w * kUnitsW / 2 * scale, hh = s.h * kUnitsH / 2 * scale;
    float x0 = cx - hw, x1 = cx + hw, y0 = cy - hh, y1 = cy + hh;
    if (st.found && st.has_box && st.box[2] > st.box[0] && st.box[3] > st.box[1]) {
        x0 = st.box[0];
        y0 = st.box[1];
        x1 = st.box[2];
        y1 = st.box[3];
    }
    float sx = fb_w / kUnitsW, sy = fb_h / kUnitsH;
    p.x = (x0 + kUnitsW / 2) * sx;
    p.y = (kUnitsH / 2 - y1) * sy;
    p.w = (x1 - x0) * sx;
    p.h = (y1 - y0) * sy;
    return p;
}

#ifdef _WIN32
namespace detail {

using ReadFn = int (*)(const char *slot, const void *caller, SwoftyHudSlotState *out);

inline ReadFn read_fn()
{
    static std::atomic<ReadFn> fn{nullptr};
    static std::atomic<bool> looked{false};
    if (!looked.load()) {
        HMODULE core = GetModuleHandleW(L"swofty_core.dll");
        fn = core ? reinterpret_cast<ReadFn>(GetProcAddress(core, "SwoftyCoreHudSlotRead")) : nullptr;
        looked = true;
    }
    return fn.load();
}

inline void anchor() {}

}

inline bool available() { return detail::read_fn() != nullptr; }

inline Result read(const Slot &s, SwoftyHudSlotState &st)
{
    st = SwoftyHudSlotState{};
    st.size = sizeof st;
    auto fn = detail::read_fn();
    if (!fn)
        return Result::unavailable;
    return fn(s.id, reinterpret_cast<const void *>(&detail::anchor), &st) == 0 ? Result::ok : Result::no_slot;
}

inline Result get(const Slot &s, float fb_w, float fb_h, Placement &out)
{
    SwoftyHudSlotState st;
    Result r = read(s, st);
    out = place(s, st, fb_w, fb_h);
    return r;
}

inline bool editor_open()
{
    SwoftyHudSlotState st;
    Slot none{"", "", 0, 0, 0, 0};
    read(none, st);
    return st.editor_open != 0;
}

inline Result declare(const Slot &s)
{
    SwoftyHudSlotState st;
    Result r = read(s, st);
    if (r == Result::unavailable || !st.key[0])
        return Result::unavailable;
    std::string row = std::string("lng_hud_component_") + st.key;
    if (!swofty::lang::available())
        return Result::unavailable;
    swofty::lang::set(row.c_str(), s.name);
    swofty::lang::refresh();
    return Result::ok;
}

#endif

}
