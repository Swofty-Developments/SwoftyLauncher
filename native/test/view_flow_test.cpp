// Host unit test for the view tracker's flow table (view_flow_logic.hpp). The hashes are the ones a real run
// logged: session 20261105-215752 entered Instant Replay as flow node 24941c93 and logged no
// djb2("SessionStateViewInstantReplay") (203f3b13).
#include "core/frontend/replay_camera_logic.hpp"
#include "core/frontend/view_flow_logic.hpp"
#include "core/render/postfx/view_logic.hpp"

#include <cstdio>
#include <string>

using namespace frontend::view_flow;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

int main()
{
    expect(group_of(0x24941c93) == G_REPLAY, "instant replay node 24941c93 is a replay state");
    expect(group_of(djb2("SessionStateViewInstantReplay")) == 0, "the class name hash 203f3b13 is not a flow node");
    expect(group_of(0x89b82f18) == G_REPLAY, "highlights replay node 89b82f18 is a replay state");
    expect(group_of(0x5764b973) == G_REPLAY, "post race replay node 5764b973 is a replay state");
    expect(group_of(0xba16bc16) == G_DRIVING, "InGame node ba16bc16 is the driving state");
    expect(group_of(0x1505f42a) == G_SESSION, "StateSession 1505f42a is the session state");
    expect(group_of(0x963b35bf) == G_CUTSCENE, "Cutscene node 963b35bf is a cutscene state");
    expect(group_of(0xb55facbe) == G_SPECTATE, "Spectate node b55facbe is a spectating state");
    expect(group_of(0x835af69b) == G_REPLAY, "LoadReplaySession 835af69b is a replay state");
    for (size_t i = 0; i < TRACKED_N; i++)
        for (size_t j = i + 1; j < TRACKED_N; j++)
            if (TRACKED[i].hash == TRACKED[j].hash)
                expect(false, "duplicate hash " + std::to_string(i) + "/" + std::to_string(j));
    using namespace frontend::view;
    ReplayCamera camera;
    expect(!camera.observe(0, 3) && camera.kind() == OTHER, "camera commits outside replay are ignored");
    camera.set_active(true);
    expect(camera.kind() == OTHER, "replay starts with no confirmed onboard view");

    // Monza post-race replay, Steam depot 26566308, 2026-11-17. The game logged these
    // primary viewport camera writes while Swofty reported other for the entire replay.
    const struct {
        uint8_t type;
        Kind expected;
        const char *name;
    } cuts[] = {
        {8, OTHER, "s01a_cam"},  {8, OTHER, "s02a_cam"},    {8, OTHER, "s03a_cam"},
        {8, OTHER, "s04a_cam"},  {4, TCAM, "pod"},          {5, TCAM_OFFSET, "podoffset"},
        {6, OTHER, "chase"},     {7, OTHER, "chasefar"},    {0, OTHER, "wing"},
        {1, OTHER, "nose"},      {8, OTHER, "pod_reverse"}, {1, OTHER, "nose"},
        {2, OTHER, "nose_alt"},  {8, OTHER, "pod_reverse"}, {2, OTHER, "nose_alt"},
        {3, COCKPIT, "cockpit"}, {8, OTHER, "pod_reverse"}, {3, COCKPIT, "cockpit"},
        {8, OTHER, "s08a_cam"},  {8, OTHER, "s11a_cam"},    {8, OTHER, "s11b_cam"},
        {8, OTHER, "s01b_cam"},  {8, OTHER, "s11b_cam"},    {8, OTHER, "s01a_cam"},
    };
    const auto onboard =
        render::postfx::view_bit("cockpit") | render::postfx::view_bit("tcam") | render::postfx::view_bit("tcam_offset");
    for (const auto &cut : cuts) {
        camera.observe(0, cut.type);
        expect(camera.kind() == cut.expected, cut.name);
        const bool eligible = !render::postfx::ineligible({onboard, 0}, camera.kind(), REPLAY, true);
        expect(eligible == (cut.expected != OTHER), std::string(cut.name) + " onboard pass eligibility");
    }
    camera.observe(0, 5);
    expect(!camera.observe(1, 8) && camera.kind() == TCAM_OFFSET, "the secondary viewport cannot change the replay view");
    camera.set_active(true);
    expect(!camera.observe(0, 5) && camera.kind() == TCAM_OFFSET, "repeated activity and type preserve the result");
    expect(camera.observe(0, 255) && camera.kind() == OTHER, "an unknown type clears the onboard classification");
    camera.observe(0, 3);
    camera.set_active(false);
    camera.set_active(true);
    expect(camera.kind() == OTHER, "a new replay cannot inherit the last camera");
    printf("%s\n", fails ? "view flow test: FAILED" : "view flow test: ok");
    return fails ? 1 : 0;
}
