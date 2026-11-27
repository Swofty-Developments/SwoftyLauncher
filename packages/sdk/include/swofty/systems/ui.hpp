// SPDX-License-Identifier: MIT
#pragma once
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <string_view>
#include <vector>
#include <swofty/events.hpp>
#include <swofty/registry.hpp>
#include <swofty/mixin.hpp>
#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

// UI and strings: texture slices, tiles, screens, routes, options, HUD widgets, one text key in every language,
// and the options an addon shows in ADDON SETTINGS. set_text, set_bool and set_ref write a view-model leaf through
// the core's SwoftyUi table (queued, applied on the flow thread) and return unavailable on a core without it.
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
    LooseRef<Screen> opens;
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
    LooseRef<TextureSlice> image;
    LooseRef<Route> route;
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

namespace detail {

inline const SwoftyUi *&table_override()
{
    static const SwoftyUi *t = nullptr;
    return t;
}

inline const SwoftyUi *table()
{
    if (auto *o = table_override())
        return o;
#if defined(_WIN32)
    static std::atomic<const SwoftyUi *> found{nullptr};
    if (auto t = found.load())
        return t;
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    auto get = core ? reinterpret_cast<SwoftyCoreUiFn>(GetProcAddress(core, "SwoftyCoreUi")) : nullptr;
    const SwoftyUi *t = get ? get() : nullptr;
    if (t && t->version < 1)
        t = nullptr;
    found = t;
    return t;
#else
    return nullptr;
#endif
}

inline Result ui_result(int r)
{
    return r == SWOFTY_ASSET_OK ? Result::ok
           : r == SWOFTY_ASSET_LATE ? Result::late
           : r == SWOFTY_ASSET_UNAVAILABLE ? Result::unavailable
                                           : Result::invalid;
}

}

inline Result set_text(std::string_view path, std::string_view text)
{
    auto t = detail::table();
    if (!t || !t->set_text)
        return Result::unavailable;
    return detail::ui_result(t->set_text(std::string(path).c_str(), std::string(text).c_str()));
}

inline Result set_bool(std::string_view path, bool value)
{
    auto t = detail::table();
    if (!t || !t->set_bool)
        return Result::unavailable;
    return detail::ui_result(t->set_bool(std::string(path).c_str(), value ? 1 : 0));
}

inline Result set_ref(std::string_view path, std::string_view target)
{
    auto t = detail::table();
    if (!t || !t->set_ref)
        return Result::unavailable;
    return detail::ui_result(t->set_ref(std::string(path).c_str(), std::string(target).c_str()));
}

struct TileItem {
    std::string_view id, title, route, image, place;
    bool literal = false;
};

struct TileSpec {
    std::string_view id, title, route, image;
    bool literal = false;
};

// The list of a ui tile event (payload version 2): the tiles of the screen's place the core is about to add, which a
// handler reads, drops, reorders and extends before they are added. A function the core does not implement returns
// Result::unavailable (count 0, get nullopt). The views a get or an iteration returns point into the core and are
// valid only during the handler call and until the next edit of the list: copy what you keep.
class TileList {
public:
    TileList() = default;
    explicit TileList(const SwoftyTileList *t)
        : t_(t && t->version >= 1 && t->size >= sizeof(SwoftyTileList) ? t : nullptr)
    {
    }

    explicit operator bool() const { return t_ != nullptr; }

    size_t count() const
    {
        if (!t_ || !t_->count)
            return 0;
        int n = t_->count(t_->self);
        return n > 0 ? size_t(n) : 0;
    }

    std::optional<TileItem> get(size_t index) const
    {
        if (!t_ || !t_->get || index > size_t(INT32_MAX))
            return std::nullopt;
        SwoftyTileView v{};
        if (t_->get(t_->self, int(index), &v) != SWOFTY_ASSET_OK)
            return std::nullopt;
        return TileItem{swofty::detail::payload_str(v.id), swofty::detail::payload_str(v.title),
                        swofty::detail::payload_str(v.route), swofty::detail::payload_str(v.image),
                        swofty::detail::payload_str(v.place), v.literal != 0};
    }

