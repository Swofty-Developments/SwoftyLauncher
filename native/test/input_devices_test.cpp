#include "core/live/input_devices_logic.hpp"

#include <swofty_api.h>

#include <cstddef>
#include <cstdio>
#include <string>

namespace in = input_devices;

static int failed = 0;
static void expect(bool ok, const std::string &what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    if (!ok)
        failed++;
}

static std::string clean(const std::u16string &s)
{
    return in::clean_name(reinterpret_cast<const uint16_t *>(s.data()), s.size());
}

static void layout()
{
    expect(sizeof(SwoftyLapInputDevice) == 76 && offsetof(SwoftyLapInputDevice, vendor) == 4 &&
               offsetof(SwoftyLapInputDevice, usage) == 10 && offsetof(SwoftyLapInputDevice, name) == 12,
           "a device entry is 12 bytes of ids and a 64-byte name");
    expect(offsetof(SwoftyLapInput, pad_set) == 16 && offsetof(SwoftyLapInput, keyboards) == 40 &&
               offsetof(SwoftyLapInput, age_s) == 56 && offsetof(SwoftyLapInput, devices) == 64 &&
               sizeof(SwoftyLapInput) == 64 + 16 * 76,
           "the input table's layout is fixed");
}

static void kinds()
{
    expect(in::kind_of(0x01, 0x04) == SWOFTY_LAP_INPUT_KIND_JOYSTICK &&
               in::kind_of(0x01, 0x05) == SWOFTY_LAP_INPUT_KIND_GAMEPAD &&
               in::kind_of(0x01, 0x08) == SWOFTY_LAP_INPUT_KIND_MULTI_AXIS,
           "joysticks, gamepads and multi-axis controllers are game controllers");
    expect(in::kind_of(0x01, 0x06) == 0 && in::kind_of(0x01, 0x02) == 0 && in::kind_of(0x0c, 0x01) == 0 &&
               in::kind_of(0x02, 0x04) == 0,
           "keyboards, mice, consumer controls and other pages are not");
    expect(in::xinput_path("\\\\?\\HID#VID_045E&PID_028E&IG_00#3&2a3c4f&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}") &&
               in::xinput_path("\\\\?\\hid#vid_045e&pid_0b12&ig_00#a"),
           "an IG_ in the device path marks an XInput device's HID entry");
    expect(!in::xinput_path("\\\\?\\HID#VID_046D&PID_C24F#7&1b2f&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}") &&
               !in::xinput_path("IG"),
           "a wheel's path has no IG_");
}

static void names()
{
    expect(clean(u"G29 Driving Force Racing Wheel") == "G29 Driving Force Racing Wheel", "a plain product string is kept");
    expect(clean(u"  Xbox\t360   Controller \u0001") == "Xbox 360 Controller", "control characters go and spaces collapse");
    expect(clean(u"Mando de Fútbol é") == "Mando de F\xc3\xbatbol \xc3\xa9", "accents are UTF-8");
    expect(clean(u"\xd83c\xdfce wheel") == "\xf0\x9f\x8f\x8e wheel", "a surrogate pair is one character");
    std::u16string lone = u"a";
    lone += char16_t(0xd800);
    lone += u"b";
    expect(clean(lone) == "a\xef\xbf\xbd"
                          "b",
           "a lone surrogate becomes U+FFFD");
    std::u16string nul = u"Pad";
    nul += char16_t(0);
    nul += u"tail";
    expect(clean(nul) == "Pad", "the string ends at its terminator");
    std::u16string long_name(100, u'x');
    expect(clean(long_name).size() == SWOFTY_LAP_INPUT_NAME - 1, "a long name is cut to 63 bytes");
    std::u16string wide(40, u'é');
    std::string cut = clean(wide);
    expect(cut.size() == 62 && (unsigned char)cut.back() == 0xa9, "the cut falls on a character boundary");
}

static in::Device dev(uint8_t kind, uint8_t slot, uint16_t vendor, uint16_t product, std::string name = {})
{
    in::Device d;
    d.kind = kind;
    d.slot = slot;
    d.vendor = vendor;
    d.product = product;
    d.name = std::move(name);
    return d;
}

