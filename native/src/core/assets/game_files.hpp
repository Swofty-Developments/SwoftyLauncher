#pragma once

#include "core/apply/content/erp_file.hpp"
#include "core/apply/content/uidb.hpp"

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-function"
#pragma GCC diagnostic ignored "-Wsign-compare"
#pragma GCC diagnostic ignored "-Wimplicit-fallthrough"
#endif
#define BCDEC_STATIC
#define BCDEC_IMPLEMENTATION
#include "../../../third_party/bcdec/bcdec.h"
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

// The game's own resources for the game assets library (swofty/game_assets.hpp): a resource's decompressed fragments by
// its eaid:// URI, a texture or texture-array slice decoded to RGBA8, and a UI image catalogue entry's texture array and
// slice. A URI's package (eaid://<package>/...) names the folder 2025_asset_groups/<package>/; the first read of a
// package opens the index of every ERP archive in that folder, as the badge and logo readers do, and keeps which
// archive carries each URI (by a 64-bit hash of the URI, so a package of 118,000 resources keeps about 2 MB). An
// overlay archive the launcher layers on a game archive wins over it. The file reads come
// from Env, so the same code runs in the core and in host tests.

namespace swofty_core::assets::game_files {

namespace erp = swofty_core::apply::content::erpfile;
namespace uidb = swofty_core::apply::content::uidb;

inline constexpr size_t MAX_RESOURCE = size_t(256) << 20, MAX_URI = 1023, MAX_ARCHIVES = 16384;
inline constexpr uint32_t HEAD_FORMAT = 8, HEAD_WIDTH = 12, HEAD_HEIGHT = 16, HEAD_LEVELS = 24, HEAD_LAYERS = 28, HEAD_SIZE = 32,
                          MAX_SIDE = 16384;

struct Env {
    std::string root_folder = "2025_asset_groups/";
    std::function<std::vector<std::string>(const std::string &folder)> list;
    std::function<std::shared_ptr<erp::Source>(const std::string &rel)> open;
    std::function<std::vector<std::string>(const std::string &rel)> overlays;
    std::function<void(const std::string &folder, size_t archives, size_t uris)> indexed;
};

struct Texture {
    uint32_t format = 0, width = 0, height = 0, levels = 0, layers = 0;
};

inline size_t block_bytes(uint32_t format)
{
    switch (format) {
    case 52:
    case 54:
        return 8;
    case 55:
    case 57:
    case 65:
    case 69:
    case 70:
        return 16;
    default:
        return 0;
    }
}

inline size_t level_bytes(uint32_t format, uint32_t w, uint32_t h, uint32_t level)
{
    size_t bw = (std::max(1u, w >> level) + 3) / 4, bh = (std::max(1u, h >> level) + 3) / 4;
    return bw * bh * block_bytes(format);
}

inline bool texture_of(const std::string &head, Texture &out, std::string &why)
{
    if (head.size() < HEAD_SIZE)
        return why = "the texture header is " + std::to_string(head.size()) + " byte(s), shorter than 32", false;
    out.format = erp::rd32(head, HEAD_FORMAT);
    out.width = erp::rd32(head, HEAD_WIDTH);
    out.height = erp::rd32(head, HEAD_HEIGHT);
    out.levels = std::max(1u, erp::rd32(head, HEAD_LEVELS));
    out.layers = std::max(1u, erp::rd32(head, HEAD_LAYERS));
    if (!block_bytes(out.format))
        return why = "format " + std::to_string(out.format) + " is not one this core decodes", false;
    if (!out.width || !out.height || out.width > MAX_SIDE || out.height > MAX_SIDE || out.levels > 32 || out.layers > 65536)
        return why = "the texture header says " + std::to_string(out.width) + "x" + std::to_string(out.height) + ", " +
                     std::to_string(out.levels) + " level(s), " + std::to_string(out.layers) + " layer(s)",
               false;
    return true;
}

inline size_t layer_stride(const Texture &t)
{
    size_t s = 0;
    for (uint32_t l = 0; l < t.levels; l++)
        s += level_bytes(t.format, t.width, t.height, l);
    return s;
}

// One level of one layer of a texture's pixel fragment, decoded to straight RGBA8 rows, top row first.
inline bool decode(const Texture &t, const std::string &data, uint32_t layer, uint32_t level, std::vector<uint8_t> &rgba,
                   std::string &why)
{
    if (layer >= t.layers || level >= t.levels)
        return why = "layer " + std::to_string(layer) + " level " + std::to_string(level) + " is outside the texture's " +
                     std::to_string(t.layers) + " layer(s) and " + std::to_string(t.levels) + " level(s)",
               false;
    size_t stride = layer_stride(t);
    if (data.size() < stride * t.layers)
        return why =
                   "the pixel fragment is " + std::to_string(data.size()) + " byte(s), not " + std::to_string(stride * t.layers),
               false;
    size_t at = size_t(layer) * stride;
    for (uint32_t l = 0; l < level; l++)
        at += level_bytes(t.format, t.width, t.height, l);
    uint32_t w = std::max(1u, t.width >> level), h = std::max(1u, t.height >> level);
    size_t block = block_bytes(t.format);
    rgba.assign(size_t(w) * h * 4, 0);
    for (uint32_t y = 0; y < h; y += 4)
        for (uint32_t x = 0; x < w; x += 4, at += block) {
            alignas(16) uint8_t px[64] = {};
            const char *src = data.data() + at;
            switch (t.format) {
            case 52:
            case 54:
                bcdec_bc1(src, px, 16);
                break;
            case 55:
            case 57:
                bcdec_bc3(src, px, 16);
                break;
            case 65: {
                uint8_t rg[32] = {};
                bcdec_bc5(src, rg, 8);
                for (int p = 0; p < 16; p++) {
                    px[p * 4] = rg[p * 2];
                    px[p * 4 + 1] = rg[p * 2 + 1];
                    px[p * 4 + 3] = 255;
                }
                break;
            }
            default:
                bcdec_bc7(src, px, 16);
            }
            for (uint32_t by = 0; by < 4 && y + by < h; by++)
                for (uint32_t bx = 0; bx < 4 && x + bx < w; bx++)
                    std::memcpy(&rgba[(size_t(y + by) * w + x + bx) * 4], &px[(by * 4 + bx) * 4], 4);
        }
    return true;
}

inline bool package_of(const std::string &uri, std::string &package)
{
    static constexpr const char *SCHEME = "eaid://";
    if (uri.size() > MAX_URI || uri.compare(0, 7, SCHEME) != 0)
        return false;
    size_t slash = uri.find('/', 7);
    if (slash == std::string::npos || slash == 7 || slash + 1 >= uri.size())
        return false;
    package = uri.substr(7, slash - 7);
    return package != "." && package != ".." && package.find_first_of("\\:") == std::string::npos;
}

struct Catalogue {
    std::string texture;
    std::vector<std::string> names;
};

class Files {
public:
    explicit Files(Env env) : env_(std::move(env)) {}

