#pragma once

#include "core/apply/content/erp_file.hpp"
#include "core/apply/content/logo_array.hpp"
#include "core/apply/content/logo_cache.hpp"
#include "core/apply/content/png_straight.hpp"
#include "core/apply/content/uidb.hpp"
#include "core/runtime/hashing.hpp"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <exception>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <thread>
#include <vector>

// The archives a set of team logos changes. Each logo is a slice for one stock texture array (an encoded image, or a
// copy of the slice a stock key already has) under a key in overrides.uidb. The result names, for every archive in
// the game's ui_package folder that carries a changed array and for databases.erp, the fragments to replace, and the
// archives whose contents it read: databases.erp and every archive that carries a wanted array. The archive list may
// name the boot archive too (see array_layers): the game draws from its copy of an array when it has one, so that
// copy gets the same slices at the same indices. A copy there that is an image with no view of its own is built with
// the view of a ui_package copy, and one that holds fewer slices than a ui_package copy of the same slice size takes
// that copy's slices for the keys it lacks.

namespace swofty_core::apply::content::logo_build {

inline constexpr const char *UI_FOLDER = "2025_asset_groups/ui_package/";
inline constexpr const char *DATABASES = "2025_asset_groups/ui_package/databases.erp";
inline constexpr const char *BOOT_ARCHIVE = "2025_asset_groups/vfx_package/common.erp";
inline constexpr const char *OVERRIDES = "eaid://ui_package/databases/overrides.uidb";
inline constexpr const char *ARRAY_URI = "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.";
inline constexpr const char *TEXTURE_PATH = "ui_package/art/textures/swofty/team_logos/";

// An image want carries its decoded pixels, or, when the slice cache already knew its PNG decodes, the PNG bytes
// (png) to decode only if a slice of it has to be encoded; source names those bytes in the slice cache.
struct Want {
    std::string array, key, donor;
    std::string rgba;
    uint32_t width = 0, height = 0;
    bool fallback = false;
    std::string png{}, source{};
};

/* Encoded slices on disk, so a change to one addon re-encodes only its own logos. An entry is named by the PNG's
   bytes (two 64-bit hashes and the length), the slice size and SLICE_ENCODER, which changes with anything that
   changes slice_from_rgba's output; it holds a checksum of the slice, and an entry that fails it is encoded again.
   A PNG's decode result (its size, or why it does not decode) is kept the same way. */
inline constexpr const char *SLICE_ENCODER = "bc7logo1";

struct SliceCache {
    std::function<bool(const std::string &name, std::string &bytes)> get;
    std::function<void(const std::string &name, const std::string &bytes)> put;
    bool on() const { return get && put; }
};

inline std::string source_of(const std::string &png)
{
    char out[64];
    std::snprintf(out, sizeof out, "%016llx%016llx-%zx", (unsigned long long)runtime::fnv64(png.data(), png.size()),
                  (unsigned long long)runtime::mix64(png.data(), png.size()), png.size());
    return out;
}

inline std::string slice_name(const std::string &source, uint32_t width, uint32_t height)
{
    return source + "-" + std::to_string(width) + "x" + std::to_string(height) + "-" + SLICE_ENCODER + ".slice";
}

inline std::string info_name(const std::string &source) { return source + ".png"; }

inline std::string sealed(const std::string &bytes)
{
    uint64_t sum = runtime::mix64(bytes.data(), bytes.size());
    std::string out = "SLC1";
    out.append(reinterpret_cast<const char *>(&sum), 8);
    return out + bytes;
}

inline bool unseal(const std::string &stored, std::string &bytes)
{
    if (stored.size() < 12 || stored.compare(0, 4, "SLC1"))
        return false;
    uint64_t sum;
    std::memcpy(&sum, stored.data() + 4, 8);
    if (runtime::mix64(stored.data() + 12, stored.size() - 12) != sum)
        return false;
    bytes.assign(stored, 12, std::string::npos);
    return true;
}

struct PngInfo {
    bool ok = false;
    uint32_t width = 0, height = 0;
    std::string why;
};

inline std::string info_text(const PngInfo &i)
{
    return i.ok ? "ok " + std::to_string(i.width) + " " + std::to_string(i.height) : "no " + i.why;
}

inline bool info_parse(const std::string &text, PngInfo &i)
{
    if (!text.compare(0, 3, "no ")) {
        i = {false, 0, 0, text.substr(3)};
        return true;
    }
    unsigned w = 0, h = 0;
    char tail = 0;
    if (text.compare(0, 3, "ok ") || std::sscanf(text.c_str() + 3, "%u %u%c", &w, &h, &tail) != 2 || !w || !h)
        return false;
    i = {true, w, h, {}};
    return true;
}

// Reads a want's PNG the way deliver_logos always has, but answers from the slice cache when it knows these bytes, so
// the pixels are decoded only when a slice must be encoded. Returns the decode result; rgba is filled when decoded.
inline PngInfo look_at_png(const SliceCache &cache, const std::string &png, const std::string &source, std::string &rgba)
{
    PngInfo i;
    std::string stored, text;
    if (cache.on() && cache.get(info_name(source), stored) && unseal(stored, text) && info_parse(text, i))
        return i;
    i.ok = png::decode(png, rgba, i.width, i.height, i.why);
    if (cache.on())
        cache.put(info_name(source), sealed(info_text(i)));
    return i;
}

/* One slice of a 700x640 array takes hundreds of milliseconds to encode and a boot can ask for hundreds of slices,
   while the game's main thread waits in the database load. Each encode reads only its own image, so they run on
   worker threads; a failure or exception is kept for the caller, which uses the results in its own order. */
struct Encoded {
    bool ok = false, scaled = false;
    std::string bytes, why;
};

inline void run_parallel(size_t n, const std::function<void(size_t)> &fn)
{
    unsigned cores = std::thread::hardware_concurrency();
    size_t workers = std::min<size_t>(n, std::clamp<unsigned>(cores > 1 ? cores - 1 : 1, 1, 8));
    std::atomic<size_t> next{0};
    std::vector<std::exception_ptr> failed(n);
    auto drain = [&] {
        for (size_t i; (i = next.fetch_add(1)) < n;) {
            try {
                fn(i);
            } catch (...) {
                failed[i] = std::current_exception();
            }
        }
    };
    std::vector<std::thread> pool;
    for (size_t k = 1; k < workers; k++) {
        try {
            pool.emplace_back(drain);
        } catch (...) {
            break;
        }
    }
    drain();
    for (auto &t : pool)
        t.join();
    for (auto &e : failed)
        if (e)
            std::rethrow_exception(e);
}

struct EncodeStats {
    size_t cached = 0, encoded = 0, decoded = 0;
};

inline std::vector<std::vector<Encoded>> encode_slices(const std::vector<const Want *> &wants,
                                                       const std::vector<logo_array::Shape> &shapes, const SliceCache &cache = {},
                                                       EncodeStats *stats = nullptr)
{
    std::vector<std::vector<Encoded>> out(wants.size(), std::vector<Encoded>(shapes.size()));
    std::vector<size_t> misses;
    std::vector<char> needs_pixels(wants.size());
    for (size_t job = 0; job < wants.size() * shapes.size(); job++) {
        const Want *w = wants[job / shapes.size()];
        if (!w->donor.empty())
            continue;
        const logo_array::Shape &shape = shapes[job % shapes.size()];
        Encoded &e = out[job / shapes.size()][job % shapes.size()];
        std::string stored;
        if (cache.on() && !w->source.empty() && cache.get(slice_name(w->source, shape.width, shape.height), stored) &&
            unseal(stored, e.bytes)) {
            e.ok = true;
            e.scaled = w->width != shape.width || w->height != shape.height;
            if (stats)
                stats->cached++;
            continue;
        }
        misses.push_back(job);
        needs_pixels[job / shapes.size()] = w->rgba.empty();
    }
    std::vector<std::string> pixels(wants.size());
    std::vector<std::string> pixel_why(wants.size());
    std::vector<size_t> decode;
    for (size_t i = 0; i < wants.size(); i++)
        if (needs_pixels[i])
            decode.push_back(i);
    run_parallel(decode.size(), [&](size_t k) {
        const Want *w = wants[decode[k]];
        uint32_t dw = 0, dh = 0;
        if (!png::decode(w->png, pixels[decode[k]], dw, dh, pixel_why[decode[k]]) || dw != w->width || dh != w->height) {
            pixels[decode[k]].clear();
            if (pixel_why[decode[k]].empty())
                pixel_why[decode[k]] = "the image decodes to another size than the slice cache says";
        }
    });
    if (stats)
        stats->decoded += decode.size(), stats->encoded += misses.size();
    run_parallel(misses.size(), [&](size_t k) {
        size_t job = misses[k], wi = job / shapes.size();
        const Want *w = wants[wi];
        Encoded &e = out[wi][job % shapes.size()];
        const std::string &rgba = w->rgba.empty() ? pixels[wi] : w->rgba;
        if (rgba.empty()) {
            e.why = "the image does not decode: " + pixel_why[wi];
            return;
        }
        e.ok = logo_array::slice_from_rgba(shapes[job % shapes.size()], reinterpret_cast<const uint8_t *>(rgba.data()), w->width,
                                           w->height, e.bytes, e.why, &e.scaled);
    });
    if (cache.on())
        for (size_t job : misses) {
            const Want *w = wants[job / shapes.size()];
            const Encoded &e = out[job / shapes.size()][job % shapes.size()];
            const logo_array::Shape &shape = shapes[job % shapes.size()];
            if (e.ok && !w->source.empty())
                cache.put(slice_name(w->source, shape.width, shape.height), sealed(e.bytes));
        }
    return out;
}

struct Env {
    std::function<std::vector<std::string>()> list;
    std::function<std::shared_ptr<erpfile::Source>(const std::string &rel)> open;
    std::function<std::vector<std::string>(const std::string &rel)> overlays;
    SliceCache slices;
};

// The slice sizes each image want was encoded at, for the next start to encode ahead of the database load.
struct Planned {
    std::string source;
    std::vector<std::pair<uint32_t, uint32_t>> sizes;
};

struct Result {
    std::map<std::string, erpfile::Changes> changes;
    std::set<std::string> read;
    std::vector<std::string> lines;
    int built = 0, refused = 0;
    double encode_ms = 0;
    size_t encodes = 0;
    EncodeStats slices;
    std::map<const Want *, Planned> planned;
};

struct Refusals {
    std::map<std::string, std::pair<int, std::string>> by;

