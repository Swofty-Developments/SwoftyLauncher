// Host-native unit test for the core's direct start (native/src/core/direct_start_logic.hpp).
// Built and run by native/test/run_direct_start_test.sh.

#include "direct_start_logic.hpp"

#include <cstdio>

namespace ds = runtime::direct_start;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

constexpr uint32_t FTUE_DONE = 0x0002b5d6;

int main()
{
    ds::Driver d;
    expect(d.on_update(ds::STATE_FTUE, FTUE_DONE, false) == FTUE_DONE, "an unarmed start leaves the menu path alone");
    expect(d.on_update(ds::STATE_FRONTEND_WORLD, ds::LINK_STAY, false) == ds::LINK_STAY,
           "an unarmed start forces nothing");
    expect(!d.engage_title(), "an unarmed start leaves the title alone");

    d.arm(ds::session_target());
    expect(d.engage_title(), "an armed start passes the title");
    d.setup_result(ds::Setup::done);
    expect(d.phase == ds::Phase::profile, "a setup result before the profile load is ignored");
    expect(d.on_update(ds::STATE_FTUE, FTUE_DONE, false) == ds::LINK_STAY,
           "the first-run state is held until the profile has loaded");
    expect(d.on_update(ds::STATE_FTUE, ds::LINK_STAY, false) == ds::LINK_STAY, "a first-run state still busy stays");

    d.profile_loaded();
    expect(d.phase == ds::Phase::setup, "the profile load starts the setup");
    expect(d.on_update(ds::STATE_FTUE, FTUE_DONE, false) == ds::LINK_STAY, "the first-run state is held during the setup");
    d.setup_result(ds::Setup::pending);
    expect(d.phase == ds::Phase::setup, "a pending setup keeps waiting");
    d.setup_result(ds::Setup::done);
    expect(d.phase == ds::Phase::leave, "a finished setup leaves the front end");

    expect(d.on_update(ds::STATE_FRONTEND_WORLD, ds::LINK_STAY, true) == ds::LINK_STAY,
           "the front end world is not forced while a child runs");
    expect(d.on_update(ds::STATE_FTUE, FTUE_DONE, false) == ds::LINK_EXIT,
           "the first-run state exits instead of opening the menu");
    expect(d.on_update(ds::STATE_FTUE, FTUE_DONE, false) == FTUE_DONE, "the exit is forced once");
    expect(d.on_update(ds::STATE_ONLINE_FRONTEND, ds::LINK_STAY, false) == ds::LINK_STAY, "another state keeps its link");
    expect(d.on_update(ds::STATE_FRONTEND_WORLD, 0x42b3b163, false) == ds::LINK_SESSION,
           "the front end world leaves with session whatever it picked");
    expect(d.phase == ds::Phase::left && !d.engage_title(), "the start is done after the forced session link");
    expect(d.on_update(ds::STATE_FRONTEND_WORLD, ds::LINK_STAY, false) == ds::LINK_STAY,
           "a later front end world is left alone");

    ds::Driver f;
    f.arm(ds::session_target());
    f.profile_loaded();
    f.setup_result(ds::Setup::failed, "no Grand Prix mode");
    expect(f.phase == ds::Phase::failed && f.why == "no Grand Prix mode", "a failed setup keeps its reason");
    expect(f.on_update(ds::STATE_FTUE, FTUE_DONE, false) == FTUE_DONE, "a failed start lets the menu open");
    expect(!f.engage_title(), "a failed start stops passing the title");

    ds::Driver r;
    r.arm(ds::room_target());
    expect(r.on_update(ds::STATE_FTUE, FTUE_DONE, false) == ds::LINK_STAY,
           "a room target holds the first-run state until the profile has loaded");
    r.profile_loaded();
    expect(r.on_update(ds::STATE_FTUE, FTUE_DONE, false) == ds::LINK_STAY,
           "a room target holds the first-run state while the room is set up");
    r.setup_result(ds::Setup::done);
    expect(r.phase == ds::Phase::left, "a room target is done once its setup is");
    expect(r.on_update(ds::STATE_FTUE, FTUE_DONE, false) == FTUE_DONE, "a set-up room lets the first-run state go home");
    expect(r.on_update(ds::STATE_FRONTEND_WORLD, ds::LINK_STAY, false) == ds::LINK_STAY,
           "a room target forces nothing on the front end world");
    expect(r.on_update(ds::STATE_ONLINE_FRONTEND, ds::LINK_STAY, false) == ds::LINK_STAY,
           "a room target lets home take the lobby link itself");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
