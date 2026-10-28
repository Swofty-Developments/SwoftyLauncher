// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include "telemetry.hpp"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

namespace steward {

constexpr size_t kHeaderSize = 29;
constexpr int kMaxCars = 22;
constexpr size_t kLapDataStride = 57;
constexpr size_t kParticipantStride = 57;
constexpr size_t kSessionSafetyCarOffset = kHeaderSize + 124;
constexpr size_t kEventsMax = 200;

enum PacketId : uint8_t {
    kPacketSession = 1,
    kPacketLapData = 2,
    kPacketEvent = 3,
    kPacketParticipants = 4,
    kPacketFinalClassification = 8,
};

inline uint16_t rd16(const uint8_t *p) { return static_cast<uint16_t>(p[0] | (p[1] << 8)); }

inline const char *track_name(int id)
{
    static const char *names[] = {
        "Melbourne", "Paul Ricard", "Shanghai", "Sakhir", "Catalunya", "Monaco", "Montreal", "Silverstone",
        "Hockenheim", "Hungaroring", "Spa", "Monza", "Singapore", "Suzuka", "Abu Dhabi", "Austin",
        "Interlagos", "Red Bull Ring", "Sochi", "Mexico City", "Baku", "Sakhir Short", "Silverstone Short",
        "Austin Short", "Suzuka Short", "Hanoi", "Zandvoort", "Imola", "Portimao", "Jeddah", "Miami",
        "Las Vegas", "Lusail"};
    if (id >= 0 && id < static_cast<int>(sizeof(names) / sizeof(names[0]))) return names[id];
    switch (id) {
    case 39: return "Silverstone Reverse";
    case 40: return "Red Bull Ring Reverse";
    case 41: return "Zandvoort Reverse";
    default: return "";
    }
}

inline std::string team_name(int id)
{
    static const char *names[] = {"Mercedes", "Ferrari", "Red Bull", "Williams", "Aston Martin", "Alpine", "RB",
                                  "Haas", "McLaren", "Sauber"};
    if (id >= 0 && id < 10) return names[id];
    if (id == 255) return "";
    return "Team " + std::to_string(id);
}

inline const char *infringement_name(int id)
{
    static const char *names[] = {
        "Blocking by slow driving", "Blocking by wrong way driving", "Reversing off the start line",
        "Big collision", "Small collision", "Collision, failed to hand back position (single)",
        "Collision, failed to hand back position (multiple)", "Corner cutting, gained time",
        "Corner cutting overtake (single)", "Corner cutting overtake (multiple)", "Crossed pit exit lane",
        "Ignoring blue flags", "Ignoring yellow flags", "Ignoring drive through", "Too many drive throughs",
        "Drive through reminder (serve within N laps)", "Drive through reminder (serve this lap)",
        "Pit lane speeding", "Parked for too long", "Ignoring tyre regulations", "Too many penalties",
        "Multiple warnings", "Approaching disqualification", "Tyre regulations select (single)",
        "Tyre regulations select (multiple)", "Lap invalidated, corner cutting", "Lap invalidated, running wide",
        "Corner cutting, ran wide (minor)", "Corner cutting, ran wide (significant)",
        "Corner cutting, ran wide (extreme)", "Lap invalidated, wall riding", "Lap invalidated, flashback used",
        "Lap invalidated, reset to track", "Blocking the pit lane", "Jump start", "Safety car to car collision",
        "Safety car illegal overtake", "Safety car exceeding allowed pace", "Virtual safety car exceeding allowed pace",
        "Formation lap below allowed speed", "Formation lap parking", "Retired, mechanical failure",
        "Retired, terminally damaged", "Safety car falling too far back", "Black flag timer",
        "Unserved stop-go penalty", "Unserved drive-through penalty", "Engine component change",
        "Gearbox change", "Parc ferme change", "League grid penalty", "Retry penalty", "Illegal time gain",
        "Mandatory pitstop", "Attribute assigned"};
    if (id >= 0 && id < static_cast<int>(sizeof(names) / sizeof(names[0]))) return names[id];
    return "Unknown infringement";
}

inline const char *penalty_kind_name(int id)
{
    static const char *names[] = {
        "Drive-through", "Stop-go", "Grid penalty", "Penalty reminder", "Time penalty", "Warning", "Disqualified",
        "Removed from formation lap", "Parked too long timer", "Tyre regulations", "Lap invalidated",
        "This and next lap invalidated", "Lap invalidated (no reason)", "This and next lap invalidated (no reason)",
        "This and previous lap invalidated", "This and previous lap invalidated (no reason)", "Retired",
        "Black flag timer"};
    if (id >= 0 && id < static_cast<int>(sizeof(names) / sizeof(names[0]))) return names[id];
    return "Penalty";
}

class Model {
public:
    void feed(const uint8_t *d, size_t n, double now)
    {
        if (n < kHeaderSize) return;
        if (rd16(d) < 2024) return;
        last_packet_ = now;
        any_ = true;
        const uint8_t id = d[6];
        switch (id) {
        case kPacketSession: on_session(d, n); break;
        case kPacketLapData: on_lap_data(d, n); break;
        case kPacketParticipants: on_participants(d, n); break;
        case kPacketEvent: on_event(d, n); break;
        case kPacketFinalClassification: finished_ = true; break;
        default: break;
        }
    }

