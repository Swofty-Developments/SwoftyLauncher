#pragma once
#include <cstdint>

#include <swofty_api.h>

#include "core/frontend/view_state.hpp"

namespace swofty_core::live::hud {

struct Shown {
    bool known = false;
    uint8_t hidden_by = 0;
};

inline Shown from_view(bool flow_seen, uint32_t flags, uint32_t pause_reasons)
{
    if (!flow_seen)
        return {};
    uint8_t by = 0;
    if (flags & frontend::view::MENU)
        by |= SWOFTY_LIVE_HUD_MENU;
    if (flags & frontend::view::CUTSCENE)
        by |= SWOFTY_LIVE_HUD_CUTSCENE;
    if (flags & frontend::view::REPLAY)
        by |= SWOFTY_LIVE_HUD_REPLAY;
    if (flags & frontend::view::PHOTO)
        by |= SWOFTY_LIVE_HUD_PHOTO;
    if (pause_reasons)
        by |= SWOFTY_LIVE_HUD_PAUSED;
    return {true, by};
}

// At lights out the game passes through unnamed flow nodes between StartLights and InGame for two or three frames,
// which read as the menu. A menu-only hide within BRIDGE_MS of the last shown frame keeps the HUD shown.
class Bridge {
public:
    static constexpr uint64_t BRIDGE_MS = 100;

    Shown step(const Shown &h, uint64_t now_ms)
    {
        if (h.known && !h.hidden_by) {
            shown_ = true;
            shown_at_ = now_ms;
            return h;
        }
        if (shown_ && h.known && h.hidden_by == SWOFTY_LIVE_HUD_MENU && now_ms - shown_at_ <= BRIDGE_MS)
            return {true, 0};
        shown_ = false;
        return h;
    }

private:
    bool shown_ = false;
    uint64_t shown_at_ = 0;
};

inline void apply(SwoftyLiveSession &s, const Shown &h)
{
    s.hud_visible = h.known && !h.hidden_by;
    s.hud_hidden_by = h.known ? h.hidden_by : 0;
    if (h.known)
        s.have |= SWOFTY_LIVE_SHAVE_HUD;
    else
        s.have &= ~SWOFTY_LIVE_SHAVE_HUD;
}

}