    void add(const std::string &reason, const std::string &example, int n = 1)
    {
        auto &r = by[reason];
        if (!r.first)
            r.second = example;
        r.first += n;
    }

    void flush(std::vector<std::string> &lines) const
    {
        for (auto &[reason, r] : by)
            lines.push_back("team.logo: " + std::to_string(r.first) + " slot(s) refused: " + reason + " (first " + r.second +
                            ")");
    }
};

inline std::string list_path(const std::string &array)
{
    std::string path = "overrides.Contents.", rest = array;
    size_t dot;
    while ((dot = rest.find('.')) != std::string::npos) {
        path += rest.substr(0, dot) + ".Contents.";
        rest = rest.substr(dot + 1);
    }
    return path + rest + ".Contents";
}

inline std::vector<std::string> array_layers(const Env &env, const std::string &rel)
{
    std::vector<std::string> layers{rel}, out;
    if (env.overlays)
        for (auto &o : env.overlays(rel))
            if (std::find(layers.begin(), layers.end(), o) == layers.end())
                layers.push_back(o);
    size_t prefix = std::char_traits<char>::length(ARRAY_URI);
    for (auto &l : layers) {
        auto src = env.open(l);
        erpfile::Archive arc;
        std::string why;
        if (!src || !erpfile::open(*src, arc, why))
            continue;
        if (std::any_of(arc.entries.begin(), arc.entries.end(),
                        [&](const erpfile::Entry &e) { return e.name.compare(0, prefix, ARRAY_URI) == 0; }))
            out.push_back(l);
    }
    return out;
}

// What the build reads from databases.erp: whether it carries a texture array, and the contents of the first fragment
// of its overrides.uidb. A databases.erp that carries none counts by those contents alone (true), wherever the entry
// lies and however it is packed; one that carries one, or does not read, counts by all of its bytes (false).
inline bool databases_hash(const erpfile::Source &src, uint64_t &hash)
{
    erpfile::Archive arc;
    std::string why, data;
    if (!erpfile::open(src, arc, why))
        return false;
    size_t prefix = std::char_traits<char>::length(ARRAY_URI);
    if (std::any_of(arc.entries.begin(), arc.entries.end(),
                    [&](const erpfile::Entry &e) { return e.name.compare(0, prefix, ARRAY_URI) == 0; }))
        return false;
    const erpfile::Entry *e = arc.find(OVERRIDES);
    if (e && !e->frags.empty() && !erpfile::fragment(src, arc, e->frags[0], data, why))
        return false;
    std::string tag = !e ? "no overrides.uidb" : e->frags.empty() ? "an empty overrides.uidb" : "overrides.uidb";
    logo_cache::Bytes b;
    b.add(tag.data(), tag.size() + 1);
    b.add(data.data(), data.size());
    hash = b.done();
    return true;
}

// The hash an input counts by: databases_hash for databases.erp when it applies, all of its bytes (whole) otherwise.
inline bool input_hash(const Env &env, const std::string &rel, const std::function<bool(const std::string &, uint64_t &)> &whole,
                       uint64_t &hash)
{
    if (rel == DATABASES) {
        auto src = env.open(rel);
        if (src && databases_hash(*src, hash))
            return true;
    }
    return whole(rel, hash);
}

// The copy of databases.erp a cached build wrote, made from databases.erp as it is now (now): the overrides.uidb the
// build wrote (from copy) in place of its own, and every other byte as now has it. A build under the same key reads
// the same overrides.uidb (databases_hash) and writes the same one, so this is the copy it writes.
inline bool databases_copy(const erpfile::Source &now, const std::string &copy, const erpfile::Sink &sink, std::string &why)
{
    erpfile::Source held;
    held.size = copy.size();
    held.read = [&copy](uint64_t off, size_t n, char *out) {
        if (off > copy.size() || n > copy.size() - off)
            return false;
        std::memcpy(out, copy.data() + off, n);
        return true;
    };
    erpfile::Archive was, arc;
    std::string written;
    if (!erpfile::open(held, was, why) || !erpfile::open(now, arc, why))
        return false;
    const erpfile::Entry *e = was.find(OVERRIDES);
    if (!e || e->frags.empty())
        return why = "the copy has no overrides.uidb", false;
    if (!arc.find(OVERRIDES))
        return why = "databases.erp has no overrides.uidb", false;
    if (!erpfile::fragment(held, was, e->frags[0], written, why))
        return false;
    erpfile::Changes changes;
    changes[OVERRIDES] = {written};
    return erpfile::rewrite(now, arc, changes, sink, why);
}

inline bool build(const Env &env, const std::vector<Want> &wants, Result &out)
{
    using erpfile::Archive;
    struct Open {
        std::shared_ptr<erpfile::Source> src;
        Archive arc;
    };
    std::map<std::string, Open> opened;
    std::string why;
    Refusals refusals;
    struct Flush {
        Refusals &r;
        Result &out;
        ~Flush() { r.flush(out.lines); }
    } flush{refusals, out};
    auto refuse = [&](const std::string &reason, const std::string &example, int n = 1) {
        out.refused += n;
        refusals.add(reason, example, n);
    };
    auto get = [&](const std::string &rel) -> Open * {
        auto it = opened.find(rel);
        if (it != opened.end())
            return &it->second;
        auto src = env.open(rel);
        Open o;
        if (!src || !erpfile::open(*src, o.arc, why))
            return nullptr;
        o.src = src;
        return &opened.emplace(rel, std::move(o)).first->second;
    };
    auto layers_of = [&](const std::string &rel) {
        std::vector<std::string> layers{rel};
        if (env.overlays)
            for (auto &o : env.overlays(rel))
                if (std::find(layers.begin(), layers.end(), o) == layers.end())
                    layers.push_back(o);
        return layers;
    };
    struct Found {
        Open *open = nullptr;
        const erpfile::Entry *entry = nullptr;
    };
    auto top = [&](const std::vector<std::string> &layers, const std::string &name) {
        Found f;
        for (auto &l : layers) {
            Open *o = get(l);
            const erpfile::Entry *e = o ? o->arc.find(name) : nullptr;
            if (e)
                f = {o, e};
        }
        return f;
    };
    auto carrying = [&](const std::vector<std::string> &layers, const std::string &name) {
        std::vector<std::string> out_layers;
        for (auto &l : layers) {
            Open *o = get(l);
            if (o && o->arc.find(name))
                out_layers.push_back(l);
        }
        return out_layers;
    };
    std::vector<std::string> db_layers = layers_of(DATABASES);
    Open *db = get(DATABASES);
    if (db)
        out.read.insert(DATABASES);
    Found uf = db ? top(db_layers, OVERRIDES) : Found{};
    const erpfile::Entry *ue = uf.entry;
    std::string uidb_bytes;
    if (!ue || ue->frags.empty() || !erpfile::fragment(*uf.open->src, uf.open->arc, ue->frags[0], uidb_bytes, why)) {
        refuse("overrides.uidb is not in " + std::string(DATABASES) + (why.empty() ? "" : " (" + why + ")"),
               wants.empty() ? "" : wants[0].array + "/" + wants[0].key, int(wants.size()));
        return false;
    }
    uidb::Pkg pkg;
    if (!pkg.read(uidb_bytes)) {
        refuse("overrides.uidb does not read: " + pkg.why, wants.empty() ? "" : wants[0].array + "/" + wants[0].key,
               int(wants.size()));
        return false;
    }
    std::vector<std::string> files = env.list();
    std::map<std::string, std::vector<const Want *>> by_array;
    for (auto &w : wants)
        by_array[w.array].push_back(&w);
    bool uidb_changed = false;
    std::set<std::string> viewed;
    for (auto &[array, list] : by_array) {
        uidb::Node *node = pkg.find(list_path(array));
        if (!node || node->kind != uidb::Node::array) {
            refuse("overrides.uidb has no list " + array, array + "/" + list[0]->key, int(list.size()));
            continue;
        }
        std::vector<std::string> names;
        for (auto &e : node->list)
            names.push_back(uidb::Pkg::text_of(e, "Name"));
        std::string image = ARRAY_URI + array + ".texarray.image", view = ARRAY_URI + array + ".texarray";
        struct Copy {
            logo_array::Stock stock;
            logo_array::Shape shape;
            std::vector<std::string> carriers;
            std::vector<logo_array::Slice> add;
            bool outside = false;
            uint32_t had = 0;
        };
        std::vector<Copy> copies;
        std::map<std::string, std::vector<std::string>> skipped;
        std::map<std::string, std::string> repaired;
        std::map<std::string, std::vector<std::string>> image_in, view_in;
        auto where = [](const std::vector<std::string> &rels) {
            return rels[0] + (rels.size() > 1 ? " and " + std::to_string(rels.size() - 1) + " more archive(s)" : "");
        };
        for (auto &rel : files) {
            if (!get(rel))
                continue;
            std::vector<std::string> layers = layers_of(rel);
            Found fi = top(layers, image), fv = top(layers, view);
            if (!fi.entry && !fv.entry)
                continue;
            for (auto &l : carrying(layers, image))
                image_in[rel].push_back(l), out.read.insert(l);
            for (auto &l : carrying(layers, view))
                view_in[rel].push_back(l), out.read.insert(l);
            out.read.insert(rel);
            const erpfile::Entry *ei = fi.entry, *ev = fv.entry;
            bool outside = rel.compare(0, std::char_traits<char>::length(UI_FOLDER), UI_FOLDER) != 0;
            bool lent = ei && !ev && !copies.empty() && outside;
            logo_array::Stock got;
            if (!ei || (!ev && !lent) || ei->frags.size() != 2 || (ev && ev->frags.size() != 1) ||
                !erpfile::fragment(*fi.open->src, fi.open->arc, ei->frags[0], got.head, why) ||
                !erpfile::fragment(*fi.open->src, fi.open->arc, ei->frags[1], got.data, why) ||
                (ev && !erpfile::fragment(*fv.open->src, fv.open->arc, ev->frags[0], got.view, why))) {
                skipped["a layout this build does not read"].push_back(rel);
                continue;
            }
            if (lent) {
                got.view = copies.front().stock.view;
                if (got.head.size() < logo_array::HEAD_SIZE || got.view.size() < logo_array::VIEW_COUNT + 4) {
                    skipped["a layout this build does not read"].push_back(rel);
                    continue;
                }
                uint32_t held = logo_array::le32(got.head, logo_array::HEAD_COUNT);
                logo_array::put32(got.view, logo_array::VIEW_COUNT, held);
                out.lines.push_back("team.logo: " + array + " in " + rel + " is an image of " + std::to_string(held) +
                                    " slice(s) with no view; it is built with the view of " + copies.front().carriers[0]);
            }
            uint32_t said = 0;
            if (logo_array::repair_view(got, said)) {
                repaired[rel] = got.view;
                if (viewed.insert(array).second)
                    out.lines.push_back("team.logo: " + array + " view said " + std::to_string(said) + " slices for " +
                                        std::to_string(logo_array::le32(got.head, logo_array::HEAD_COUNT)) + "; fixed");
            }
            auto same = std::find_if(copies.begin(), copies.end(), [&](const Copy &c) {
                return c.stock.head == got.head && c.stock.data == got.data && c.stock.view == got.view;
            });
            if (same != copies.end()) {
                same->carriers.push_back(rel);
                same->outside = same->outside && outside;
                continue;
            }
            logo_array::Shape shape;
            if (!logo_array::shape_of_images(got, shape, why)) {
                skipped[why].push_back(rel);
                continue;
            }
            copies.push_back({std::move(got), shape, {rel}, {}, outside, shape.count});
        }
        for (auto &[rel, bytes] : repaired)
            for (auto &l : view_in[rel])
                out.changes[l][view] = {bytes};
        size_t building = 0;
        for (auto &c : copies)
            building += c.carriers.size();
        for (auto &[reason, rels] : skipped)
            out.lines.push_back("team.logo: skipped " + array + " in " + where(rels) + ": " + reason + "; left as it is" +
                                (copies.empty() ? "" : ", the other " + std::to_string(building) + " archive(s) build"));
        if (copies.empty()) {
            std::string reason = skipped.empty()       ? array + " is in no archive of the ui_package folder"
                                 : skipped.size() == 1 ? array + ": " + skipped.begin()->first
                                                       : "no archive's copy of " + array + " can grow";
            refuse(reason, array + "/" + list[0]->key, int(list.size()));
            continue;
        }
        size_t keys = names.size();
        uint32_t most = 0;
        for (auto &c : copies)
            most = std::max(most, c.shape.count);
        if (copies.size() > 1) {
            std::string counts;
            for (auto &c : copies)
                counts += (counts.empty() ? "" : ", ") + std::to_string(c.shape.count);
            out.lines.push_back("team.logo: the " + std::to_string(building) + " archives that carry " + array + " hold " +
                                std::to_string(copies.size()) + " different copies (" + counts +
                                " slices); each keeps its slices and grows to the same key list");
        }
        if (names.size() < most) {
            size_t unnamed = most - names.size();
            std::string first = std::to_string(names.size()), failed;
            std::set<std::string> used(names.begin(), names.end());
            for (size_t n = names.size(); n < most && failed.empty(); n++) {
                std::string pad = "swofty_pad_" + std::to_string(n);
                while (!used.insert(pad).second)
                    pad += "_";
                if (!pkg.append_entry(*node, pad, std::string(TEXTURE_PATH) + pad + ".tif", failed))
                    break;
                names.push_back(pad);
            }
            if (!failed.empty()) {
                refuse(array + ": " + failed, array + "/" + list[0]->key, int(list.size()));
                continue;
            }
            uidb_changed = true;
            out.lines.push_back("team.logo: " + array + " holds " + std::to_string(most) + " slices for " + std::to_string(keys) +
                                " keys: other addons added slices without keys, so " + std::to_string(unnamed) +
                                " placeholder key(s) from slice " + first + " on name them and new logos land on their own keys");
        }
        std::map<std::pair<uint32_t, uint32_t>, std::string> blanks;
        auto fill = [&](Copy &c) {
            std::string in = copies.size() > 1 ? " in " + where(c.carriers) : "";
            const Copy *donor = nullptr;
            if (c.outside)
                for (auto &d : copies)
                    if (!d.outside && d.shape.width == c.shape.width && d.shape.height == c.shape.height && d.had > c.had &&
                        (!donor || d.had > donor->had))
                        donor = &d;
            if (donor) {
                out.lines.push_back("team.logo: " + array + in + " holds " + std::to_string(c.had) + " slice(s) for " +
                                    std::to_string(names.size()) + " key(s); slices " + std::to_string(c.had) + ".." +
                                    std::to_string(donor->had - 1) + " are copied from " + donor->carriers[0] +
                                    " so every key reads its own logo from this copy too");
                c.stock.data += donor->stock.data.substr(size_t(c.had) * c.shape.slice_bytes());
                logo_array::put32(c.stock.head, logo_array::HEAD_COUNT, uint32_t(names.size()));
                logo_array::put32(c.stock.view, logo_array::VIEW_COUNT, uint32_t(names.size()));
                c.shape.count = uint32_t(names.size());
                c.stock.names = names;
                return;
            }
            if (c.shape.count < keys)
                out.lines.push_back(
                    "team.logo: " + array + in + " lists " + std::to_string(keys) + " keys but its image holds " +
                    std::to_string(c.shape.count) + " slice(s): another addon replaced the whole array, so " +
                    std::to_string(keys - c.shape.count) + " key(s) from " + names[c.shape.count] +
                    " on have no image of their own; padded with blank slices so new logos land on their own keys");
            else if (c.shape.count < names.size())
                out.lines.push_back("team.logo: " + array + in + " holds " + std::to_string(c.shape.count) + " slice(s), " +
                                    std::to_string(names.size() - c.shape.count) +
                                    " fewer than another archive's copy; padded with blank slices so new logos land on the same "
                                    "slice in every archive");
            if (c.shape.count < names.size()) {
                auto [b, fresh] = blanks.try_emplace({c.shape.width, c.shape.height});
                if (fresh)
                    b->second =
                        bc7::encode_logo(reinterpret_cast<const uint8_t *>(std::string(c.shape.slice_bytes() * 4, '\0').data()),
                                         int(c.shape.width), int(c.shape.height));
                logo_array::pad(c.stock, c.shape, b->second, uint32_t(names.size()));
                c.shape.count = uint32_t(names.size());
            }
            c.stock.names = names;
        };
        for (auto &c : copies)
            if (!c.outside)
                fill(c);
        for (auto &c : copies)
            if (c.outside)
                fill(c);
        std::vector<logo_array::Shape> shapes;
        for (auto &c : copies)
            if (std::none_of(shapes.begin(), shapes.end(),
                             [&](const logo_array::Shape &s) { return s.width == c.shape.width && s.height == c.shape.height; }))
                shapes.push_back(c.shape);
        auto encode_start = std::chrono::steady_clock::now();
        std::vector<std::vector<Encoded>> encodes = encode_slices(list, shapes, env.slices, &out.slices);
        out.encode_ms += std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - encode_start).count();
        for (const Want *w : list)
            if (w->donor.empty()) {
                out.encodes += shapes.size();
                Planned &p = out.planned[w];
                p.source = w->source;
                for (auto &sh : shapes)
                    if (std::find(p.sizes.begin(), p.sizes.end(), std::make_pair(sh.width, sh.height)) == p.sizes.end())
                        p.sizes.push_back({sh.width, sh.height});
            }
        auto encode_of = [&](size_t want, const logo_array::Shape &shape) -> Encoded & {
            size_t at = 0;
            while (shapes[at].width != shape.width || shapes[at].height != shape.height)
                at++;
            return encodes[want][at];
        };
        std::set<std::string> seen(names.begin(), names.end());
        std::vector<const Want *> used;
        for (size_t wi = 0; wi < list.size(); wi++) {
            const Want *w = list[wi];
            if (!seen.insert(w->key).second) {
                out.lines.push_back("team.logo: " + array + " lists " + w->key + " already; its slice is kept");
                continue;
            }
            std::vector<std::string> slices(copies.size());
            if (!w->donor.empty()) {
                auto at = std::find(names.begin(), names.end(), w->donor);
                if (at == names.end()) {
                    out.lines.push_back("team.logo: " + array + " has no stock key " + w->donor + " for " + w->key +
                                        (w->fallback ? "; the donor fallback is left out" : ""));
                    if (!w->fallback)
                        refuse(array + " has no stock key to copy", w->donor + " for " + w->key);
                    seen.erase(w->key);
                    continue;
                }
                for (size_t i = 0; i < copies.size(); i++) {
                    size_t bytes = copies[i].shape.slice_bytes();
                    slices[i] = copies[i].stock.data.substr(size_t(at - names.begin()) * bytes, bytes);
                }
            } else {
                std::map<std::pair<uint32_t, uint32_t>, std::string> encoded;
                bool ok = true;
                for (size_t i = 0; i < copies.size() && ok; i++) {
                    const logo_array::Shape &shape = copies[i].shape;
                    auto [e, fresh] = encoded.try_emplace({shape.width, shape.height});
                    bool scaled = false;
                    if (fresh) {
                        Encoded &done = encode_of(wi, shape);
                        if (!(ok = done.ok)) {
                            why = done.why;
                            break;
                        }
                        e->second = std::move(done.bytes);
                        scaled = done.scaled;
                    }
                    if (scaled)
                        out.lines.push_back("team.logo: " + w->key + " scaled " + std::to_string(w->width) + "x" +
                                            std::to_string(w->height) + " -> " + std::to_string(shape.width) + "x" +
                                            std::to_string(shape.height) + " for " + array);
                    slices[i] = e->second;
                }
                if (!ok) {
                    refuse(array + ": " + why, w->key);
                    seen.erase(w->key);
                    continue;
                }
            }
            for (size_t i = 0; i < copies.size(); i++)
                copies[i].add.push_back({w->key, std::move(slices[i])});
            used.push_back(w);
        }
        if (used.empty())
            continue;
        size_t grew = 0, archives = 0;
        for (auto &c : copies) {
            logo_array::Stock grown;
            if (!logo_array::grow(c.stock, c.add, grown, why)) {
                if (copies.size() > 1)
                    out.lines.push_back("team.logo: skipped " + array + " in " + where(c.carriers) + ": " + why +
                                        "; left as it is");
                continue;
            }
            for (auto &rel : c.carriers) {
                for (auto &l : image_in[rel])
                    out.changes[l][image] = {grown.head, grown.data};
                for (auto &l : view_in[rel])
                    out.changes[l][view] = {grown.view};
            }
            grew++;
            archives += c.carriers.size();
        }
        if (!grew) {
            refuse(array + ": " + why, array + "/" + used[0]->key, int(used.size()));
            continue;
        }
        for (size_t at = 0; at < used.size(); at++)
            out.lines.push_back(
                "team.logo: " + array + " slice " + std::to_string(names.size() + at) + " = " + used[at]->key +
                (used[at]->donor.empty()
                     ? " (its image)"
                     : " (copy of " + used[at]->donor + ", slice " +
                           std::to_string(std::find(names.begin(), names.end(), used[at]->donor) - names.begin()) + ")"));
        for (auto *w : used) {
            std::string e;
            if (!pkg.append_entry(*node, w->key, std::string(TEXTURE_PATH) + w->key + ".tif", e)) {
                refuse(array + ": " + e, w->key);
            }
        }
        uidb_changed = true;
        out.built += int(used.size());
        out.lines.push_back("team.logo: " + array + " grows by " + std::to_string(used.size()) + " slice(s) in " +
                            std::to_string(archives) + " archive(s)" +
                            (grew > 1 ? " (" + std::to_string(grew) + " different copies)" : ""));
    }
    if (uidb_changed) {
        std::string written = pkg.write();
        for (auto &l : carrying(db_layers, OVERRIDES)) {
            out.changes[l][OVERRIDES] = {written};
            out.read.insert(l);
        }
    }
    return true;
}

}
