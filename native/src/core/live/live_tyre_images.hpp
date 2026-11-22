#pragma once
#include "core/render/badges/live_image_reference.hpp"
#include <swofty_api.h>
#include <cstring>
#include <string>

namespace swofty_core::live::tyre_images {
// Read the effective loaded GameDB, not the stock file. Explicit row stems
// matter: VisualCompound and ui_texture_path are not the generic db cursor's
// singular form of their table names. Resolve afresh so edits cannot leave a
// stale stat-id mapping or template behind. Calls and strings are bounded.
inline int reference(const SwoftyDb *db, uint8_t visual, char *out, uint32_t cap)
{
    if (!out || cap < 2)
        return SWOFTY_ASSET_INVALID;
    out[0] = 0;
    if (!db || !db->available || !db->available() || !db->int_at || !db->string_at)
        return SWOFTY_ASSET_UNAVAILABLE;
    auto text = [&](const std::string &key, std::string &value) {
        const char *s = nullptr;
        if (db->string_at(key.c_str(), &s) != SWOFTY_ASSET_OK || !s)
            return false;
        size_t n = 0;
        while (n < 160 && s[n])
            ++n;
        if (n == 160)
            return false;
        value.assign(s, n);
        return true;
    };
    int64_t count = 0;
    if (db->int_at("tyres.visual_compounds._VisualCompound_instancecount_", &count) != SWOFTY_ASSET_OK || count < 1 ||
        count > 256)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::string texture;
    bool found = false;
    for (int64_t i = 0; i < count; ++i) {
        const auto row = "tyres.visual_compounds.VisualCompound_" + std::to_string(i);
        int64_t id = -1;
        if (db->int_at((row + ".stat_id").c_str(), &id) != SWOFTY_ASSET_OK)
            return SWOFTY_ASSET_UNAVAILABLE;
        if (id != visual)
            continue;
        if (found || !text(row + ".texture_name", texture) || !live_image_reference::name(texture))
            return SWOFTY_ASSET_UNAVAILABLE;
        found = true;
    }
    if (!found)
        return SWOFTY_ASSET_UNAVAILABLE;
    constexpr const char *table = "entrants.ui.asset_templates.tyre_compounds.textures";
    if (db->int_at((std::string(table) + "._ui_texture_path_instancecount_").c_str(), &count) != SWOFTY_ASSET_OK || count < 1 ||
        count > 32)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::string result;
    found = false;
    for (int64_t i = 0; i < count; ++i) {
        const auto row = std::string(table) + ".ui_texture_path_" + std::to_string(i);
        std::string name, path;
        if (!text(row + ".name", name))
            return SWOFTY_ASSET_UNAVAILABLE;
        if (name != "small")
            continue;
        if (found || !text(row + ".path", path))
            return SWOFTY_ASSET_UNAVAILABLE;
        constexpr std::string_view token = "[COMPOUND]";
        const auto at = path.find(token);
        if (at == path.npos || path.find(token, at + token.size()) != path.npos)
            return SWOFTY_ASSET_UNAVAILABLE;
        path.replace(at, token.size(), texture);
        if (!live_image_reference::tyre(path))
            return SWOFTY_ASSET_UNAVAILABLE;
        result = std::move(path);
        found = true;
    }
    if (!found || result.size() >= 160)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (result.size() + 1 > cap)
        return SWOFTY_ASSET_INVALID;
    std::memcpy(out, result.c_str(), result.size() + 1);
    return SWOFTY_ASSET_OK;
}
} // namespace swofty_core::live::tyre_images
