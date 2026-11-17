#pragma once

#include <cstdint>

// The car features each game build runs. The KB build runs every one. Another build runs a feature only when everything
// the feature hooks, calls, reads or writes was compared against the KB build's code on that exe and found the same
// (re/tools/portcheck.py; the evidence for each build is in re/research/port-<build>.md): its bindings land on the same
// code and its fields sit at the same displacements. A wrong offset there would corrupt every car, so a build that is
// not listed runs none of them.

namespace swofty_core::hooks::car_builds {

enum Feature : uint32_t {
    // car.built, car.reset, car.upgrades.replayed, session.pit, session.pit_stop and car.setup.applied
    POINTS = 1u << 0,
    // the live car operations (SwoftyCoreCarOps): reads and writes of the cars' handling fields
    OPS = 1u << 1,
    // the engine torque, AI grip and AI racecraft mixins
    LEVERS = 1u << 2,
};

inline constexpr uint32_t ALL = POINTS | OPS | LEVERS;

struct Proof {
    uint32_t pe_timestamp;
    const char *name;
    uint32_t features;
};

inline constexpr Proof PORTED[] = {
    {1794312636u, "Steam 26566308 (1.0.145.2472)", POINTS},
};

constexpr uint32_t features_of(uint32_t pe_timestamp, bool kb_build)
{
    if (kb_build)
        return ALL;
    for (const Proof &p : PORTED)
        if (p.pe_timestamp == pe_timestamp)
            return p.features;
    return 0;
}

constexpr const char *name_of(uint32_t pe_timestamp, bool kb_build)
{
    if (kb_build)
        return "the KB build";
    for (const Proof &p : PORTED)
        if (p.pe_timestamp == pe_timestamp)
            return p.name;
    return nullptr;
}

static_assert(features_of(1789118616u, true) == ALL);
static_assert(features_of(1794312636u, false) == POINTS);
static_assert(features_of(1789118616u, false) == 0);
static_assert(features_of(1789128892u, false) == 0);
static_assert(features_of(0, false) == 0 && name_of(0, false) == nullptr);

}
