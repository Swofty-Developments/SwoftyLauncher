// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the new 1990 Driver Career wizard: Recommended or Custom settings, then Choose Driver
// with the official 1990 drivers. Its own DLL, so a binding it can't resolve drops only these hooks;
// option career_wizard_1990 turns them off. Each hook acts only on a wizard whose content data
// group (wizard state +0x128, copied from the Career tile) is f1_career_1990; the 2025 and 2026
// wizards keep every step.
//
// - Game_CareerWizard_StartsInF1 answers yes: no Skip to F1 / Start in F2 step, no team-icons step
//   (My Team / Konnersport), and the driver select takes the F1 path.
// - The Career tile router (FeTile_Route_CareerItem) fires career_wizard_driver_career_1990 instead
//   of career_wizard_driver_career_to_f1. erp/wizard.json adds that route to the content select's
//   grouping: it opens the settings step, so there is no Full Season / Custom Season choice and the
//   career runs the full 1990 calendar.
// - Game_CareerWizard_PushNextStep: after the settings, the classic drivers step (14, Add Driver
//   Icons To Market) becomes the confirm step (6).
// - At site_Game_CareerWizardConfirm_NextRoute the confirm step's route driver_career_driver_type_select
//   (Official Driver / Custom Driver) becomes driver_career_driver_select, the route the game takes
//   for a team-icon career: Choose Driver opens directly.
// - The settings step's continue applies season length preset 0 (Game_SeasonLength_ApplyPreset on the
//   game mode's season length component), as the skipped season length step's Full Season does, so the
//   career gets the 1990 calendar (all three 1990 presets hold its 16 rounds).
// - Back on Choose Driver (site_Game_DriverSelect_BackRoute) fires screen_group_back_1990 instead of
//   screen_group_back and drops the confirm step and the settings choice from the wizard state: the
//   route (erp/wizard.json) closes the driver select screen group and opens the settings step again.
// - Back on the settings step (site_Game_CareerWizard_BackRoute, the wizard base handler, after its own
//   pop) fires back_1990, a copy of the settings step's back to the home screen that returns to the
//   Career tiles (fe_career) like the season length step's back does.
// - The 1990 calendar: a career runs its rounds from the season length component's calendar, each
//   dated by its event's own date_time (the stock 2026 weekend), which GameModeChampionship::begin
//   reads after the component's vt+0xd0 (slot 26). For a 1990 career (data group f1_career_1990, or
//   setup mode f1_1990) whose season has no started weekend, at each 1990 wizard step, at career load,
//   before and after slot 26 and before slot 77, the DLL refills the calendar from the career's
//   F1_1990 dynamic event list, moves each round to its 1990 day in the career year and sets its
//   minute offset from the season start (data/calendar-1990.json), so the season runs Phoenix first.


#include <atomic>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#include "calendar_1990.hpp"
#include "career_1990.hpp"

SWOFTY_SETTING(bool, career_wizard_1990, true);

#ifdef SWOFTY_API_CAREER_WIZARD
#include <swofty/career_wizard.hpp>
#define F1990_CORE_WIZARD 1
#else
#define F1990_CORE_WIZARD 0
#endif

