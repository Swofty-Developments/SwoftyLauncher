#include "core/apply/content/vehicle_override.hpp"

#include "core/apply/applier.hpp"
#include "core/apply/content/appliers.hpp"
#include "core/apply/content/vehicle_override_logic.hpp"
#include "core/apply/ids.hpp"
#include "core/apply/moments.hpp"
#include "core/apply/scheduler.hpp"
#include "core/career/save_splice_logic.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/hook_set.hpp"
#include "core/runtime/pages.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

// team.vehicle_override (experimental): one package team's car for chosen events, natively, before the car assets
// load. Static design in re/research/session_vehicle_swap.md; nothing here is runtime-confirmed (T333-T335).
//
// The car an entrant drives is resolved again at every session load from the current championship session's own copy
// of the entrant records (weekend +0x220 + current (+0x238) * 0xa0, records at Session +0x20..+0x28, 0x88 each):
// Game_ChampionshipGameMode_BuildSessionSettings (0x1407b7b90, the championship modes' slot 0x80, run by
// Sess_Loader_FillSessionDescriptor) resolves rec+0x5c for each entrant and builds its session participant, whose
// asset groups ChangeResidentAssets streams afterwards. A wrap at that function's start writes the override's vehicle
// ref ((stat id << 16) | kind 0) at +0x5c and 0x3fff0000 (no preset: the vehicle's own livery) at +0x64 of the matching
// records, then runs the game's function unchanged. Only the session copy changes: the career rows and the boot entrant
// list keep the team's car, and the team (+0x58), the results and the standings stay the team's.
//
// At the session load (Applies::session) the core arms the records whose package is the box's (Grand Prix) or the
// career season's, and whose `when` matches the session's track, year and session type. Every fill of the session
// writes the same values (the loader's keep checks fill a temporary descriptor and compare). The writes are put back at
// the session's end, and before every career save (career.saving, fired by the save splice), then written again after
// it (career.saved), so a save never holds an override's vehicle ref. A record is put back only while it still holds
// what the core wrote. Refused in Time Trial (its own fill), online modes and shared sessions (a local car would desync
// the other players), and in a career while the save splice is not armed.
//
// SWOFTY_VEHICLE_OVERRIDE=1 turns it on (off by default: the swap is static-only so far). `[vehicle-override]` lines.

