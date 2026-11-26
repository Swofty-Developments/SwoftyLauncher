#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <memory>
#include <mutex>
#include <string>

#include "core/hooks/game_point.hpp"
#include "core/hooks/hooks.hpp"
#include "core/hooks/track_rows.hpp"
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

constexpr const char *GP = "Trk_GrandPrixTrackSelectVm_RefreshTrackList";
constexpr const char *CS = "Trk_SettingsSelectTrackTabVm_RefreshTrackList";

constexpr int LOGGED = 6;

using Refresh_t = void(void *vm);

std::unique_ptr<GamePoint> g_gp;
std::unique_ptr<GamePoint> g_cs;
std::atomic<int> g_told{0};

void announce(void *vm, const char *screen, track_rows::Screen kind)
{
    if (g_told.fetch_add(1) < LOGGED)
        swofty::log("[hooks] ui.track_select.build: fired after the %s track list refresh (vm %p, %d rows)", screen, vm,
                    track_rows::rows_of(vm, kind));
    track_rows::Table rows(vm, kind);
    ui_points::fire_tracks(core(), vm, rows.table());
}

void d_gp(void *vm)
{
    g_gp->original<void>(vm);
    announce(vm, "Grand Prix", track_rows::Screen::grand_prix);
}

void d_cs(void *vm)
{
    g_cs->original<void>(vm);
    announce(vm, "championship settings", track_rows::Screen::settings);
}

class TrackSelectLink final : public GameLink {
public:
    bool install(bool late, const std::string &subscribers) override
    {
        std::lock_guard<std::mutex> g(lock_);
        bool gp = g_gp->install(late, subscribers);
        bool settings = g_cs->install(late, subscribers);
        if (!gp && !settings) {
            swofty::log("[hooks] ui.track_select.build unavailable: neither track list refresh resolves or hooks on this build; "
                        "subscribers %s not run",
                        subscribers.c_str());
            return false;
        }
        swofty::log("[hooks] ui.track_select.build live: Grand Prix %s, championship settings %s; Time Trial is not covered "
                    "(its list builder is not found)",
                    gp ? "yes" : "no", settings ? "yes" : "no");
        return true;
    }

    void uninstall() override
    {
        std::lock_guard<std::mutex> g(lock_);
        g_gp->uninstall();
        g_cs->uninstall();
    }

    bool installed() const override { return g_gp->installed() || g_cs->installed(); }

private:
    std::mutex lock_;
};

}

void declare_track_select_point(Table &t)
{
    g_gp = std::make_unique<GamePoint>(ui_points::TRACK_SELECT_POINT, GP, reinterpret_cast<void *>(&d_gp), game_ops());
    g_cs = std::make_unique<GamePoint>(ui_points::TRACK_SELECT_POINT, CS, reinterpret_cast<void *>(&d_cs), game_ops());
    auto *link = new TrackSelectLink;
    t.declare_game(ui_points::TRACK_SELECT_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::TrackSelectBuild::version, link);
}

}
