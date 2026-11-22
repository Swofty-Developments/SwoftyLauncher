#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <cstddef>
#include <cstring>
#include <mutex>
#include <set>
#include <string>

#include "core/apply/content/teams.hpp"
#include "core/hooks/game_points.hpp"
#include "core/hooks/hooks.hpp"
#include "core/registry/table.hpp"
#include "core/runtime/hook_errors.hpp"

// The core's four game hook points on the live game (and team.logo.resolve, below): the Probe reads what each payload carries beyond
// the function's arguments, through KB structs (L2), every pointer checked before it is read. A build
// whose game headers lack a struct leaves that field at its default (0, false, "").

namespace swofty::game {
struct ActivityDef;
struct DriverCalendar;
struct ScheduleComponent;
struct ScheduleSeason;
struct TrackDataRecord;
}

namespace swofty_core::hooks {
namespace {

namespace g = swofty::game;
namespace mem = swofty::mem;

struct CoreTag {};
using Points = game_points::Set<CoreTag>;

constexpr size_t MAX_SEASONS = 64;

template <class T> T rd(uintptr_t a, T def = T{})
{
    T v{};
    return a && mem::read(a, v) ? v : def;
}

template <class D = g::ActivityDef> uint8_t activity_type(const void *activity)
{
    if constexpr (requires(D *d) { d->type; })
        return rd<uint8_t>(reinterpret_cast<uintptr_t>(activity) + offsetof(D, type));
    else
        return 0;
}

template <class S = g::ScheduleComponent, class Z = g::ScheduleSeason, class C = g::DriverCalendar>
bool driver_calendar(uintptr_t calendar, uint32_t &driver)
{
    if constexpr (requires(S *s, Z *z, C *c) {
                      s->seasons_begin;
                      s->seasons_end;
                      z->driver_calendars_begin;
                      z->driver_calendars_end;
                      c->driver;
                  }) {
        const SwoftyCareer *career = runtime::career_table();
        auto schedule =
            reinterpret_cast<uintptr_t>(career && career->component ? career->component("ScheduleComponent") : nullptr);
        uintptr_t b = rd<uintptr_t>(schedule + offsetof(S, seasons_begin));
        uintptr_t e = rd<uintptr_t>(schedule + offsetof(S, seasons_end));
        if (!schedule || !b || e <= b || (e - b) % sizeof(Z) || (e - b) / sizeof(Z) > MAX_SEASONS)
            return false;
        for (uintptr_t season = b; season < e; season += sizeof(Z)) {
            uintptr_t db = rd<uintptr_t>(season + offsetof(Z, driver_calendars_begin));
            uintptr_t de = rd<uintptr_t>(season + offsetof(Z, driver_calendars_end));
            if (db && de > db && calendar >= db && calendar < de && (calendar - db) % sizeof(C) == 0)
                return mem::read(calendar + offsetof(C, driver), driver);
        }
        return false;
    } else {
        (void)calendar, (void)driver;
        return false;
    }
}

void probe_add_activity(game_points::ce::AddActivity &a)
{
    a.type = activity_type(a.activity);
    uint32_t driver = 0;
    if (!driver_calendar(reinterpret_cast<uintptr_t>(a.calendar), driver))
        return;
    a.driver = driver;
    SwoftyCareerState s{};
    const SwoftyCareer *career = runtime::career_table();
    if (career && career->state && career->state(&s) == SWOFTY_ASSET_OK && s.has_player && s.humans == 1)
        a.player = ((s.player_driver ^ driver) & 0xffff00ffu) == 0;
}

constexpr uintptr_t CONTRACT_TEAM = 0x00;
constexpr uintptr_t CONTRACT_TYPE = 0x0e;

void probe_contract_sign(game_points::ce::ContractSign &s)
{
    auto c = reinterpret_cast<uintptr_t>(s.contract);
    s.team = rd<uint32_t>(c + CONTRACT_TEAM);
    s.type = rd<uint8_t>(c + CONTRACT_TYPE);
}

// Game_CarSetup_SessionFuelLoad's own first reads: the session source at +0x270 (vtable +0xa8 the
// session, type at +8; +0x98 the weekend, track slot byte at +0x278) and the TRACK_DATA system at
// [+0x278]+0xf58 (records +0x28..+0x30, 16-byte items, the record at +0x08, its name at +0xd0).
constexpr uintptr_t SETUP_SOURCE = 0x270, SETUP_OWNER = 0x278, OWNER_TRACK_DATA = 0xf58;
constexpr uintptr_t VT_SESSION = 0xa8, VT_WEEKEND = 0x98, SESSION_TYPE = 0x08, WEEKEND_TRACK_SLOT = 0x278;
constexpr uintptr_t RECORDS_BEGIN = 0x28, RECORDS_END = 0x30, RECORD_ITEM = 0x10, ITEM_RECORD = 0x08;

using Getter = void *(void *);

uintptr_t call_getter(uintptr_t obj, uintptr_t slot)
{
    uintptr_t vt = rd<uintptr_t>(obj), fn = vt ? rd<uintptr_t>(vt + slot) : 0;
    if (!mem::in_image(vt) || !mem::in_image(fn))
        return 0;
    return reinterpret_cast<uintptr_t>(reinterpret_cast<Getter *>(fn)(reinterpret_cast<void *>(obj)));
}

template <class R = g::TrackDataRecord> std::string fuel_track(void *setup, uint32_t &session_type)
{
    auto s = reinterpret_cast<uintptr_t>(setup);
    uintptr_t source = rd<uintptr_t>(s + SETUP_SOURCE);
    if (!mem::object_like(source))
        return {};
    uintptr_t session = call_getter(source, VT_SESSION);
    session_type = rd<uint32_t>(session ? session + SESSION_TYPE : 0);
    if constexpr (requires(R *r) { r->name; }) {
        uintptr_t weekend = call_getter(source, VT_WEEKEND);
        uintptr_t owner = rd<uintptr_t>(s + SETUP_OWNER);
        uintptr_t system = rd<uintptr_t>(owner ? owner + OWNER_TRACK_DATA : 0);
        uintptr_t b = rd<uintptr_t>(system ? system + RECORDS_BEGIN : 0), e = rd<uintptr_t>(system ? system + RECORDS_END : 0);
        if (!weekend || !b || e <= b || (e - b) % RECORD_ITEM)
            return {};
        uint8_t slot = rd<uint8_t>(weekend + WEEKEND_TRACK_SLOT);
        if (slot >= (e - b) / RECORD_ITEM)
            return {};
        uintptr_t record = rd<uintptr_t>(b + slot * RECORD_ITEM + ITEM_RECORD);
        uintptr_t name = rd<uintptr_t>(record ? record + offsetof(R, name) : 0);
        std::string out;
        if (!name || !mem::c_string(name, out, 64) || out.empty())
            return {};
        return "game/" + out;
    } else {
        return {};
    }
}

// team.logo.resolve: a MidHook on each call to Ne_Format_String that builds a team logo key from the team record's
// label (r8, the %s of "overrides.teams.driver_select_team_logos.%s" and of
// "#overrides.team_logos.driver_career_2024_team_select.%s"). The payload's key is that label, the key the core's
// logo slices are named by (a team.identity's label for its own logos); set_key puts another label in r8, so the game
// formats the other team's logo. Before the handlers run, a teams::set_logo override for the team picks the label of
// the team whose slice holds that team.logo record in the site's array. Installed while the point has subscribers or
// an override exists; not hot (the details panel and the carousel items are built on focus and on open).
struct LogoSite {
    const char *binding;
    swofty::teams::LogoContext context;
    const char *array;
};

constexpr LogoSite LOGO_SITES[] = {
    {"site_Game_DriverSelect_TeamLogoFormat", swofty::teams::LogoContext::driver_select, "teams.driver_select_team_logos"},
    {"site_Game_TeamSelect_TeamLogoFormat", swofty::teams::LogoContext::team_select, "team_logos.driver_career_2024_team_select"},
};
constexpr const char *LOGO_POINT = "team.logo.resolve";
constexpr size_t MAX_LABEL = 128;

Table *logo_table = nullptr;

// The labels put in r8 outlive the format call: each distinct one is kept for the rest of the run.
const char *intern_label(const std::string &label)
{
    static auto *m = new std::mutex;
    static auto *pool = new std::set<std::string>;
    std::lock_guard<std::mutex> g(*m);
    return pool->insert(label).first->c_str();
}

// The registry id of the team a row label belongs to: an addon's team.identity whose label it is, else the stock row.
std::string team_of_label(const std::string &label)
{
    namespace content = swofty_core::apply::content;
    for (auto &id : swofty_core::registry::store().list("team.identity", ""))
        if (id.rfind("game/", 0) != 0 && content::key_of(id) == label)
            return id;
    return "game/" + label;
}

// The label whose slice in `array` holds the logo a teams::set_logo override gave the team, or "".
std::string override_label(const std::string &team, const char *array)
{
    namespace content = swofty_core::apply::content;
    std::lock_guard<std::recursive_mutex> g(content::live_mutex());
    content::Live &l = content::live();
    auto it = l.logo_override.find(team);
    if (it == l.logo_override.end())
        return {};
    for (auto &img : l.manifest)
        if (img.id == it->second && img.array == array && !img.key.empty())
            return img.key;
    return {};
}

struct LogoSlot {
    std::string key;
    swofty::teams::events::LogoResolve *payload;
};

void logo_site(const LogoSite &site, safetyhook::Context &c)
{
    std::string label;
    if (!c.r8 || !mem::c_string(uintptr_t(c.r8), label, MAX_LABEL) || label.empty())
        return;
    std::string team = team_of_label(label);
    std::string from = override_label(team, site.array);
    namespace tev = swofty::teams::events;
    tev::LogoResolve e{};
    LogoSlot slot{from.empty() ? label : from, &e};
    e.size = sizeof e;
    e.payload_version = tev::LogoResolve::version;
    e.context = site.context;
    e.team_id = team.c_str();
    e.livery_name_str = "";
    e.key_str = slot.key.c_str();
    e.set_key_fn = [](void *user, const char *key) -> int32_t {
        if (!key || !*key || std::strlen(key) >= MAX_LABEL)
            return SWOFTY_ASSET_INVALID;
        auto *s = static_cast<LogoSlot *>(user);
        s->key = key;
        s->payload->key_str = s->key.c_str();
        return SWOFTY_ASSET_OK;
    };
    e.user = &slot;
    if (logo_table) {
        int replaced = 0;
        logo_table->fire("", LOGO_POINT, &e, &replaced);
    }
    if (slot.key != label)
        c.r8 = uintptr_t(intern_label(slot.key));
}

void logo_driver_select(safetyhook::Context &c) { logo_site(LOGO_SITES[0], c); }
void logo_team_select(safetyhook::Context &c) { logo_site(LOGO_SITES[1], c); }

class LogoLink final : public GameLink {
public:
    explicit LogoLink(GameOps ops) : ops_(std::move(ops)) {}

