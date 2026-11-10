#pragma once

#include "lap_rec_logic.hpp"

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace runtime::lap_rec {

constexpr uintptr_t LT_CAR_DATA = 0x58, MSG_CAR = 0x10, MSG_LAP = 0x14, REC_INDEX = 0x38, REC_STRIDE = 200;
constexpr uintptr_t REC_S1 = 0x48, REC_S2 = 0x58, REC_S3 = 0x68, REC_LAP = 0xe8;
constexpr uintptr_t REC_LAP_BITS = 0xe0, REC_LAP_KEY = 0xd8;
constexpr uintptr_t REC_SECTOR_BITS[3] = {0xb0, 0xc0, 0xd0}, REC_SECTOR_KEY[3] = {0xa8, 0xb8, 0xc8};
constexpr size_t MAX_CARS = 64;

constexpr uintptr_t TL_LAP = 0x268, TL_SECTORS = 0x26c, TL_CFG = 0x130, CFG_PERFORMANCE = 0x12c, CFG_INPUT = 0x220;
constexpr uintptr_t TL_BRK = 0x278, TL_ABS = 0x279, TL_TRC = 0x27a, TL_DRL = 0x27b, TL_GBX = 0x27c, TL_ERS = 0x27d;
constexpr uintptr_t COMP_VEC_BEGIN = 0x50, COMP_VEC_END = 0x58, COMP_LAST_LAP_SLOT = 0x18;
constexpr uintptr_t MAP_SLOTS = 0x18, MAP_MASK = 0x30;
constexpr size_t MAX_MAP_SLOTS = size_t(1) << 16;
constexpr uintptr_t PART_BEGIN = 0x2e0, PART_END = 0x2e8, PART_STRIDE = 0x88, PART_CAR = 0x82, PART_TEAM = 0x58,
                    PART_VEHICLE = 0x5c, PART_LIVERY = 0x64, SESSION_LIST = 0x260;
constexpr uintptr_t MGR_SYSTEMS = 0x30, MGR_MODES = 0xc0, MODES_HOLDER = 0x88;
constexpr size_t MAX_GHOST_BYTES = size_t(1) << 20;
constexpr size_t MAX_GHOST_BLOB = 0x20000;

// Memory as the decoders see it: read(address, destination, byte count) is true when every byte was read.
template <class R, class T> inline bool rd(R &mem, uint64_t at, T &out)
{
    return mem(at, &out, sizeof out);
}

template <class R> inline bool rd_xor_f32(R &mem, uint64_t at, float &out)
{
    uint32_t a = 0, b = 0;
    if (!rd(mem, at, a) || !rd(mem, at + 8, b))
        return false;
    a ^= b;
    std::memcpy(&out, &a, 4);
    return true;
}

inline int64_t game_ms(float seconds)
{
    return int64_t(int32_t(seconds * 1000.0f));
}

struct TLapExtras {
    bool ok = false;
    uint8_t brk = 0, abs = 0, trc = 0, drl = 0, gbx = 0, ers = 0;
    int input = -1, performance = -1;
    float lap_s = 0;
};

struct Stats {
    bool ok = false;
    uint32_t team = 0, vehicle = 0, livery = 0;
};

// A participant's stat id is a dword: the id in its high word, two flag bits in the low word. The leaderboard row carries
// the game's 16-bit packed form (Game_GhostLapHandler_StartLeaderboardEntryWrite): bits 14-15 from the low bits, bits 0-13 from the id.
inline int64_t packed_stat(uint32_t raw)
{
    return int64_t(((raw & 3u) << 14) | ((raw >> 16) & 0x3fffu));
}

struct GameLap {
    bool read = false, usable = false;
    uint8_t car = 0;
    uint32_t msg_lap = 0;
    int record = -1;
    float lap_s = 0, sector_s[3] = {0, 0, 0};
    bool lap_valid = false, sector_valid[3] = {false, false, false};
    int64_t time_ms = 0;
    std::vector<int64_t> sectors_ms;
    std::string why;
    double at_s = 0;
    TLapExtras tlap;
    Stats stats;
};

// lapTimes is the rcx of Sess_LapTimes_ReportNewLapTime, msg its rdx. The record of the lap just completed is the
// car's entry (+0x58 list, one pointer per car) at the entry's own index (u8 +0x38), 200 bytes per lap.
template <class R> inline GameLap decode_lap(R &mem, uint64_t lap_times, uint64_t msg)
{
    GameLap g;
    uint64_t list = 0, entry = 0;
    uint8_t idx = 0;
    if (!lap_times || !msg || !rd(mem, msg + MSG_CAR, g.car) || !rd(mem, msg + MSG_LAP, g.msg_lap) ||
        g.car >= MAX_CARS || !rd(mem, lap_times + LT_CAR_DATA, list) || !list ||
        !rd(mem, list + uint64_t(g.car) * 8, entry) || !entry || !rd(mem, entry + REC_INDEX, idx)) {
        g.why = "the lap record could not be reached";
        return g;
    }
    g.record = idx;
    uint64_t base = entry + uint64_t(idx) * REC_STRIDE;
    bool ok = rd_xor_f32(mem, base + REC_LAP, g.lap_s);
    const uintptr_t at[3] = {REC_S1, REC_S2, REC_S3};
    for (int i = 0; i < 3; i++)
        ok = ok && rd_xor_f32(mem, base + at[i], g.sector_s[i]);
    uint64_t bits = 0;
    uint8_t key = 0;
    ok = ok && rd(mem, base + REC_LAP_BITS, bits) && rd(mem, base + REC_LAP_KEY, key);
    g.lap_valid = (bits & 1) == key;
    for (int i = 0; i < 3 && ok; i++) {
        ok = rd(mem, base + REC_SECTOR_BITS[i], bits) && rd(mem, base + REC_SECTOR_KEY[i], key);
        g.sector_valid[i] = (bits & 1) == key;
    }
    if (!ok) {
        g.why = "the lap record fields could not be read";
        return g;
    }
    g.read = true;
    g.time_ms = game_ms(g.lap_s);
    for (int i = 0; i < 3; i++)
        g.sectors_ms.push_back(game_ms(g.sector_s[i]));
    return g;
}

// Decides whether a decoded record is trusted; the reason is left in g.why when it is not.
inline void sanity(GameLap &g)
{
    g.usable = false;
    if (!g.read)
        return;
    if (!std::isfinite(g.lap_s) || g.lap_s <= 5.0f || g.lap_s >= 3600.0f) {
        g.why = "the lap time is out of range";
        return;
    }
    for (int i = 0; i < 3; i++) {
        if (!std::isfinite(g.sector_s[i]) || g.sector_s[i] <= 0.0f) {
            g.why = "a sector time is not positive";
            return;
        }
    }
    if (g.lap_valid && g.sector_valid[0] && g.sector_valid[1] && g.sector_valid[2]) {
        int64_t sum = g.sectors_ms[0] + g.sectors_ms[1] + g.sectors_ms[2], d = sum - g.time_ms;
        if (d < -10 || d > 10) {
            g.why = "the sectors do not add up to the lap time";
            return;
        }
    }
    g.usable = true;
    g.why.clear();
}


template <class R> inline TLapExtras decode_tlap(R &mem, uint64_t tlap, int64_t expect_ms)
{
    TLapExtras t;
    uint64_t cfg = 0;
    uint32_t perf = 0, input = 0;
    if (!tlap || !rd(mem, tlap + TL_LAP, t.lap_s) || !std::isfinite(t.lap_s) ||
        std::llabs(game_ms(t.lap_s) - expect_ms) > 2 || !rd(mem, tlap + TL_CFG, cfg) || !cfg ||
        !rd(mem, cfg + CFG_PERFORMANCE, perf) || !rd(mem, cfg + CFG_INPUT, input) || perf > 1 || input > 3 ||
        !rd(mem, tlap + TL_BRK, t.brk) || !rd(mem, tlap + TL_ABS, t.abs) || !rd(mem, tlap + TL_TRC, t.trc) ||
        !rd(mem, tlap + TL_DRL, t.drl) || !rd(mem, tlap + TL_GBX, t.gbx) || !rd(mem, tlap + TL_ERS, t.ers))
        return TLapExtras{};
    t.performance = int(perf);
    t.input = int(input);
    t.ok = true;
    return t;
}

// The telemetry lap component sits in the session's component map (robin hood, 16-byte {key, value} slots at +0x18,
// slot count minus one at +0x30). The game's own lookup hashes the key; this walks the slots instead, so a stale key
// in a free slot is told apart by the value reading as a component with a lap vector.
template <class R> inline uint64_t find_last_tlap(R &mem, uint64_t map, uint64_t key)
{
    uint64_t slots = 0, mask = 0;
    if (!map || !rd(mem, map + MAP_SLOTS, slots) || !slots || !rd(mem, map + MAP_MASK, mask) || (mask & (mask + 1)) ||
        mask + 1 > MAX_MAP_SLOTS)
        return 0;
    for (uint64_t i = 0; i <= mask; i++) {
        uint64_t k = 0, comp = 0, begin = 0, end = 0, tlap = 0;
        if (!rd(mem, slots + i * 16, k) || k != key)
            continue;
        if (rd(mem, slots + i * 16 + 8, comp) && comp && rd(mem, comp + COMP_VEC_BEGIN, begin) &&
            rd(mem, comp + COMP_VEC_END, end) && begin && end > begin && end - begin > COMP_LAST_LAP_SLOT &&
            rd(mem, begin + COMP_LAST_LAP_SLOT, tlap) && tlap)
            return tlap;
    }
    return 0;
}


// The participant list of a session, one 0x88-byte entry per car; a car's entry carries its index at +0x82.
template <class R> inline Stats find_stats(R &mem, uint64_t list_owner, int car)
{
    Stats s;
    uint64_t begin = 0, end = 0;
    if (!rd(mem, list_owner + PART_BEGIN, begin) || !rd(mem, list_owner + PART_END, end) || !begin || end <= begin ||
        (end - begin) % PART_STRIDE || (end - begin) / PART_STRIDE > MAX_CARS)
        return s;
    for (uint64_t at = begin; at < end; at += PART_STRIDE) {
        uint8_t c = 0;
        if (!rd(mem, at + PART_CAR, c) || c != car)
            continue;
        if (rd(mem, at + PART_TEAM, s.team) && rd(mem, at + PART_VEHICLE, s.vehicle) && rd(mem, at + PART_LIVERY, s.livery))
            s.ok = true;
        return s;
    }
    return s;
}

// Where the lap's extras came from. The game manager reaches both without the ghost handler: its systems map
// ([manager+0x30], the map the handler's constructor reads its systems from) holds the telemetry component, and the
// participant list hangs under [[manager+0xc0]+0x88] (+0x260 is the entity, as for the handler's session).
enum class From : uint8_t { none, manager, handler };

inline const char *from_name(From f)
{
    return f == From::manager ? "the game manager" : f == From::handler ? "the ghost handler" : "nowhere";
}

struct ExtrasFrom {
    From tlap = From::none, stats = From::none;
};

struct ExtrasRoots {
    uint64_t manager = 0, handler_map = 0, handler_base = 0, key = 0;
};

template <class R> inline uint64_t manager_map(R &mem, uint64_t manager)
{
    uint64_t map = 0;
    return manager && rd(mem, manager + MGR_SYSTEMS, map) ? map : 0;
}

template <class R> inline uint64_t manager_session(R &mem, uint64_t manager)
{
    uint64_t modes = 0, holder = 0;
    return manager && rd(mem, manager + MGR_MODES, modes) && modes && rd(mem, modes + MODES_HOLDER, holder) ? holder : 0;
}

template <class R> inline Stats stats_at(R &mem, uint64_t base, int car)
{
    Stats s;
    uint64_t inner = 0;
    if (!base)
        return s;
    s = find_stats(mem, base, car);
    if (!s.ok && rd(mem, base + SESSION_LIST, inner) && inner)
        s = find_stats(mem, inner, car);
    return s;
}

template <class R> inline ExtrasFrom read_extras(R &mem, GameLap &g, const ExtrasRoots &roots)
{
    ExtrasFrom from;
    const std::pair<From, uint64_t> maps[] = {{From::manager, manager_map(mem, roots.manager)},
                                              {From::handler, roots.handler_map}};
    for (auto &[kind, map] : maps) {
        if (g.tlap.ok || !map)
            continue;
        uint64_t tlap = find_last_tlap(mem, map, roots.key);
        TLapExtras t = tlap ? decode_tlap(mem, tlap, g.time_ms) : TLapExtras{};
        if (t.ok) {
            g.tlap = t;
            from.tlap = kind;
        }
    }
    const std::pair<From, uint64_t> bases[] = {{From::manager, manager_session(mem, roots.manager)},
                                               {From::handler, roots.handler_base}};
    for (auto &[kind, base] : bases) {
        if (g.stats.ok || !base)
            continue;
        Stats st = stats_at(mem, base, g.car);
        if (st.ok) {
            g.stats = st;
            from.stats = kind;
        }
    }
    return from;
}

struct Sealed {
    Lap lap;
    std::optional<GameLap> game;
    std::vector<uint8_t> ghost;
    bool timed_out = false;
};

// Pairs each completed lap with the game's lap result and, on a personal best, its ghost blob. A lap is released when
// everything it waits for has arrived or `wait_s` after its line crossing, whichever is first. A lap whose result
// never arrives is released without one and the caller falls back to the clock.
class Sealer {
public:
    struct Config {
        double wait_s = 2.0;
        double match_s = 2.0;
    };

    Sealer() = default;
    explicit Sealer(Config c) : cfg_(c) {}

    void expect(bool game, bool ghost)
    {
        game_armed_ = game;
        ghost_armed_ = ghost;
    }

    bool game_armed() const { return game_armed_; }
    size_t waiting() const { return laps_.size(); }
    uint32_t stale_games() const { return stale_games_; }
    uint32_t stale_ghosts() const { return stale_ghosts_; }

    void lap(Lap l, double now)
    {
        laps_.push_back({std::move(l), now, std::nullopt, {}});
        match();
    }

    void game(GameLap g, double now)
    {
        g.at_s = now;
        games_.push_back(std::move(g));
        match();
    }

    void ghost(std::vector<uint8_t> blob, double now)
    {
        ghosts_.push_back({std::move(blob), now});
        match();
    }

    std::vector<Sealed> poll(double now)
    {
        match();
        std::vector<Sealed> out;
        for (size_t i = 0; i < laps_.size();) {
            Waiting &w = laps_[i];
            bool done = !needs_game(w) && !needs_ghost(w);
            bool late = now - w.ended_s >= cfg_.wait_s;
            if (done || late) {
                out.push_back({std::move(w.lap), std::move(w.game), std::move(w.ghost), !done});
                laps_.erase(laps_.begin() + long(i));
            } else
                i++;
        }
        for (size_t i = 0; i < games_.size();)
            if (now - games_[i].at_s > cfg_.wait_s + cfg_.match_s) {
                stale_games_++;
                games_.erase(games_.begin() + long(i));
            } else
                i++;
        for (size_t i = 0; i < ghosts_.size();)
            if (now - ghosts_[i].at_s > cfg_.wait_s + cfg_.match_s) {
                stale_ghosts_++;
                ghosts_.erase(ghosts_.begin() + long(i));
            } else
                i++;
        return out;
    }

    std::vector<Sealed> flush()
    {
        match();
        std::vector<Sealed> out;
        for (auto &w : laps_)
            out.push_back({std::move(w.lap), std::move(w.game), std::move(w.ghost), true});
        laps_.clear();
        games_.clear();
        ghosts_.clear();
        return out;
    }

private:
    struct Waiting {
        Lap lap;
        double ended_s;
        std::optional<GameLap> game;
        std::vector<uint8_t> ghost;
    };
    struct Blob {
        std::vector<uint8_t> bytes;
        double at_s;
    };

    bool needs_game(const Waiting &w) const { return game_armed_ && !w.game; }
    bool needs_ghost(const Waiting &w) const
    {
        return ghost_armed_ && w.game && (!w.game->usable || w.game->lap_valid) && w.ghost.empty();
    }

    void match()
    {
        for (auto &w : laps_) {
            if (w.game)
                continue;
            size_t best = games_.size();
            for (size_t i = 0; i < games_.size(); i++) {
                double d = std::fabs(games_[i].at_s - w.ended_s);
                if (d <= cfg_.match_s && (best == games_.size() || d < std::fabs(games_[best].at_s - w.ended_s)))
                    best = i;
            }
            if (best < games_.size()) {
                w.game = std::move(games_[best]);
                games_.erase(games_.begin() + long(best));
            }
        }
        for (size_t g = 0; g < ghosts_.size();) {
            size_t best = laps_.size();
            for (size_t i = 0; i < laps_.size(); i++) {
                double d = ghosts_[g].at_s - laps_[i].ended_s;
                if (needs_ghost(laps_[i]) && d >= -cfg_.match_s && d <= cfg_.wait_s &&
                    (best == laps_.size() || laps_[i].ended_s > laps_[best].ended_s))
                    best = i;
            }
            if (best < laps_.size()) {
                laps_[best].ghost = std::move(ghosts_[g].bytes);
                ghosts_.erase(ghosts_.begin() + long(g));
            } else
                g++;
        }
    }

    Config cfg_{};
    bool game_armed_ = false, ghost_armed_ = false;
    std::vector<Waiting> laps_;
    std::vector<GameLap> games_;
    std::vector<Blob> ghosts_;
    uint32_t stale_games_ = 0, stale_ghosts_ = 0;
};

inline bool ghost_size_ok(uint64_t size)
{
    return size >= 1 && size <= MAX_GHOST_BLOB;
}

inline const char *input_name(int v)
{
    static const char *names[] = {"pad", "wheel", "keyboard", "direct"};
    return v >= 0 && v < 4 ? names[v] : "";
}

inline const char *performance_name(int v)
{
    return v == 0 ? "realistic" : v == 1 ? "equal" : "";
}

}
