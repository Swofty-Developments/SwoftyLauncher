#pragma once

#include "core/runtime/hashing.hpp"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <map>
#include <set>
#include <string>
#include <vector>

// The key the team logo archives are cached under. The logos are built from the bytes of a few archives: databases.erp
// and the archives that carry a wanted texture array (the inputs). An install copies its archives into the game folder
// again before every session, and a rollback copies the stock ones back, so a write time changes while the bytes stay
// the same. Each input counts by a hash of its bytes. The stamps file next to the cached archives names the inputs of
// the last build and keeps each hash under the size and write time it was taken at; only an input whose size or write
// time differs is read again. Every other archive counts by its path and size and is never read: the build only looks
// in it for a wanted array, and an archive that gains one grows. databases.erp counts by what the build reads from it
// (logo_build::databases_hash): an install writes the addon option values into it before every session, and the
// build reads only its overrides.uidb. The manifest names the hash of all of databases.erp its copy was made from
// (SOURCE_LINE); a cached build whose databases.erp has changed since gets that copy made again from the new one.

namespace swofty_core::apply::content::logo_cache {

inline constexpr const char *STAMPS = "inputs.stamps";
inline constexpr const char *OLD_STAMPS = "archives.stamps";
inline constexpr const char *STAMPS_FORMAT = "#format\t2";

class Bytes {
public:
    void add(const char *p, size_t n)
    {
        total_ += n;
        while (have_ && n) {
            pending_[have_++] = *p++;
            n--;
            if (have_ == sizeof pending_) {
                block(pending_);
                have_ = 0;
            }
        }
        if (have_)
            return;
        for (; n >= sizeof pending_; p += sizeof pending_, n -= sizeof pending_)
            block(p);
        std::memcpy(pending_, p, n);
        have_ = n;
    }

    uint64_t done() const
    {
        uint64_t l[4] = {lane_[0], lane_[1], lane_[2], lane_[3]};
        for (size_t i = 0; i < have_; i++)
            l[i % 4] = mix(l[i % 4], uint8_t(pending_[i]) + 1);
        uint64_t h = total_ * 0x9e3779b97f4a7c15ull;
        for (auto v : l)
            h = mix(h, v);
        return h ^ (h >> 31);
    }

private:
    static uint64_t mix(uint64_t h, uint64_t v)
    {
        h = (h ^ v) * 0xff51afd7ed558ccdull;
        return h ^ (h >> 29);
    }

    void block(const char *p)
    {
        for (int i = 0; i < 4; i++) {
            uint64_t v;
            std::memcpy(&v, p + 8 * i, 8);
            lane_[i] = mix(lane_[i], v);
        }
    }

    uint64_t lane_[4] = {0x243f6a8885a308d3ull, 0x13198a2e03707344ull, 0xa4093822299f31d0ull, 0x082efa98ec4e6c89ull};
    char pending_[32];
    size_t have_ = 0;
    uint64_t total_ = 0;
};

struct Seen {
    uint64_t size = 0, time = 0, hash = 0;
    bool hashed = false;
};

using Stamps = std::map<std::string, Seen>;

inline Stamps parse(const std::string &text)
{
    Stamps out;
    size_t at = 0;
    while (at < text.size()) {
        size_t end = text.find('\n', at);
        if (end == std::string::npos)
            end = text.size();
        std::string line = text.substr(at, end - at);
        at = end + 1;
        std::vector<std::string> f;
        for (size_t from = 0;;) {
            size_t tab = line.find('\t', from);
            f.push_back(line.substr(from, tab == std::string::npos ? std::string::npos : tab - from));
            if (tab == std::string::npos)
                break;
            from = tab + 1;
        }
        if (f.size() != 4 || f[0].empty())
            continue;
        Seen s;
        char *stop = nullptr;
        s.size = std::strtoull(f[1].c_str(), &stop, 10);
        if (*stop)
            continue;
        s.time = std::strtoull(f[2].c_str(), &stop, 10);
        if (*stop)
            continue;
        if (f[3] != "-") {
            s.hash = std::strtoull(f[3].c_str(), &stop, 16);
            if (*stop || f[3].empty())
                continue;
            s.hashed = true;
        }
        out[f[0]] = s;
    }
    return out;
}

inline bool stamps_current(const std::string &text)
{
    return text.compare(0, std::strlen(STAMPS_FORMAT) + 1, std::string(STAMPS_FORMAT) + "\n") == 0;
}

inline std::string text(const Stamps &stamps)
{
    std::string out = std::string(STAMPS_FORMAT) + "\n";
    char hex[17];
    for (auto &[rel, s] : stamps) {
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(s.hash));
        out += rel + "\t" + std::to_string(s.size) + "\t" + std::to_string(s.time) + "\t" + (s.hashed ? hex : "-") + "\n";
    }
    return out;
}