    bool install(bool late, const std::string &subscribers) override
    {
        std::lock_guard<std::recursive_mutex> g(lock_);
        if (live_)
            return true;
        constexpr safetyhook::MidHookFn FNS[] = {&logo_driver_select, &logo_team_select};
        int placed = 0;
        for (size_t i = 0; i < 2; i++) {
            const char *b = LOGO_SITES[i].binding;
            uintptr_t at = ops_.resolve ? ops_.resolve(b) : 0;
            if (!at) {
                log(std::string("[hooks] team.logo.resolve: ") + b + " does not resolve on this build; that site stays stock");
                continue;
            }
            // An addon that patches the site already keeps it (as a game point yields to a raw overwrite).
            std::string there = ops_.overwritten_by ? ops_.overwritten_by(b, at) : std::string();
            if (there.empty() && ops_.occupied_by)
                there = ops_.occupied_by(at);
            if (!there.empty()) {
                log(std::string("[hooks] team.logo.resolve: ") + b + " is occupied by " + there + "; that site stays stock");
                continue;
            }
            auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), FNS[i]);
            if (!h) {
                log(std::string("[hooks] team.logo.resolve: the hook on ") + b + " failed: " + runtime::hook_error(h.error()));
                continue;
            }
            hooks_[i] = std::move(*h);
            placed++;
        }
        live_ = placed > 0;
        log("[hooks] team.logo.resolve " + std::string(live_ ? "live" : "unavailable") + " on " + std::to_string(placed) +
            " of 2 logo format sites" + (late ? " (installed late)" : " (window stage)") + ": subscribers " + subscribers);
        return live_;
    }

