// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - a new 1990 Driver Career opens with an intro video, in place of the agent's intro
// cutscene. Its own DLL.
//
// A new Driver Career reaches DriverCareerContractNegotiations Intro, the state that plays Elena's
// intro cutscene. In a 1990 career f1_1990.dll signs the first contract and passes that state at
// once (contract_skip.cpp, exported as F1990_ContractSceneSkipped). These wraps run outside its
// wraps: the Intro state's first update plays videos/f1_1990_intro.bik through the game's own
// fullscreen video player (Ne_Play_FullscreenVideo, layer 4, as the session intro videos) and
// posts AllowVideoSkip, so the stock skip prompt and input stop it after two seconds. The state
// returns the stay link until the player's end callback runs (the video ended or was skipped), or
// the video's length plus a margin has passed, then posts AllowVideoSkip off and lets
// contract_skip continue to the Driver HQ intro cutscene. The addon installs the video from
// carpack/video (carpack/tools/intro_video.sh encodes one with RAD Video Tools). The game plays .bik files as Bink 1. Without the file, the option or the contract skip,
// the state behaves as before.

#include <windows.h>

#include <atomic>
#include <chrono>
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <iterator>
#include <string>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "intro_video.hpp"

SWOFTY_SETTING(bool, intro_video_1990, true);