namespace {

using f1990::at;
using f1990::event_ids;
using f1990::Events;
using f1990::kCalendarEntry;
using f1990::kTrackList1990;
using swofty::game::Action;
using swofty::game::HomeItem;
using swofty::game::NeStr;

constexpr char kSettingsRoute[] = "career_wizard_driver_career_1990";
#if !F1990_CORE_WIZARD
constexpr char kToF1Route[] = "career_wizard_driver_career_to_f1";
constexpr char kDriverTypeRoute[] = "driver_career_driver_type_select";
constexpr char kDriverSelectRoute[] = "driver_career_driver_select";
#endif
constexpr char kDriverSelectBack[] = "screen_group_back";
constexpr char kDriverSelectBack1990[] = "screen_group_back_1990";
constexpr char kBack[] = "back";
constexpr char kSettingsBack1990[] = "back_1990";
constexpr uint8_t kStepClassicDrivers = 14;
constexpr uint8_t kStepConfirm = 6;
constexpr uint8_t kOptionFullSeason = 9;
constexpr uint8_t kOptionCustomSeason = 10;
constexpr uint8_t kOptionRecommended = 0xb;
constexpr uint8_t kOptionCustomSettings = 0xc;
constexpr uint8_t kPresetFullSeason = 0;
constexpr uint8_t kSetupMode1990 = 16;
constexpr char kDatesFile[] = "data/calendar-1990.json";

std::atomic<uint32_t> g_step_logged, g_season_logged, g_back_logged, g_calendar_logged, g_other_logged, g_loaded_logged,
    g_started_logged, g_begin_logged;
#if !F1990_CORE_WIZARD
std::atomic<uint32_t> g_starts_logged, g_route_logged, g_confirm_logged;
#endif

bool is_1990_group(const char *group)
{
    return group && !std::strcmp(group, f1990::kCareerGroup1990);
}

bool is_1990_wizard(const void *wizard)
{
    return wizard && is_1990_group(at<const char *>(wizard, 0x128));
}

#if !F1990_CORE_WIZARD
const char *text(const NeStr *s)
{
    return s ? s->chars : "";
}
#endif

size_t steps(const void *wizard)
{
    auto *b = at<const uint8_t *>(wizard, 0xe0), *e = at<const uint8_t *>(wizard, 0xe8);
    return b && e > b ? size_t(e - b) : 0;
}

uint8_t last_option(const void *wizard)
{
    auto *b = at<const uint8_t *>(wizard, 0x100), *e = at<const uint8_t *>(wizard, 0x108);
    return b && e > b ? e[-1] : 0xff;
}

bool has_option(const void *wizard, uint8_t option)
{
    auto *b = at<const uint8_t *>(wizard, 0x100), *e = at<const uint8_t *>(wizard, 0x108);
    for (; b && b < e; b++)
        if (*b == option)
            return true;
    return false;
}

void drop_last(uint8_t *wizard, size_t end)
{
    uint8_t *last = at<uint8_t *>(wizard, end) - 1;
    std::memcpy(wizard + end, &last, sizeof last);
}

void *career_of(const void *wizard)
{
    auto *ctx = at<const uint8_t *>(wizard, 0x28);
    auto *holder = ctx ? at<const uint8_t *>(ctx, 0x88) : nullptr;
    return holder ? at<void *>(holder, 0x260) : nullptr;
}

void *season_component(void *career)
{
    return career ? f1990::registry_get(static_cast<uint8_t *>(career) + 0x1030,
                                        swofty::game::k_Game_TrackListComponentDesc.address())
                  : nullptr;
}

void refill(void *component, const Events &list)
{
    auto *c = static_cast<uint8_t *>(component);
    auto *b = at<uint8_t *>(c, 0x2b0), *e = at<uint8_t *>(c, 0x2b8);
    for (auto *p = b; p && p < e; p += kCalendarEntry)
        reinterpret_cast<void (*)(void *, int)>((*reinterpret_cast<void ***>(p))[0])(p, 0);
    std::memcpy(c + 0x2b8, &b, sizeof b);
    for (auto *p = list.begin; p < list.end; p += kCalendarEntry)
        swofty::game::Game_SeasonLength_PushCalendarEntry(c + 0x2a8, p);
    uint32_t n = uint32_t(list.size());
    std::memcpy(c + 0x2c8, &n, sizeof n);
}

bool is_1990_season(const void *career, uint8_t &setup_mode)
{
    auto *mode = at<const uint8_t *>(career, 0x2b0);
    setup_mode = mode ? mode[0x80] : 0xff;
    return f1990::is_1990_career(career) || setup_mode == kSetupMode1990;
}

struct RoundDates {
    std::vector<f1990::EventShift> shifts;
    int64_t season_start = 0;
    bool has_start = false;
};

const RoundDates &round_dates()
{
    static std::once_flag once;
    static RoundDates dates;
    std::call_once(once, [] {
        std::string json;
        size_t error_at = 0;
        std::vector<f1990::StringLeaf> leaves;
        if (auto r = swofty::storage::read(kDatesFile, json); r != swofty::storage::Result::ok)
            swofty::log("[f1-1990] calendar: read %s: %s; the 1990 rounds keep the events' stock dates", kDatesFile,
                        swofty::storage::name(r));
        else if (!f1990::parse_string_leaves(json, leaves, &error_at))
            swofty::log("[f1-1990] calendar: %s is not an object of strings (byte %zu)", kDatesFile, error_at);
        else {
            dates.shifts = f1990::read_shifts(leaves);
            dates.has_start = f1990::read_season_start(leaves, dates.season_start);
        }
        swofty::log("[f1-1990] calendar: %zu 1990 round date(s) from %s, season start %s", dates.shifts.size(), kDatesFile,
                    dates.has_start ? f1990::civil_day(dates.season_start).c_str() : "missing");
    });
    return dates;
}

std::string season_weekends(const void *career, size_t &count, size_t &started);

bool force_1990_calendar(void *component, const char *when, bool wizard_1990)
{
    void *career = component ? at<void *>(component, 0x278) : nullptr;
    uint8_t setup_mode = 0xff;
    if (!career_wizard_1990 || !career || (!is_1990_season(career, setup_mode) && !wizard_1990))
        return false;
    size_t weekends = 0, started = 0;
    season_weekends(career, weekends, started);
    if (started) {
        if (g_started_logged++ < 4)
            swofty::log("[f1-1990] calendar: 1990 career at %s has %zu of %zu weekend(s) started; calendar left as it is",
                        when, started, weekends);
        return true;
    }
    const char *key = at<const char *>(component, 0x298);
    auto *data = at<const uint8_t *>(component, 0x290);
    const char *fallback = data ? at<const char *>(data, 0xf8) : nullptr;
    Events before = f1990::calendar_events(component), list = f1990::track_list_events(career, kTrackList1990);
    bool same = f1990::same_events(before, list);
    std::string before_ids = event_ids(before);
    if (!same && list.size())
        refill(component, list);
    auto *c = static_cast<uint8_t *>(component);
    const RoundDates &dates = round_dates();
    size_t moved = f1990::shift_dates(at<uint8_t *>(c, 0x2b0), at<uint8_t *>(c, 0x2b8), dates.shifts);
    size_t offsets = dates.has_start ? f1990::set_offsets(at<uint8_t *>(c, 0x2b0), at<uint8_t *>(c, 0x2b8), dates.shifts,
                                                          dates.season_start)
                                     : 0;
    if (g_calendar_logged++ < 32)
        swofty::log("[f1-1990] calendar: 1990 career at %s (setup mode %u, default track list \"%s\", %zu weekend(s)); "
                    "calendar %zu event(s) [%s]; %s has %zu event(s); %s; %zu date(s) moved to the 1990 days, %zu offset(s) "
                    "set [%s]",
                    when, setup_mode, key && *key ? key : fallback ? fallback : "(null)", weekends, before.size(),
                    before_ids.c_str(), kTrackList1990, list.size(),
                    !list.size() ? "no F1_1990 list in the dynamic event data"
                    : same       ? "calendar kept"
                                 : "calendar refilled from F1_1990",
                    moved, offsets, f1990::event_days(f1990::calendar_events(component)).c_str());
    return true;
}

std::string season_weekends(const void *career, size_t &count, size_t &started)
{
    auto *champ = at<const uint8_t *>(career, 0x1058);
    auto *sb = champ ? at<const uint8_t *>(champ, 0x548) : nullptr, *se = champ ? at<const uint8_t *>(champ, 0x550) : nullptr;
    uint32_t index = champ ? at<uint32_t>(champ, 0x560) : 0;
    auto *season = sb && se > sb && index < size_t(se - sb) / 0x10 ? at<const uint8_t *>(sb + index * 0x10, 8) : nullptr;
    auto *wb = season ? at<const uint8_t *const *>(season, 0x288) : nullptr;
    auto *we = season ? at<const uint8_t *const *>(season, 0x290) : nullptr;
    count = wb && we > wb ? size_t(we - wb) : 0;
    started = 0;
    std::string out;
    for (auto *w = wb; w && w < we; w++)
        started += *w && at<uint32_t>(*w, 0x23c) != 0;
    for (auto *w = wb; w && w < we && out.size() < 600; w++) {
        const char *name = *w ? at<const char *>(*w, 0x28) : nullptr;
        out += out.empty() ? "" : ",";
        out += name ? name : "?";
    }
    return out;
}

void on_career(const SwoftyEvent *ev, void *)
{
    if (!career_wizard_1990 || std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED))
        return;
    void *career = swofty::career::entity();
    uint8_t setup_mode = 0xff;
    if (!career || !is_1990_season(career, setup_mode) || g_loaded_logged++ >= 8)
        return;
    size_t count = 0, started = 0;
    std::string names = season_weekends(career, count, started);
    void *component = season_component(career);
    swofty::log("[f1-1990] calendar: 1990 career loaded; season has %zu weekend(s) (%zu started) [%s]; calendar %zu event(s) "
                "[%s]",
                count, started, names.c_str(), component ? f1990::calendar_events(component).size() : 0,
                component ? event_ids(f1990::calendar_events(component)).c_str() : "");
    if (component)
        force_1990_calendar(component, "career load", false);
}