    // The resource's fragments, decompressed. fragment ~0u: all of them, one after another.
    bool read(const std::string &uri, uint32_t fragment, std::string &out, std::string &why)
    {
        std::lock_guard g(lock_);
        std::vector<std::string> frags;
        if (!fragments(uri, fragment, frags, why))
            return false;
        if (frags.size() == 1) {
            out = std::move(frags[0]);
            return true;
        }
        size_t total = 0;
        for (auto &f : frags)
            total += f.size();
        if (total > MAX_RESOURCE)
            return why = uri + " is " + std::to_string(total) + " byte(s), more than the 256 MiB a read returns", false;
        out.clear();
        out.reserve(total);
        for (auto &f : frags)
            out += f;
        return true;
    }

    // The fragment count and sizes without reading the data.
    bool sizes(const std::string &uri, std::vector<uint64_t> &out, std::string &why)
    {
        std::lock_guard g(lock_);
        Where w;
        if (!locate(uri, w, why))
            return false;
        out.clear();
        for (auto &f : w.entry->frags)
            out.push_back(f.size);
        return true;
    }

    bool texture(const std::string &uri, Texture &out, std::string &why)
    {
        std::string head;
        return read(uri, 0, head, why) && texture_of(head, out, why);
    }

    bool rgba(const std::string &uri, uint32_t layer, uint32_t level, Texture &t, std::vector<uint8_t> &out, std::string &why)
    {
        std::string data;
        if (!texture(uri, t, why))
            return false;
        if (!read(uri, 1, data, why))
            return false;
        return decode(t, data, layer, level, out, why);
    }

    // A UI image catalogue by its dotted name, "overrides.team_logos.driver_career_2024_team_select": its first part
    // names the database eaid://ui_package/databases/<part>.uidb, the rest the lists under its Contents.
    bool catalogue(const std::string &name, Catalogue &out, std::string &why)
    {
        std::lock_guard g(lock_);
        auto it = catalogues_.find(name);
        if (it != catalogues_.end()) {
            out = it->second;
            return true;
        }
        size_t dot = name.find('.');
        if (name.empty() || name.size() > 512 || dot == 0 || dot == std::string::npos || dot + 1 >= name.size())
            return why = "'" + name + "' is not a dotted catalogue name such as overrides.team_logos.<list>", false;
        std::string database = name.substr(0, dot), path = database;
        for (size_t at = dot + 1; at <= name.size();) {
            size_t end = name.find('.', at);
            if (end == std::string::npos)
                end = name.size();
            if (end == at)
                return why = "'" + name + "' has an empty part", false;
            path += ".Contents." + name.substr(at, end - at);
            at = end + 1;
        }
        std::string uri = "eaid://ui_package/databases/" + database + ".uidb";
        std::vector<std::string> frags;
        if (!fragments(uri, 0, frags, why))
            return false;
        uidb::Pkg pkg;
        if (!pkg.read(frags[0]))
            return why = uri + " does not read: " + pkg.why, false;
        uidb::Node *node = pkg.find(path);
        if (!node || node->kind != uidb::Node::object)
            return why = uri + " has no catalogue " + name, false;
        std::string array = uidb::Pkg::text_of(*node, "TextureArrayName");
        uidb::Node *list = nullptr;
        for (auto &[k, v] : node->items)
            if (k.value == "Contents")
                list = &v;
        if (array.empty() || !list || list->kind != uidb::Node::array)
            return why = name + " in " + uri + " is not an image catalogue (no TextureArrayName and Contents list)", false;
        Catalogue c;
        c.texture = "eaid://" + array + ".image";
        for (auto &e : list->list)
            c.names.push_back(e.kind == uidb::Node::object ? uidb::Pkg::text_of(e, "Name") : std::string());
        out = catalogues_.emplace(name, std::move(c)).first->second;
        return true;
    }

