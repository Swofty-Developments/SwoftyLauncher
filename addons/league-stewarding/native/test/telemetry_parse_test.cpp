// SPDX-License-Identifier: AGPL-3.0-or-later

#include "telemetry_parse.hpp"

#include <cstdio>
#include <vector>

using namespace steward;

namespace {

int failures = 0;

void expect(bool ok, const char *what, int line)
{
    if (!ok) {
        std::printf("FAIL line %d: %s\n", line, what);
        ++failures;
    }
}

#define EXPECT(x) expect((x), #x, __LINE__)

std::vector<uint8_t> header(uint8_t id, size_t total)
{
    std::vector<uint8_t> v(total, 0);
    v[0] = 2025 & 0xff;
    v[1] = 2025 >> 8;
    v[6] = id;
    return v;
}

}

int main()
{
    Model m;

    auto parts = header(kPacketParticipants, kHeaderSize + 1 + kParticipantStride * kMaxCars);
    parts[kHeaderSize] = 2;
    for (int i = 0; i < 2; ++i) {
        uint8_t *p = parts.data() + kHeaderSize + 1 + kParticipantStride * i;
        p[0] = i == 0 ? 0 : 1;
        p[3] = i == 0 ? 8 : 1;
        const char *nm = i == 0 ? "Alice" : "Bob";
        std::memcpy(p + 7, nm, std::strlen(nm));
    }
    m.feed(parts.data(), parts.size(), 1.0);

    auto lap = header(kPacketLapData, kHeaderSize + kLapDataStride * kMaxCars + 2);
    for (int i = 0; i < 2; ++i) {
        uint8_t *p = lap.data() + kHeaderSize + kLapDataStride * i;
        p[32] = i == 0 ? 2 : 1;
        p[33] = 7;
        p[34] = i == 0;
        p[38] = i == 0 ? 5 : 0;
        p[39] = i == 0 ? 2 : 0;
    }
    m.feed(lap.data(), lap.size(), 1.1);

    auto ses = header(kPacketSession, 753);
    ses[kHeaderSize + 3] = 25;
    ses[kHeaderSize + 6] = 15;
    ses[kHeaderSize + 7] = 10;
    ses[kSessionSafetyCarOffset] = 2;
    m.feed(ses.data(), ses.size(), 1.2);

    auto pena = header(kPacketEvent, 45);
    std::memcpy(pena.data() + kHeaderSize, "PENA", 4);
    uint8_t *e = pena.data() + kHeaderSize + 4;
    e[0] = 4; e[1] = 7; e[2] = 0; e[3] = 1; e[4] = 5; e[5] = 7; e[6] = 0;
    const float session_time = 612.5f;
    std::memcpy(pena.data() + 15, &session_time, 4);
    m.feed(pena.data(), pena.size(), 1.3);

    auto sc = header(kPacketEvent, 45);
    std::memcpy(sc.data() + kHeaderSize, "SCAR", 4);
    sc[kHeaderSize + 4] = 1;
    sc[kHeaderSize + 5] = 0;
    m.feed(sc.data(), sc.size(), 1.4);

    Snapshot s = m.snapshot(2.0);
    EXPECT(s.receiving);
    EXPECT(s.track == "Spa");
    EXPECT(s.total_laps == 25);
    EXPECT(s.sc_status == 2);
    EXPECT(s.lap == 7);
    EXPECT(s.drivers.size() == 2);
    EXPECT(s.drivers[0].name == "Bob" && s.drivers[0].position == 1 && !s.drivers[0].human);
    EXPECT(s.drivers[1].name == "Alice" && s.drivers[1].human && s.drivers[1].in_pit);
    EXPECT(s.drivers[1].penalty_s == 5 && s.drivers[1].warnings == 2);
    EXPECT(s.drivers[1].team == "McLaren");
    EXPECT(s.events.size() == 2);
    EXPECT(s.events[0].code == "PENA" && s.events[0].car == 0 && s.events[0].other == 1);
    EXPECT(s.events[0].t == 612.5);
    EXPECT(s.events[0].text.find("Time penalty +5s") != std::string::npos);
    EXPECT(s.events[1].text == "Safety car deployed");
    EXPECT(!m.snapshot(10.0).receiving);

    std::vector<uint8_t> junk(10, 0);
    m.feed(junk.data(), junk.size(), 3.0);

    if (failures) return 1;
    std::printf("telemetry_parse ok\n");
    return 0;
}
