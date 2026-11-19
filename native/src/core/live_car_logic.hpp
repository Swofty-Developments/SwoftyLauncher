#pragma once

// Which of the live car's reads a build is allowed. The vehicle table and the CarLive, engine, gearbox, chassis, body
// and steering fields are the std live layouts (live/live_layout.hpp), compared against the EA App build's code
// (docs/reference/std/live.md), and CarLive.suspension, CarSuspension.wheels and CarF1Aero.wing_open_fraction were
// compared with re/tools/layoutcmp.py against the EA dump (same instruction, same displacement), so they read on every
// verified build. On EA App 1.0.145 the vehicle table's car fields (CarLive +0x124, inputs +0x188, +0x190, +0x224,
// engine, gearbox, chassis, steering, suspension and aero pointers, the gear index, engine rates, chassis g and body,
// body position and velocity, steering angle and lock) and the aero's wing_open_fraction +0x19a8 are read at the KB
// displacements by code that is the same there (layoutcmp.py, sigport.py --witness), so they read on every ported build
// (runtime::ported_build()) too. The per-wheel block (surface, wheel wetness, spin, slip), the chassis basis and
// angular velocity have no KB xref to compare, and the track wetness singleton is a hard-coded Steam RVA, so they read
// on the KB build only.

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

constexpr Fields fields_of(bool same_build, bool ported_build)
{
    Fields f;
    f.table = f.drs = ported_build;
    f.wetness = f.surface = f.orient = f.angvel = f.wheels = same_build && ported_build;
    return f;
}

static_assert(fields_of(false, false).table == false);
static_assert(fields_of(false, true).table && fields_of(false, true).drs && !fields_of(false, true).surface &&
              !fields_of(false, true).wetness);
static_assert(fields_of(true, true).table && fields_of(true, true).surface && fields_of(true, true).drs);

}
