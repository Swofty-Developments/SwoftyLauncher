#pragma once

#include "core/frontend/view_state.hpp"

namespace frontend::view {

class ReplayCamera {
public:
    void set_active(bool active)
    {
        active_ = active;
        if (!active)
            kind_ = OTHER;
    }

    Kind kind() const { return kind_; }

    bool observe(uint32_t viewport, uint8_t type)
    {
        if (!active_ || viewport != 0)
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

    bool active_ = false;
    Kind kind_ = OTHER;
};

}
