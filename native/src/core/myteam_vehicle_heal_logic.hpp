#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace runtime::myteam_heal {

constexpr uint64_t TABLES = 0x328;
constexpr uint64_t TABLE_STRIDE = 0x10;
constexpr uint64_t SLOTS_BEGIN = 0x8;
constexpr uint64_t SLOTS_END = 0x10;
constexpr uint64_t SLOT_SIZE = 0x10;
constexpr uint64_t SLOT_VEHICLE = 0x8;
constexpr uint64_t VEHICLE_DATA = 0x10;
constexpr uint64_t VEHICLE_FILE_NAME = 0x70;
constexpr uint64_t VEHICLE_BASE = 0xcf2;
constexpr size_t VEHICLE_DATA_SIZE = 0xce6;
constexpr uint32_t NO_VEHICLE = 0x3fff0000;
constexpr size_t LABEL_CHARS = 0x100;
constexpr uint64_t ENTRANT_SIZE = 0x88;
constexpr uint64_t ENTRANT_VEHICLE = 0x5c;
constexpr uint64_t FRAME_FIRST_ENTRANT = 0x8;
constexpr uint8_t KIND_STOCK = 0;
constexpr uint8_t KIND_CUSTOM = 1;
constexpr uint8_t KINDS = 3;
constexpr uint16_t MAX_ID = 0x400;
constexpr size_t MAX_SLOTS = 0x4000;
constexpr size_t MAX_ENTRANTS = 128;
constexpr size_t LOG_IDS = 32;

inline bool switch_off(const char *value)
{
    if (!value)
        return false;
    return value[0] == '0' && value[1] == 0 ? true : value[0] == 'o' && value[1] == 'f' && value[2] == 'f' && value[3] == 0;
}

inline uint8_t kind(uint32_t handle)
{
    return static_cast<uint8_t>(handle & 0xff);
}

inline uint16_t id(uint32_t handle)
{
    return static_cast<uint16_t>(handle >> 16);
}

inline bool healable(uint32_t handle)
{
    return kind(handle) == KIND_CUSTOM && id(handle) < MAX_ID;
}

inline bool ends_with(const std::string &s, const char *tail)
{
    size_t n = 0;
    while (tail[n])
        n++;
    return s.size() >= n && s.compare(s.size() - n, n, tail) == 0;
}

inline bool starts_with(const std::string &s, const char *head)
{
    return s.rfind(head, 0) == 0;
}

inline constexpr const char *STOCK_2026[] = {"myteam_mercedes_2026", "myteam_ferrari_2026", "myteam_honda_2026",
                                             "myteam_redbull_ford_2026", "myteam_audi_2026"};
inline constexpr const char *STOCK_2025[] = {"myteam_mercedes", "myteam_ferrari", "myteam_honda", "myteam_renault"};
inline constexpr const char *ADDON_PREFIXES[] = {"myteam_mes_"};

inline bool stock_myteam(const std::string &label)
{
    if (!starts_with(label, "myteam_"))
        return false;
    for (const char *p : ADDON_PREFIXES)
        if (starts_with(label, p))
            return false;
    return true;
}

template <class Mem> bool slots(Mem &mem, uint64_t entrants, uint8_t k, uint64_t &begin, size_t &count)
{
    begin = 0;
    count = 0;
    uint64_t table = 0, end = 0;
    if (k >= KINDS || !mem.u64(entrants + TABLES + k * TABLE_STRIDE, table) || !table)
        return false;
    if (!mem.u64(table + SLOTS_BEGIN, begin) || !mem.u64(table + SLOTS_END, end) || end < begin || (end - begin) % SLOT_SIZE)
        return false;
    count = static_cast<size_t>((end - begin) / SLOT_SIZE);
    return true;
}

template <class Mem> uint64_t vehicle(Mem &mem, uint64_t entrants, uint32_t handle)
{
    uint64_t begin = 0, v = 0;
    size_t count = 0;
    if (!slots(mem, entrants, kind(handle), begin, count) || id(handle) >= count)
        return 0;
    if (!mem.u64(begin + id(handle) * SLOT_SIZE + SLOT_VEHICLE, v))
        return 0;
    return v;
}