    Result remove(size_t index)
    {
        if (!t_ || !t_->remove)
            return Result::unavailable;
        return index > size_t(INT32_MAX) ? Result::invalid : detail::ui_result(t_->remove(t_->self, int(index)));
    }

    Result move(size_t from, size_t to)
    {
        if (!t_ || !t_->move)
            return Result::unavailable;
        if (from > size_t(INT32_MAX) || to > size_t(INT32_MAX))
            return Result::invalid;
        return detail::ui_result(t_->move(t_->self, int(from), int(to)));
    }

    Result insert(size_t at, const TileSpec &spec)
    {
        if (!t_ || !t_->insert)
            return Result::unavailable;
        if (at > size_t(INT32_MAX))
            return Result::invalid;
        std::string id(spec.id), title(spec.title), route(spec.route), image(spec.image);
        SwoftyTileSpec s{id.c_str(), title.c_str(), route.c_str(), image.c_str(), spec.literal ? 1 : 0};
        return detail::ui_result(t_->insert(t_->self, int(at), &s));
    }

    Result append(const TileSpec &spec) { return insert(count(), spec); }

    class iterator {
    public:
        iterator(const TileList *l, size_t i) : l_(l), i_(i) {}
        TileItem operator*() const { return l_->get(i_).value_or(TileItem{}); }
        iterator &operator++()
        {
            ++i_;
            return *this;
        }
        bool operator!=(const iterator &o) const { return i_ != o.i_; }

    private:
        const TileList *l_;
        size_t i_;
    };

    iterator begin() const { return {this, 0}; }
    iterator end() const { return {this, count()}; }

private:
    const SwoftyTileList *t_ = nullptr;
};

// The track list of ui.track_select.build (payload version 2), read-only: a core that cannot read the screen's list
// leaves every function null, and then count is 0 and get is nullopt.
class TrackList {
public:
    TrackList() = default;
    explicit TrackList(const SwoftyTrackList *t)
        : t_(t && t->version >= 1 && t->size >= sizeof(SwoftyTrackList) ? t : nullptr)
    {
    }

    explicit operator bool() const { return t_ != nullptr; }

    size_t count() const
    {
        if (!t_ || !t_->count)
            return 0;
        int n = t_->count(t_->self);
        return n > 0 ? size_t(n) : 0;
    }

    std::optional<std::string_view> get(size_t index) const
    {
        const char *id = nullptr;
        if (!t_ || !t_->get || index > size_t(INT32_MAX) || t_->get(t_->self, int(index), &id) != SWOFTY_ASSET_OK)
            return std::nullopt;
        return swofty::detail::payload_str(id);
    }

private:
    const SwoftyTrackList *t_ = nullptr;
};

}

namespace swofty::ui::events {

struct EraTiles {
    static constexpr const char *point = "ui.era.tiles";
    static constexpr uint32_t version = 2;
    uint32_t size, payload_version;
    void *list;
    const SwoftyTileList *tiles_table;

    TileList tiles() const { return TileList(payload_version >= 2 && size >= sizeof(*this) ? tiles_table : nullptr); }
    Result remove(size_t index) { return tiles().remove(index); }
    Result insert(size_t at, const TileSpec &spec) { return tiles().insert(at, spec); }
};

struct VmResolve {
    static constexpr const char *point = "ui.vm.resolve";
    static constexpr uint32_t version = 2;
    uint32_t size, payload_version;
    const char *path_str;
    void *node;
    int32_t (*node_of_fn)(void *user, const char *path, void **node);
    void *user;

    std::string_view path() const { return swofty::detail::payload_str(path_str); }
    Result use_node_of(std::string_view other)
    {
        if (payload_version < 2 || size < sizeof(*this) || !node_of_fn)
            return Result::unavailable;
        std::string p(other);
        void *found = nullptr;
        int32_t r = node_of_fn(user, p.c_str(), &found);
        if (r == SWOFTY_ASSET_OK)
            node = found;
        return swofty::detail::result(r);
    }
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
    static constexpr uint32_t version = 2;
    uint32_t size, payload_version;
    void *list;
    const SwoftyTileList *tiles_table;