void apply_full_season(void *wizard)
{
    void *career = career_of(wizard);
    void *season = season_component(career);
    if (!season) {
        if (g_season_logged++ < 4)
            swofty::log("[f1-1990] wizard: no season length component on the career (%p); calendar left as it is", career);
        return;
    }
    swofty::game::Game_SeasonLength_ApplyPreset(season, kPresetFullSeason);
    if (g_season_logged++ < 4)
        swofty::log("[f1-1990] wizard: season length preset %u (Full Season) applied to the 1990 career", kPresetFullSeason);
    force_1990_calendar(season, "the settings step", true);
}

}

#if !F1990_CORE_WIZARD
SWOFTY_WRAP(Game_CareerWizard_StartsInF1, bool, void *entrants, void *wizard_data, const char *entrant_list)
{
    bool stock = orig(entrants, wizard_data, entrant_list);
    if (!career_wizard_1990 || !is_1990_group(entrant_list))
        return stock;
    if (g_starts_logged++ < 8)
        swofty::log("[f1-1990] wizard: %s starts in F1, no F2 choice and no team-icons step (game said %s)",
                    entrant_list, stock ? "yes" : "no");
    return true;
}

SWOFTY_WRAP(FeTile_Route_CareerItem, Action *, HomeItem *item, Action *out)
{
    Action *r = orig(item, out);
    auto *group = item ? at<const NeStr *>(item, 0x180) : nullptr;
    if (!career_wizard_1990 || !out || !group || !is_1990_group(group->chars))
        return r;
    if (out->type != 1 || std::strcmp(text(out->str), kToF1Route)) {
        if (g_route_logged++ < 4)
            swofty::log("[f1-1990] wizard: 1990 tile route \"%s\" (type %d) left as it is", text(out->str), out->type);
        return r;
    }
    swofty::game::Ne_Make_StrFromCStr(&out->str, kSettingsRoute, 0, 0);
    if (g_route_logged++ < 4)
        swofty::log("[f1-1990] wizard: 1990 tile opens the settings step (%s -> %s)", kToF1Route, text(out->str));
    return r;
}
#endif

