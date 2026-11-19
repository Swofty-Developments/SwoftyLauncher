#pragma once
#include <cstddef>
#include <cstdint>
#include <vector>

namespace hud::arm_policy {

constexpr uint64_t REMOVED_AFTER_DRAW_MS = 10000;

inline bool removal_after_draw(uint64_t now_ms, uint64_t last_draw_ms)
{
    return last_draw_ms && now_ms >= last_draw_ms && now_ms - last_draw_ms <= REMOVED_AFTER_DRAW_MS;
}

constexpr unsigned STATE_PRESENT = 0, STATE_RENDER_TARGET = 0x4;
constexpr unsigned BARRIER_FLAG_BEGIN_ONLY = 0x1;
constexpr uint64_t PENDING_MAX_AGE_MS = 250;

inline bool presents_back_buffer(unsigned before, unsigned after, unsigned flags)
{
    return before == STATE_RENDER_TARGET && after == STATE_PRESENT && !(flags & BARRIER_FLAG_BEGIN_ONLY);
}

inline unsigned state_after_draw(unsigned left_in) { return left_in; }

inline bool pending_fresh(uint64_t now_ms, uint64_t built_ms)
{
    return now_ms >= built_ms && now_ms - built_ms <= PENDING_MAX_AGE_MS;
}

inline bool target_fits(unsigned frame_w, unsigned frame_h, uint64_t res_w, unsigned res_h)
{
    return frame_w && frame_h && frame_w == res_w && frame_h == res_h;
}

inline int renderable_format(int format)
{
    switch (format) {
    case 9:
        return 10;
    case 23:
        return 24;
    case 27:
        return 28;
    case 90:
        return 87;
    default:
        return format;
    }
}

// The batch's list that presents a back buffer, last one wins; n when none does.
inline size_t presenting_list(const std::vector<bool> &presents)
{
    for (size_t i = presents.size(); i-- > 0;)
        if (presents[i])
            return i;
    return presents.size();
}

}