template <class Mem> bool label(Mem &mem, uint64_t v, std::string &out)
{
    out.clear();
    return v && mem.text(v + VEHICLE_FILE_NAME, out) && !out.empty();
}

template <class Mem> bool season_2026(Mem &mem, uint64_t entrants, uint64_t first, uint64_t end)
{
    if (!first || end <= first)
        return false;
    size_t n = static_cast<size_t>((end - first) / ENTRANT_SIZE);
    if (n > MAX_ENTRANTS)
        n = MAX_ENTRANTS;
    std::string name;
    for (size_t i = 0; i < n; i++) {
        uint32_t handle = 0;
        if (!mem.u32(first + i * ENTRANT_SIZE + ENTRANT_VEHICLE, handle) || kind(handle) != KIND_STOCK)
            continue;
        if (label(mem, vehicle(mem, entrants, handle), name) && ends_with(name, "_2026"))
            return true;
    }
    return false;
}

struct Source {
    uint64_t vehicle = 0;
    uint32_t handle = NO_VEHICLE;
    std::string label;
    bool y2026 = false;
};

struct Stock {
    std::vector<std::pair<uint64_t, std::string>> myteam;
};

template <class Mem> Stock stock(Mem &mem, uint64_t entrants)
{
    Stock out;
    uint64_t begin = 0;
    size_t count = 0;
    if (!slots(mem, entrants, KIND_STOCK, begin, count))
        return out;
    if (count > MAX_SLOTS)
        count = MAX_SLOTS;
    std::string name;
    for (size_t i = 0; i < count; i++) {
        uint64_t v = 0;
        if (!mem.u64(begin + i * SLOT_SIZE + SLOT_VEHICLE, v) || !v)
            continue;
        if (label(mem, v, name) && stock_myteam(name))
            out.myteam.emplace_back(v, name);
    }
    return out;
}

inline const std::pair<uint64_t, std::string> *named(const Stock &s, const char *want)
{
    for (const auto &e : s.myteam)
        if (e.second == want)
            return &e;
    return nullptr;
}

inline bool pick(const Stock &s, bool y2026, Source &out)
{
    out = Source{};
    out.y2026 = y2026;
    const auto *first = y2026 ? STOCK_2026 : STOCK_2025;
    size_t first_n = y2026 ? sizeof STOCK_2026 / sizeof *STOCK_2026 : sizeof STOCK_2025 / sizeof *STOCK_2025;
    const auto *second = y2026 ? STOCK_2025 : STOCK_2026;
    size_t second_n = y2026 ? sizeof STOCK_2025 / sizeof *STOCK_2025 : sizeof STOCK_2026 / sizeof *STOCK_2026;
    for (size_t i = 0; i < first_n; i++)
        if (const auto *e = named(s, first[i])) {
            out.vehicle = e->first;
            out.label = e->second;
            return true;
        }
    for (const auto &e : s.myteam)
        if (ends_with(e.second, "_2026") == y2026) {
            out.vehicle = e.first;
            out.label = e.second;
            return true;
        }
    for (size_t i = 0; i < second_n; i++)
        if (const auto *e = named(s, second[i])) {
            out.vehicle = e->first;
            out.label = e->second;
            return true;
        }
    if (!s.myteam.empty()) {
        out.vehicle = s.myteam.front().first;
        out.label = s.myteam.front().second;
        return true;
    }
    return false;
}

template <class Mem> bool plan(Mem &mem, uint64_t entrants, uint64_t first, uint64_t end, Source &out)
{
    if (!pick(stock(mem, entrants), season_2026(mem, entrants, first, end), out))
        return false;
    uint32_t h = NO_VEHICLE;
    if (!mem.u32(out.vehicle, h) || kind(h) != KIND_STOCK || id(h) >= 0x3fff)
        return false;
    out.handle = h;
    return true;
}

