#pragma once
#include "json.hpp"

#include <cstdint>
#include <cstring>
#include <map>
#include <string>
#include <vector>

// Rewrites the team and driver ids of the game's outgoing UDP telemetry packets. The launcher writes
// swofty/udp_ids.json (packages/core/src/udp_ids.rs): added team id -> stock ancestor id, added driver id ->
// stock ancestor id or 255, each as a full table (`teams`, `drivers`, read by the 16-bit 2026 format) and
// as a byte table (`teams_byte`, `drivers_byte`, read by the 2024 and 2025 formats, which carry one byte).
//
// A buffer is touched only when it parses as an F1 packet: m_packetFormat (u16 at 0) is 2024, 2025 or
// 2026, m_packetId (u8 at 6) is one of the packets below, and the length is exactly that packet's size in
// that format. Fields rewritten, offsets from the packet start:
//   Participants (4), 2024: 1350 bytes; numActiveCars at 29, entries from 30, 60 bytes each, driverId at
//     entry+1, teamId at entry+3, both u8.
//   Participants (4), 2025: 1284 bytes; the same, 57 bytes each.
//   Participants (4), 2026: 1470 bytes; entries from 30, 60 bytes each, 24 cars; driverId u16 at entry+1,
//     teamId u16 at entry+5 (the game copies its internal record whole; the 2025 builder reads the same
//     record's driver id from +1 and team id from +5).
//   Lobby Info (9), 2024: 1306 bytes; numPlayers at 29, entries from 30, 58 bytes each, teamId u8 at entry+1.
//   Lobby Info (9), 2025: 954 bytes; 42 bytes each, teamId u8 at entry+1.
//   Time Trial (14), 2024 and 2025: 101 bytes; three data sets of 24 bytes from 29, teamId u8 at set+1.
// The 2026 Lobby Info and Time Trial packets are left alone.

namespace runtime::udp_ids {

constexpr size_t HEADER = 29;
constexpr size_t MAX_PACKET = 1500;
constexpr int NETWORK_DRIVER = 255;

struct Map {
    int16_t team8[256];
    int16_t driver8[256];
    std::map<int, int> teams, drivers;
    std::vector<std::string> lines;
    bool any = false;

    Map()
    {
        for (auto &v : team8)
            v = -1;
        for (auto &v : driver8)
            v = -1;
    }
};

inline bool load(const std::string &text, Map &out, std::string &why)
{
    json::Value v;
    if (!json::parse(text, v) || v.type != json::Value::Obj) {
        why = "not a JSON object";
        return false;
    }
    if (v["version"].num() != 1) {
        why = "unknown version";
        return false;
    }
    Map m;
    auto table = [&](const char *key, std::map<int, int> &full) {
        for (auto &[k, to] : v[key].o) {
            long long id = strtoll(k.c_str(), nullptr, 10), t = to.num(-1);
            if (id >= 0 && id <= 0xffff && t >= 0 && t <= 0xffff)
                full[int(id)] = int(t);
        }
    };
    table("teams", m.teams);
    table("drivers", m.drivers);
    for (auto &[k, to] : v["teams_byte"].o) {
        long long id = strtoll(k.c_str(), nullptr, 10), t = to.num(-1);
        if (id >= 0 && id < 256 && t >= 0 && t < 256)
            m.team8[id] = int16_t(t);
    }
    for (auto &[k, to] : v["drivers_byte"].o) {
        long long id = strtoll(k.c_str(), nullptr, 10), t = to.num(-1);
        if (id >= 0 && id < 256 && t >= 0 && t < 256)
            m.driver8[id] = int16_t(t);
    }
    for (auto &l : v["lines"].a)
        m.lines.push_back(l.str());
    m.any = !m.teams.empty() || !m.drivers.empty();
    out = std::move(m);
    return true;
}

struct Layout {
    int format, id;
    size_t size, stride, max_entries;
    size_t driver, team;
    bool wide;
    bool fixed_sets;
};

inline const Layout *layouts(size_t &n)
{
    static const Layout table[] = {
        {2024, 4, 1350, 60, 22, 1, 3, false, false},
        {2025, 4, 1284, 57, 22, 1, 3, false, false},
        {2026, 4, 1470, 60, 24, 1, 5, true, false},
        {2024, 9, 1306, 58, 22, 0, 1, false, false},
        {2025, 9, 954, 42, 22, 0, 1, false, false},
        {2024, 14, 101, 24, 3, 0, 1, false, true},
        {2025, 14, 101, 24, 3, 0, 1, false, true},
    };
    n = sizeof table / sizeof *table;
    return table;
}

inline const Layout *identify(const uint8_t *p, size_t len)
{
    if (!p || len < HEADER + 1 || len > MAX_PACKET)
        return nullptr;
    int format = p[0] | p[1] << 8;
    if (format != 2024 && format != 2025 && format != 2026)
        return nullptr;
    size_t n;
    const Layout *t = layouts(n);
    for (size_t i = 0; i < n; i++)
        if (t[i].format == format && t[i].id == p[6] && t[i].size == len)
            return &t[i];
    return nullptr;
}

struct Change {
    bool driver;
    int from, to;
};

inline bool rewrite(const Map &m, uint8_t *p, size_t len, std::vector<Change> *changes = nullptr, int *format = nullptr,
                    int *id = nullptr)
{
    if (!m.any)
        return false;
    const Layout *l = identify(p, len);
    if (!l)
        return false;
    if (format)
        *format = l->format;
    if (id)
        *id = l->id;
    size_t count = l->max_entries;
    size_t first = HEADER;
    if (!l->fixed_sets) {
        count = p[HEADER];
        first = HEADER + 1;
        if (count > l->max_entries)
            return false;
    }
    bool changed = false;
    auto edit = [&](uint8_t *at, bool driver) {
        int from = l->wide ? at[0] | at[1] << 8 : at[0], to = -1;
        if (l->wide) {
            auto &t = driver ? m.drivers : m.teams;
            auto it = t.find(from);
            to = it == t.end() ? -1 : it->second;
        } else {
            to = driver ? m.driver8[from] : m.team8[from];
        }
        if (to < 0 || to == from)
            return;
        at[0] = uint8_t(to);
        if (l->wide)
            at[1] = uint8_t(to >> 8);
        changed = true;
        if (changes)
            changes->push_back({driver, from, to});
    };
    for (size_t i = 0; i < count; i++) {
        uint8_t *e = p + first + i * l->stride;
        if (l->id == 4)
            edit(e + l->driver, true);
        edit(e + l->team, false);
    }
    return changed;
}

}