static void filling()
{
    in::Snapshot s;
    s.devices = {dev(SWOFTY_LAP_INPUT_KIND_JOYSTICK, in::NO_SLOT, 0x046d, 0xc24f, "G29 Driving Force Racing Wheel"),
                 dev(SWOFTY_LAP_INPUT_KIND_XINPUT, 1, 0x045e, 0x028e),
                 dev(SWOFTY_LAP_INPUT_KIND_GAMEPAD, in::NO_SLOT, 0x045e, 0x028e, "Controller (XBOX 360 For Windows)"),
                 dev(SWOFTY_LAP_INPUT_KIND_XINPUT, 0, 0, 0)};
    s.devices[2].flags = SWOFTY_LAP_INPUT_XINPUT_HID;
    s.devices[3].flags = SWOFTY_LAP_INPUT_VIRTUAL;
    s.devices[3].subtype = 1;
    s.keyboards = 2;
    s.mice = 1;
    in::order(s.devices);
    expect(s.devices[0].slot == 0 && s.devices[1].slot == 1 && s.devices[2].vendor == 0x045e && s.devices[3].vendor == 0x046d,
           "XInput slots come first by slot, then HID entries by vendor");
    in::Pad pad{SWOFTY_LAP_PAD_TITLE, SWOFTY_LAP_PAD_TITLE | SWOFTY_LAP_PAD_AGENT, 3, 1200, 0};

    SwoftyLapInput out{};
    out.size = sizeof out;
    expect(in::fill(in::Snapshot{}, pad, 10.0, &out) == 1 && out.age_s < 0 && out.count == 0 && out.devices_total == 0,
           "before the first read the table has no devices and a negative age");
    s.read = true;
    s.read_at_s = 7.5;
    out = SwoftyLapInput{};
    out.size = sizeof out;
    expect(in::fill(s, pad, 10.0, &out) == 1, "a full-size table is filled");
    expect(out.size == sizeof out && out.version == SWOFTY_LAP_INPUT_VERSION && out.age_s == 2.5, "size, version and age");
    expect(out.pad_owners == SWOFTY_LAP_PAD_TITLE && out.pad_owners_seen == 3 && out.pad_set == 3 && out.pad_idle == 1200 &&
               out.pad_real == 0,
           "the virtual pad's owners and reads are copied");
    expect(out.keyboards == 2 && out.mice == 1 && out.count == 4 && out.devices_total == 4, "the counts are copied");
    expect(out.devices[0].flags == SWOFTY_LAP_INPUT_VIRTUAL && out.devices[3].product == 0xc24f &&
               std::string(out.devices[3].name) == "G29 Driving Force Racing Wheel" &&
               out.devices[2].flags == SWOFTY_LAP_INPUT_XINPUT_HID && out.devices[1].name[0] == 0,
           "each device keeps its ids, flags and name");

    SwoftyLapInput shorter{};
    shorter.size = sizeof shorter - 1;
    expect(in::fill(s, pad, 10.0, &shorter) == 0 && shorter.count == 0, "a short table is refused and left alone");
    expect(in::fill(s, pad, 10.0, nullptr) == 0, "a null table is refused");

    in::Snapshot many;
    for (int i = 0; i < 20; i++)
        many.devices.push_back(dev(SWOFTY_LAP_INPUT_KIND_JOYSTICK, in::NO_SLOT, uint16_t(i), 1));
    many.read = true;
    out = SwoftyLapInput{};
    out.size = sizeof out;
    expect(in::fill(many, pad, 0.0, &out) == 1 && out.count == SWOFTY_LAP_INPUT_DEVICES && out.devices_total == 20,
           "past 16 devices the table holds the first 16 and counts all");
}

static void changes()
{
    in::Snapshot a, b;
    a.devices = {dev(SWOFTY_LAP_INPUT_KIND_XINPUT, 0, 0x045e, 0x028e)};
    b = a;
    b.keyboards = 3;
    expect(in::same_controllers(a, b), "a keyboard count alone is no controller change");
    b.devices[0].slot = 1;
    expect(!in::same_controllers(a, b), "a pad in another slot is a change");
    b = a;
    b.devices.push_back(dev(SWOFTY_LAP_INPUT_KIND_JOYSTICK, in::NO_SLOT, 0x0eb7, 0x0020));
    expect(!in::same_controllers(a, b), "a wheel plugged in is a change");
}

int main()
{
    layout();
    kinds();
    names();
    filling();
    changes();
    if (failed)
        std::printf("%d check(s) failed\n", failed);
    return failed ? 1 : 0;
}