SWOFTY_WRAP(Game_CareerWizard_PushNextStep, void, void *wizard)
{
    size_t before = wizard ? steps(wizard) : 0;
    if (career_wizard_1990 && is_1990_wizard(wizard)) {
        uint8_t option = last_option(wizard);
        if ((option == kOptionRecommended || option == kOptionCustomSettings) && !has_option(wizard, kOptionFullSeason) &&
            !has_option(wizard, kOptionCustomSeason))
            apply_full_season(wizard);
        else if (void *component = season_component(career_of(wizard)))
            force_1990_calendar(component, "a wizard step", true);
    }
    orig(wizard);
    if (F1990_CORE_WIZARD || !career_wizard_1990 || !is_1990_wizard(wizard) || steps(wizard) != before + 1)
        return;
    auto *last = at<uint8_t *>(wizard, 0xe8) - 1;
    if (*last != kStepClassicDrivers)
        return;
    *last = kStepConfirm;
    if (g_step_logged++ < 4)
        swofty::log("[f1-1990] wizard: classic drivers step (%u) replaced by the confirm step (%u)", kStepClassicDrivers,
                    kStepConfirm);
}

#if !F1990_CORE_WIZARD
SWOFTY_SITE(site_Game_CareerWizardConfirm_NextRoute, swofty::Context &c)
{
    auto *route = reinterpret_cast<const char *>(c.rdx);
    auto *vm = reinterpret_cast<const uint8_t *>(c.rsi);
    if (!career_wizard_1990 || !route || !vm || std::strcmp(route, kDriverTypeRoute) ||
        !is_1990_wizard(at<const void *>(vm, 0x270)))
        return;
    c.rdx = reinterpret_cast<uintptr_t>(kDriverSelectRoute);
    if (g_confirm_logged++ < 4)
        swofty::log("[f1-1990] wizard: confirm goes to %s, no driver type step", kDriverSelectRoute);
}
#endif