    size_t archives_opened() const { return opened_; }

private:
    struct Where {
        std::shared_ptr<erp::Source> src;
        erp::Archive arc;
        const erp::Entry *entry = nullptr;
        std::string rel;
    };
    struct Package {
        std::vector<std::string> archives;
        std::vector<std::pair<uint64_t, uint32_t>> at;
    };

    bool index(const std::string &package, Package *&out, std::string &why)
    {
        auto it = packages_.find(package);
        if (it != packages_.end())
            return (out = &it->second), true;
        std::string folder = env_.root_folder + package + "/";
        Package p;
        std::vector<std::string> stock = env_.list ? env_.list(folder) : std::vector<std::string>{};
        if (stock.size() > MAX_ARCHIVES)
            return why = folder + " holds " + std::to_string(stock.size()) + " archives, more than this core reads", false;
        struct Seen {
            uint64_t hash;
            uint32_t id;
            bool overlay;
        };
        std::vector<Seen> seen;
        auto open_into = [&](const std::string &rel, bool overlay) {
            auto src = env_.open(rel);
            erp::Archive arc;
            std::string w;
            opened_++;
            if (!src || !erp::open(*src, arc, w))
                return false;
            uint32_t id = uint32_t(p.archives.size());
            p.archives.push_back(rel);
            for (auto &e : arc.entries)
                seen.push_back({runtime::fnv64(e.name), id, overlay});
            return true;
        };
        for (auto &rel : stock)
            if (open_into(rel, false) && env_.overlays)
                for (auto &o : env_.overlays(rel))
                    open_into(o, true);
        std::stable_sort(seen.begin(), seen.end(), [](const Seen &a, const Seen &b) { return a.hash < b.hash; });
        for (size_t i = 0; i < seen.size();) {
            size_t j = i;
            uint32_t pick = seen[i].id;
            bool overlaid = false;
            for (; j < seen.size() && seen[j].hash == seen[i].hash; j++)
                if (seen[j].overlay)
                    pick = seen[j].id, overlaid = true;
            if (!overlaid)
                pick = seen[i].id;
            p.at.push_back({seen[i].hash, pick});
            i = j;
        }
        if (env_.indexed)
            env_.indexed(folder, p.archives.size(), p.at.size());
        if (p.archives.empty())
            return why = "no archive in " + folder + " reads", false;
        out = &packages_.emplace(package, std::move(p)).first->second;
        return true;
    }

    bool locate(const std::string &uri, Where &w, std::string &why)
    {
        std::string package;
        if (!package_of(uri, package))
            return why = "'" + uri + "' is not an eaid://<package>/... URI", false;
        Package *p = nullptr;
        if (!index(package, p, why))
            return false;
        uint64_t h = runtime::fnv64(uri);
        auto it = std::lower_bound(p->at.begin(), p->at.end(), std::pair<uint64_t, uint32_t>{h, 0});
        if (it == p->at.end() || it->first != h)
            return why = uri + " is in no archive of " + env_.root_folder + package + "/", false;
        w.rel = p->archives[it->second];
        w.src = env_.open(w.rel);
        if (!w.src || !erp::open(*w.src, w.arc, why))
            return why = w.rel + " does not open: " + why, false;
        w.entry = w.arc.find(uri);
        if (!w.entry)
            return why = w.rel + " no longer carries " + uri, false;
        return true;
    }

    bool fragments(const std::string &uri, uint32_t fragment, std::vector<std::string> &out, std::string &why)
    {
        Where w;
        if (!locate(uri, w, why))
            return false;
        auto &frags = w.entry->frags;
        if (fragment != ~0u && fragment >= frags.size())
            return why =
                       uri + " has " + std::to_string(frags.size()) + " fragment(s), not a fragment " + std::to_string(fragment),
                   false;
        size_t total = 0;
        out.clear();
        for (size_t i = 0; i < frags.size(); i++) {
            if (fragment != ~0u && i != fragment)
                continue;
            if (frags[i].size > MAX_RESOURCE || (total += frags[i].size) > MAX_RESOURCE)
                return why = uri + " is larger than the 256 MiB a read returns", false;
            std::string bytes, reason;
            if (!erp::fragment(*w.src, w.arc, frags[i], bytes, reason))
                return why = uri + " in " + w.rel + ": " + reason, false;
            out.push_back(std::move(bytes));
        }
        return true;
    }

    Env env_;
    std::mutex lock_;
    std::map<std::string, Package> packages_;
    std::map<std::string, Catalogue> catalogues_;
    size_t opened_ = 0;
};

} // namespace swofty_core::assets::game_files