namespace swofty_core::apply::content {

namespace {

namespace vo = vehicle_override;
namespace mem = swofty::mem;
using swofty::json::Value;

constexpr const char *REGISTRY = "team.vehicle_override";
constexpr int LOG_FILLS = 8;

std::mutex mu;
std::vector<vo::Target> armed;   // the overrides of the loaded session
std::vector<vo::Write> kept;     // what the fills of this session wrote
std::string armed_session;       // "<mode> <session type> <track>", for the log lines
bool rewrite_after_save = false; // career.saving put the writes back; career.saved writes them again
int fills_logged = 0;
bool unreadable_logged = false, unarmed_logged = false;
std::atomic<bool> wrap_live{false};
std::atomic<bool> any_records{false};

bool feature_on()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_VEHICLE_OVERRIDE", v, sizeof v);
    return n && n < sizeof v && vo::enabled(v);
}

std::string owner_of(const std::string &id)
{
    auto slash = id.rfind('/');
    return slash == std::string::npos ? std::string() : id.substr(0, slash);
}

// The addon records of the registry, in load order (the owner's place in the store's addon order, then the id).
std::vector<std::string> in_order(registry::Store &st)
{
    std::vector<std::string> ids = st.list(REGISTRY, "");
    std::vector<std::string> order = st.order();
    auto rank = [&](const std::string &id) {
        auto it = std::find(order.begin(), order.end(), owner_of(id));
        return it == order.end() ? order.size() : std::size_t(it - order.begin());
    };
    std::stable_sort(ids.begin(), ids.end(), [&](const std::string &a, const std::string &b) {
        auto ra = rank(a), rb = rank(b);
        return ra != rb ? ra < rb : a < b;
    });
    return ids;
}

// A record's stat id: its own stat_id when it pins one, else the id the boot allocated for it; -1 when it has none.
int64_t stat_id_of(registry::Store &st, const char *reg, const std::string &id)
{
    registry::Record r = st.get(reg, id);
    if (r.status != registry::Status::ok)
        return -1;
    const Value &v = r.value["stat_id"];
    if (v.is_int() && v.as_int() >= 0)
        return v.as_int();
    return ids::store().get(reg, id).id;
}

std::string str_of(const Value &v) { return v.is_string() ? std::string(v.as_string()) : std::string(); }

// One record resolved against the store for this session, or the reason it is not armed.
bool resolve(registry::Store &st, const std::string &id, const Value &rec, std::size_t order, vo::Target &out, std::string &why)
{
    std::string package = str_of(rec["package"]), team = str_of(rec["team"]), vehicle = str_of(rec["vehicle"]);
    registry::Record pkg = st.get("content.package", package);
    if (pkg.status != registry::Status::ok) {
        why = "its package " + package + " is not a content.package record";
        return false;
    }
    const Value *entry = nullptr;
    const Value &teams = pkg.value["teams"];
    for (std::size_t t = 0; t < teams.size(); t++)
        if (str_of(teams[t]["key"]) == team) {
            entry = &teams[t];
            break;
        }
    if (!entry) {
        why = "package " + package + " has no team " + team;
        return false;
    }
    std::string own = str_of((*entry)["vehicle"]);
    int64_t own_stat = own.empty() ? -1 : stat_id_of(st, "vehicle", own);
    if (!vo::valid_stat(own_stat)) {
        why = "team " + team + "'s own car " + (own.empty() ? std::string("(none)") : own) + " has no stat id";
        return false;
    }
    int64_t stat = vehicle.empty() ? -1 : stat_id_of(st, "vehicle", vehicle);
    if (!vo::valid_stat(stat)) {
        why = "vehicle " + (vehicle.empty() ? std::string("(none)") : vehicle) +
              " is not a vehicle registered at boot (no stat id); a ref without a row would drop the entrant";
        return false;
    }
    if (stat == own_stat) {
        why = "vehicle " + vehicle + " is the team's own car";
        return false;
    }
    std::vector<std::string> team_drivers, wanted;
    const Value &seats = (*entry)["drivers"];
    for (std::size_t s = 0; s < seats.size(); s++)
        team_drivers.push_back(str_of(seats[s]["driver"]));
    const Value &drivers = rec["drivers"];
    for (std::size_t d = 0; d < drivers.size(); d++)
        wanted.push_back(str_of(drivers[d]));
    out = vo::Target{};
    if (!vo::seats_of(team_drivers, wanted, out.seats, why))
        return false;
    out.id = id;
    out.order = order;
    out.team_vehicle = own_stat;
    out.vehicle_ref = vo::ref_of(stat);
    out.livery_key = vo::NO_KEY;
    return true;
}

vo::When when_of(const Value &rec)
{
    vo::When w;
    const Value &v = rec["when"];
    w.track = str_of(v["track"]);
    w.year = v["year"].is_int() ? int(v["year"].as_int()) : 0;
    const Value &types = v["session_types"];
    for (std::size_t i = 0; i < types.size(); i++)
        w.session_types.push_back(str_of(types[i]));
    return w;
}

bool put_u32(uintptr_t at, uint32_t v)
{
    if (!runtime::pages::read_write_span(at, sizeof v))
        return false;
    std::memcpy(reinterpret_cast<void *>(at), &v, sizeof v);
    return true;
}

// Puts back every kept write whose record still holds what the core wrote. Caller holds mu.
int put_back_locked(const char *why)
{
    int n = 0, changed = 0;
    for (auto it = kept.rbegin(); it != kept.rend(); ++it) {
        uint32_t v = 0, l = 0;
        if (mem::read(it->at + vo::VEHICLE_REF, v) && mem::read(it->at + vo::CUSTOMISATION, l) && vo::restorable(*it, v, l) &&
            put_u32(it->at + vo::CUSTOMISATION, it->livery_before) && put_u32(it->at + vo::VEHICLE_REF, it->vehicle_before))
            n++;
        else
            changed++;
    }
    std::size_t had = kept.size();
    kept.clear();
    if (had)
        swofty::log("[vehicle-override] %d of %zu write(s) put back (%s)%s", n, had, why,
                    changed ? (", " + std::to_string(changed) + " changed since and left as they are").c_str() : "");
    return n;
}

// Writes the armed overrides into the current session's entrant copy. Caller holds mu.
void write_locked(const char *when)
{
    if (armed.empty())
        return;
    moments::SessionEntrants se;
    if (!moments::session_entrants(se)) {
        if (!unreadable_logged)
            swofty::log("[vehicle-override] %s: the current session's entrant copy does not read; nothing written", when);
        unreadable_logged = true;
        return;
    }
    std::vector<vo::Entrant> list;
    for (uintptr_t at = se.begin; at + vo::RECORD_SIZE <= se.end && list.size() < vo::MAX_ENTRANTS; at += vo::RECORD_SIZE) {
        vo::Entrant e;
        e.at = at;
        if (!mem::read(at + vo::VEHICLE_REF, e.vehicle_ref) || !mem::read(at + vo::CUSTOMISATION, e.customisation) ||
            !mem::read(at + vo::TEAM_POSITION, e.team_position))
            continue;
        list.push_back(e);
    }
    vo::Plan p = vo::plan(armed, list, kept);
    int n = 0;
    for (const vo::Write &w : p.writes) {
        if (!put_u32(w.at + vo::VEHICLE_REF, w.vehicle_after))
            continue;
        if (!put_u32(w.at + vo::CUSTOMISATION, w.livery_after)) {
            put_u32(w.at + vo::VEHICLE_REF, w.vehicle_before);
            continue;
        }
        kept.push_back(w);
        n++;
        if (fills_logged < LOG_FILLS)
            swofty::log("[vehicle-override] %s: entrant at %p: vehicle %s -> %s, livery key %s -> %s", w.id.c_str(),
                        reinterpret_cast<void *>(w.at), vo::hex32(w.vehicle_before).c_str(), vo::hex32(w.vehicle_after).c_str(),
                        vo::hex32(w.livery_before).c_str(), vo::hex32(w.livery_after).c_str());
    }
    if (fills_logged < LOG_FILLS) {
        fills_logged++;
        for (const std::string &l : p.lines)
            swofty::log("[vehicle-override] %s", l.c_str());
        swofty::log("[vehicle-override] %s (session id %u, %zu entrant(s), %s): %d written, %zu already written, %zu armed "
                    "override(s)",
                    when, se.session_id, list.size(), armed_session.c_str(), n, p.kept, armed.size());
    }
}

class Overrides : public Applier {
public:
    const char *registry() const override { return REGISTRY; }
    Applies applies() const override { return Applies::session; }

