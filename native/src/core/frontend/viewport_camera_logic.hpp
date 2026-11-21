#pragma once

#include "core/frontend/view_state.hpp"

namespace frontend::view {

constexpr bool is_reverse_pod(std::string_view definition)
{
    return definition == "pod_reverse" || definition == "podoff_reverse";
}

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

    bool observe(uint32_t viewport, uint8_t type, Kind fallback = OTHER)
    {
        if (!in_session_ || viewport != 0)
            return false;

        Kind next = type == 8 ? fallback : classify(type);
        if (kind_ == next)
            return false;
        kind_ = next;
        return true;
    }

    bool recover(uint8_t type, Kind fallback = OTHER)
    {
        // The game writes native types 0 through 8, with 8 resolved from the camera object and definition.
        return kind() == UNKNOWN && type <= 8 && observe(0, type, fallback);
    }

private:
    static constexpr Kind classify(uint8_t type)
    {
        switch (type) {
        case 0:
            return WING;
        case 1:
            return NOSE;
        case 2:
            return NOSE_ALT;
        case 3:
            return COCKPIT;
        case 4:
            return TCAM;
        case 5:
            return TCAM_OFFSET;
        case 6:
            return CHASE_NEAR;
        case 7:
            return CHASE_FAR;
        default:
            return OTHER;
        }
    }

    bool in_session_ = false;
    bool replay_ = false;
    Kind kind_ = UNKNOWN;
};

}
