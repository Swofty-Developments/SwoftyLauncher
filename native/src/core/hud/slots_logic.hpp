#pragma once

#include <cstdint>
#include <cstring>
#include <string>

// The game's HUD customisation data (the object the game loads hud_customisation.xml into, applies to the HUD,
// edits in the HUD editor and saves) and what a hud.slot reads from it. Every read goes through Mem, so the host
// tests walk a fake heap and the core checks each address before it reads.
namespace hud::slots {

inline constexpr const char *kLayouts[] = {"race", "qualifying", "practice", "time_trial", "elimination"};

inline constexpr uintptr_t DATA_AT_OWNER = 0x838;
inline constexpr uintptr_t MAP = 0x640;
inline constexpr uintptr_t MAP_ENTRIES = 0x18, MAP_INFO = 0x20;
inline constexpr uintptr_t ENTRY = 0x24;
inline constexpr uintptr_t E_X = 0x8, E_Y = 0xc, E_ENABLED = 0x10, E_OPACITY = 0x14, E_SCALE = 0x18;
inline constexpr uintptr_t LISTENERS = 0x6c8, LISTENERS_END = 0x6d0, LISTENER = 0x10;
inline constexpr uintptr_t LAYOUT_LIST = 0x2e0, LAYOUT_LIST_END = 0x2e8;
inline constexpr uintptr_t C_EDITOR = 0x8a, C_LAYOUT = 0x94;
inline constexpr uintptr_t C_BOXES = 0x130, C_BOXES_INFO = 0x138;
inline constexpr uintptr_t L_COUNT = 0x10, L_KEYS = 0x18, L_FILTERS = 0x28, L_FILTERS_END = 0x30, FILTER = 0x28;
inline constexpr uintptr_t F_KEYS = 0x10, F_KEYS_END = 0x18;
inline constexpr uintptr_t B_MIN = 0x10, B_MAX = 0x20, B_VALID = 0x34;
inline constexpr size_t MAX_ENTRIES = 1 << 16;

inline uint32_t djb2(const std::string &s)
{
    uint32_t h = 5381;
    for (unsigned char c : s)
        h = h * 33 + c;
    return h;
}

inline std::string clean(const std::string &s)
{
    std::string out;
    for (char c : s) {
        char l = (c >= 'A' && c <= 'Z') ? char(c - 'A' + 'a') : c;
        out += ((l >= 'a' && l <= 'z') || (l >= '0' && l <= '9')) ? l : '_';
    }
    return out;
}

inline std::string key(const std::string &addon, const std::string &slot) { return "swofty_" + clean(addon) + "_" + clean(slot); }

struct Entry {
    bool found = false;
    float x = 0, y = 0, opacity = 1, scale = 1;
    bool enabled = true;
};

struct Box {
    bool valid = false;
    float x0 = 0, y0 = 0, x1 = 0, y1 = 0;
};

template <class Mem> struct Reader {
    const Mem &m;

    template <class T> bool get(uintptr_t a, T &out) const
    {
        if (!a || !m.readable(a, sizeof(T)))
            return false;
        std::memcpy(&out, reinterpret_cast<const void *>(m.translate(a)), sizeof(T));
        return true;
    }

    uintptr_t ptr(uintptr_t a) const
    {
        uintptr_t v = 0;
        return get(a, v) ? v : 0;
    }

    Entry entry(uintptr_t data, uint32_t key) const
    {
        Entry e;
        uintptr_t entries = ptr(data + MAP + MAP_ENTRIES), info = ptr(data + MAP + MAP_INFO);
        if (!entries || info <= entries || (info - entries) % ENTRY)
            return e;
        size_t n = (info - entries) / ENTRY;
        if (n > MAX_ENTRIES)
            return e;
        for (size_t i = 0; i < n; i++) {
            uint8_t used = 0;
            uint32_t k = 0;
            uintptr_t at = entries + i * ENTRY;
            if (!get(info + i, used) || !used || !get(at, k) || k != key)
                continue;
            uint8_t on = 1;
            if (!get(at + E_X, e.x) || !get(at + E_Y, e.y) || !get(at + E_ENABLED, on) || !get(at + E_OPACITY, e.opacity) ||
                !get(at + E_SCALE, e.scale))
                return Entry{};
            e.enabled = on != 0;
            e.found = true;
            return e;
        }
        return e;
    }

    uintptr_t editor(uintptr_t data) const
    {
        uintptr_t b = ptr(data + LISTENERS), end = ptr(data + LISTENERS_END);
        if (!b || end <= b || (end - b) / LISTENER > 64)
            return 0;
        for (uintptr_t at = b; at < end; at += LISTENER) {
            uintptr_t ctrl = ptr(at + 8);
            uint8_t mode = 0;
            if (ctrl && get(ctrl + C_EDITOR, mode) && mode)
                return ctrl;
        }
        return 0;
    }

    uintptr_t any_controller(uintptr_t data) const
    {
        uintptr_t b = ptr(data + LISTENERS), end = ptr(data + LISTENERS_END);
        if (!b || end <= b || (end - b) / LISTENER > 64)
            return 0;
        uintptr_t editor_ctrl = editor(data);
        if (editor_ctrl)
            return editor_ctrl;
        return ptr(b + 8);
    }

    bool in_list(uintptr_t keys, size_t n, uint32_t key) const
    {
        if (n > MAX_ENTRIES)
            return false;
        for (size_t i = 0; i < n; i++) {
            uint32_t k = 0;
            if (get(keys + i * 4, k) && k == key)
                return true;
        }
        return false;
    }

    bool layout_has(uintptr_t data, uintptr_t ctrl, uint32_t key) const
    {
        int32_t index = -1;
        uintptr_t list = ptr(data + LAYOUT_LIST), end = ptr(data + LAYOUT_LIST_END);
        if (!ctrl || !get(ctrl + C_LAYOUT, index) || index < 0 || !list || end <= list || size_t(index) >= (end - list) / 8)
            return false;
        uintptr_t l = ptr(list + size_t(index) * 8);
        if (!l)
            return false;
        uint32_t count = 0;
        if (get(l + L_COUNT, count) && in_list(ptr(l + L_KEYS), count, key))
            return true;
        uintptr_t f = ptr(l + L_FILTERS), fend = ptr(l + L_FILTERS_END);
        if (!f || fend <= f || (fend - f) / FILTER > 16)
            return false;
        for (uintptr_t at = f; at < fend; at += FILTER) {
            uintptr_t kb = ptr(at + F_KEYS), ke = ptr(at + F_KEYS_END);
            if (kb && ke > kb && in_list(kb, (ke - kb) / 4, key))
                return true;
        }
        return false;
    }

    Box box(uintptr_t ctrl, uint32_t key) const
    {
        Box b;
        uintptr_t entries = ptr(ctrl + C_BOXES), info = ptr(ctrl + C_BOXES_INFO);
        if (!entries || info <= entries || (info - entries) % 8 || (info - entries) / 8 > MAX_ENTRIES)
            return b;
        size_t n = (info - entries) / 8;
        for (size_t i = 0; i < n; i++) {
            uint8_t used = 0;
            if (!get(info + i, used) || !used)
                continue;
            uintptr_t rec = ptr(entries + i * 8);
            uint32_t k = 0;
            if (!rec || !get(rec, k) || k != key)
                continue;
            uint8_t valid = 0;
            float mn[2] = {}, mx[2] = {};
            if (get(rec + B_VALID, valid) && valid && get(rec + B_MIN, mn) && get(rec + B_MAX, mx) && mx[0] > mn[0] &&
                mx[1] > mn[1]) {
                b = {true, mn[0], mn[1], mx[0], mx[1]};
            }
            return b;
        }
        return b;
    }
};

}
