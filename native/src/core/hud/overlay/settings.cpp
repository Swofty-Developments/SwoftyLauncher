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

int forced(const char *env, const char *key)
{
    int f = -1;
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA(env, v, sizeof v);
    const json::Value &j = runtime::config()[key];
    if (n && n < sizeof v)
        f = v[0] != '0' && v[0] != '\0';
    else if (j.type == json::Value::Bool)
        f = j.b;
    return f;
}

}

Settings Settings::read()
{
    Settings out;
    out.gate = runtime::enabled("overlay", nullptr);
    for (auto &x : runtime::config()["overlay_off"].a)
        if (x.type == json::Value::Str)
            out.off.push_back(x.s);
    char kill[256] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_OVERLAY_KILL", kill, sizeof kill);
    out.kill = n && n < sizeof kill ? std::string(kill, n) : std::string();
    for (auto &x : runtime::config()["overlay_kill"].a)
        if (x.type == json::Value::Str)
            out.kill += "," + x.s;
    out.before_fg_forced = forced("SWOFTY_OVERLAY_FG", "overlay_fg");
    out.rival_forced = forced("SWOFTY_OVERLAY_RIVAL", "overlay_rival");
    char nits[32] = {};
    n = GetEnvironmentVariableA("SWOFTY_OVERLAY_NITS", nits, sizeof nits);
    float f = n && n < sizeof nits ? float(atof(nits)) : 0;
    out.paper_white = f >= 80 && f <= 1000 ? f : 350.0f;
    return out;
}

bool Settings::killed(const char *what) const { return kill_has(kill, "all") || kill_has(kill, what); }

const Settings &settings()
{
    static const Settings read = Settings::read();
    return read;
}

bool before_fg_wanted()
{
    int forced = settings().before_fg_forced;
    std::wstring path = forced < 0 ? hud::hardware_settings::path() : std::wstring();
    hud::arm_policy::FgSetting game;
    if (!path.empty())
        game = hud::hardware_settings::frame_gen(path);
    bool want = hud::arm_policy::before_fg_wanted(forced, game);
    if (forced >= 0)
        swofty::log("[overlay] at start SWOFTY_OVERLAY_FG / \"overlay_fg\" is %d, so the game's frame generation "
                    "setting is not read; drawing before frame generation is %s",
                    forced, want ? "on" : "off");
    else
        swofty::log("[overlay] at start the game's hardware settings (%s) have frame generation %s (frame_gen mode "
                    "%d, multi_frame_gen %d); drawing before frame generation is %s",
                    path.empty() ? "path unknown" : runtime::narrow(path).c_str(), game.word(), game.mode, game.multi,
                    want ? "on" : "off");
    return want;
}

std::string device_hook_rival()
{
    for (auto name : hud::arm_policy::DEVICE_HOOK_RIVALS)
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
    if (!poll.due(now))
        return !setting.on();
    const std::wstring &path = poll.file_path();
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (path.empty() || !GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &fa)) {
        setting = {};
    } else if (CompareFileTime(&fa.ftLastWriteTime, &at) || !told) {
        at = fa.ftLastWriteTime;
        setting = hud::hardware_settings::frame_gen(path);
    }
    bool off = !setting.on();
    if (!told) {
        told = true;
        swofty::log("[overlay] the game's hardware settings (%s) have frame generation %s (frame_gen mode %d, "
                    "multi_frame_gen %d)",
                    runtime::narrow(path).c_str(),
                    !setting.known ? "unknown"
                    : off          ? "off"
                                   : "on",
                    setting.mode, setting.multi);
    } else if (!off) {
        swofty::log("[overlay] the game's hardware settings now have frame generation %s (frame_gen mode %d); the "
                    "overlay stops drawing for this session",
                    setting.known ? "on" : "unknown", setting.mode);
    }
    if (!off)
        tripped = true;
    return off;
}

int HdrWatch::mode(ULONGLONG now)
{
    if (!poll.due(now))
        return value;
    const std::wstring &path = poll.file_path();
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (!path.empty() && GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &fa) &&
        CompareFileTime(&fa.ftLastWriteTime, &at)) {
        at = fa.ftLastWriteTime;
        value = hud::arm_policy::hdr_mode(runtime::read_file(path, 1 << 20));
    }
    return value;
}

UINT OutputMode::pick(IDXGISwapChain3 *sc, DXGI_FORMAT fmt, UINT color_space)
{
    int game = color_space == hud::arm_policy::COLOR_SPACE_UNSET && fmt == DXGI_FORMAT_R10G10B10A2_UNORM
                   ? hdr.mode(GetTickCount64())
                   : -1;
    auto out = hud::arm_policy::output_for(int(fmt), color_space, game);
    if (told_sc != sc || told_out != out) {
        told_sc = sc;
        told_out = out;
        static constexpr const char *names[] = {"SDR", "HDR10 (BT.2020 PQ)", "scRGB (linear)"};
        char why[96];
        if (color_space != hud::arm_policy::COLOR_SPACE_UNSET)
            snprintf(why, sizeof why, "colour space %u from SetColorSpace1", color_space);
        else if (game >= 0)
            snprintf(why, sizeof why, "no SetColorSpace1 seen; the game's hdr mode is %d", game);
        else
            snprintf(why, sizeof why, "no SetColorSpace1 seen");
        swofty::log("[overlay] swapchain %p output: %s, paper white %.0f nits (format %d, %s)", static_cast<void *>(sc),
                    names[out], double(settings().paper_white), int(fmt), why);
    }
    return out;
}

}
