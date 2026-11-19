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
// and the archives that carry a wanted texture array (the inputs). Each input counts by a hash of its bytes. The stamps
// file next to the cached archives names the inputs of the last build and keeps each hash under the stamp of the file it
// was taken from, by the launcher's digest rule: size, write time, change time and file id. A copy put back with its old
// size and write time is a new file with a new change time, so it is read again; a file the launcher links into the game
// folder again keeps its id, size and write time, and linking it moves only its change time, so for a file with more
// than one link the change time is not compared. A hash is kept only when the file's last write or change is two seconds
// old and the stamp read after hashing equals the one before. Every other archive counts by its path and size and is
// never read: the build only looks in it for a wanted array, and an archive that gains one grows. databases.erp counts
// by what the build reads from it (logo_build::databases_hash): an install writes the addon option values into it
// before every session, and the build reads only its overrides.uidb. The manifest names the hash of all of databases.erp
// its copy was made from (SOURCE_LINE); a cached build whose databases.erp has changed since gets that copy made again
// from the new one.

namespace swofty_core::apply::content::logo_cache {

inline constexpr const char *STAMPS = "inputs.stamps";
inline constexpr const char *OLD_STAMPS = "archives.stamps";
inline constexpr const char *STAMPS_FORMAT = "#format\t3";
inline constexpr uint64_t SETTLE = 20000000;

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
    uint64_t size = 0, time = 0, changed = 0, id = 0;
    uint32_t links = 0;
    uint64_t hash = 0;
    bool hashed = false;
};

using Stamps = std::map<std::string, Seen>;

inline bool same_file(const Seen &was, const Seen &now)
{
    return was.changed && was.size == now.size && was.time == now.time && was.id == now.id &&
           (was.changed == now.changed || was.links > 1 || now.links > 1);
}

inline bool unmoved(const Seen &a, const Seen &b)
{
    return a.size == b.size && a.time == b.time && a.changed == b.changed && a.id == b.id && a.links == b.links;
}

inline bool settled(const Seen &s, uint64_t now)
{
    uint64_t last = std::max(s.time, s.changed);
    return now >= last && now - last >= SETTLE;
}

inline bool number(const std::string &f, uint64_t &out, int base = 10)
{
    if (f.empty() || f.size() > (base == 16 ? 16u : 20u))
        return false;
    uint64_t v = 0;
    for (char c : f) {
        int d = c >= '0' && c <= '9'                 ? c - '0'
                : base == 16 && c >= 'a' && c <= 'f' ? c - 'a' + 10
                : base == 16 && c >= 'A' && c <= 'F' ? c - 'A' + 10
                                                     : -1;
        if (d < 0 || v > (UINT64_MAX - uint64_t(d)) / uint64_t(base))
            return false;
        v = v * uint64_t(base) + uint64_t(d);
    }
    out = v;
    return true;
}

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
        if ((f.size() != 4 && f.size() != 7) || f[0].empty())
            continue;
        Seen s;
        uint64_t links = 0;
        if (!number(f[1], s.size) || !number(f[2], s.time))
            continue;
        if (f.size() == 7 && (!number(f[3], s.changed) || !number(f[4], s.id) || !number(f[5], links) || links > UINT32_MAX))
            continue;
        s.links = uint32_t(links);
        const std::string &hash = f.back();
        if (hash != "-") {
            if (!number(hash, s.hash, 16))
                continue;
            s.hashed = true;
        }
        out[f[0]] = s;
    }
    return out;
}

inline std::string text(const Stamps &stamps)
{
    std::string out = std::string(STAMPS_FORMAT) + "\n";
    char hex[17];
    for (auto &[rel, s] : stamps) {
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(s.hash));
        out += rel + "\t" + std::to_string(s.size) + "\t" + std::to_string(s.time) + "\t" + std::to_string(s.changed) + "\t" +
               std::to_string(s.id) + "\t" + std::to_string(s.links) + "\t" + (s.hashed ? hex : "-") + "\n";
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

inline bool manifest_parse(const std::string &text, const std::function<bool(const std::string &file)> &exists, int &built,
                           Routes &routes, Sources *sources = nullptr)
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

inline bool cacheable(size_t routes, size_t changes, int refused) { return routes && routes == changes && refused == 0; }

struct Env {
    std::function<bool(const std::string &rel, uint64_t &size)> size;
    std::function<bool(const std::string &rel, Seen &now)> stat;
    std::function<bool(const std::string &rel, uint64_t &hash)> hash;
    std::function<uint64_t()> clock;
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

inline Keyed key_archives(uint64_t key, const std::vector<std::string> &files, const std::set<std::string> &inputs,
                          const Env &env, const Stamps &before)
{
    Keyed out;
    std::vector<std::string> ordered = files;
    std::sort(ordered.begin(), ordered.end());
    ordered.erase(std::unique(ordered.begin(), ordered.end()), ordered.end());
    for (auto &rel : ordered) {
        if (!inputs.count(rel)) {
            uint64_t size = 0;
            if (env.size(rel, size))
                key = runtime::fnv64_field(key, rel + "|" + std::to_string(size) + "|s");
            continue;
        }
        Seen now;
        if (!env.stat(rel, now))
            continue;
        Seen kept = now;
        auto was = before.find(rel);
        if (was != before.end() && was->second.hashed && same_file(was->second, now)) {
            now.hash = was->second.hash;
            now.hashed = true;
            kept = now;
        } else if (env.hash(rel, now.hash)) {
            now.hashed = true;
            out.hashed++;
            out.hashed_bytes += now.size;
            Seen after;
            if (env.stat(rel, after) && unmoved(now, after) && settled(now, env.clock()))
                kept = now;
        }
        char hex[17];
        std::snprintf(hex, sizeof hex, "%016llx", static_cast<unsigned long long>(now.hash));
        key = runtime::fnv64_field(
            key, rel + "|" + (now.hashed ? std::string("h") + hex : std::to_string(now.size) + "|t" + std::to_string(now.time)));
        out.stamps[rel] = kept;
    }
    out.key = key;
    return out;
}

}