inline void set_base(uint8_t *data, uint32_t base)
{
    uint8_t *p = data + (VEHICLE_BASE - VEHICLE_DATA);
    p[0] = uint8_t(base), p[1] = uint8_t(base >> 8), p[2] = uint8_t(base >> 16), p[3] = uint8_t(base >> 24);
}

template <class Mem> bool unbased(Mem &mem, uint64_t car)
{
    uint32_t base = 0;
    return car && mem.u32(car + VEHICLE_BASE, base) && (kind(base) != KIND_STOCK || id(base) >= 0x3fff);
}

template <class Mem> uint32_t base_for(Mem &mem, uint64_t entrants, uint64_t car)
{
    std::string name;
    if (!label(mem, car, name) || !stock_myteam(name))
        return NO_VEHICLE;
    for (const auto &e : stock(mem, entrants).myteam) {
        uint32_t h = NO_VEHICLE;
        if (e.second == name && mem.u32(e.first, h) && kind(h) == KIND_STOCK && id(h) < 0x3fff)
            return h;
    }
    return NO_VEHICLE;
}

template <class Mem> std::vector<uint64_t> held(Mem &mem, uint64_t entrants, uint64_t first, uint64_t end)
{
    std::vector<uint64_t> out;
    if (!first || end <= first || (end - first) % ENTRANT_SIZE)
        return out;
    size_t n = static_cast<size_t>((end - first) / ENTRANT_SIZE);
    if (n > MAX_ENTRANTS)
        n = MAX_ENTRANTS;
    for (size_t i = 0; i < n; i++) {
        uint32_t handle = 0;
        if (!mem.u32(first + i * ENTRANT_SIZE + ENTRANT_VEHICLE, handle) || !healable(handle))
            continue;
        uint64_t v = vehicle(mem, entrants, handle);
        bool seen = !v;
        for (uint64_t x : out)
            seen = seen || x == v;
        if (!seen)
            out.push_back(v);
    }
    return out;
}

template <class Mem> std::vector<uint32_t> missing(Mem &mem, uint64_t entrants, uint64_t first, uint64_t end)
{
    std::vector<uint32_t> out;
    if (!first || end <= first || (end - first) % ENTRANT_SIZE)
        return out;
    size_t n = static_cast<size_t>((end - first) / ENTRANT_SIZE);
    if (n > MAX_ENTRANTS)
        n = MAX_ENTRANTS;
    for (size_t i = 0; i < n; i++) {
        uint32_t handle = 0;
        if (!mem.u32(first + i * ENTRANT_SIZE + ENTRANT_VEHICLE, handle) || !healable(handle) || vehicle(mem, entrants, handle))
            continue;
        bool seen = false;
        for (uint32_t h : out)
            seen = seen || h == handle;
        if (!seen)
            out.push_back(handle);
    }
    return out;
}

struct Census {
    size_t entrants = 0;
    size_t on_my_team = 0;
    std::vector<uint16_t> ids;
};

template <class Mem> Census census(Mem &mem, uint64_t first, uint64_t end)
{
    Census c;
    if (!first || end <= first || (end - first) % ENTRANT_SIZE)
        return c;
    size_t n = static_cast<size_t>((end - first) / ENTRANT_SIZE);
    if (n > MAX_ENTRANTS)
        n = MAX_ENTRANTS;
    c.entrants = n;
    for (size_t i = 0; i < n; i++) {
        uint32_t handle = 0;
        if (!mem.u32(first + i * ENTRANT_SIZE + ENTRANT_VEHICLE, handle) || !healable(handle))
            continue;
        c.on_my_team++;
        bool seen = false;
        for (uint16_t x : c.ids)
            seen = seen || x == id(handle);
        if (!seen)
            c.ids.push_back(id(handle));
    }
    return c;
}

inline bool replaced(int result)
{
    return result == 0 || result == 2 || result == 4;
}

struct Ids {
    std::vector<uint16_t> seen;

    bool first(uint16_t v)
    {
        for (uint16_t s : seen)
            if (s == v)
                return false;
        if (seen.size() >= LOG_IDS)
            return false;
        seen.push_back(v);
        return true;
    }
};

}
