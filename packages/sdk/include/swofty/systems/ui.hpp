// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#include <map>
#include <string>
#include <string_view>
#include <vector>
#include <swofty/events.hpp>
#include <swofty/registry.hpp>

// UI and strings: texture slices, tiles, screens, routes, options, HUD widgets, one text key in every language,
// and the options an addon shows in ADDON SETTINGS. The records are declared and stored; a record whose registry
// has no applier yet is logged as `this core applies nothing from it`. The set_* operations and the
// events below return unavailable or never fire until the core implements them.
// The 1.x string library (swofty/lang.hpp) writes the calling addon's global layer of a lang.string
// record; the options of an addon's swofty.json are addon.option records under its id.

namespace swofty::ui {

using Result = swofty::Result;

struct String {
    static constexpr Version version{1, 0};
    std::map<std::string, std::string> text;
    json::Value within;
    json::Value extra;
};
SWOFTY_RECORD(String, text, within)
SWOFTY_RECORD_REGISTRY(String, "lang.string")
inline Registry<String> strings{"lang.string", {.applies = Applies::read}};

struct OptionState {
    std::string text;
    json::Value value;
};
SWOFTY_RECORD(OptionState, text, value)

struct AddonOption {
    static constexpr Version version{1, 0};
    std::string type;
    std::vector<OptionState> states;
    json::Value value;
    json::Value extra;
};
SWOFTY_RECORD(AddonOption, type, states, value)
SWOFTY_RECORD_REGISTRY(AddonOption, "addon.option")
inline Registry<AddonOption> addon_options{"addon.option", {.applies = Applies::read}};


struct TextureSlice {
    static constexpr Version version{1, 0};
    std::string array;
    std::string name;
    Path image;
    json::Value extra;
};
SWOFTY_RECORD(TextureSlice, array, name, image)
SWOFTY_RECORD_REGISTRY(TextureSlice, "ui.texture_slice")
inline Registry<TextureSlice> texture_slices{"ui.texture_slice", {.applies = Applies::boot}};

struct Screen {
    static constexpr Version version{1, 0};
    Path file;
    std::string flow;
    std::string anchor;
    std::string view_model;
    json::Value extra;
};
SWOFTY_RECORD(Screen, file, flow, anchor, view_model)
SWOFTY_RECORD_REGISTRY(Screen, "ui.screen")
inline Registry<Screen> screens{"ui.screen", {.applies = Applies::boot}};

struct Route {
    static constexpr Version version{1, 0};
    std::string file;
    std::string clone_of;
    Ref<Screen> opens;
    json::Value extra;
};
SWOFTY_RECORD(Route, file, clone_of, opens)
SWOFTY_RECORD_REGISTRY(Route, "ui.route")
inline Registry<Route> routes{"ui.route", {.applies = Applies::boot}};

enum class Place { home, career, f1_world, car_category };
SWOFTY_ENUM(Place, home, career, f1_world, car_category)

struct Tile {
    static constexpr Version version{1, 0};
    Place place = Place::home;
    Text title;
    Ref<TextureSlice> image;
    Ref<Route> route;
    json::Value gate;
    json::Value extra;
};
SWOFTY_RECORD(Tile, place, title, image, route, gate)
SWOFTY_RECORD_REGISTRY(Tile, "ui.tile")
inline Registry<Tile> tiles{"ui.tile", {.applies = Applies::read}};

struct Option {
    static constexpr Version version{1, 0};
    std::string id;
    std::vector<std::string> restriction_tags;
    json::Value extra;
};
SWOFTY_RECORD(Option, id, restriction_tags)
SWOFTY_RECORD_REGISTRY(Option, "ui.option")
inline Registry<Option> options{"ui.option", {.applies = Applies::boot}};

struct HudWidget {
    static constexpr Version version{1, 0};
    std::string view_model;
    Path tmpl;
    json::Value extra;
};
SWOFTY_RECORD(HudWidget, view_model, tmpl)
SWOFTY_RECORD_REGISTRY(HudWidget, "hud.widget")
inline Registry<HudWidget> hud_widgets{"hud.widget", {.applies = Applies::session}};

inline Result set_text(std::string_view, std::string_view) { return Result::unavailable; }
inline Result set_bool(std::string_view, bool) { return Result::unavailable; }
inline Result set_ref(std::string_view, std::string_view) { return Result::unavailable; }

}

namespace swofty::ui::events {

struct EraTiles {
    static constexpr const char *point = "ui.era.tiles";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *list;
};

struct VmResolve {
    static constexpr const char *point = "ui.vm.resolve";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    const char *path_str;
    void *node;

    std::string_view path() const { return swofty::detail::payload_str(path_str); }
};

struct LangLookup {
    static constexpr const char *point = "lang.lookup";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    const char *key_str;
    const char *language_str;
    const char *text_str;
    int32_t (*set_text_fn)(void *user, const char *text);
    void *user;

    std::string_view key() const { return swofty::detail::payload_str(key_str); }
    std::string_view language() const { return swofty::detail::payload_str(language_str); }
    std::string_view text() const { return swofty::detail::payload_str(text_str); }
    Result set_text(std::string_view text)
    {
        if (!set_text_fn)
            return Result::unavailable;
        std::string t(text);
        return swofty::detail::result(set_text_fn(user, t.c_str()));
    }
};

struct HomeTiles {
    static constexpr const char *point = "ui.home.tiles";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *list;
};

struct CareerContentTiles {
    static constexpr const char *point = "ui.career_content.tiles";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *list;
};

struct CarCategoryTiles {
    static constexpr const char *point = "ui.car_category.tiles";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *list;
};

struct TrackSelectBuild {
    static constexpr const char *point = "ui.track_select.build";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *list;
};

struct ScreenBuilt {
    static constexpr const char *point = "ui.screen.built";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    const char *screen_str;
    void *view;

    std::string_view screen() const { return swofty::detail::payload_str(screen_str); }
};

struct RouteOpen {
    static constexpr const char *point = "ui.route.open";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    const char *route_str;
    const char *screen_str;

    std::string_view route() const { return swofty::detail::payload_str(route_str); }
    std::string_view screen() const { return swofty::detail::payload_str(screen_str); }
};

struct MfdPages {
    static constexpr const char *point = "hud.mfd.pages";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *pages;
    int32_t count;
    int32_t capacity;
};

}
