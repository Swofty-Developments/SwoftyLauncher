// Host unit test for the view tracker's flow table (view_flow_logic.hpp). The hashes are the ones a real run
// logged: session 20261105-215752 entered Instant Replay as flow node 24941c93 and logged no
// djb2("SessionStateViewInstantReplay") (203f3b13).
#include "core/frontend/viewport_camera_logic.hpp"
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
    ViewportCamera camera;
    expect(!camera.observe(0, 3) && camera.kind() == UNKNOWN, "camera commits outside a session are ignored");
    camera.update_phase(true, true);
    expect(camera.kind() == OTHER, "replay starts with no confirmed onboard view");

    // Monza post-race replay, Steam depot 26566308, 2026-11-17. The game logged these
    // primary viewport camera writes while Swofty reported other for the entire replay.
    const struct {
        uint8_t type;
        Kind expected;
        const char *name;
    } cuts[] = {
        {8, OTHER, "s01a_cam"},    {8, OTHER, "s02a_cam"},     {8, OTHER, "s03a_cam"},
        {8, OTHER, "s04a_cam"},    {4, TCAM, "pod"},           {5, TCAM_OFFSET, "podoffset"},
        {6, CHASE_NEAR, "chase"},  {7, CHASE_FAR, "chasefar"}, {0, WING, "wing"},
        {1, NOSE, "nose"},         {8, OTHER, "pod_reverse"},  {1, NOSE, "nose"},
        {2, NOSE_ALT, "nose_alt"}, {8, OTHER, "pod_reverse"},  {2, NOSE_ALT, "nose_alt"},
        {3, COCKPIT, "cockpit"},   {8, OTHER, "pod_reverse"},  {3, COCKPIT, "cockpit"},
        {8, OTHER, "s08a_cam"},    {8, OTHER, "s11a_cam"},     {8, OTHER, "s11b_cam"},
        {8, OTHER, "s01b_cam"},    {8, OTHER, "s11b_cam"},     {8, OTHER, "s01a_cam"},
    };
    const auto onboard = render::postfx::view_bit("cockpit") | render::postfx::view_bit("tcam") |
                         render::postfx::view_bit("tcam_offset") | render::postfx::view_bit("nose") |
                         render::postfx::view_bit("nose_alt") | render::postfx::view_bit("wing");
    for (const auto &cut : cuts) {
        camera.observe(0, cut.type);
        expect(camera.kind() == cut.expected, cut.name);
        const bool eligible = !render::postfx::ineligible({onboard, 0}, camera.kind(), REPLAY, true);
        expect(eligible == (cut.type <= 5), std::string(cut.name) + " forward onboard pass eligibility");
    }
    camera.observe(0, 5);
    expect(!camera.observe(1, 8) && camera.kind() == TCAM_OFFSET, "the secondary viewport cannot change the replay view");
    camera.update_phase(true, true);
    expect(!camera.observe(0, 5) && camera.kind() == TCAM_OFFSET, "repeated activity and type preserve the result");
    expect(camera.observe(0, 255) && camera.kind() == OTHER, "an unknown type clears the onboard classification");
    camera.observe(0, 3);
    camera.update_phase(false, false);
    camera.update_phase(true, true);
    expect(camera.kind() == OTHER, "a new replay cannot inherit the last camera");

    // Melbourne GP. The offset T-cam was assigned before InGame at race start
    // and again between replay exit and the return to InGame.
    camera.update_phase(false, false);
    camera.update_phase(true, false);
    expect(camera.kind() == UNKNOWN, "a new session begins without a camera");
    expect(camera.observe(0, 0) && camera.kind() == WING, "raw type zero publishes wing while driving");
    expect(camera.observe(0, 8) && camera.kind() == OTHER, "a cinematic assignment is a non-onboard view");
    expect(camera.observe(0, 3) && camera.observe(0, 5), "retain camera writes during the grid setup");
    camera.update_phase(true, false);
    expect(camera.kind() == TCAM_OFFSET, "first driving frame keeps the assignment from grid setup");
    expect(camera.observe(0, 4) && camera.kind() == TCAM, "a live camera switch takes effect");
    expect(!camera.observe(1, 3) && camera.kind() == TCAM, "secondary viewport cannot change the driving view");
    expect(camera.observe(0, 1) && camera.kind() == NOSE, "publish the primary nose camera while driving");
    expect(camera.observe(0, 2) && camera.kind() == NOSE_ALT, "publish the alternate nose camera while driving");
    expect(camera.observe(0, 6) && camera.kind() == CHASE_NEAR, "publish the near chase camera while driving");
    expect(camera.observe(0, 7) && camera.kind() == CHASE_FAR, "publish the far chase camera while driving");
    camera.update_phase(true, true);
    expect(camera.kind() == OTHER, "replay does not inherit the driving camera");
    expect(camera.observe(0, 3) && camera.kind() == COCKPIT, "replay camera updates during the replay");
    camera.update_phase(true, false);
    expect(camera.kind() == UNKNOWN, "replay camera is invalidated before driving resumes");
    expect(camera.observe(0, 5), "cache the restored driving camera before InGame");
    camera.update_phase(true, false);
    expect(camera.kind() == TCAM_OFFSET, "first frame back in driving reports the restored camera");
    camera.update_phase(false, false);
    expect(!camera.observe(0, 5) && camera.kind() == UNKNOWN, "session exit drops the saved camera");

    expect(is_reverse_pod("pod_reverse") && is_reverse_pod("podoff_reverse"), "recognize both reverse-pod definitions");
    expect(!is_reverse_pod("pod") && !is_reverse_pod("pod_reverse_extra") && !is_reverse_pod("s01a_cam") && !is_reverse_pod(""),
           "forward, unrelated and unavailable definitions are not reverse pods");
    camera.update_phase(true, true);
    expect(camera.observe(0, 8, TRACKSIDE) && camera.kind() == TRACKSIDE, "publish trackside during replay");
    expect(camera.observe(0, 8, REVERSE_POD) && camera.kind() == REVERSE_POD, "publish a type 8 switch to the reverse pod");
    expect(!camera.observe(1, 8, TRACKSIDE) && camera.kind() == REVERSE_POD, "ignore a secondary type 8 assignment");
    expect(camera.observe(0, 8) && camera.kind() == OTHER, "an unknown type 8 cannot retain the previous classification");

    ViewportCamera recovered;
    expect(!recovered.recover(0), "recovery requires an active session");
    recovered.update_phase(true, true);
    expect(!recovered.recover(3), "recovery cannot set a camera in replay");
    const Kind native_kinds[] = {WING, NOSE, NOSE_ALT, COCKPIT, TCAM, TCAM_OFFSET, CHASE_NEAR, CHASE_FAR, OTHER};
    for (uint8_t type = 0; type < sizeof(native_kinds) / sizeof(*native_kinds); ++type) {
        recovered.update_phase(true, true);
        recovered.update_phase(true, false);
        expect(recovered.recover(type) && recovered.kind() == native_kinds[type],
               "recover native viewport type " + std::to_string(type));
    }
    recovered.update_phase(true, true);
    recovered.update_phase(true, false);
    expect(!recovered.recover(9) && !recovered.recover(255) && recovered.kind() == UNKNOWN,
           "recovery ignores unrecognized native types");
    expect(recovered.recover(8, TRACKSIDE) && recovered.kind() == TRACKSIDE, "recover a verified trackside camera");
    expect(!recovered.recover(4) && recovered.kind() == TRACKSIDE, "recovery preserves a known classification");
    recovered.update_phase(true, true);
    recovered.update_phase(true, false);
    expect(recovered.recover(8, REVERSE_POD) && recovered.kind() == REVERSE_POD, "recover a verified reverse pod");

    printf("%s\n", fails ? "view flow test: FAILED" : "view flow test: ok");
    return fails ? 1 : 0;
}
