#include "core/guards/garage/pit_garage_state_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <utility>

// Pit garage state guard.
//
// The pit lane system (PitLaneMovementManager +0xae8) keeps one state word per garage at +0x210 (0, 1 or 2), and every
// writer indexes it with the slot it takes from the car's pit lane box (+0xe0): box >> 1, 12 for the box 99, with no
// bound. A car without a garage has the box -1 (a third car of a team, or a team past the track world's garage count:
// the pit box and garage slot guards), so its slot is 0x7fffffff and the store lands 8 GB past the system. The pit
// lane update Game_PitLane_UpdateMovement (0x1434b83e0) skips only the box 0x7fffffff (0x1434b87ee) before its five
// stores: 0x1434b93bb, 0x1434b9de8, 0x1434ba25e, 0x1434baa4d and 0x1434bace1 (the hold-in-garage states 10 and 11 the
// pit box guard already sees such a car in). Game_PitLane_PutVehicleInPitBox (0x1434d12ba), which the red flag calls
// for every car, and the crew ready body in .data1 (0x1503d24fb) store the same way. The copy of the crew ready
// handler in .xtls (0x1434b83cb) stores only for the box 99.
//
// At each store the guard looks at the slot. A slot past the game's own ones (13) skips the store, so the car's
// garage state is not written. The cached pose store in Game_PitLane_ProjectBoxPose (0x1434c36a6) follows the pit box
// guard's site, which already sends a box of 24 or more to the function's end.
//
// The stores were found on the KB build and on Steam 1.0.145, where every pattern below matches once with the same
// bytes (the box +0xe0, the system +0xae8, the array +0x210 and the 99 to 12 mapping), in .tls$ for .xtls and .trace
// for .data1. A build that matches no pattern leaves that store as it is.
//
// SWOFTY_PIT_GARAGE_STATE_GUARD=0 turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace pgs = runtime::pit_garage_state;
namespace bp = runtime::build_proof;

constexpr bp::Proof STORES{"the pit lane's per-garage state stores (PitLaneMovementManager +0xae8, PitLaneSystem +0x210, "
                           "slot = box +0xe0 >> 1, 12 for 99)",
                           bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};
constexpr bp::Gate GARAGE_STATE{"pit garage state guard", &STORES};

const runtime::Sig S_UPDATE_A{"site_Game_PitLane_GarageStateClearA",
                              "49 8B 47 08 8B 80 E0 00 00 00 89 C1 D1 E9 83 F8 63 B8 0C 00 00 00 0F 44 C8 C7 84 8A 10 02 00 00 "
                              "00 00 00 00",
                              0x34b93bb,
                              runtime::Sig::addr,
                              0,
                              0x19,
                              0};
const runtime::Sig S_UPDATE_B{"site_Game_PitLane_GarageStateSetB",
                              "41 8B 86 E0 00 00 00 89 C1 D1 E9 83 F8 63 B8 0C 00 00 00 0F 44 C8 48 8B 86 E8 0A 00 00 C7 84 88 "
                              "10 02 00 00 01 00 00 00 48 8B 8E E8 0A 00 00 48 8B 44 24 58 48 8B 44 01 68 83 78 04 04 0F 85 BE "
                              "07 00 00",
                              0x34b9de8,
                              runtime::Sig::addr,
                              0,
                              0x1d,
                              0};
const runtime::Sig S_UPDATE_C{"site_Game_PitLane_GarageStateClearC",
                              "8B 85 E0 00 00 00 89 C1 D1 E9 83 F8 63 B8 0C 00 00 00 0F 44 C8 48 8B 86 E8 0A 00 00 C7 84 88 10 "
                              "02 00 00 00 00 00 00 E9",
                              0x34ba25e,
                              runtime::Sig::addr,
                              0,
                              0x1c,
                              0};
const runtime::Sig S_UPDATE_D{"site_Game_PitLane_GarageStateSetD",
                              "41 8B 86 E0 00 00 00 89 C1 D1 E9 83 F8 63 B8 0C 00 00 00 0F 44 C8 48 8B 86 E8 0A 00 00 C7 84 88 "
                              "10 02 00 00 01 00 00 00 48 8B 8E E8 0A 00 00 48 8B 44 24 58 48 8B 44 01 68 83 78 04 04 0F 85 B0 "
                              "00 00 00",
                              0x34baa4d,
                              runtime::Sig::addr,
                              0,
                              0x1d,
                              0};
