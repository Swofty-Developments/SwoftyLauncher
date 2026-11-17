#pragma once

#include <cstdint>

// Which of the live car's reads a build is allowed. The vehicle table and the CarLive, engine, gearbox, chassis, body
// and steering fields are the std live layouts (live/live_layout.hpp), compared against the EA App build's code
// (docs/reference/std/live.md), and CarLive.suspension, CarSuspension.wheels and CarF1Aero.wing_open_fraction were
// compared with re/tools/layoutcmp.py against the EA dump (same instruction, same displacement), so they read on every
// verified build. The per-wheel block (surface, wheel wetness, spin, slip), the chassis basis and angular velocity have
// no KB xref to compare, and the track wetness singleton is a hard-coded Steam RVA, so they read on the KB build only.

namespace live_car {

struct Fields {
    bool table = false;
    bool wetness = false;
    bool surface = false;
    bool orient = false;
    bool angvel = false;
    bool wheels = false;
    bool drs = false;
};

// Builds other than the KB build whose extra reads were compared against the KB build's code (re/tools/portcheck.py,
// re/research/port-steam-26566308.md). On Steam 26566308 the wheel spin and slip fields and the suspension's wheel
// pointers are at the same displacements, and the track wetness is read through g_Game_WeatherSurfaceState, found by
// signature, at the same +0x3f0a38. The surface, wheel wetness, orientation and angular velocity offsets are not
// proven there.
struct Ported {
    uint32_t pe_timestamp;
    bool wetness, wheels;
};

inline constexpr Ported PORTED[] = {{1794312636u, true, true}};

constexpr Fields fields_of(bool same_build, bool verified_build, uint32_t pe_timestamp = 0)
{
    Fields f;
    f.table = f.drs = verified_build;
    f.wetness = f.surface = f.orient = f.angvel = f.wheels = same_build && verified_build;
    if (verified_build && !same_build)
        for (const Ported &p : PORTED)
            if (p.pe_timestamp == pe_timestamp) {
                f.wetness = p.wetness;
                f.wheels = p.wheels;
            }
    return f;
}

static_assert(fields_of(false, false).table == false);
static_assert(fields_of(false, true).table && fields_of(false, true).drs && !fields_of(false, true).surface &&
              !fields_of(false, true).wetness);
static_assert(fields_of(true, true).table && fields_of(true, true).surface && fields_of(true, true).drs);
static_assert(fields_of(false, true, 1794312636u).wheels && fields_of(false, true, 1794312636u).wetness &&
              !fields_of(false, true, 1794312636u).surface && !fields_of(false, true, 1794312636u).orient);
static_assert(!fields_of(false, false, 1794312636u).wheels && !fields_of(false, true, 1789128892u).wheels);

}
