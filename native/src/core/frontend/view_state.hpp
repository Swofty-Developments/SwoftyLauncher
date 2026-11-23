#pragma once
#include <cstdint>
#include <string_view>

namespace frontend::view {

enum Kind : uint32_t {
    UNKNOWN = 0,
    COCKPIT,
    TCAM,
    TCAM_OFFSET,
    CHASE_NEAR,
    CHASE_FAR,
    OTHER,
    TRACKSIDE,
    REVERSE_POD,
    NOSE,
    NOSE_ALT,
    WING
};
enum Flag : uint32_t { PAUSED = 1, REPLAY = 2, PHOTO = 4, MENU = 8, CUTSCENE = 16, SPECTATING = 32 };

struct State {
    uint32_t kind = UNKNOWN, flags = 0;
    bool valid = false;
    uint32_t serial = 0;
};

State current();
bool flow_seen();
uint32_t pause_reasons();
void setup();
void load_trackside_cameras(std::string_view track);
void flow(uint32_t hash, uint32_t to);

}