    void session_load(Store &st, const Context &ctx, Report &report) override
    {
        std::vector<std::string> ids = in_order(st);
        {
            std::lock_guard g(mu);
            put_back_locked("another session loads");
            armed.clear();
            rewrite_after_save = false;
            fills_logged = 0;
            unreadable_logged = unarmed_logged = false;
        }
        any_records = !ids.empty();
        if (ids.empty())
            return;
        std::string n = std::to_string(ids.size()) + " record(s)";
        if (!feature_on()) {
            report.lines.push_back(n + " not applied: experimental, SWOFTY_VEHICLE_OVERRIDE=1 turns it on");
            return;
        }
        if (!wrap_live.load()) {
            report.refused += int(ids.size());
            report.lines.push_back(n + " not applied: the Game_ChampionshipGameMode_BuildSessionSettings wrap is not "
                                       "installed (see the [vehicle-override] line at the window stage)");
            return;
        }
        bool in_career = ctx.mode == "my_team" || ctx.mode == "driver_career";
        std::string blocked = in_career ? ::career::save_splice::entry_blocked() : std::string();
        std::string why = vo::mode_refusal(ctx.mode, moments::room_boot(), blocked);
        if (!why.empty()) {
            report.refused += int(ids.size());
            report.lines.push_back(n + " not applied: " + why);
            return;
        }
        std::vector<std::string> packages = ctx.content;
        if (in_career) {
            CareerBox box = career_box_now(ctx);
            packages = box.package.empty() ? std::vector<std::string>() : scheduler().chain_of("content.package", box.package);
        }
        if (packages.empty()) {
            report.lines.push_back(n + " not applied: no box or career season package is active");
            return;
        }
        vo::Session s{ctx.mode, ctx.session_type, ctx.track, ctx.year, ctx.has_year};
        std::vector<vo::Target> targets;
        for (std::size_t i = 0; i < ids.size(); i++) {
            registry::Record r = st.get(REGISTRY, ids[i]);
            if (r.status != registry::Status::ok)
                continue;
            const Value &rec = r.value;
            std::string package = str_of(rec["package"]);
            if (std::find(packages.begin(), packages.end(), package) == packages.end())
                continue;
            if (!vo::when_matches(when_of(rec), s))
                continue;
            vo::Target t;
            std::string fail;
            if (!resolve(st, ids[i], rec, i, t, fail)) {
                report.refused++;
                report.lines.push_back(ids[i] + ": not armed: " + fail);
                continue;
            }
            if (!str_of(rec["livery"]).empty())
                report.lines.push_back(ids[i] + ": livery " + str_of(rec["livery"]) +
                                       " is not supported in 1.0 (the +0x64 preset key of a livery is not mapped yet); the "
                                       "vehicle's own livery is used");
            std::string seats = t.seats.empty() ? std::string("every seat") : std::to_string(t.seats.size()) + " seat(s)";
            report.lines.push_back(ids[i] + ": armed: team " + str_of(rec["team"]) + " (car stat id " +
                                   std::to_string(t.team_vehicle) + "), " + seats + " -> " + str_of(rec["vehicle"]) +
                                   " (stat id " + std::to_string(vo::stat_of_ref(t.vehicle_ref)) + ")");
            targets.push_back(std::move(t));
        }
        report.applied += int(targets.size());
        std::lock_guard g(mu);
        armed = std::move(targets);
        armed_session = (ctx.mode.empty() ? std::string("unknown") : ctx.mode) + " " +
                        (ctx.session_type.empty() ? std::string("session") : ctx.session_type) + " " +
                        (ctx.track.empty() ? std::string("(no track)") : ctx.track);
    }

