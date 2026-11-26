#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <memory>
#include <mutex>
#include <string>

#include "core/hooks/hooks.hpp"
#include "core/hooks/track_select_point.hpp"
#include "core/hooks/ui_points.hpp"

// ui.track_select.build fires after the game rebuilds a track select's list, not from Game_Read_TrackInfoConfig: that
// function (0x14309e400) publishes track_info.gm_track_list for the network and featured-event UI (callers
// Game_NetworkUi_PopulateUIData and Game_Network_SetupFromFeaturedEvent), while the Grand Prix, championship and
// championship settings screens read #track_data.track_list. So the point wraps the two refreshes that consume it:
// Trk_GrandPrixTrackSelectVm_RefreshTrackList 0x141c05600 and Trk_SettingsSelectTrackTabVm_RefreshTrackList 0x141de65e0.
// The payload's list is that screen's view model. Time Trial's carousel list builder is not found, so it never fires.

namespace swofty_core::hooks {
namespace {

namespace ev = swofty::ui::events;

const runtime::Sig S_GP{
    "Trk_GrandPrixTrackSelectVm_RefreshTrackList",
    "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC B8 00 00 00 C5 F8 29 B4 24 A0 00 00 00 48 89 CB 48 C7 81 28 09 "
    "00 00 01",
    0x1c05600};
const runtime::Sig S_SETTINGS{
    "Trk_SettingsSelectTrackTabVm_RefreshTrackList",
    "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC A8 00 00 00 C5 F8 29 B4 24 90 00 00 00 48 89 CE C7 81 "
    "04 0B 00 00 01",
    0x1de65e0};

constexpr int LOGGED = 6;

using Refresh_t = void(void *vm);

struct Hooks {
    safetyhook::InlineHook gp, settings;
    bool live = false;
};

Hooks *g_hooks = nullptr;
std::atomic<int> g_told{0};

void announce(void *vm, const char *screen)
{
    if (g_told.fetch_add(1) < LOGGED)
        swofty::log("[hooks] ui.track_select.build: fired after the %s track list refresh (vm %p)", screen, vm);
    SwoftyTrackList none{sizeof(SwoftyTrackList), SWOFTY_TRACK_LIST_VERSION, nullptr, nullptr, nullptr, nullptr, nullptr};
    ui_points::fire_tracks(core(), vm, &none);
}

void d_gp(void *vm)
{
    g_hooks->gp.call<void>(vm);
    if (g_hooks->live)
        announce(vm, "Grand Prix");
}

void d_settings(void *vm)
{
    g_hooks->settings.call<void>(vm);
    if (g_hooks->live)
        announce(vm, "championship settings");
}

class TrackSelectLink final : public GameLink {
public:
    bool install(bool late, const std::string &subscribers) override
    {
        std::lock_guard<std::mutex> g(lock_);
        Hooks &h = *g_hooks;
        if (h.live)
            return true;
        std::string live;
        if (!h.gp) {
            uintptr_t at = runtime::find(S_GP, "ui.track_select.build", "hooks");
            if (at)
                if (auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(&d_gp)))
                    h.gp = std::move(*r);
        }
        if (!h.settings) {
            uintptr_t at = runtime::find(S_SETTINGS, "ui.track_select.build", "hooks");
            if (at)
                if (auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(&d_settings)))
                    h.settings = std::move(*r);
        }
        if (h.gp)
            live += "Trk_GrandPrixTrackSelectVm_RefreshTrackList";
        if (h.settings)
            live += std::string(live.empty() ? "" : " and ") + "Trk_SettingsSelectTrackTabVm_RefreshTrackList";
        if (live.empty()) {
            swofty::log("[hooks] ui.track_select.build unavailable: neither track list refresh resolves or hooks on this build; "
                        "subscribers %s not run",
                        subscribers.c_str());
            return false;
        }
        h.live = true;
        swofty::log("[hooks] ui.track_select.build live on %s%s: subscribers %s; Time Trial is not covered (its list builder is "
                    "not found)",
                    live.c_str(), late ? " (installed late)" : " (window stage)", subscribers.c_str());
        return true;
    }

    void uninstall() override
    {
        std::lock_guard<std::mutex> g(lock_);
        if (!g_hooks->live)
            return;
        g_hooks->live = false;
        swofty::log("[hooks] ui.track_select.build has no subscriber left; the detours stay and fire nothing");
    }

    bool installed() const override { return g_hooks->live; }

private:
    std::mutex lock_;
};

}

void declare_track_select_point(Table &t)
{
    g_hooks = new Hooks;
    auto *link = new TrackSelectLink;
    t.declare_game(ui_points::TRACK_SELECT_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::TrackSelectBuild::version, link);
}

}