    TileList tiles() const { return TileList(payload_version >= 2 && size >= sizeof(*this) ? tiles_table : nullptr); }
    Result remove(size_t index) { return tiles().remove(index); }
    Result insert(size_t at, const TileSpec &spec) { return tiles().insert(at, spec); }
};

struct CareerContentTiles {
    static constexpr const char *point = "ui.career_content.tiles";
    static constexpr uint32_t version = 2;
    uint32_t size, payload_version;
    void *list;
    const SwoftyTileList *tiles_table;

    TileList tiles() const { return TileList(payload_version >= 2 && size >= sizeof(*this) ? tiles_table : nullptr); }
    Result remove(size_t index) { return tiles().remove(index); }
    Result insert(size_t at, const TileSpec &spec) { return tiles().insert(at, spec); }
};

struct CarCategoryTiles {
    static constexpr const char *point = "ui.car_category.tiles";
    static constexpr uint32_t version = 2;
    uint32_t size, payload_version;
    void *list;
    const SwoftyTileList *tiles_table;

    TileList tiles() const { return TileList(payload_version >= 2 && size >= sizeof(*this) ? tiles_table : nullptr); }
    Result remove(size_t index) { return tiles().remove(index); }
    Result insert(size_t at, const TileSpec &spec) { return tiles().insert(at, spec); }
};

// Fires after the Grand Prix / championship track select (Trk_GrandPrixTrackSelectVm_RefreshTrackList) and the
// championship settings track tab (Trk_SettingsSelectTrackTabVm_RefreshTrackList) rebuild their track list, once per
// rebuild. `list` is that screen's view model, opaque to handlers. Payload version 2 adds `tracks`, a read-only
// SwoftyTrackList: the point fires after the game has filled the screen's list, whose element layout this core has not
// proven, so its functions are null (count 0) and remove and insert are not offered at all. Time Trial's carousel is
// not covered: its list builder is not found.
struct TrackSelectBuild {
    static constexpr const char *point = "ui.track_select.build";
    static constexpr uint32_t version = 2;
    uint32_t size, payload_version;
    void *list;
    const SwoftyTrackList *tracks_table;

    TrackList tracks() const { return TrackList(payload_version >= 2 && size >= sizeof(*this) ? tracks_table : nullptr); }
};

// Fires for every screen that builds without error. `screen` is the id of the addon's ui.screen record
// (`<addon>/<key>`) for an addon screen, and the name of the screen's root node for any other screen (stock screens,
// screens declared in swofty.json only, launcher-generated pages).
struct ScreenBuilt {
    static constexpr const char *point = "ui.screen.built";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    const char *screen_str;
    void *view;

    std::string_view screen() const { return swofty::detail::payload_str(screen_str); }
};

// Fires when the game runs a route. `route` is the route name the game runs: the key of a ui.route record, whose
// cloned group has that name, or a stock route name. `screen` is the id of the ui.screen record the route opens, and
// empty for a route no ui.route record declares. A handler that takes the route cancels it.
struct RouteOpen {
    static constexpr const char *point = "ui.route.open";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    const char *route_str;
    const char *screen_str;

    std::string_view route() const { return swofty::detail::payload_str(route_str); }
    std::string_view screen() const { return swofty::detail::payload_str(screen_str); }
};

// One MFD page of osd_main. `name` is the page's panel name (car_settings, pit_settings, car_damage, car_engine,
// car_temp, mods); a handler sets `hide` to 1 to hide it.
struct MfdPage {
    const char *name;
    uint8_t hide;
    uint8_t reserved[7];
};

// `pages` points at `count` MfdPage entries in the game's order, `capacity` is 8. Handlers can only hide: the core
// writes <panel>_enabled false for each page left with `hide` set, after the game's own availability rules, so a
// handler never shows a page the game disabled. Reordering, adding and removing pages are refused.
struct MfdPages {
    static constexpr const char *point = "hud.mfd.pages";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    void *pages;
    int32_t count;
    int32_t capacity;

    MfdPage *page(int index) const { return index >= 0 && index < count ? static_cast<MfdPage *>(pages) + index : nullptr; }
};

}
