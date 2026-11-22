#include "core/live/input_devices.hpp"

#include "core/live/input_devices_logic.hpp"
#include "core/runtime/runtime.hpp"
#include "core/testing/virtual_pad.hpp"

#include <swofty/mixin.hpp>

#include <chrono>
#include <condition_variable>
#include <cwchar>
#include <map>
#include <mutex>
#include <thread>

namespace input_devices {
namespace {

constexpr auto REFRESH = std::chrono::seconds(5);
constexpr double WANTED_FOR_S = 30.0;
constexpr size_t NAMES_MAX = 64;
constexpr DWORD XINPUT_SLOTS = 4;

struct XCapsEx {
    uint8_t type, subtype;
    uint16_t flags;
    uint8_t pad[12];
    uint16_t vib[2];
    uint16_t vendor, product, version, unknown;
    uint32_t more;
    uint8_t spare[32];
};

using CapsExFn = DWORD(WINAPI *)(DWORD, DWORD, DWORD, XCapsEx *);
using ProductFn = BOOLEAN(WINAPI *)(HANDLE, PVOID, ULONG);

struct State {
    std::mutex lock;
    std::condition_variable wake;
    Snapshot last;
    double asked_s = -1e9;
    std::map<std::wstring, std::string> names;
    CapsExFn caps_ex = nullptr;
    ProductFn product = nullptr;
    bool logged = false;
};

// Never destroyed: the reader thread may still hold it while the process ends.
State &state()
{
    static State *s = new State;
    return *s;
}

std::once_flag started;

double now_s() { return double(GetTickCount64()) / 1000.0; }

std::string product_name(State &st, const std::wstring &path)
{
    if (auto it = st.names.find(path); it != st.names.end())
        return it->second;
    std::string name;
    if (st.product) {
        HANDLE h = CreateFileW(path.c_str(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr);
        if (h != INVALID_HANDLE_VALUE) {
            uint16_t buf[127] = {};
            if (st.product(h, buf, sizeof buf - sizeof buf[0]))
                name = clean_name(buf, sizeof buf / sizeof buf[0]);
            CloseHandle(h);
        }
    }
    if (st.names.size() < NAMES_MAX)
        st.names.emplace(path, name);
    return name;
}

std::string narrow_path(const std::wstring &w)
{
    std::string s;
    s.reserve(w.size());
    for (wchar_t c : w)
        s += c < 0x80 ? char(c) : '?';
    return s;
}

void read_xinput(State &st, Snapshot &out, bool pad_claimed)
{
    for (DWORD slot = 0; slot < XINPUT_SLOTS; slot++) {
        XCapsEx caps{};
        if (runtime::pad::real_caps(slot, 0, &caps) != ERROR_SUCCESS) {
            if (slot == 0 && pad_claimed) {
                Device d;
                d.kind = SWOFTY_LAP_INPUT_KIND_XINPUT;
                d.slot = 0;
                d.subtype = 1;
                d.flags = SWOFTY_LAP_INPUT_VIRTUAL;
                out.devices.push_back(d);
            }
            continue;
        }
        Device d;
        d.kind = SWOFTY_LAP_INPUT_KIND_XINPUT;
        d.slot = uint8_t(slot);
        d.subtype = caps.subtype;
        XCapsEx ex{};
        if (st.caps_ex && st.caps_ex(1, slot, 0, &ex) == ERROR_SUCCESS) {
            d.vendor = ex.vendor;
            d.product = ex.product;
        }
        out.devices.push_back(d);
    }
}

void read_raw(State &st, Snapshot &out)
{
    UINT n = 0;
    if (GetRawInputDeviceList(nullptr, &n, sizeof(RAWINPUTDEVICELIST)) != 0 || !n)
        return;
    std::vector<RAWINPUTDEVICELIST> list(n + 8);
    UINT cap = UINT(list.size());
    UINT got = GetRawInputDeviceList(list.data(), &cap, sizeof(RAWINPUTDEVICELIST));
    if (got == UINT(-1))
        return;
    for (UINT i = 0; i < got && i < list.size(); i++) {
        if (list[i].dwType == RIM_TYPEKEYBOARD) {
            out.keyboards++;
            continue;
        }
        if (list[i].dwType == RIM_TYPEMOUSE) {
            out.mice++;
            continue;
        }
        if (list[i].dwType != RIM_TYPEHID)
            continue;
        RID_DEVICE_INFO info{};
        info.cbSize = sizeof info;
        UINT size = sizeof info;
        if (GetRawInputDeviceInfoW(list[i].hDevice, RIDI_DEVICEINFO, &info, &size) == UINT(-1) || info.dwType != RIM_TYPEHID)
            continue;
        uint8_t kind = kind_of(info.hid.usUsagePage, info.hid.usUsage);
        if (!kind)
            continue;
        Device d;
        d.kind = kind;
        d.vendor = uint16_t(info.hid.dwVendorId);
        d.product = uint16_t(info.hid.dwProductId);
        d.usage_page = info.hid.usUsagePage;
        d.usage = info.hid.usUsage;
        UINT chars = 0;
        if (GetRawInputDeviceInfoW(list[i].hDevice, RIDI_DEVICENAME, nullptr, &chars) == 0 && chars > 0 && chars < 1024) {
            std::wstring path(chars, L'\0');
            UINT copied = GetRawInputDeviceInfoW(list[i].hDevice, RIDI_DEVICENAME, path.data(), &chars);
            if (copied != UINT(-1) && copied > 0) {
                path.resize(wcsnlen(path.c_str(), path.size()));
                if (xinput_path(narrow_path(path)))
                    d.flags |= SWOFTY_LAP_INPUT_XINPUT_HID;
                d.name = product_name(st, path);
            }
        }
        out.devices.push_back(d);
    }
}

struct Summary {
    size_t controllers = 0, xinput = 0, named = 0;
    uint32_t keyboards = 0, mice = 0;
};

Summary summary(const Snapshot &s)
{
    Summary out{s.devices.size(), 0, 0, s.keyboards, s.mice};
    for (const Device &d : s.devices) {
        out.xinput += d.kind == SWOFTY_LAP_INPUT_KIND_XINPUT;
        out.named += !d.name.empty();
    }
    return out;
}

void loop()
{
    State &st = state();
    if (HMODULE x = GetModuleHandleW(L"xinput1_4.dll"))
        st.caps_ex = reinterpret_cast<CapsExFn>(reinterpret_cast<void *>(GetProcAddress(x, MAKEINTRESOURCEA(108))));
    if (HMODULE h = LoadLibraryW(L"hid.dll"))
        st.product = reinterpret_cast<ProductFn>(reinterpret_cast<void *>(GetProcAddress(h, "HidD_GetProductString")));
    for (;;) {
        double began = now_s();
        Snapshot next;
        read_xinput(st, next, runtime::pad::stats().owners != 0);
        read_raw(st, next);
        order(next.devices);
        next.read = true;
        next.read_at_s = now_s();
        Summary sum = summary(next);
        bool log_it = false, changed = false;
        {
            std::lock_guard<std::mutex> g(st.lock);
            changed = st.logged && !same_controllers(st.last, next);
            log_it = !st.logged || changed;
            st.logged = true;
            st.last = std::move(next);
        }
        if (log_it)
            swofty::log("[input] %s the input devices: %zu game controller(s) (%zu XInput, %zu named), %u keyboard(s), %u "
                        "mouse/mice in %.0f ms",
                        changed ? "the game controllers changed; read" : "read", sum.controllers, sum.xinput, sum.named,
                        sum.keyboards, sum.mice, 1000.0 * (now_s() - began));
        std::unique_lock<std::mutex> g(st.lock);
        st.wake.wait_for(g, REFRESH);
        st.wake.wait(g, [&] { return now_s() - st.asked_s < WANTED_FOR_S; });
    }
}

}

int input(SwoftyLapInput *out)
{
    if (!out || out->size < sizeof(SwoftyLapInput))
        return 0;
    State &st = state();
    runtime::pad::Stats p = runtime::pad::stats();
    Pad pad{p.owners, p.seen, p.set, p.idle, p.real};
    double now = now_s();
    bool woke = false;
    int r;
    {
        std::lock_guard<std::mutex> g(st.lock);
        woke = now - st.asked_s >= WANTED_FOR_S;
        st.asked_s = now;
        r = fill(st.last, pad, now, out);
    }
    std::call_once(started, [] { std::thread(loop).detach(); });
    if (woke)
        st.wake.notify_one();
    return r;
}

}
