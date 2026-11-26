#pragma once
#include <functional>
#include <string>
#include <vector>

namespace swofty_core::hooks::ui_names {

inline std::string key_of(const std::string &id)
{
    auto at = id.find('/');
    return at == std::string::npos ? id : id.substr(at + 1);
}

inline std::string addon_of(const std::string &id)
{
    auto at = id.find('/');
    return at == std::string::npos ? std::string() : id.substr(0, at);
}

// The record id among `ids` (`<addon>/<key>`) the name the game uses stands for: the clone of a ui.route record and the
// registered name of a ui.screen record are the record's bare key. `prefer` is an addon whose record wins when several
// addons share a key. Empty when no record has the name.
inline std::string id_of_key(const std::vector<std::string> &ids, const std::string &key, const std::string &prefer = {})
{
    std::string found;
    for (auto &id : ids) {
        if (key_of(id) != key)
            continue;
        if (!prefer.empty() && addon_of(id) == prefer)
            return id;
        if (found.empty())
            found = id;
    }
    return found;
}

// The `screen` of ui.route.open: the id of the ui.screen record a route's `opens` names. `opens` is a screen id
// (`<addon>/<key>`, returned as written) or a bare key (looked up among `screens`, the route's own addon first); a name
// no record has is returned as written.
inline std::string screen_of_opens(const std::string &opens, const std::string &route_addon,
                                   const std::vector<std::string> &screens)
{
    if (opens.empty() || opens.find('/') != std::string::npos)
        return opens;
    std::string id = id_of_key(screens, opens, route_addon);
    return id.empty() ? opens : id;
}

// The `screen` of ui.route.open for the route name the game fired, which is the bare key the build cloned the route
// group under (a record id is accepted as well). `routes` and `screens` are the enabled ui.route and ui.screen record
// ids, `opens_of` reads a route record's `opens`. Empty for a route no ui.route record has (a stock route).
inline std::string screen_of_route(const std::string &route, const std::vector<std::string> &routes,
                                   const std::vector<std::string> &screens,
                                   const std::function<std::string(const std::string &)> &opens_of)
{
    std::string id;
    for (auto &r : routes)
        if (r == route)
            id = r;
    if (id.empty())
        id = id_of_key(routes, key_of(route));
    if (id.empty())
        return {};
    return screen_of_opens(opens_of(id), addon_of(id), screens);
}

// The `screen` of ui.screen.built: an addon screen reports the id of its ui.screen record; a screen with no record (a
// stock screen, or one the launcher generates) reports `root`, the name of its root node.
inline std::string built_screen(const std::string &root, const std::string &registered, const std::vector<std::string> &screens)
{
    if (registered.empty())
        return root;
    std::string id = id_of_key(screens, registered);
    return id.empty() ? root : id;
}

}