    void session_end(Store &, Report &report) override
    {
        std::lock_guard g(mu);
        report.put_back += put_back_locked("the session ended");
        armed.clear();
        rewrite_after_save = false;
    }
};

}

void register_vehicle_override()
{
    static std::once_flag once;
    std::call_once(once, [] { make<Overrides>(); });
}

void vehicle_override_fill()
{
    std::lock_guard g(mu);
    if (armed.empty()) {
        if (any_records.load() && !unarmed_logged && feature_on())
            swofty::log("[vehicle-override] the session settings were built with no override armed (the session load "
                        "moment had not armed one for this session)");
        unarmed_logged = true;
        return;
    }
    write_locked("session settings");
}

void vehicle_override_wrap_live(bool live) { wrap_live = live; }

void vehicle_override_saving()
{
    std::lock_guard g(mu);
    if (kept.empty())
        return;
    put_back_locked("before the career save");
    rewrite_after_save = true;
}

void vehicle_override_saved()
{
    std::lock_guard g(mu);
    if (!rewrite_after_save)
        return;
    rewrite_after_save = false;
    write_locked("after the career save");
}

}

namespace runtime {

namespace {

constexpr const char *MIXIN = "swofty_core.dll (vehicle overrides)";
constexpr const char *SETTINGS_FN = "Game_ChampionshipGameMode_BuildSessionSettings";

using Settings_t = uint64_t(void *mode, uint8_t *settings);

struct OverrideHooks : loader::HookSet<> {
    loader::Original<Settings_t> build_settings;
};

constinit OverrideHooks hooks;
bool wanted = false;

uint64_t settings_entry(void *mode, uint8_t *settings)
{
    swofty_core::apply::content::vehicle_override_fill();
    return hooks.build_settings(mode, settings);
}

}

static std::vector<CoreMixins> vehicle_override_mixins()
{
    std::vector<CoreMixins> out;
    if (hooks.gate.offered() || !code_ready())
        return out;
    hooks.gate.offer();
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_VEHICLE_OVERRIDE", v, sizeof v);
    if (!(n && n < sizeof v && swofty_core::apply::content::vehicle_override::enabled(v))) {
        swofty::log("[vehicle-override] off (experimental): SWOFTY_VEHICLE_OVERRIDE=1 turns team.vehicle_override on");
        return out;
    }
    if (!swofty::api->resolve(SETTINGS_FN)) {
        swofty::log("[vehicle-override] off: this build does not resolve %s", SETTINGS_FN);
        return out;
    }
    wanted = true;
    out.push_back({MIXIN, {{SETTINGS_FN, SWOFTY_MIXIN_WRAP, 100, (void *)&settings_entry, hooks.build_settings.slot()}}});
    return out;
}

static void vehicle_override_commit()
{
    if (!wanted || !hooks.gate.settle())
        return;
    bool live = core_mixins_active(MIXIN);
    swofty_core::apply::content::vehicle_override_wrap_live(live);
    swofty::log("[vehicle-override] team.vehicle_override %s",
                loader::install_state(live,
                                      "armed: a matching record swaps the seat's car in the session's entrant copy at each "
                                      "session load",
                                      "did not install (see the [core] lines); every team keeps its car"));
}

static loader::FunctionSet vehicle_override_core_set{loader::CoreSetId::vehicle_override, vehicle_override_mixins,
                                                     vehicle_override_commit};

}
