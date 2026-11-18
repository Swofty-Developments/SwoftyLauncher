#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

#include <zstd.h>

#include "core/career/snapshots/items.hpp"

// The swofty_registry save entry, without the game (packages/sdk/std/fixtures/format.md has the byte
// layout). The payload is a list of tagged items; a career is one item, and inside it the freeze mode,
// the stat-id map, the registry majors used and one item per season hold the frozen records of each
// snapshot registry, compressed one registry and season at a time. Every item keeps its own bytes, so
// what this core does not know (a newer tag, an unknown registry, a newer major) is written back byte
// for byte, and a re-encode only changes the items this core froze.
//
// The game carries the payload as a property bag: bag.hpp.

namespace career::snapshots::format {

constexpr uint16_t ENTRY_MAJOR = 1, ENTRY_MINOR = 0;
constexpr const char *ENTRY_NAME = "swofty_registry";
constexpr const char *FORMAT_TAG = "swofty_registry/1";
constexpr size_t CAREER_CAP = 256 * 1024;
constexpr size_t ENTRY_CAP = 4u << 20;
constexpr size_t MAX_RECORDS_RAW = 8u << 20;

enum Top : uint16_t { T_CAREER = 1, T_TEST_PAD = 0x7f01 };
enum CareerTag : uint16_t { C_ID = 1, C_FREEZE = 2, C_IDMAP = 3, C_MAJORS = 4, C_SEASON = 5 };
enum SeasonTag : uint16_t { S_YEAR = 1, S_REGISTRY = 2, S_REFUSED = 3 };
enum RegistryTag : uint16_t { R_NAME = 1, R_VERSION = 2, R_RECORDS = 3 };

enum class Freeze : uint8_t { per_season = 0, at_creation = 1 };

inline bool compress(const std::string &in, Bytes &out)
{
    size_t cap = ZSTD_compressBound(in.size());
    out.assign(cap, 0);
    size_t z = ZSTD_compress(out.data(), cap, in.data(), in.size(), 9);
    if (ZSTD_isError(z))
        return false;
    out.resize(z);
    return true;
}

inline bool decompress(const Bytes &in, std::string &out, size_t limit = MAX_RECORDS_RAW)
{
    unsigned long long size = ZSTD_getFrameContentSize(in.data(), in.size());
    if (size == ZSTD_CONTENTSIZE_ERROR || size == ZSTD_CONTENTSIZE_UNKNOWN || size > limit)
        return false;
    out.assign(size_t(size), '\0');
    size_t got = ZSTD_decompress(out.data(), out.size(), in.data(), in.size());
    return !ZSTD_isError(got) && got == size;
}

// One snapshot registry's frozen records for one season. version is what it was written with; body
// is its raw item list (decoded only when read), so it round-trips byte for byte.
struct Frozen {
    std::string registry;
    uint16_t major = 1, minor = 0;
    Bytes records;
    std::vector<Item> extra;

    Bytes encode() const
    {
        Writer w;
        w.item(R_NAME, registry);
        Writer v;
        v.u16(major), v.u16(minor);
        w.item(R_VERSION, v.out);
        w.item(R_RECORDS, records);
        for (auto &e : extra) {
            Bytes b = e.encoded();
            w.raw(b.data(), b.size());
        }
        return w.out;
    }

    static bool decode(const Bytes &b, Frozen &out)
    {
        std::vector<Item> list;
        if (!items(b, list))
            return false;
        bool name = false, version = false, recs = false;
        for (auto &it : list) {
            if (it.tag == R_NAME && !name)
                out.registry = text(it.body), name = true;
            else if (it.tag == R_VERSION && !version && it.body.size() >= 4) {
                Reader r(it.body);
                out.major = r.u16(), out.minor = r.u16(), version = true;
            } else if (it.tag == R_RECORDS && !recs)
                out.records = it.body, recs = true;
            else
                out.extra.push_back(it);
        }
        return name && version && recs && !out.registry.empty();
    }
};

struct Season {
    int32_t year = 0;
    std::vector<Frozen> frozen;
    std::vector<std::string> refused;
    std::vector<Item> extra;

    const Frozen *find(const std::string &registry) const
    {
        for (auto &f : frozen)
            if (f.registry == registry)
                return &f;
        return nullptr;
    }

    Bytes encode() const
    {
        Writer w;
        Writer y;
        y.u32(uint32_t(year));
        w.item(S_YEAR, y.out);
        for (auto &f : frozen)
            w.item(S_REGISTRY, f.encode());
        for (auto &r : refused)
            w.item(S_REFUSED, r);
        for (auto &e : extra) {
            Bytes b = e.encoded();
            w.raw(b.data(), b.size());
        }
        return w.out;
    }

