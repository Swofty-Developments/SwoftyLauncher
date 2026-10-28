#pragma once
#include <cstdint>
#include <string>
#include <vector>
namespace steward {
struct Driver { int index = 0; std::string name, team; int position = 0, lap = 0; bool in_pit = false, human = false; float penalty_s = 0; int warnings = 0; };
struct Event { double t = 0; std::string code, text; int car = -1, other = -1; };
struct Snapshot { bool receiving = false; double last_packet_age_s = -1; std::string track; int lap = 0, total_laps = 0; int sc_status = 0; std::vector<Driver> drivers; std::vector<Event> events; };
bool telemetry_start(uint16_t port);  // background UDP listener on 127.0.0.1:port; idempotent
void telemetry_stop();
Snapshot telemetry_snapshot();        // thread-safe copy
}
