#include "core/hud/overlay/settings.hpp"

#include "core/hud/hardware_settings.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <tlhelp32.h>

#include <cstdio>
#include <cstdlib>

namespace hud::overlay {

namespace {

bool kill_has(const std::string &list, const char *what)
{
    std::string l = "," + runtime::lower(list) + ",";
    for (auto &c : l)
        if (c == ' ' || c == ';')
            c = ',';
    return l.find(std::string(",") + what + ",") != std::string::npos;
}

int game_hdr_mode()
{
    static std::wstring path;
    static FILETIME at{};
    static ULONGLONG checked;
    static int mode = -1;
    ULONGLONG now = GetTickCount64();
    if (checked && now - checked < 2000)
        return mode;
    checked = now;
    if (path.empty())
        path = hardware_settings::path();
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (!path.empty() && GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &fa) &&
        CompareFileTime(&fa.ftLastWriteTime, &at)) {
        at = fa.ftLastWriteTime;
        mode = hud::guard::hdr_mode(runtime::read_file(path, 1 << 20));
    }
    return mode;
}

}

bool gate_on()
{
    static const bool on = runtime::enabled("overlay", nullptr);
    return on;
}

const std::vector<std::string> &overlay_off()
{
    static const std::vector<std::string> ids = [] {
        std::vector<std::string> out;
        for (auto &x : runtime::config()["overlay_off"].a)
            if (x.type == json::Value::Str)
                out.push_back(x.s);
        return out;
    }();
    return ids;
}

bool killed(const char *what)
{
    static std::string list = [] {
        char v[256] = {};
        DWORD n = GetEnvironmentVariableA("SWOFTY_OVERLAY_KILL", v, sizeof v);
        std::string out = n && n < sizeof v ? std::string(v, n) : std::string();
        for (auto &x : runtime::config()["overlay_kill"].a)
            if (x.type == json::Value::Str)
                out += "," + x.s;
        return out;
    }();
    return kill_has(list, "all") || kill_has(list, what);
}

int before_fg_forced()
{
    static const int forced = [] {
        int f = -1;
        char v[8] = {};
        DWORD n = GetEnvironmentVariableA("SWOFTY_OVERLAY_FG", v, sizeof v);
        const json::Value &j = runtime::config()["overlay_fg"];
        if (n && n < sizeof v)
            f = v[0] != '0' && v[0] != '\0';
        else if (j.type == json::Value::Bool)
            f = j.b;
        return f;
    }();
    return forced;
}

int rival_forced()
{
    static const int forced = [] {
        int f = -1;
        char v[8] = {};
        DWORD n = GetEnvironmentVariableA("SWOFTY_OVERLAY_RIVAL", v, sizeof v);
        const json::Value &j = runtime::config()["overlay_rival"];
        if (n && n < sizeof v)
            f = v[0] != '0' && v[0] != '\0';
        else if (j.type == json::Value::Bool)
            f = j.b;
        return f;
    }();
    return forced;
}

bool before_fg_wanted()
{
    static const bool on = [] {
        int forced = before_fg_forced();
        std::wstring path = forced < 0 ? hardware_settings::path() : std::wstring();
        hud::guard::FgSetting game;
        if (!path.empty())
            game = hardware_settings::frame_gen(path);
        bool want = hud::guard::before_fg_wanted(forced, game);
        if (forced >= 0)
            swofty::log("[overlay] at start SWOFTY_OVERLAY_FG / \"overlay_fg\" is %d, so the game's frame generation "
                        "setting is not read; drawing before frame generation is %s",
                        forced, want ? "on" : "off");
        else
            swofty::log("[overlay] at start the game's hardware settings (%s) have frame generation %s (frame_gen mode "
                        "%d, multi_frame_gen %d); drawing before frame generation is %s",
                        path.empty() ? "path unknown" : runtime::narrow(path).c_str(), game.word(), game.mode,
                        game.multi, want ? "on" : "off");
        return want;
    }();
    return on;
}