const runtime::Sig S_UPDATE_E{"site_Game_PitLane_GarageStateClearE",
                              "C7 85 D8 00 00 00 00 00 00 00 48 8B 8E E8 0A 00 00 89 C0 C7 84 81 10 02 00 00 00 00 00 00",
                              0x34bace1,
                              runtime::Sig::addr,
                              0,
                              0x13,
                              0};
const runtime::Sig S_PUT_IN_BOX{"site_Game_PitLane_PutInBoxGarageState",
                                "41 BE 0C 00 00 00 44 0F 45 F1 EB 03 45 31 F6 48 89 F1 89 DA E8 ?? ?? ?? ?? 42 C7 84 B6 10 02 00 "
                                "00 01 00 00 00",
                                0x34d12ba,
                                runtime::Sig::addr,
                                0,
                                0x19,
                                0};
const runtime::Sig S_CREW_READY{"site_Game_PitLane_CrewReadyGarageState",
                                "C6 87 04 01 00 00 01 8B 87 E0 00 00 00 89 C1 D1 E9 83 F8 63 B8 0C 00 00 00 0F 45 C1 48 8B 8E E8 "
                                "0A 00 00 C7 84 81 10 02 00 00 02 00 00 00",
                                0x103d24fb,
                                runtime::Sig::addr,
                                0,
                                0x23,
                                0,
                                ".data1"};

enum class Slot : uint8_t { rax, rcx, r14 };

struct Store {
    const runtime::Sig &sig;
    Slot slot;
    uint8_t len;
};

constexpr size_t STORE_COUNT = 7;
const Store STORE_SITES[STORE_COUNT] = {
    {S_UPDATE_A, Slot::rcx, 11}, {S_UPDATE_B, Slot::rcx, 11},   {S_UPDATE_C, Slot::rcx, 11},   {S_UPDATE_D, Slot::rcx, 11},
    {S_UPDATE_E, Slot::rax, 11}, {S_PUT_IN_BOX, Slot::r14, 12}, {S_CREW_READY, Slot::rax, 11},
};

uintptr_t resume[STORE_COUNT];
safetyhook::MidHook hooks[STORE_COUNT];
std::atomic<bool> said{false};

template <size_t N> void store(safetyhook::Context &c)
{
    const Store &s = STORE_SITES[N];
    uint64_t slot = s.slot == Slot::rax ? c.rax : s.slot == Slot::rcx ? c.rcx : c.r14;
    if (pgs::slot_usable(slot))
        return;
    c.rip = resume[N];
    if (!said.exchange(true))
        swofty::log("[pit] garage state guard: a car with no garage (slot %llu at %s) left the pit lane's garage "
                    "state alone",
                    (unsigned long long)slot, s.sig.what);
}

template <size_t... N> constexpr std::array<safetyhook::MidHookFn, STORE_COUNT> store_fns(std::index_sequence<N...>)
{
    return {&store<N>...};
}

constexpr auto STORE_FNS = store_fns(std::make_index_sequence<STORE_COUNT>());

bool arm(size_t n)
{
    const Store &s = STORE_SITES[n];
    uintptr_t at = runtime::find(s.sig, "pit garage state guard", "pit");
    if (!at)
        return false;
    resume[n] = at + s.len;
    runtime::loader::Installed r = runtime::loader::install(hooks[n], at, s.sig.what, STORE_FNS[n]);
    if (!r) {
        resume[n] = 0;
        swofty::log("[pit] garage state guard: %s: the hook failed: %s", s.sig.what, r.error.c_str());
        return false;
    }
    return true;
}

void setup()
{
    char v[16] = {};
    if (guards::env_off("SWOFTY_PIT_GARAGE_STATE_GUARD", v)) {
        swofty::log("[pit] garage state guard off (SWOFTY_PIT_GARAGE_STATE_GUARD=%s)", v);
        return;
    }
    if (!runtime::holds(GARAGE_STATE))
        return;
    unsigned armed = 0;
    for (size_t n = 0; n < STORE_COUNT; n++)
        armed += arm(n);
    if (armed)
        swofty::log("[pit] garage state guard armed at %u of %u stores: a car without a garage leaves the pit lane's "
                    "garage state alone instead of faulting",
                    armed, unsigned(STORE_COUNT));
    else
        swofty::log("[pit] garage state guard: not hooked");
}

}

SWOFTY_HOOK_GUARD(pit_garage_state, "pit garage state guard", setup)