    static bool decode(const Bytes &b, Season &out)
    {
        std::vector<Item> list;
        if (!items(b, list))
            return false;
        bool year = false;
        for (auto &it : list) {
            if (it.tag == S_YEAR && !year && it.body.size() == 4) {
                Reader r(it.body);
                out.year = int32_t(r.u32()), year = true;
            } else if (it.tag == S_REGISTRY) {
                Frozen f;
                if (!Frozen::decode(it.body, f))
                    return false;
                out.frozen.push_back(std::move(f));
            } else if (it.tag == S_REFUSED)
                out.refused.push_back(text(it.body));
            else
                out.extra.push_back(it);
        }
        return year;
    }
};

struct Career {
    std::string id;
    Freeze freeze = Freeze::per_season;
    std::string idmap = "{}";
    std::string majors = "{}";
    std::vector<Season> seasons;
    std::vector<Item> extra;
    // A career whose section could not be read is kept as its raw section, written back unchanged.
    bool unreadable = false;
    Bytes raw;

    Season *season(int32_t year)
    {
        for (auto &s : seasons)
            if (s.year == year)
                return &s;
        return nullptr;
    }
    const Season *season(int32_t year) const { return const_cast<Career *>(this)->season(year); }

    Bytes encode() const
    {
        if (unreadable)
            return raw;
        Writer w;
        w.item(C_ID, id);
        uint8_t f = uint8_t(freeze);
        w.item(C_FREEZE, &f, 1);
        Bytes map;
        compress(idmap, map);
        w.item(C_IDMAP, map);
        w.item(C_MAJORS, majors);
        for (auto &s : seasons)
            w.item(C_SEASON, s.encode());
        for (auto &e : extra) {
            Bytes b = e.encoded();
            w.raw(b.data(), b.size());
        }
        return w.out;
    }

    size_t size() const { return encode().size(); }

    static bool decode(const Bytes &b, Career &out)
    {
        std::vector<Item> list;
        if (!items(b, list))
            return false;
        bool id = false;
        for (auto &it : list) {
            if (it.tag == C_ID && !id)
                out.id = text(it.body), id = true;
            else if (it.tag == C_FREEZE && it.body.size() == 1)
                out.freeze = it.body[0] == 1 ? Freeze::at_creation : Freeze::per_season;
            else if (it.tag == C_IDMAP) {
                if (!decompress(it.body, out.idmap, 4u << 20))
                    return false;
            } else if (it.tag == C_MAJORS)
                out.majors = text(it.body);
            else if (it.tag == C_SEASON) {
                Season s;
                if (!Season::decode(it.body, s))
                    return false;
                out.seasons.push_back(std::move(s));
            } else
                out.extra.push_back(it);
        }
        return id && !out.id.empty();
    }
};

// The career id of a section that did not decode, when its id item at least reads.
inline std::string raw_id(const Bytes &b)
{
    std::vector<Item> list;
    if (!items(b, list) || list.empty() || list[0].tag != C_ID)
        return {};
    return text(list[0].body);
}

struct Entry {
    uint16_t major = ENTRY_MAJOR, minor = ENTRY_MINOR;
    std::vector<Career> careers;
    std::vector<Item> extra;

    Career *career(const std::string &id)
    {
        for (auto &c : careers)
            if (c.id == id)
                return &c;
        return nullptr;
    }

    Bytes encode() const
    {
        Writer body;
        for (auto &c : careers)
            body.item(T_CAREER, c.encode());
        for (auto &e : extra) {
            Bytes b = e.encoded();
            body.raw(b.data(), b.size());
        }
        Writer w;
        w.raw("SWRG", 4);
        w.u16(major), w.u16(minor);
        w.u32(crc32(body.out.data(), body.out.size()));
        w.raw(body.out.data(), body.out.size());
        return w.out;
    }
};

enum class Decoded { ok, corrupt, newer };

inline const char *decoded_name(Decoded d) { return d == Decoded::ok ? "ok" : d == Decoded::newer ? "newer" : "corrupt"; }

inline Decoded decode(const Bytes &b, Entry &out, std::string &why)
{
    out = Entry{};
    if (b.size() < 12 || memcmp(b.data(), "SWRG", 4) != 0)
        return why = "no SWRG header", Decoded::corrupt;
    Reader r(b);
    r.at = 4;
    out.major = r.u16(), out.minor = r.u16();
    uint32_t crc = r.u32();
    if (out.major > ENTRY_MAJOR)
        return why = "entry format " + std::to_string(out.major) + "." + std::to_string(out.minor) +
                     " is newer than this core's " + std::to_string(ENTRY_MAJOR),
               Decoded::newer;
    if (out.major < 1)
        return why = "entry format 0 is not a format", Decoded::corrupt;
    if (crc32(b.data() + 12, b.size() - 12) != crc)
        return why = "its checksum does not match", Decoded::corrupt;
    std::vector<Item> list;
    if (!items(b.data() + 12, b.size() - 12, list))
        return why = "an item runs past the end", Decoded::corrupt;
    for (auto &it : list) {
        if (it.tag != T_CAREER) {
            out.extra.push_back(it);
            continue;
        }
        Career c;
        if (!Career::decode(it.body, c)) {
            c = Career{};
            c.id = raw_id(it.body);
            c.unreadable = true;
            c.raw = it.body;
        }
        out.careers.push_back(std::move(c));
    }
    return Decoded::ok;
}

}
