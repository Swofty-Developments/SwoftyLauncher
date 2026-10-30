// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>

namespace f1990::pit {

struct Phases {
    float start = 0, stop = 0, jc = 0;
    std::array<float, 8> guns{};
    size_t gun_count = 0;
    float jack = -1;
};

constexpr float kGunShare = 0.7f;
constexpr float kJackLead = 3.0f;

inline std::string key(std::string_view name)
{
    if (auto slash = name.find_last_of("/\\"); slash != std::string_view::npos)
        name.remove_prefix(slash + 1);
    if (auto dot = name.find('.'); dot != std::string_view::npos)
        name = name.substr(0, dot);
    std::string k(name);
    for (auto &c : k)
        c = char(c >= 'A' && c <= 'Z' ? c - 'A' + 'a' : c);
    for (std::string_view prefix : {"pitstop", "f1"})
        if (k.rfind(prefix, 0) == 0) {
            k.erase(0, prefix.size());
            break;
        }
    return k;
}

// The table holds frames authored at 30 fps; the game keeps a sequence's time in its own units (seconds in game:
// the crew speed is already set at 0.5 when the Start event sits at frame 13), advancing it by dt x rate x the
// resource's scale (+0x54). One authored frame is scale / fps units.
inline Phases in_units(const Phases &p, float per_frame)
{
    Phases out = p;
    out.start *= per_frame;
    out.stop *= per_frame;
    out.jc *= per_frame;
    for (size_t i = 0; i < out.gun_count; i++)
        out.guns[i] *= per_frame;
    if (out.jack >= 0)
        out.jack *= per_frame;
    return out;
}

struct Plan {
    bool valid = false;
    float end = 0;
    std::array<float, 8> hold_frame{};
    std::array<float, 8> hold_left{};
    size_t holds = 0;
    float window_start = -1;
    float window_rate = 1.0f;
    float window_left = 0;
    float ups = 1.f;
    float extra = 0;
};

// The game plays the sequence at its crew speed r from the frame it is set (`from`, at or before Start) to Stop and
// at 1 from Stop to JC, so the stop lasts (stop - from) / (fps r) + (jc - stop) / fps when JC follows Stop. The plan
// plays every frame from `from` at rate 1 and spends the difference, (stop - from) / fps * (1 / r - 1), as holds at
// the wheel-gun-on frames after `from` (kGunShare of it, shared equally) and as a slower stretch from just before
// the jack drop to the end of the work (the rest).
inline Plan plan(const Phases &p, float rate, float fps, float from, float frame = 1.f)
{
    Plan out;
    from = std::max(from, 0.f);
    if (!(rate > 0.01f && rate < 0.999f) || !(fps > 0.f) || !(frame > 0.f) || !(p.stop > from + frame))
        return out;
    out.end = std::max(p.stop, p.jc);
    out.ups = fps;
    out.extra = (p.stop - from) / fps * (1.f / rate - 1.f);
    size_t guns = 0;
    for (size_t i = 0; i < p.gun_count; i++)
        guns += p.guns[i] > from;
    float gun_time = guns ? out.extra * kGunShare : 0.f;
    float window = p.jack >= 0 ? out.end - std::max(p.jack - kJackLead * frame, from) : 0.f;
    float jack_time = out.extra - gun_time;
    if (window < frame) {
        gun_time = out.extra;
        jack_time = 0;
    }
    if (!guns && window < frame)
        return out;
    if (!guns) {
        gun_time = 0;
        jack_time = out.extra;
    }
    for (size_t i = 0; i < p.gun_count && out.holds < out.hold_frame.size(); i++) {
        if (p.guns[i] <= from)
            continue;
        out.hold_frame[out.holds] = p.guns[i];
        out.hold_left[out.holds] = gun_time / float(guns);
        out.holds++;
    }
    if (jack_time > 0 && window >= frame) {
        out.window_start = out.end - window;
        float natural = window / fps;
        out.window_rate = natural / (natural + jack_time);
        out.window_left = natural + jack_time;
    }
    out.valid = true;
    return out;
}

// A table row for a sequence name: the exact key, else the longest key that starts the name (a suffix the game
// appended), else the key sharing the longest prefix with it, at least kFamily characters (the same family, such
// as startjobdostandardstop*).
constexpr size_t kFamily = 16;

template <class Table> const typename Table::mapped_type *find(const Table &table, std::string_view name)
{
    std::string k = key(name);
    if (auto it = table.find(k); it != table.end())
        return &it->second;
    const typename Table::mapped_type *best = nullptr;
    size_t best_len = 0;
    for (const auto &[row, phases] : table)
        if (row.size() > best_len && k.rfind(row, 0) == 0) {
            best = &phases;
            best_len = row.size();
        }
    if (best)
        return best;
    for (const auto &[row, phases] : table) {
        size_t n = 0;
        while (n < row.size() && n < k.size() && row[n] == k[n])
            n++;
        if (n >= kFamily && n > best_len) {
            best = &phases;
            best_len = n;
        }
    }
    return best;
}

// The rate for this update at sequence frame `frame`; a hold whose frame has been reached spends dt.
inline float step(Plan &p, float frame, float dt)
{
    for (size_t i = 0; i < p.holds; i++)
        if (p.hold_left[i] > 0 && frame >= p.hold_frame[i]) {
            p.hold_left[i] -= dt;
            return 0.f;
        }
    if (p.window_start >= 0 && frame >= p.window_start && frame < p.end) {
        float rate = p.window_left > dt && dt > 0 ? (p.end - frame) / (p.window_left * p.ups) : 1.f;
        p.window_left -= dt;
        return std::clamp(rate, 0.f, 1.f);
    }
    return 1.f;
}

// One update of an active plan, before the game's: the game scales this frame's dt by `rate` (+0x3bc) and then
// rewrites it as its transition factor x `base` (+0x33c). The factor the game applied last is read back from the
// pair, the plan's rate goes into `base` and base x factor into `rate`. At the plan's end both go back to the
// game's after-Stop values (base 1). Returns false once the plan has ended.
inline bool drive(Plan &p, float &factor, float frame, float dt, float &base, float &rate)
{
    if (base > 0.001f)
        factor = std::clamp(rate / base, 0.25f, 4.f);
    if (frame >= p.end) {
        base = 1.f;
        rate = factor;
        return false;
    }
    float want = step(p, frame, dt);
    base = want;
    rate = want * factor;
    return true;
}

}