std::string device_hook_rival()
{
    for (auto name : hud::guard::DEVICE_HOOK_RIVALS)
        if (GetModuleHandleA(std::string(name).c_str()))
            return std::string(name);
    if (HANDLE rtss = OpenFileMappingW(FILE_MAP_READ, FALSE, L"RTSSSharedMemoryV2")) {
        CloseHandle(rtss);
        return "RTSSHooks64.dll (RivaTuner Statistics Server is running)";
    }
    return {};
}

std::vector<std::string> loaded_modules()
{
    std::vector<std::string> mods;
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, GetCurrentProcessId());
    if (snap == INVALID_HANDLE_VALUE)
        return mods;
    MODULEENTRY32W me{};
    me.dwSize = sizeof me;
    for (BOOL ok = Module32FirstW(snap, &me); ok; ok = Module32NextW(snap, &me)) {
        char name[MAX_PATH];
        snprintf(name, sizeof name, "%ls", me.szModule);
        mods.push_back(name);
    }
    CloseHandle(snap);
    return mods;
}

bool FgWatch::frame_gen_off(ULONGLONG now)
{
    if (tripped)
        return false;
    if (checked && now - checked < 2000)
        return !setting.on();
    checked = now;
    if (path.empty())
        path = hardware_settings::path();
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (path.empty() || !GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &fa)) {
        setting = {};
    } else if (CompareFileTime(&fa.ftLastWriteTime, &at) || !told) {
        at = fa.ftLastWriteTime;
        setting = hardware_settings::frame_gen(path);
    }
    bool off = !setting.on();
    if (!told) {
        told = true;
        swofty::log("[overlay] the game's hardware settings (%s) have frame generation %s (frame_gen mode %d, "
                    "multi_frame_gen %d)",
                    runtime::narrow(path).c_str(), !setting.known ? "unknown" : off ? "off" : "on", setting.mode,
                    setting.multi);
    } else if (!off) {
        swofty::log("[overlay] the game's hardware settings now have frame generation %s (frame_gen mode %d); the "
                    "overlay stops drawing for this session",
                    setting.known ? "on" : "unknown", setting.mode);
    }
    if (!off)
        tripped = true;
    return off;
}

float paper_white()
{
    static float nits = [] {
        char v[32] = {};
        DWORD n = GetEnvironmentVariableA("SWOFTY_OVERLAY_NITS", v, sizeof v);
        float f = n && n < sizeof v ? float(atof(v)) : 0;
        return f >= 80 && f <= 1000 ? f : 350.0f;
    }();
    return nits;
}

UINT output_mode(IDXGISwapChain3 *sc, DXGI_FORMAT fmt, UINT color_space)
{
    int game = color_space == hud::guard::COLOR_SPACE_UNSET && fmt == DXGI_FORMAT_R10G10B10A2_UNORM ? game_hdr_mode() : -1;
    auto out = hud::guard::output_for(int(fmt), color_space, game);
    static void *told_sc;
    static unsigned told_out = ~0u;
    if (told_sc != sc || told_out != out) {
        told_sc = sc;
        told_out = out;
        static const char *names[] = {"SDR", "HDR10 (BT.2020 PQ)", "scRGB (linear)"};
        char why[96];
        if (color_space != hud::guard::COLOR_SPACE_UNSET)
            snprintf(why, sizeof why, "colour space %u from SetColorSpace1", color_space);
        else if (game >= 0)
            snprintf(why, sizeof why, "no SetColorSpace1 seen; the game's hdr mode is %d", game);
        else
            snprintf(why, sizeof why, "no SetColorSpace1 seen");
        swofty::log("[overlay] swapchain %p output: %s, paper white %.0f nits (format %d, %s)",
                    static_cast<void *>(sc), names[out], double(paper_white()), int(fmt), why);
    }
    return out;
}

}
