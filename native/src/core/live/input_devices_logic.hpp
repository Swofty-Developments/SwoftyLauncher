#pragma once

#include <swofty_api.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <string>
#include <string_view>
#include <vector>

// The input devices of the lap host's input call (SwoftyLapHost version 4): what Raw Input and XInput list, folded
// into the table the lap module copies. Windows-free so the host tests cover it; live/input_devices.cpp reads the
// devices and the virtual pad.
namespace input_devices {

constexpr uint8_t NO_SLOT = 255;
constexpr uint16_t PAGE_GENERIC = 0x01, USAGE_JOYSTICK = 0x04, USAGE_GAMEPAD = 0x05, USAGE_MULTI_AXIS = 0x08;

// The kind of a HID top-level collection, or 0 for one that is not a game controller.
inline uint8_t kind_of(uint16_t usage_page, uint16_t usage)
{
    if (usage_page != PAGE_GENERIC)
        return 0;
    switch (usage) {
    case USAGE_JOYSTICK:
        return SWOFTY_LAP_INPUT_KIND_JOYSTICK;
    case USAGE_GAMEPAD:
        return SWOFTY_LAP_INPUT_KIND_GAMEPAD;
    case USAGE_MULTI_AXIS:
        return SWOFTY_LAP_INPUT_KIND_MULTI_AXIS;
    default:
        return 0;
    }
}

// Windows marks the HID collection of an XInput device with IG_ in its device path (\\?\HID#VID_045E&PID_028E&IG_00#...).
inline bool xinput_path(std::string_view path)
{
    for (size_t i = 0; i + 3 <= path.size(); i++)
        if ((path[i] == 'I' || path[i] == 'i') && (path[i + 1] == 'G' || path[i + 1] == 'g') && path[i + 2] == '_')
            return true;
    return false;
}

// A product string for the table: UTF-8 from UTF-16, control characters dropped, white space collapsed and trimmed, at most
// SWOFTY_LAP_INPUT_NAME - 1 bytes cut on a character boundary. A lone surrogate becomes U+FFFD.
inline std::string clean_name(const uint16_t *w, size_t n)
{
    std::string out;
    bool space = false;
    auto put = [&](uint32_t c) {
        char b[4];
        size_t len = 0;
        if (c < 0x80) {
            b[len++] = char(c);
        } else if (c < 0x800) {
            b[len++] = char(0xc0 | c >> 6);
            b[len++] = char(0x80 | (c & 0x3f));
        } else if (c < 0x10000) {
            b[len++] = char(0xe0 | c >> 12);
            b[len++] = char(0x80 | (c >> 6 & 0x3f));
            b[len++] = char(0x80 | (c & 0x3f));
        } else {
            b[len++] = char(0xf0 | c >> 18);
            b[len++] = char(0x80 | (c >> 12 & 0x3f));
            b[len++] = char(0x80 | (c >> 6 & 0x3f));
            b[len++] = char(0x80 | (c & 0x3f));
        }
        if (out.size() + len > SWOFTY_LAP_INPUT_NAME - 1)
            return false;
        out.append(b, len);
        return true;
    };
    for (size_t i = 0; i < n && w[i]; i++) {
        uint32_t c = w[i];
        if (c >= 0xd800 && c <= 0xdbff && i + 1 < n && w[i + 1] >= 0xdc00 && w[i + 1] <= 0xdfff) {
            c = 0x10000 + ((c - 0xd800) << 10) + (w[i + 1] - 0xdc00);
            i++;
        } else if (c >= 0xd800 && c <= 0xdfff) {
            c = 0xfffd;
        }
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == 0xa0) {
            space = !out.empty();
            continue;
        }
        if (c < 0x20 || c == 0x7f || (c >= 0x80 && c < 0xa0))
            continue;
        if (space && !put(' '))
            break;
        space = false;
        if (!put(c))
            break;
    }
    return out;
}

struct Device {
    uint8_t kind = 0, slot = NO_SLOT, subtype = 0, flags = 0;
    uint16_t vendor = 0, product = 0, usage_page = 0, usage = 0;
    std::string name;
};

struct Snapshot {
    std::vector<Device> devices;
    uint32_t keyboards = 0, mice = 0;
    bool read = false;
    double read_at_s = 0;
};

struct Pad {
    uint32_t owners = 0, seen = 0;
    uint64_t set = 0, idle = 0, real = 0;
};

// XInput slots first by slot, then the HID entries by vendor, product, usage and name, so the same devices give the
// same table whatever order Windows lists them in.
inline void order(std::vector<Device> &devices)
{
    std::stable_sort(devices.begin(), devices.end(), [](const Device &a, const Device &b) {
        bool ax = a.kind == SWOFTY_LAP_INPUT_KIND_XINPUT, bx = b.kind == SWOFTY_LAP_INPUT_KIND_XINPUT;
        if (ax != bx)
            return ax;
        if (ax)
            return a.slot < b.slot;
        if (a.vendor != b.vendor)
            return a.vendor < b.vendor;
        if (a.product != b.product)
            return a.product < b.product;
        if (a.usage != b.usage)
            return a.usage < b.usage;
        return a.name < b.name;
    });
}

// The table for a caller whose struct is out->size bytes: 0 for a NULL or short one, else 1 with the snapshot's first
// SWOFTY_LAP_INPUT_DEVICES devices.
inline int fill(const Snapshot &s, const Pad &pad, double now_s, SwoftyLapInput *out)
{
    if (!out || out->size < sizeof(SwoftyLapInput))
        return 0;
    SwoftyLapInput t{};
    t.size = sizeof t;
    t.version = SWOFTY_LAP_INPUT_VERSION;
    t.pad_owners = pad.owners;
    t.pad_owners_seen = pad.seen;
    t.pad_set = pad.set;
    t.pad_idle = pad.idle;
    t.pad_real = pad.real;
    t.keyboards = s.keyboards;
    t.mice = s.mice;
    t.devices_total = uint32_t(s.devices.size());
    t.age_s = s.read ? std::max(0.0, now_s - s.read_at_s) : -1.0;
    for (const Device &d : s.devices) {
        if (t.count >= SWOFTY_LAP_INPUT_DEVICES)
            break;
        SwoftyLapInputDevice &o = t.devices[t.count++];
        o.kind = d.kind;
        o.slot = d.slot;
        o.subtype = d.subtype;
        o.flags = d.flags;
        o.vendor = d.vendor;
        o.product = d.product;
        o.usage_page = d.usage_page;
        o.usage = d.usage;
        std::memcpy(o.name, d.name.data(), std::min(d.name.size(), size_t(SWOFTY_LAP_INPUT_NAME - 1)));
    }
    std::memcpy(out, &t, sizeof t);
    return 1;
}

// Whether two reads list the same controllers, for the log line on a change.
inline bool same_controllers(const Snapshot &a, const Snapshot &b)
{
    if (a.devices.size() != b.devices.size())
        return false;
    for (size_t i = 0; i < a.devices.size(); i++) {
        const Device &x = a.devices[i], &y = b.devices[i];
        if (x.kind != y.kind || x.slot != y.slot || x.vendor != y.vendor || x.product != y.product || x.flags != y.flags)
            return false;
    }
    return true;
}

}