    Snapshot snapshot(double now) const
    {
        Snapshot s;
        s.receiving = any_ && (now - last_packet_) < 3.0;
        s.last_packet_age_s = any_ ? now - last_packet_ : -1;
        s.track = track_;
        s.total_laps = total_laps_;
        s.sc_status = sc_status_;
        int lead_lap = 0;
        for (int i = 0; i < kMaxCars; ++i) {
            const Car &c = cars_[i];
            if (!c.active) continue;
            Driver dr;
            dr.index = i;
            dr.name = c.name.empty() ? "Car " + std::to_string(i) : c.name;
            dr.team = team_name(c.team);
            dr.position = c.position;
            dr.lap = c.lap;
            dr.in_pit = c.pit;
            dr.human = c.human;
            dr.penalty_s = c.penalty_s;
            dr.warnings = c.warnings;
            if (c.position == 1) lead_lap = c.lap;
            s.lap = std::max(s.lap, c.lap);
            s.drivers.push_back(dr);
        }
        if (lead_lap > 0) s.lap = lead_lap;
        std::stable_sort(s.drivers.begin(), s.drivers.end(), [](const Driver &a, const Driver &b) {
            const int pa = a.position ? a.position : 99, pb = b.position ? b.position : 99;
            return pa < pb;
        });
        s.events = events_;
        return s;
    }

    bool finished() const { return finished_; }
    int session_type() const { return session_type_; }

private:
    struct Car {
        bool active = false, human = false, pit = false;
        std::string name;
        int team = 255, position = 0, lap = 0, warnings = 0;
        float penalty_s = 0;
    };

    std::string name_of(int i) const
    {
        if (i < 0 || i >= kMaxCars) return "";
        return cars_[i].name.empty() ? "Car " + std::to_string(i) : cars_[i].name;
    }

    void push(double t, const char *code, std::string text, int car, int other)
    {
        Event e;
        e.t = t;
        e.code = code;
        e.text = std::move(text);
        e.car = car;
        e.other = other;
        events_.push_back(std::move(e));
        if (events_.size() > kEventsMax) events_.erase(events_.begin(), events_.begin() + (events_.size() - kEventsMax));
    }

    void on_session(const uint8_t *d, size_t n)
    {
        if (n <= kSessionSafetyCarOffset) return;
        const uint8_t *b = d + kHeaderSize;
        total_laps_ = b[3];
        session_type_ = b[6];
        track_ = track_name(static_cast<int8_t>(b[7]));
        sc_status_ = d[kSessionSafetyCarOffset];
    }

