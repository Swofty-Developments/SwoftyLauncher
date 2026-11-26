#pragma once
#include <functional>
#include <string>

#include <swofty/systems/ui.hpp>

#include "core/hooks/dispatch.hpp"

namespace swofty_core::hooks::ui_points {

namespace ev = swofty::ui::events;

inline constexpr const char *ERA_POINT = "ui.era.tiles";
inline constexpr const char *HOME_POINT = "ui.home.tiles";
inline constexpr const char *CAREER_CONTENT_POINT = "ui.career_content.tiles";
inline constexpr const char *CAR_CATEGORY_POINT = "ui.car_category.tiles";
inline constexpr const char *TRACK_SELECT_POINT = "ui.track_select.build";
inline constexpr const char *SCREEN_POINT = "ui.screen.built";
inline constexpr const char *ROUTE_POINT = "ui.route.open";

class SiteLink final : public GameLink {
public:
    SiteLink(std::string point, std::string site, std::function<bool()> site_live, std::function<void(const std::string &)> log)
        : point_(std::move(point)), site_(std::move(site)), site_live_(std::move(site_live)), log_(std::move(log))
    {
    }

    bool install(bool late, const std::string &subscribers) override
    {
        if (active_)
            return true;
        active_ = true;
        bool live = !site_live_ || site_live_();
        say("[hooks] " + point_ + " live on " + site_ + (late ? " (installed late)" : " (window stage)") + ": subscribers " +
            subscribers + (live ? "" : "; the site is not installed yet, so nothing fires until it is"));
        return true;
    }

    void uninstall() override
    {
        if (!active_)
            return;
        active_ = false;
        say("[hooks] " + point_ + " has no subscriber left; " + site_ + " fires nothing for it");
    }

    bool installed() const override { return active_; }

private:
    void say(const std::string &line)
    {
        if (log_)
            log_(line);
    }

    std::string point_, site_;
    std::function<bool()> site_live_;
    std::function<void(const std::string &)> log_;
    bool active_ = false;
};

template <class E> void fire_list(Table &t, void *list, const SwoftyTileList *tiles = nullptr)
{
    E e{};
    e.size = sizeof e;
    e.payload_version = E::version;
    e.list = list;
    e.tiles_table = tiles;
    int replaced = 0;
    t.fire("", E::point, &e, &replaced);
}

inline void fire_tracks(Table &t, void *vm, const SwoftyTrackList *tracks = nullptr)
{
    ev::TrackSelectBuild e{};
    e.size = sizeof e;
    e.payload_version = ev::TrackSelectBuild::version;
    e.list = vm;
    e.tracks_table = tracks;
    int replaced = 0;
    t.fire("", ev::TrackSelectBuild::point, &e, &replaced);
}

inline void fire_screen(Table &t, const char *screen, void *view)
{
    ev::ScreenBuilt e{};
    e.size = sizeof e;
    e.payload_version = ev::ScreenBuilt::version;
    e.screen_str = screen ? screen : "";
    e.view = view;
    int replaced = 0;
    t.fire("", ev::ScreenBuilt::point, &e, &replaced);
}

inline bool fire_route(Table &t, const char *route, const char *screen)
{
    ev::RouteOpen e{};
    e.size = sizeof e;
    e.payload_version = ev::RouteOpen::version;
    e.route_str = route ? route : "";
    e.screen_str = screen ? screen : "";
    int replaced = 0;
    t.fire("", ev::RouteOpen::point, &e, &replaced);
    return replaced == 0;
}

void era_tiles(void *list, const SwoftyTileList *tiles);
void home_tiles(void *list, void *chosen);
void career_content_tiles(void *chosen);
void car_category_tiles(void *chosen);
// `root` is the screen's root node name; `registered` the name an addon registered the screen under (null: a stock screen).
void screen_built(const char *root, const char *registered = nullptr);
bool route_open(const char *route);
bool wants(const char *point);

}

namespace swofty_core::hooks {
void declare_ui_points(Table &t);
}
