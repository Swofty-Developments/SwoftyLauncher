#pragma once
#include <cstdint>
#include <deque>
#include <string>

// Timing marks for server-timed laps (swofty_api.h SwoftyLapHost mark). At each timing line of the player's Time
// Trial lap, and as the game pauses and resumes during it, the lap module says which mark it was and the game's lap
// clock there; the core stamps the moment and
// writes one file per mark into <files root>tt\ticks\, named <at_ms>-<seq>.json. The launcher sends each mark to the
// server as a tick and keeps the receipt for the lap's upload. A mark it can't send in time is dropped, and the lap
// is simply untimed.
namespace runtime::lap_marks {

constexpr int32_t START = 0, END = 3, PAUSES = 4, LAST = END + 2 * PAUSES;
constexpr int32_t MAX_LAP = 100000;
constexpr int32_t MAX_GAME_MS = 3600 * 1000;
constexpr size_t MAX_PENDING = 32;

struct Mark {
    uint64_t seq = 0;
    int32_t lap = 0, which = 0, game_ms = 0;
    int64_t at_ms = 0;
};

inline const char *name(int32_t which)
{
    static const char *const names[] = {"start", "s1", "s2", "end", "p1", "r1", "p2", "r2", "p3", "r3", "p4", "r4"};
    return which >= START && which <= LAST ? names[which] : "";
}

inline bool valid(int32_t lap, int32_t which, int32_t game_ms)
{
    return lap >= 0 && lap <= MAX_LAP && which >= START && which <= LAST && game_ms >= 0 && game_ms <= MAX_GAME_MS;
}

inline std::string file_name(const Mark &m) { return std::to_string(m.at_ms) + "-" + std::to_string(m.seq) + ".json"; }

inline std::string json(const Mark &m)
{
    return "{\"seq\":" + std::to_string(m.seq) + ",\"lap\":" + std::to_string(m.lap) + ",\"mark\":\"" + name(m.which) +
           "\",\"game_ms\":" + std::to_string(m.game_ms) + ",\"at_ms\":" + std::to_string(m.at_ms) + "}";
}

// The marks the writer thread has yet to write. Past MAX_PENDING the oldest goes: it would be too stale to time the
// lap by the time it reached the server.
struct Queue {
    std::deque<Mark> marks;
    uint64_t next = 1;
    uint64_t dropped = 0;

    bool push(int32_t lap, int32_t which, int32_t game_ms, int64_t at_ms)
    {
        if (!valid(lap, which, game_ms))
            return false;
        if (marks.size() >= MAX_PENDING) {
            marks.pop_front();
            dropped++;
        }
        marks.push_back({next++, lap, which, game_ms, at_ms});
        return true;
    }
};

}
