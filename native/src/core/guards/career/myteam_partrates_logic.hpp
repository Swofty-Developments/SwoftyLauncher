#pragma once

#include "core/guards/career/myteam_vehicle_heal_logic.hpp"

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>

namespace runtime::myteam_partrates {

namespace mh = runtime::myteam_heal;

constexpr uint64_t RECORDS_BEGIN = 0x2a0;
constexpr uint64_t RECORDS_END = 0x2a8;
constexpr uint64_t RECORD_SIZE = 0xa60;
constexpr uint64_t CAREER = 0x318;
constexpr uint64_t CAREER_ENTITY = 0x278;
constexpr uint64_t ENTRANTS_BEGIN = 0x2e0;
constexpr uint64_t ENTRANTS_END = 0x2e8;
constexpr size_t MAX_RECORDS = 0x400;
constexpr size_t LOG_KEYS = 24;
constexpr uintptr_t RESUME = 0x29;
constexpr int LOG_LIMIT = 3;

inline bool skips(uint64_t record) { return record == 0; }

struct Registers {
    uint64_t engine = 0, matched = 0, key = 0, ruleset = 0;
};

struct Records {
    size_t count = 0;
    std::string keys;
    bool read = false;
};

template <class Mem> Records records(Mem &mem, uint64_t engine)
{
    Records out;
    uint64_t begin = 0, end = 0;
    if (!engine || !mem.u64(engine + RECORDS_BEGIN, begin) || !mem.u64(engine + RECORDS_END, end) || end < begin ||
        (end - begin) % RECORD_SIZE)
        return out;
    out.read = true;
    out.count = static_cast<size_t>((end - begin) / RECORD_SIZE);
    size_t n = out.count < MAX_RECORDS ? out.count : MAX_RECORDS;
    for (size_t i = 0; i < n && i < LOG_KEYS; i++) {
        uint32_t key = 0;
        char buf[16];
        if (mem.u32(begin + i * RECORD_SIZE, key))
            snprintf(buf, sizeof buf, "%08x", key);
        else
            snprintf(buf, sizeof buf, "unread");
        out.keys += (out.keys.empty() ? "" : ", ") + std::string(buf);
    }
    if (n > LOG_KEYS)
        out.keys += ", ...";
    return out;
}

template <class Mem> bool entrant_range(Mem &mem, uint64_t engine, uint64_t &first, uint64_t &end)
{
    first = 0;
    end = 0;
    uint64_t career = 0, entity = 0;
    return engine && mem.u64(engine + CAREER, career) && career && mem.u64(career + CAREER_ENTITY, entity) && entity &&
           mem.u64(entity + ENTRANTS_BEGIN, first) && mem.u64(entity + ENTRANTS_END, end);
}

template <class Mem> std::string describe(Mem &mem, const Registers &r)
{
    Records rec = records(mem, r.engine);
    uint64_t first = 0, end = 0;
    bool listed = entrant_range(mem, r.engine, first, end);
    mh::Census c = listed ? mh::census(mem, first, end) : mh::Census{};
    char who[96];
    if (static_cast<uint32_t>(r.key) == mh::NO_VEHICLE)
        snprintf(who, sizeof who, "no entrant matches the local player (key %08x)", static_cast<uint32_t>(r.key));
    else
        snprintf(who, sizeof who, "local player key %08x has no record", static_cast<uint32_t>(r.key));
    char player[48];
    if (c.humans)
        snprintf(player, sizeof player, "%zu human (car %08x)", c.humans, c.human_vehicle);
    else
        snprintf(player, sizeof player, "no human");
    char out[1024];
    snprintf(out, sizeof out,
             "part rates: %s (matched entrant 0x%llx, part-rate ruleset record 0x%llx); engine management 0x%llx holds %zu "
             "part-rate records%s%s; the career's entrant list %s: %zu entrants: %zu on kind 1 cars, %zu on stock cars, "
             "%zu on no car, %zu unread, %s; the player's part rates are left unset so the career can start",
             who, (unsigned long long)r.matched, (unsigned long long)r.ruleset, (unsigned long long)r.engine, rec.count,
             rec.keys.empty() ? "" : " with keys ", rec.keys.c_str(), listed ? "reads" : "is unreadable", c.entrants, c.kind1,
             c.stock, c.empty, c.unread, player);
    return out;
}

}