    void on_participants(const uint8_t *d, size_t n)
    {
        const size_t need = kHeaderSize + 1 + kParticipantStride * kMaxCars;
        const int count = n > kHeaderSize ? d[kHeaderSize] : 0;
        for (int i = 0; i < kMaxCars; ++i) {
            const size_t off = kHeaderSize + 1 + kParticipantStride * i;
            if (off + kParticipantStride > n) break;
            const uint8_t *p = d + off;
            Car &c = cars_[i];
            c.active = i < count;
            c.human = p[0] == 0;
            c.team = p[3];
            char nm[33];
            std::memcpy(nm, p + 7, 32);
            nm[32] = 0;
            c.name = nm;
        }
        (void)need;
    }

    void on_lap_data(const uint8_t *d, size_t n)
    {
        for (int i = 0; i < kMaxCars; ++i) {
            const size_t off = kHeaderSize + kLapDataStride * i;
            if (off + kLapDataStride > n) break;
            const uint8_t *p = d + off;
            Car &c = cars_[i];
            c.position = p[32];
            c.lap = p[33];
            c.pit = p[34] != 0;
            c.penalty_s = p[38];
            c.warnings = p[39];
            if (c.position > 0 && !c.active) c.active = true;
        }
    }

    void on_event(const uint8_t *d, size_t n)
    {
        if (n < kHeaderSize + 4) return;
        const double t = [&] {
            float f;
            std::memcpy(&f, d + 15, 4);
            return static_cast<double>(f);
        }();
        char code[5] = {static_cast<char>(d[kHeaderSize]), static_cast<char>(d[kHeaderSize + 1]),
                        static_cast<char>(d[kHeaderSize + 2]), static_cast<char>(d[kHeaderSize + 3]), 0};
        const uint8_t *p = d + kHeaderSize + 4;
        const size_t avail = n - kHeaderSize - 4;
        const std::string c = code;
        char buf[256];
        if (c == "SSTA") {
            finished_ = false;
            push(t, "SSTA", "Session started", -1, -1);
        } else if (c == "SEND") {
            finished_ = true;
            push(t, "SEND", "Session ended", -1, -1);
        } else if (c == "PENA" && avail >= 7) {
            const int kind = p[0], inf = p[1], car = p[2], other = p[3], secs = p[4], lap = p[5], places = p[6];
            std::string what = penalty_kind_name(kind);
            if (kind == 4) what += " +" + std::to_string(secs) + "s";
            if (kind == 2) what += " " + std::to_string(places) + " places";
            std::snprintf(buf, sizeof buf, "%s, %s: %s%s%s (lap %d)", what.c_str(), name_of(car).c_str(),
                          infringement_name(inf), other != 255 ? ", with " : "",
                          other != 255 ? name_of(other).c_str() : "", lap);
            push(t, "PENA", buf, car, other == 255 ? -1 : other);
        } else if (c == "COLL" && avail >= 2) {
            std::snprintf(buf, sizeof buf, "Collision: %s and %s", name_of(p[0]).c_str(), name_of(p[1]).c_str());
            push(t, "COLL", buf, p[0], p[1]);
        } else if (c == "SCAR" && avail >= 2) {
            static const char *types[] = {"No safety car", "Safety car", "Virtual safety car", "Formation lap"};
            static const char *evs[] = {"deployed", "returning to pits", "returned", "resume race"};
            std::snprintf(buf, sizeof buf, "%s %s", p[0] < 4 ? types[p[0]] : "Safety car", p[1] < 4 ? evs[p[1]] : "event");
            push(t, "SCAR", buf, -1, -1);
        } else if (c == "RDFL") {
            push(t, "RDFL", "Red flag", -1, -1);
        } else if (c == "DTSV" && avail >= 1) {
            push(t, "DTSV", "Drive-through served: " + name_of(p[0]), p[0], -1);
        } else if (c == "SGSV" && avail >= 1) {
            push(t, "SGSV", "Stop-go served: " + name_of(p[0]), p[0], -1);
        } else if (c == "OVTK" && avail >= 2) {
            push(t, "OVTK", name_of(p[0]) + " overtook " + name_of(p[1]), p[0], p[1]);
        }
    }

    Car cars_[kMaxCars];
    std::vector<Event> events_;
    std::string track_;
    int total_laps_ = 0, sc_status_ = 0, session_type_ = 0;
    bool finished_ = false, any_ = false;
    double last_packet_ = 0;
};

}
