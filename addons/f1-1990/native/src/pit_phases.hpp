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

struct Plan {
    bool valid = false;
    float end = 0;
    std::array<float, 8> hold_frame{};
    std::array<float, 8> hold_left{};
    size_t holds = 0;
    float window_start = -1;
    float window_rate = 1.0f;
    float extra = 0;
};

// The game plays Start..Stop at rate r and Stop..JC at 1, so the stop lasts (stop - start) / (fps r) +
// (jc - stop) / fps when JC follows Stop. The plan plays every frame at rate 1 and spends the difference,
// (stop - start) / fps * (1 / r - 1), as holds at the wheel-gun-on frames (kGunShare of it, shared equally)
// and as a slower stretch from just before the jack drop to the end of the work (the rest).
inline Plan plan(const Phases &p, float rate, float fps)
{
    Plan out;
    if (!(rate > 0.01f && rate < 0.999f) || !(fps > 1.f) || !(p.stop > p.start))
        return out;
    out.end = std::max(p.stop, p.jc);
    out.extra = (p.stop - p.start) / fps * (1.f / rate - 1.f);
    float gun_time = p.gun_count ? out.extra * kGunShare : 0.f;
    float window = p.jack >= 0 ? out.end - std::max(p.jack - kJackLead, p.start) : 0.f;
    float jack_time = out.extra - gun_time;
    if (window < 1.f) {
        gun_time = out.extra;
        jack_time = 0;
    }
    if (!p.gun_count && window < 1.f)
        return out;
    if (!p.gun_count) {
        gun_time = 0;
        jack_time = out.extra;
    }
    for (size_t i = 0; i < p.gun_count && i < out.hold_frame.size(); i++) {
        out.hold_frame[out.holds] = p.guns[i];
        out.hold_left[out.holds] = gun_time / float(p.gun_count);
        out.holds++;
    }
    if (jack_time > 0 && window >= 1.f) {
        out.window_start = out.end - window;
        float natural = window / fps;
        out.window_rate = natural / (natural + jack_time);
    }
    out.valid = true;
    return out;
}

// The rate for this update at sequence frame `frame`; a hold whose frame has been reached spends dt.
inline float step(Plan &p, float frame, float dt)
{
    for (size_t i = 0; i < p.holds; i++)
        if (p.hold_left[i] > 0 && frame >= p.hold_frame[i]) {
            p.hold_left[i] -= dt;
            return 0.f;
        }
    if (p.window_start >= 0 && frame >= p.window_start && frame < p.end)
        return p.window_rate;
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
