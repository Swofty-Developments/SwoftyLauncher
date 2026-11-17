#include "core/runtime/subsystems.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/frontend/view_state.hpp"
#include "core/runtime/lifecycle.hpp"
#include "core/runtime/runtime.hpp"

void crash_attach(SwoftyApi *a, const std::wstring &dir);
void crash_game_hooks();
void settings_attach(SwoftyApi *a, const std::wstring &dir);

namespace swofty_core::apply::content {
void encode_logos_ahead();
}

namespace runtime {

namespace {

void attach_crash() { crash_attach(Runtime::instance().api(), Runtime::instance().dir()); }
void attach_settings() { settings_attach(Runtime::instance().api(), Runtime::instance().dir()); }

// postfx only observes barriers, and grade replaces some batches, so postfx subscribes first.
constexpr Subsystem SUBSYSTEMS[] = {
    {"crash", Start::attach, attach_crash},
    {"settings", Start::attach, attach_settings},
    {"perf", Start::attach, perf::attach},
    {"team logo slices ahead of the database load", Start::attach, swofty_core::apply::content::encode_logos_ahead},
    {"crash: Ne_ReportMemoryError hook", Start::attach_code, crash_game_hooks},
    {"popup: message system hook", Start::commit_code, popup_setup},
    {"mfd: development switch", Start::commit_code, mfd_setup},
    {"view state hooks", Start::commit_code, view::setup},
    {"assets", Start::ready, assets_setup},
    {"resources", Start::ready, resources_setup},
    {"gamedb", Start::ready, gamedb_setup},
    {"dbread", Start::ready, dbread_setup},
    {"overlay", Start::ready, overlay_setup},
    {"lang", Start::ready, lang_override_setup},
    {"events", Start::ready, events_setup},
    {"hud", Start::ready, hud_setup},
    {"focus", Start::ready, focus_setup},
    {"postfx", Start::ready, postfx_setup},
    {"grade", Start::ready, grade_setup},
    {"srv clamp", Start::ready, srv_clamp_setup},
    {"live", Start::ready, live_setup},
    {"telemetry", Start::ready, telemetry_setup},
    {"placed record", Start::ready, placed_record_setup},
    {"net trace", Start::ready, net_trace_setup},
    {"udp ids", Start::ready, udp_ids_setup},
    {"local tiles", Start::ready, local_tiles_setup},
    {"session host", Start::ready, session_host_ready},
    {"test agent", Start::ready, testagent_setup},
    {"direct start", Start::ready, direct_start_setup},
    {"load stall", Start::ready, load_stall_setup},
    {"perf sampler", Start::ready, perf::start},
};

}

std::span<const Subsystem> subsystems()
{
    return SUBSYSTEMS;
}

}