SWOFTY_SITE(site_Game_DriverSelect_BackRoute, swofty::Context &c)
{
    auto *route = reinterpret_cast<const char *>(c.rdx);
    auto *vm = reinterpret_cast<const uint8_t *>(c.rsi);
    if (!career_wizard_1990 || !route || !vm || std::strcmp(route, kDriverSelectBack))
        return;
    auto *wizard = at<uint8_t *>(vm, 0x438);
    if (!is_1990_wizard(wizard))
        return;
    bool step = steps(wizard) && at<uint8_t *>(wizard, 0xe8)[-1] == kStepConfirm;
    if (step)
        drop_last(wizard, 0xe8);
    uint8_t option = last_option(wizard);
    bool settings = option == kOptionRecommended || option == kOptionCustomSettings;
    if (settings)
        drop_last(wizard, 0x108);
    c.rdx = reinterpret_cast<uintptr_t>(kDriverSelectBack1990);
    if (g_back_logged++ < 8)
        swofty::log("[f1-1990] wizard: Choose Driver back goes to the settings step (%s; confirm step %s, settings "
                    "choice %s)",
                    kDriverSelectBack1990, step ? "dropped" : "not on top", settings ? "dropped" : "not last");
}

SWOFTY_SITE(site_Game_CareerWizard_BackRoute, swofty::Context &c)
{
    auto *route = reinterpret_cast<const char *>(c.rdx);
    auto *vm = reinterpret_cast<const uint8_t *>(c.rsi);
    if (!career_wizard_1990 || !route || !vm || std::strcmp(route, kBack) ||
        at<uintptr_t>(vm, 0) != swofty::game::vt_Game_CareerWizardSettingsSelectViewModel.address() ||
        !is_1990_wizard(at<const void *>(vm, 0x270)))
        return;
    c.rdx = reinterpret_cast<uintptr_t>(kSettingsBack1990);
    if (g_back_logged++ < 8)
        swofty::log("[f1-1990] wizard: settings back goes to the Career tiles (%s)", kSettingsBack1990);
}