    void uninstall() override
    {
        std::lock_guard<std::recursive_mutex> g(lock_);
        if (!live_ || keep_)
            return;
        for (auto &h : hooks_)
            h = {};
        live_ = false;
        log("[hooks] team.logo.resolve has no subscriber left; the logo format sites are stock again");
    }

    bool installed() const override
    {
        std::lock_guard<std::recursive_mutex> g(lock_);
        return live_;
    }

    // A teams::set_logo override needs the sites with no subscriber.
    void keep()
    {
        std::lock_guard<std::recursive_mutex> g(lock_);
        keep_ = true;
    }

private:
    void log(const std::string &line)
    {
        if (ops_.log)
            ops_.log(line);
    }

    GameOps ops_;
    mutable std::recursive_mutex lock_;
    safetyhook::MidHook hooks_[2];
    bool live_ = false, keep_ = false;
};

LogoLink *logo_link = nullptr;

}

void declare_game_points(Table &t)
{
    game_points::Probe p;
    p.observe_add = runtime::hub_calendar_added;
    p.add_activity = probe_add_activity;
    p.contract_sign = probe_contract_sign;
    p.fuel_track = [](void *setup, uint32_t &type) { return fuel_track(setup, type); };
    Points::declare(t, game_ops(), std::move(p));
    logo_table = &t;
    logo_link = new LogoLink(game_ops());
    t.declare_game(LOGO_POINT, SWOFTY_POINT_MODIFY, swofty::teams::events::LogoResolve::version, logo_link);
}

void keep_logo_point()
{
    if (!logo_link)
        return;
    logo_link->keep();
    logo_link->install(true, "teams::set_logo");
}

void arm_game_points()
{
    if (runtime::same_build())
        Points::arm();
}

}