inline constexpr const char *CACHE_FORMAT = "|logo-cache-4";
inline constexpr const char *BUILT_LINE = "#built";
inline constexpr const char *SOURCE_LINE = "#source";

using Routes = std::vector<std::pair<std::string, std::string>>;
using Sources = std::map<std::string, uint64_t>;

inline std::string manifest_text(int built, const Routes &routes, const Sources &sources = {})
{
    std::string out = std::string(BUILT_LINE) + "\t" + std::to_string(built) + "\n";
    char hex[17];
    for (auto &[rel, hash] : sources) {
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(hash));
        out += std::string(SOURCE_LINE) + "\t" + rel + "\t" + hex + "\n";
    }
    for (auto &[rel, file] : routes)
        out += rel + "\t" + file + "\n";
    return out;
}

inline bool manifest_parse(const std::string &text, const std::function<bool(const std::string &file)> &exists, int &built, Routes &routes,
                           Sources *sources = nullptr)
{
    built = -1;
    routes.clear();
    if (sources)
        sources->clear();
    size_t at = 0;
    while (at < text.size()) {
        size_t end = text.find('\n', at);
        if (end == std::string::npos)
            end = text.size();
        std::string line = text.substr(at, end - at);
        at = end + 1;
        size_t tab = line.find('\t');
        if (tab == std::string::npos)
            continue;
        if (line.compare(0, tab, BUILT_LINE) == 0)
            built = std::atoi(line.c_str() + tab + 1);
        else if (line.compare(0, tab, SOURCE_LINE) == 0) {
            size_t at_hash = line.rfind('\t');
            char *stop = nullptr;
            uint64_t hash = std::strtoull(line.c_str() + at_hash + 1, &stop, 16);
            if (sources && at_hash > tab && !*stop && at_hash + 1 < line.size())
                (*sources)[line.substr(tab + 1, at_hash - tab - 1)] = hash;
        } else
            routes.emplace_back(line.substr(0, tab), line.substr(tab + 1));
    }
    bool whole = built >= 0 && !routes.empty();
    for (auto &r : routes)
        whole = whole && exists(r.second);
    if (!whole) {
        routes.clear();
        if (sources)
            sources->clear();
    }
    return whole;
}

inline bool cacheable(size_t routes, size_t changes, int refused)
{
    return routes && routes == changes && refused == 0;
}

struct Env {
    std::function<bool(const std::string &rel, uint64_t &size, uint64_t &time)> stat;
    std::function<bool(const std::string &rel, uint64_t &hash)> hash;
};

struct Keyed {
    uint64_t key = 0;
    Stamps stamps;
    size_t hashed = 0;
    uint64_t hashed_bytes = 0;
};

inline std::set<std::string> inputs_of(const Stamps &stamps)
{
    std::set<std::string> out;
    for (auto &[rel, s] : stamps)
        out.insert(rel);
    return out;
}

inline Keyed key_archives(uint64_t key, const std::vector<std::string> &files, const std::set<std::string> &inputs, const Env &env,
                          const Stamps &before)
{
    Keyed out;
    std::vector<std::string> ordered = files;
    std::sort(ordered.begin(), ordered.end());
    ordered.erase(std::unique(ordered.begin(), ordered.end()), ordered.end());
    for (auto &rel : ordered) {
        Seen now;
        if (!env.stat(rel, now.size, now.time))
            continue;
        if (!inputs.count(rel)) {
            key = runtime::fnv64_field(key, rel + "|" + std::to_string(now.size) + "|s");
            continue;
        }
        auto was = before.find(rel);
        if (was != before.end() && was->second.hashed && was->second.size == now.size && was->second.time == now.time) {
            now = was->second;
        } else if (env.hash(rel, now.hash)) {
            now.hashed = true;
            out.hashed++;
            out.hashed_bytes += now.size;
        }
        char hex[17];
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(now.hash));
        key = runtime::fnv64_field(key, rel + "|" + (now.hashed ? std::string("h") + hex : std::to_string(now.size) + "|t" + std::to_string(now.time)));
        out.stamps[rel] = now;
    }
    out.key = key;
    return out;
}

}
