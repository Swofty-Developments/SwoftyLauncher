#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <functional>
#include <vector>

namespace runtime::era {

// Points per box (content.package rules.points). A setup mode keeps its weekend structures in a vector at +0x190
// (begin +0x198, end +0x1a0, 0x88 each, the sprint flag at +0x80); a structure keeps its sessions at +0x00 (begin
// +0x08, end +0x10, 16 bytes each: +0x00 the parsed standings preset or 0, +0x08 the session id, 15-17 = race,
// race_2, race_3). The game resolves a session's standings label to a pointer into the game setup registry's preset
// vector (g_Game_Scenarios +0xe0: begin +0xe8, end +0xf0, 0x20 each) when it parses the mode, and the standings
// recompute reads the pointer live. A preset holds its points levels at +0x08 / +0x10 (0x18 each: +0x00 the
// fastest lap rule byte, +0x04 the bonus, +0x08 the points column, whose i32 values run from +0x08 to +0x10);
// level 0 is the full race, then the reduced_points_data rows. Presets carry no label, so the era picker finds the
// one the boot apply added for the box by its points (find_points_preset) and points the race sessions of every
// swofty_f1 mode's non-sprint structures at it (PointsSwaps).
constexpr size_t MODE_WEEKENDS_BEGIN = 0x198, MODE_WEEKENDS_END = 0x1a0, WEEKEND_SIZE = 0x88, WEEKEND_SPRINT = 0x80;
constexpr size_t WEEKEND_SESSIONS_BEGIN = 0x08, WEEKEND_SESSIONS_END = 0x10, SESSION_SIZE = 0x10, SESSION_ID = 0x08;
constexpr size_t SCENARIOS_PRESETS_BEGIN = 0xe8, SCENARIOS_PRESETS_END = 0xf0, PRESET_SIZE = 0x20;
constexpr size_t PRESET_LEVELS_BEGIN = 0x08, PRESET_LEVELS_END = 0x10, LEVEL_SIZE = 0x18;
constexpr size_t LEVEL_RULE = 0x00, LEVEL_BONUS = 0x04, LEVEL_POINTS = 0x08, COLUMN_BEGIN = 0x08, COLUMN_END = 0x10;
constexpr uint32_t SESSION_RACE = 15, SESSION_RACE_3 = 17;
constexpr size_t WEEKENDS_MAX = 16, SESSIONS_MAX = 64, PRESETS_MAX = 4096, LEVELS_MAX = 8, PLACES_MAX = 64;

using Readable = std::function<bool(const void *, size_t)>;

struct PointsWant {
    std::vector<int32_t> full;
    std::vector<std::vector<int32_t>> reduced;
    int32_t bonus = 0;
    uint8_t rule = 0;
};

template <class T> inline bool peek(const Readable &ok, const void *base, size_t off, T &out)
{
    if (!base)
        return false;
    auto *p = static_cast<const uint8_t *>(base) + off;
    if (!ok(p, sizeof(T)))
        return false;
    std::memcpy(&out, p, sizeof(T));
    return true;
}

inline bool level_is(const Readable &ok, const uint8_t *level, const std::vector<int32_t> &points, int32_t bonus, uint8_t rule)
{
    uint8_t r = 0;
    int32_t b = 0;
    const uint8_t *column = nullptr, *begin = nullptr, *end = nullptr;
    if (!peek(ok, level, LEVEL_RULE, r) || !peek(ok, level, LEVEL_BONUS, b) || !peek(ok, level, LEVEL_POINTS, column) ||
        !peek(ok, column, COLUMN_BEGIN, begin) || !peek(ok, column, COLUMN_END, end))
        return false;
    if (r != rule || (rule && b != bonus) || end < begin || size_t(end - begin) != points.size() * 4 ||
        points.size() > PLACES_MAX || !ok(begin, size_t(end - begin)))
        return false;
    return !std::memcmp(begin, points.data(), points.size() * 4);
}

// The parsed preset whose full and reduced points, fastest lap rule and bonus are the box's, or nullptr.
inline const uint8_t *find_points_preset(const Readable &ok, const uint8_t *begin, const uint8_t *end, const PointsWant &want)
{
    if (!begin || end <= begin || size_t(end - begin) % PRESET_SIZE || size_t(end - begin) / PRESET_SIZE > PRESETS_MAX)
        return nullptr;
    for (const uint8_t *p = begin; p < end; p += PRESET_SIZE) {
        const uint8_t *lb = nullptr, *le = nullptr;
        if (!peek(ok, p, PRESET_LEVELS_BEGIN, lb) || !peek(ok, p, PRESET_LEVELS_END, le) || le < lb ||
            size_t(le - lb) % LEVEL_SIZE)
            continue;
        size_t levels = size_t(le - lb) / LEVEL_SIZE;
        if (!levels || levels > LEVELS_MAX || !level_is(ok, lb, want.full, want.bonus, want.rule))
            continue;
        bool same = true;
        for (size_t i = 1; i < levels && i <= want.reduced.size() && same; i++)
            same = level_is(ok, lb + i * LEVEL_SIZE, want.reduced[i - 1], want.bonus, want.rule);
        if (same)
            return p;
    }
    return nullptr;
}

// The race sessions (race, race_2, race_3) of a setup mode's non-sprint weekend structures that hold a preset.
inline std::vector<uint64_t *> race_sessions(const Readable &ok, uint8_t *mode)
{
    std::vector<uint64_t *> out;
    uint8_t *wb = nullptr, *we = nullptr;
    if (!peek(ok, mode, MODE_WEEKENDS_BEGIN, wb) || !peek(ok, mode, MODE_WEEKENDS_END, we) || !wb || we < wb ||
        size_t(we - wb) % WEEKEND_SIZE || size_t(we - wb) / WEEKEND_SIZE > WEEKENDS_MAX)
        return out;
    for (uint8_t *w = wb; w < we; w += WEEKEND_SIZE) {
        uint8_t sprint = 1;
        uint8_t *sb = nullptr, *se = nullptr;
        if (!peek(ok, w, WEEKEND_SPRINT, sprint) || sprint || !peek(ok, w, WEEKEND_SESSIONS_BEGIN, sb) ||
            !peek(ok, w, WEEKEND_SESSIONS_END, se) || !sb || se < sb || size_t(se - sb) % SESSION_SIZE ||
            size_t(se - sb) / SESSION_SIZE > SESSIONS_MAX)
            continue;
        for (uint8_t *s = sb; s < se; s += SESSION_SIZE) {
            uint32_t id = 0;
            uint64_t preset = 0;
            if (peek(ok, s, SESSION_ID, id) && id >= SESSION_RACE && id <= SESSION_RACE_3 && peek(ok, s, 0, preset) && preset)
                out.push_back(reinterpret_cast<uint64_t *>(s));
        }
    }
    return out;
}

// The race sessions the era picker pointed at a box's preset, with the pointer each held before, so a box without a
// points rule, a stock box or a career load puts the game's presets back (only where the pointer is still the one
// written).
class PointsSwaps {
public:
    void swap(uint64_t *session, uint64_t preset)
    {
        if (!session)
            return;
        Swap *s = find(session);
        if (!s) {
            swaps_.push_back({session, *session, preset});
            s = &swaps_.back();
        }
        s->written = preset;
        *session = preset;
    }

    size_t put_back()
    {
        size_t n = 0;
        for (auto &s : swaps_) {
            if (*s.session != s.written)
                continue;
            *s.session = s.original;
            n++;
        }
        swaps_.clear();
        return n;
    }

    // Drops the swaps without reading or writing the sessions, which the game may have freed.
    size_t forget()
    {
        size_t n = swaps_.size();
        swaps_.clear();
        return n;
    }

    size_t active() const { return swaps_.size(); }

private:
    struct Swap {
        uint64_t *session;
        uint64_t original, written;
    };
    Swap *find(const uint64_t *session)
    {
        for (auto &s : swaps_)
            if (s.session == session)
                return &s;
        return nullptr;
    }
    std::vector<Swap> swaps_;
};

}
