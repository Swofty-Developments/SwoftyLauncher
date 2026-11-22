#pragma once

#include "core/runtime/focus_logic.hpp"
#include "core/runtime/presence_id_logic.hpp"

#include "core/testing/direct_start_logic.hpp"
#include "core/events/session_lifecycle_logic.hpp"
#include <windows.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <functional>
#include <set>
#include <string>
#include <vector>

#include "core/boxes/era/box.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/files.hpp"
#include "core/runtime/hashing.hpp"
#include "core/runtime/json.hpp"
#include "core/runtime/loader/introspection.hpp"
#include "core/runtime/robin_map.hpp"
#include "core/runtime/stage.hpp"
#include "swofty_api.h"

namespace swofty_core::registry {
class Store;
}

namespace career::saves {
struct SlotRead;
}

namespace career::resume_list {
struct Row;
}

namespace runtime {

// The text of swofty/settings.json as the launcher wrote it (empty when absent).
const std::string &settings_json_text();

namespace myteam_heal {
struct Load;
}

struct OverlayErp {
    std::string path;
    uint64_t size = 0;
    size_t resources = 0;
    std::vector<std::string> targets;
};

struct Overlay {
    std::vector<OverlayErp> erps;
    size_t replaced = 0, added = 0;
};

const Overlay *resource_overlay(DWORD wait_ms);
void overlay_registered(const std::vector<uint64_t> &erp_hashes, size_t groups);
void overlay_dropped(const std::string &why);
std::string erp_name(uint64_t hash);

const SwoftyAssets *assets_table();
const SwoftyEvents *events_table();
void frame_probe_name(const char *what);
const SwoftyOverlay *overlay_table();
// Loads the image file at `path` as the overlay image `id`: its slot, or -1. An id and path that failed once are not
// tried again.
int overlay_image(const std::string &id, const std::string &path, uint32_t *width, uint32_t *height);
const SwoftyTelemetry *telemetry_table();
const SwoftyDb *db_table();
// True once a db_table() call has resolved the database reader's signatures; the core's own readers check it so
// they never initialise the reader (and log its resolve) in a run where no addon reads the game database.
bool db_initialised();
const SwoftyStorage *storage_table();
std::wstring storage_write_path(const std::string &id, const std::string &path);
const SwoftyCareer *career_table();
const SwoftyPopup *popup_table();
const SwoftyLang *lang_table();
struct LangOp {
    bool clear = false;
    std::string language, key;
    const char *text = nullptr;
};
// Applies the string overrides in order for the calling addon in one pass (one lock, one registry update, one refresh of
// the changed keys), leaving the same state as the matching override_set and override_clear calls; returns how many failed.
size_t lang_batch(const std::vector<LangOp> &ops);
const SwoftyCharacters *characters_table();
const SwoftyCareerWizard *wizard_table();
void lang_override_setup();
const json::Value &sdk();
// Ends the addon asset registrations; false when they were already closed.
bool close_asset_registrations();
void ready();
void assets_setup();
void resources_setup();
void gamedb_setup();
void dbread_setup();
void overlay_setup();
void events_setup();
bool events_flow_armed();
bool events_session_enter_armed();
void events_drop_module(void *module);
size_t events_fire(const char *event, int64_t a, int64_t b, const void *data);
void session_emit(const session_life::Emit &e);
void session_flow(const char *known, int to);
// The live session's session_life::Kind and a counter that moves on every change of it; both are atomic loads.
int session_kind();
uint64_t session_serial();
// Arms or drops the Time Trial player name rename for the live session (player_name.cpp).
void player_name_session();

struct CoreMixins {
    const char *name;
    std::vector<SwoftyMixin> mixins;
};

enum class TileRoute { pending, live, off };

std::vector<CoreMixins> career_mixins();
void tile_route_want();
TileRoute tile_route_state();
void wizard_tile_routed(const std::string &group, void *out);
bool wizard_route_running(const std::string &route, void *name);
void career_setup();
void career_tick();
int career_block(const void *from, uint32_t mode, const char *title, const char *body);
int career_unblock(const void *from, uint32_t mode);
void career_start_setup();
void career_start_drop_module(void *module);
bool career_start_live();
bool in_session_now();
bool main_menu_account(std::string &name);
uint32_t career_start_picked();
void popup_setup();
void popup_tick();
void hud_rebuilt();
bool typing(const char *what);
inline bool game_in_front()
{
    HWND w = GetForegroundWindow();
    DWORD pid = 0;
    if (w)
        GetWindowThreadProcessId(w, &pid);
    return focus::game_focused(pid, GetCurrentProcessId(), w && IsIconic(w));
}
void mfd_setup();
void mfd_tick();
int popup_show(const std::string &title, const std::string &body, const std::string &by);
int popup_confirm(const std::string &title, const std::string &body, void *fn, void *ctx, void (*failed)(void *),
                  const std::string &by);
int hud_toast(const std::string &text, const std::string &detail, float seconds, const std::string &by,
              const std::string &icon = {});
const SwoftyCareerContent *career_content_table();
void career_content_tick();
void entitlements_tick();
void entitlements_flow(uint32_t hash, uint32_t to);
void hub_tick();
const SwoftyEra *era_table();
// The era boxes the addons registered, in picker order.
std::vector<era::Box> era_boxes();
const SwoftyRegistry *registry_table();
int registry_write_reports();
swofty_core::registry::Store &older_store();
std::string older_addon(const void *from);
// Makes an entrant list count as the box the player pressed, for a session started without the picker (a replay of that box): the
// session's car count follows the pressed box.
void era_press(const char *entrant_list);
void era_session_load();
// Whether the Time Trial session being loaded holds a list no box names (its name in `list`): its laps get no board.
bool era_session_unlisted(std::string &list);
void era_session_trace(const char *state, int to);
void era_session_unloaded(const char *state, int to);
// Puts the career season's box rules back into the career's setup mode (boxes/picker/career_rules.cpp).
void era_career_unloaded();
// Career saves (career_saves.cpp): the career index and per-career addon data.
void career_saves_start();
void career_saves_saving();
void career_saves_saved();
void career_saves_listed(const std::vector<career::saves::SlotRead> &slots, bool complete, bool at_load);
// Save sandbox (save_sandbox.cpp): under `swofty test` with a run token, or with the launcher's saves.
bool save_sandbox_test_mode();
bool save_sandbox_launcher_mode();
bool save_sandbox_armed();
std::string save_sandbox_source();
std::string save_sandbox_file();
std::string career_saves_id_for_slot(const std::string &key);
std::string career_saves_content_for_slot(const std::string &key);
std::string career_saves_slot_key(uint64_t slot_id, bool my_team);
std::vector<career::saves::SlotRead> career_listed_slots();
// Saves that reached the save device: FileWrite calls the Steam proxy served, or on the EA copy the profile saves
// written in place (save_sandbox_logic.hpp SaveSettle). save_sandbox_writes_what() names them for a log line.
uint32_t save_sandbox_writes();
const char *save_sandbox_writes_what();
// The test run `swofty test` started the game for (SWOFTY_TEST_RUN, SWOFTY_TEST_TOKEN and run.json, read
// once by save_sandbox.cpp, which ends the game when they disagree); null outside a test run.
struct TestRun {
    std::wstring dir;
    std::string id, token;
    bool sandbox = false;
    int agent_v = 0;
    std::vector<std::string> dlls;
};
const TestRun *test_run();
// run.json still holds the token: false once `swofty test` has retired the run.
bool test_run_live();
// W4 career snapshots (native/src/core/career/snapshots/runtime.cpp): started once the save splice settles.
void snapshots_start(bool splice_live);
// career::report_season from caller (an addon id): SWOFTY_ASSET_OK, _LATE, _INVALID or SWOFTY_REFUSED.
int snapshots_report_season(const std::string &caller, int year, int season_index);
// The loaded career's id as the career snapshots know it (empty with none loaded); safe from any thread.
std::string snapshots_career_now();

// Dropped scores (standings_drop.cpp): while the loaded career is filed under data_group, the drivers' championship counts
// each driver's best N results; 0 counts every result.
int standings_drop_set(const std::string &data_group, uint32_t best);
// The addon that registered the career content of data_group (and its save container name), or empty.
std::string career_content_owner(const std::string &data_group, std::string *save_name = nullptr);
// The save slot a career load selects (site_Game_SaveSlot_Selected), the career id committed when the
// career tracker sees the career loaded (<mode>:<slot guid>, else the tracker's key), and that id for a
// tracker key.
void snapshots_slot_selected(uintptr_t manager, uint64_t slot_id, uint64_t slot_guid);
std::string snapshots_career_loaded(const char *key, unsigned mode);
std::string snapshots_career_id(const char *key);
void myteam_vehicle_heal_entrants(const myteam_heal::Load &load);
bool myteam_keep_forced(const std::string &label);
bool myteam_keep_substitute(SwoftyContext *c, uint32_t stat, const std::string &label);
void watchdog_flow(uint32_t hash, uint32_t to, const char *name);
void hub_calendar_flow(uint32_t hash, uint32_t to);
void hub_calendar_team_hq();
void hub_calendar_added(void *calendar, const void *activity, uint32_t day);
void load_stall_flow(uint32_t hash, uint32_t to, const char *name);
void session_job_probe_flow(uint32_t to, const char *name);
void load_stall_screen(const char *name);
void load_stall_signal(uint32_t trigger, uint64_t detail_ms);
void load_stall_game_line(const char *line);
void load_stall_setup();
// The session loader entry probes behind the load-stall report's session loader section (session_loader.cpp).
void session_loader_setup();
// Finds the route record behind the load-stall report's front end boot route section (boot_route.cpp).
void boot_route_setup();
void tyre_models_tick();
void tyre_models_drop();
void garage_placement_reset();
bool garage_placement_listening();
bool save_heap_raised(uint64_t &bytes);
std::string track_select_time_trial();
uint8_t *team_record_of(uint16_t stat);
void team_record_inserted(std::function<void(uint16_t, uint8_t *)> fn);
void quali_tyres_box(uint8_t mask, const std::string &line);
void hud_setup();
void hud_arm();
void hud_hook_window(HWND wnd);
void focus_setup();
void hud_fatal();
void grade_setup();
void srv_clamp_setup();
void rtas_guard_setup();
void postfx_setup();
void telemetry_setup();
void telemetry_tick();
void telemetry_session_enter();
bool telemetry_wants_udp();
void telemetry_udp(const uint8_t *buf, size_t len);
struct LiveEvent {
    const char *name;
    int64_t a, b;
};
void live_setup();
bool live_on();
void live_tick(double now);
// Speech events (core/audio/speech.cpp): whether the core fires speech.line and its follow-ups, and the flow tick.
bool speech_live();
void speech_tick();
bool live_memory_verified();
bool live_collision_needs_udp();
bool live_events_from_memory();
bool live_bus_armed();
uint32_t live_fill_feed(SwoftyTelemetryFeed &f);
bool live_verify();
const SwoftyTelemetryFeed &live_memory_view();
std::vector<LiveEvent> live_take_events();
void placed_record_setup();
void placed_record_crash_text(char *out, size_t cap);
// The session host (services/session_host.cpp): the core's reports to the session plugin's listeners. Each does
// nothing while no plugin listens; session_host_message answers true when the plugin took the message, and
// session_host_cover_signal -1 (failed) when nothing answers for the cover.
void session_host_ready();
// The tile list entries the core takes out (home_tiles.cpp).
void local_tiles_setup();
void session_host_flow(uint32_t hash, uint32_t to);
void session_host_screen(const char *name);
bool session_host_message(const char *id, void *reply, void *ctx);
void session_host_home(void *vm);
void session_host_exit();
int session_host_cover_signal();
void net_trace_setup();
void udp_ids_setup();
// Notes an added track's statId for the Swofty Track telemetry packet (udp_ids.cpp).
void udp_track(int64_t stat_id, const std::string &addon, const std::string &code);
// The test agent (native/src/core/testing/agent/agent.cpp): armed only by `swofty test`'s run token.
void testagent_setup();
void direct_start_setup();
void direct_start_tick();
void direct_start_profile_loaded(bool ok);
void direct_start_profile_read();
// The external target's setup (the session host registers a plugin's): polled on the flow thread after the profile has
// loaded, while StateFTUE is held, until it answers done or failed.
using DirectStartSetup = runtime::direct_start::Setup (*)(std::string &why);
void direct_start_register_room(DirectStartSetup setup);
// The Time Trial target's setup (a session request with "mode": "time_trial"; direct_start_tt.cpp registers it):
// polled the same way, with the request's text from direct_start_request_text().
void direct_start_register_time_trial(DirectStartSetup setup);
// Every screen the UI opens while a direct start is armed (ui.cpp's open hook), for the Time Trial's own steps.
void direct_start_register_time_trial_screen(void (*screen)(const char *name));
// The flow graph's root state (StateApp) once the flow has updated it, or null.
void *direct_start_flow_root();
// A listener on the flow machine's link site: it is called for every state whose update just returned, with the link the state returned
// (0 = stay), and may replace it. One listener; the hook is installed on the first call. False when the site did not resolve.
using FlowLinkListener = void (*)(uint8_t *state, uint32_t *link);
bool flow_link_listen(FlowLinkListener listener);
// Despawns the front end's main scene (the menu backdrop) once; a direct start never runs StateFrontend's exit.
void direct_start_despawn_frontend_scene(const char *when);
std::string direct_start_request_text();
// For the cover: 0 while a session target is on its way, 1 once its session is on screen, -1 when it failed.
int direct_start_cover_signal();
// A UI screen opened (ui.cpp): the session's own screen takes the cover off.
void direct_start_screen(const char *name);
uintptr_t direct_start_set_mode();
// What the direct start knows on the flow thread from the profile load on: the game manager and [game manager+0xc0],
// an object with the shape of a game modes context. A scan of all 104 Game_Game_SetCurrentGameMode call sites found no
// caller passing [[0x14604c2c8]+0xc0].
struct DirectStartContext {
    void *game_manager = nullptr, *game_modes = nullptr;
};
DirectStartContext direct_start_context();
std::string code_where(uintptr_t a);
void testagent_tick();
void testagent_fired(const std::string &id, bool hot);
void testagent_screen(const char *name);
void testagent_flow(const std::string &name, uint32_t to);
void testagent_home_vm(void *vm);
void testagent_route_ran(const std::string &name);
bool frame_capture_request(const std::wstring &path);
int frame_capture_poll(std::string *why);
int group_add(const json::Value &entry);
int group_extend(const std::string &group, const std::string &erp);
void own_item(uint32_t key, const std::string &addon, const std::string &what);
int db_add(const json::Value &entry);
void db_older_edits(const std::function<void(uint64_t key, int tag, const std::string &text)> &fn);
int lang_add(const json::Value &entry);
int file_add(const json::Value &entry);
std::string file_route_prefix();
bool file_route(const std::string &game_path, const std::string *target, const std::string &source, std::string &why);
bool file_route_target(const std::string &game_path, std::string &target);

}