SWOFTY_VTABLE(vt_Game_TrackListComponent, 26, uintptr_t, void *component, uintptr_t a, uintptr_t b, uintptr_t c)
{
    bool mine = force_1990_calendar(component, "career begin", false);
    uintptr_t r = orig(component, a, b, c);
    if (!mine)
        return r;
    void *career = at<void *>(component, 0x278);
    Events list = f1990::track_list_events(career, kTrackList1990);
    bool same = f1990::same_events(f1990::calendar_events(component), list);
    if (g_begin_logged++ < 8)
        swofty::log("[f1-1990] calendar: career begin left the calendar %s: %zu event(s) [%s]",
                    same ? "as F1_1990" : "changed", f1990::calendar_events(component).size(),
                    event_ids(f1990::calendar_events(component)).c_str());
    if (!same)
        force_1990_calendar(component, "after career begin", false);
    return r;
}

SWOFTY_VTABLE(vt_Game_TrackListComponent, 77, void, void *component, uintptr_t index, uintptr_t keep)
{
    bool mine = force_1990_calendar(component, "season begin", false);
    if (!mine && career_wizard_1990 && component && g_other_logged++ < 4) {
        void *career = at<void *>(component, 0x278);
        uint8_t setup_mode = 0xff;
        is_1990_season(career, setup_mode);
        auto seen = f1990::career_data_group_seen(career, {f1990::setup_component, f1990::group_index});
        swofty::log("[f1-1990] calendar: season begin of a non-1990 career (setup mode %u, data group %s); calendar %zu "
                    "event(s), left as the game builds it",
                    setup_mode, seen.name ? seen.name : "?", f1990::calendar_events(component).size());
    }
    orig(component, index, keep);
    if (mine && g_calendar_logged++ < 12)
        swofty::log("[f1-1990] calendar: season built: %zu event(s) [%s]", f1990::calendar_events(component).size(),
                    f1990::event_days(f1990::calendar_events(component)).c_str());
}

#if F1990_CORE_WIZARD
namespace {

bool register_wizard()
{
    namespace wz = swofty::career::wizard;
    const char *group = f1990::kCareerGroup1990;
    wz::Result r[4] = {wz::start_in_f1(group), wz::tile_route(group, kSettingsRoute), wz::skip_driver_icons(group),
                       wz::skip_driver_type(group)};
    swofty::log("[f1-1990] career wizard: 1990 wizard steps registered with the core's wizard library (start in F1 %s, "
                "tile route %s, driver icons %s, driver type %s)",
                wz::name(r[0]), wz::name(r[1]), wz::name(r[2]), wz::name(r[3]));
    return true;
}

}

SWOFTY_ON_LOAD
{
    uint32_t token = 0;
    if (swofty::events::subscribe(SWOFTY_EVENT_CAREER_LOADED, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] calendar: subscribe %s failed; no schedule line on career load", SWOFTY_EVENT_CAREER_LOADED);
    if (!career_wizard_1990)
        swofty::career::wizard::clear(f1990::kCareerGroup1990);
    swofty::log("[f1-1990] career wizard loaded: 1990 wizard steps %s", career_wizard_1990 ? "on" : "off (career_wizard_1990)");
}

SWOFTY_ADDON(register_wizard)
#else
SWOFTY_ON_LOAD
{
    uint32_t token = 0;
    if (swofty::events::subscribe(SWOFTY_EVENT_CAREER_LOADED, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] calendar: subscribe %s failed; no schedule line on career load", SWOFTY_EVENT_CAREER_LOADED);
    swofty::log("[f1-1990] career wizard loaded: 1990 wizard steps %s", career_wizard_1990 ? "on" : "off (career_wizard_1990)");
}

SWOFTY_ADDON()
#endif