namespace {

using swofty::game::NeStr;
namespace iv = f1990::intro;

constexpr char kVideoFile[] = "f1_1990_intro.bik";
constexpr int kVideoLayer = 4;

struct Candidate {
    const wchar_t *disk;
    const char *game;
};

constexpr Candidate kCandidates[] = {
    {L"videos\\f1_1990_intro.bik", "videos/f1_1990_intro.bik"},
    {L"swofty\\overlay\\videos\\f1_1990_intro.bik", "videos/../swofty/overlay/videos/f1_1990_intro.bik"},
};

enum class Phase { idle, armed, playing };

struct EndCallback {
    void (*fn)(void *);
    void *ctx;
};

struct BusMessage {
    void *const *vtbl;
    uint64_t reserved;
    uint8_t value;
    uint8_t pad[7];
};

Phase g_phase = Phase::idle;
std::atomic<bool> g_ended{false};
std::chrono::steady_clock::time_point g_started;
iv::BinkInfo g_info{};
std::atomic<uint32_t> g_missing_logged;

template <class T> T at(const void *p, size_t off)
{
    T v;
    std::memcpy(&v, static_cast<const uint8_t *>(p) + off, sizeof v);
    return v;
}

void on_end(void *)
{
    g_ended.store(true);
}

void noop(void *)
{
}

const char *allow_video_skip_name(const void *)
{
    return "AllowVideoSkip";
}

void *const kAllowVideoSkipVt[] = {
    reinterpret_cast<void *>(&noop),
    reinterpret_cast<void *>(&allow_video_skip_name),
    reinterpret_cast<void *>(&noop),
    reinterpret_cast<void *>(&noop),
};

// AllowVideoSkip has no message class in the exe (UI data posts it by name), so the message's vtable
// lives in this DLL. The core's swofty::career::post only takes messages whose vtable is in the exe
// image and answers invalid for this one, so it goes to Ne_Post_BusMessage on the live bus directly,
// as the game's own flow states post theirs.
swofty::game::NeMessageBus *active_bus()
{
    namespace g = swofty::game;
    uintptr_t holder = 0, bus = 0;
    uint8_t active = 0;
    if (!swofty::mem::read(g::g_Game_MessageBus.address(), holder) || !holder ||
        !swofty::mem::read(holder + offsetof(g::GameMessageBusHolder, bus), bus) || !swofty::mem::object_like(bus) ||
        !swofty::mem::read(bus + offsetof(g::NeMessageBus, active), active) || !active)
        return nullptr;
    return reinterpret_cast<g::NeMessageBus *>(bus);
}

void allow_skip(bool on)
{
    auto *bus = active_bus();
    if (!bus) {
        swofty::log("[f1-1990] intro: AllowVideoSkip %s not posted: the game's message bus is not active", on ? "on" : "off");
        return;
    }
    BusMessage msg{kAllowVideoSkipVt, 0, uint8_t(on ? 1 : 0), {}};
    swofty::game::Ne_Post_BusMessage(bus, &msg);
    swofty::log("[f1-1990] intro: AllowVideoSkip %s posted on bus %p", on ? "on" : "off", static_cast<void *>(bus));
}

bool contract_scene_skipped()
{
    static auto fn = [] {
        HMODULE main = GetModuleHandleW(L"f1_1990.dll");
        auto f = main ? reinterpret_cast<int (*)()>(GetProcAddress(main, "F1990_ContractSceneSkipped")) : nullptr;
        if (!f)
            swofty::log("[f1-1990] intro: f1_1990.dll has no F1990_ContractSceneSkipped; no intro video");
        return f;
    }();
    return fn && fn();
}

std::wstring game_dir()
{
    wchar_t buf[MAX_PATH * 2];
    DWORD n = GetModuleFileNameW(nullptr, buf, DWORD(std::size(buf)));
    if (!n || n >= std::size(buf))
        return {};
    std::wstring path(buf, n);
    size_t slash = path.find_last_of(L"\\/");
    return slash == std::wstring::npos ? std::wstring() : path.substr(0, slash + 1);
}

bool read_header(const std::wstring &path, uint8_t (&out)[iv::kBinkHeaderSize])
{
    HANDLE f = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return false;
    DWORD got = 0;
    bool ok = ReadFile(f, out, DWORD(sizeof out), &got, nullptr) && got == sizeof out;
    CloseHandle(f);
    return ok;
}

const Candidate *find_video(iv::BinkInfo &info)
{
    std::wstring dir = game_dir();
    for (const auto &c : kCandidates) {
        uint8_t head[iv::kBinkHeaderSize];
        if (!read_header(dir + c.disk, head))
            continue;
        auto parsed = iv::parse_header(head, sizeof head);
        switch (iv::check(parsed, 1)) {
        case iv::Verdict::play:
            info = *parsed;
            return &c;
        case iv::Verdict::not_bink:
            swofty::log("[f1-1990] intro: %s is not a Bink file; no intro video", c.game);
            return nullptr;
        case iv::Verdict::wrong_version:
            swofty::log("[f1-1990] intro: %s is a Bink 2 file; the game plays a .bik as Bink 1, so no intro video", c.game);
            return nullptr;
        case iv::Verdict::too_long:
            swofty::log("[f1-1990] intro: %s runs %.0f s, over the %.0f s cap; no intro video", c.game,
                        iv::seconds(*parsed), iv::kMaxSeconds);
            return nullptr;
        }
    }
    if (g_missing_logged.fetch_add(1) == 0)
        swofty::log("[f1-1990] intro: no videos/%s (the addon installs it from carpack/video); the career starts without "
                    "the intro video",
                    kVideoFile);
    return nullptr;
}

void *feature_system()
{
    auto *root = *swofty::game::g_Game_Root;
    auto *ctx = root ? at<uint8_t *>(root, 0xa0) : nullptr;
    auto *module = ctx ? at<uint8_t *>(ctx, 0x28) : nullptr;
    if (!module)
        return nullptr;
    auto *iface = module + 0x10;
    auto **vt = *reinterpret_cast<void ***>(iface);
    return vt ? reinterpret_cast<void *(*)(void *)>(vt[0xf0 / 8])(iface) : nullptr;
}

bool start()
{
    const Candidate *c = find_video(g_info);
    if (!c)
        return false;
    void *fs = feature_system();
    void *player = fs ? at<void *>(fs, 0x108) : nullptr;
    if (!player) {
        swofty::log("[f1-1990] intro: no fullscreen video player (feature system %p); no intro video", fs);
        return false;
    }
    NeStr *path = nullptr;
    swofty::game::Ne_Make_StrFromCStr(&path, c->game, 0, 0);
    if (!path)
        return false;
    g_ended.store(false);
    EndCallback cb{&on_end, nullptr};
    swofty::game::Ne_Play_FullscreenVideo(fs, &path, &cb, nullptr, kVideoLayer);
    swofty::game::Ne_Make_StrFromCStr(&path, nullptr, 0, 0);
    g_started = std::chrono::steady_clock::now();
    swofty::log("[f1-1990] intro: new 1990 career; playing %s (Bink 1 rev %c, %ux%u, %u frames at %.2f fps, %.1f s) on "
                "layer %d (feature system %p, player %p)",
                c->game, g_info.revision, g_info.width, g_info.height, g_info.frames, iv::fps(g_info),
                iv::seconds(g_info), kVideoLayer, fs, player);
    allow_skip(true);
    return true;
}

void finish(iv::Step how, double elapsed)
{
    allow_skip(false);
    const char *why = how == iv::Step::timed_out           ? "no end callback; timed out"
                      : iv::ended_early(elapsed, g_info) ? "skipped"
                                                         : "ended";
    swofty::log("[f1-1990] intro: video %s after %.1f s; the career continues", why, elapsed);
}

}

SWOFTY_WRAP_PRIORITY(1, Career_ContractIntro_OnEnter, uint64_t, void *state)
{
    g_phase = Phase::idle;
    if (intro_video_1990 && contract_scene_skipped())
        g_phase = Phase::armed;
    return orig(state);
}

SWOFTY_WRAP_PRIORITY(1, Career_ContractIntro_OnUpdate, uint32_t *, void *state, uint32_t *link)
{
    if (g_phase == Phase::armed)
        g_phase = start() ? Phase::playing : Phase::idle;
    if (g_phase == Phase::playing) {
        double elapsed = std::chrono::duration<double>(std::chrono::steady_clock::now() - g_started).count();
        iv::Step s = iv::step(g_ended.load(), elapsed, iv::timeout(g_info));
        if (s == iv::Step::stay) {
            *link = *swofty::game::g_Game_FlowLinkStay;
            return link;
        }
        finish(s, elapsed);
        g_phase = Phase::idle;
    }
    return orig(state, link);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] intro: intro video %s (videos/%s, played in a new 1990 career's contract intro state)",
                intro_video_1990 ? "on" : "off", kVideoFile);
}

SWOFTY_ADDON()
