#pragma once

#include "core/frontend/view_state.hpp"

namespace frontend::view {

class ViewportCamera {
public:
    void update_phase(bool in_session, bool replay)
    {
        // Keep assignments made before InGame, but never reuse one from a previous replay or session.
        if (in_session_ != in_session || replay_ != replay)
            kind_ = UNKNOWN;
        in_session_ = in_session;
        replay_ = replay;
    }

    Kind kind() const { return replay_ && kind_ == UNKNOWN ? OTHER : kind_; }

    bool observe(uint32_t viewport, uint8_t type)
    {
        if (!in_session_ || viewport != 0)
            return false;

        Kind next = classify(type);
        if (kind_ == next)
            return false;
        kind_ = next;
        return true;
    }

private:
    static constexpr Kind classify(uint8_t type)
    {
        switch (type) {
        case 3:
            return COCKPIT;
        case 4:
            return TCAM;
        case 5:
            return TCAM_OFFSET;
        default:
            return OTHER;
        }
    }

    bool in_session_ = false;
    bool replay_ = false;
    Kind kind_ = UNKNOWN;
};

}
