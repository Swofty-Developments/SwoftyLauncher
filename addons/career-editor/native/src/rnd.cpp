// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: R&D (protocol/rnd.md). Both modes, every team.
//
// Layouts are KB structs (re/db/structs, swofty::game): UpgradesComponent (the per-team
// Upgrades models and its CareerTimeComponent sibling; the game's day is
// current_minutes / 1440), Upgrades (mode, team key, trees, owner cache), UpgradeTree,
// UpgradeInstance (gamedb copy plus progress) and UpgradeStateEntry (the state history;
// the last entry is the current state). Every pointer is range-checked before a struct
// is read through it.
//
// Every edit is one of the game's own transitions:
//   Career_Start_UpgradeResearch (U-level research start; end day == start day is the
//     game's instant branch, never Failed, no RP: the component start debits RP
//     before it calls this)
//   Career_Start_UpgradeDevelopment (U-level development start, same shape; its
//     instant branch reaches PendingFitting only in the research -> develop -> fit
//     lifecycle, g_Career_UpgradeLifecycle == 2, the only lifecycle with a development
//     phase at all)
//   Career_Cancel_UpgradeResearch / Career_Cancel_UpgradeDevelopment (U-level cancels,
//     no refund), used to restart an upgrade already in progress with a zero length
//   site_Career_RnDPurchaseRequestEntry (the R&D screen's purchase request handler):
//     fit, unfit and cancel-with-refund, exactly as the buttons do
//   Car_Reset_LastFittedUpgrade (the per-instance reset of the game's reset-all)
//   Career_Set_UpgradeFacilityLocked, Career_Refresh_UpgradeAvailability,
//   Career_Post_UpgradeFittedRatio
// The game functions refresh availability themselves in Upgrades modes 1/2; in mode 0
// they only invalidate the owner cache, so after every change the op runs the pass the
// game's next R&D query would run, Career_Set_UpgradesOwner (availability and facility
// locks recomputed for that owner).
//
// Department 4 (Tyres) upgrades move only when swofty.rnd-tyres-thermal admits them in
// the mutators' department gates; the read-back reports dept_unsupported otherwise.

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#include <swofty/game.hpp>

#include "ce.hpp"
#include "components.hpp"

using namespace swofty::game;

namespace {

constexpr uint32_t kMinutesPerDay = 1440;
constexpr size_t kMaxTeams = 64;
constexpr size_t kMaxInstances = 0x180;

constexpr uint32_t kOwnerNone = 0x3fff0000;
constexpr uint8_t kTeamRow = 0xff;
constexpr uint8_t kDeptTyres = 4;
constexpr uint16_t kNoDay = 0xffff;
constexpr uint8_t kAdaptNeeded = 1;
constexpr uint8_t kAdapting = 3;

enum State : uint8_t {
    Unavailable = 0,
    Available = 1,
    InProgress = 2,
    Completed = 3,
    Failed = 4,
    Invalid = 5,
    PendingDevelopment = 6,
    InDevelopment = 7,
    PendingFitting = 8,
};

const char *state_name(unsigned s)
{
    static const char *const k[] = {"Unavailable", "Available",          "InProgress",    "Completed",     "Failed",
                                    "Invalid",     "PendingDevelopment", "InDevelopment", "PendingFitting"};
    return s < 9 ? k[s] : "?";
}

const char *dept_name(unsigned d)
{
    static const char *const k[] = {"aerodynamics", "chassis", "durability", "engine", "fake"};
    return d < 5 ? k[d] : "other";
}

using PurchaseRequestFn = uint64_t(void *comp, uint32_t owner, void *inst, uint32_t option, uint8_t arg5,
                                   uint8_t cancel);

alignas(16) uint8_t g_rng[0xa00];

const UpgradeInstance *inst_of(uintptr_t u) { return reinterpret_cast<const UpgradeInstance *>(u); }

Upgrades *model_of(uintptr_t up) { return reinterpret_cast<Upgrades *>(up); }

struct Inst {
    uintptr_t at = 0;
    uint32_t hash = 0;
    uint8_t dept = 0;
    uint8_t tree = 0;
    std::string name;
};

struct Team {
    uintptr_t up = 0;
    uint32_t key = 0;
    uint8_t mode = 0;
    uint64_t trees = 0;
    uint32_t owner = kOwnerNone;
    bool player = false;
};

struct Ctx {
    uintptr_t comp = 0;
    uint16_t day = 0;
    std::vector<Team> teams;
    uint32_t player_team = kOwnerNone;
    bool have_player = false;
};

bool entries(uintptr_t inst, const UpgradeStateEntry *&b, const UpgradeStateEntry *&e)
{
    b = inst_of(inst)->states_begin;
    e = inst_of(inst)->states_end;
    if (b == e)
        return true;
    if (!b || e < b || static_cast<size_t>(e - b) > 0x1000)
        return false;
    return ce::comp::readable(reinterpret_cast<uintptr_t>(b), static_cast<size_t>(e - b) * sizeof(UpgradeStateEntry));
}

struct Last {
    bool ok = false;
    uint32_t owner = kOwnerNone;
    uint16_t day = kNoDay;
    uint8_t state = Invalid;
    uint8_t fitted = 0;
    bool enabled = false;
    bool locked = false;
    size_t count = 0;
    bool dev_history = false;
};

Last last_of(uintptr_t inst)
{
    Last l;
    const UpgradeStateEntry *b = nullptr, *e = nullptr;
    if (!entries(inst, b, e))
        return l;
    l.ok = true;
    l.count = static_cast<size_t>(e - b);
    if (!l.count)
        return l;
    const UpgradeStateEntry &x = e[-1];
    l.owner = x.owner;
    l.day = x.day;
    l.state = x.state;
    l.fitted = x.adaptation;
    l.enabled = x.enabled != 0;
    l.locked = x.facility_locked != 0;
    for (const UpgradeStateEntry *p = b; p < e; ++p)
        if (p->state == PendingDevelopment || p->state == InDevelopment || p->state == PendingFitting)
            l.dev_history = true;
    return l;
}

std::string inst_name(uintptr_t inst)
{
    uintptr_t s = reinterpret_cast<uintptr_t>(inst_of(inst)->name);
    if (!s)
        return {};
    const char *p = reinterpret_cast<const char *>(s);
    if (s >= inst && s + 64 <= inst + sizeof(UpgradeInstance))
        return std::string(p, strnlen(p, 64));
    if (ce::comp::readable(s, 64))
        return std::string(p, strnlen(p, 64));
    std::string out;
    for (size_t i = 0; i < 64; ++i) {
        char c = 0;
        if (!ce::comp::read(s + i, c) || !c)
            break;
        out.push_back(c);
    }
    return out;
}

bool listed(uintptr_t inst)
{
    uint8_t dept = inst_of(inst)->department;
    if (dept < kDeptTyres)
        return true;
    if (dept != kDeptTyres)
        return false;
    Last l = last_of(inst);
    return l.ok && l.count && l.enabled && l.state != Invalid;
}

bool owner_valid(uint32_t o) { return (o & 0xffff0000u) != kOwnerNone; }

bool same_key(uint32_t a, uint32_t b) { return (a & 0xffff00ffu) == (b & 0xffff00ffu); }

bool load(Ctx &c)
{
    c.comp = ce::comp::get(ce::comp::Id::Upgrades);
    if (!c.comp || !ce::comp::readable(c.comp, sizeof(UpgradesComponent)))
        return false;
    const auto *comp = reinterpret_cast<const UpgradesComponent *>(c.comp);
    uint32_t minutes = 0;
    if (!comp->career_time || !ce::comp::read(reinterpret_cast<uintptr_t>(&comp->career_time->current_minutes), minutes))
        return false;
    c.day = static_cast<uint16_t>(minutes / kMinutesPerDay);
    Upgrades *const *ub = comp->upgrades_begin;
    Upgrades *const *ue = comp->upgrades_end;
    if (!ub || ue < ub || static_cast<size_t>(ue - ub) > kMaxTeams ||
        !ce::comp::readable(reinterpret_cast<uintptr_t>(ub), static_cast<size_t>(ue - ub) * sizeof(Upgrades *)))
        return false;
    std::vector<ce::comp::Entrant> es = ce::comp::entrants();
    ce::comp::Entrant me{};
    c.have_player = ce::comp::player(me);
    if (c.have_player)
        c.player_team = me.team;
    bool player_found = false;
    for (Upgrades *const *p = ub; p < ue; ++p) {
        Team t;
        t.up = reinterpret_cast<uintptr_t>(*p);
        if (!t.up || !ce::comp::readable(t.up, sizeof(Upgrades)))
            continue;
        t.key = model_of(t.up)->team_key;
        t.mode = model_of(t.up)->mode;
        t.trees = model_of(t.up)->tree_count;
        if (t.trees == 0 || t.trees > 2)
            continue;
        t.player = c.have_player && same_key(t.key, c.player_team);
        if (t.player) {
            c.player_team = t.key;
            player_found = true;
        }
        if (t.player)
            t.owner = me.driver;
        else
            for (auto &x : es)
                if (same_key(x.team, t.key)) {
                    t.owner = x.driver;
                    break;
                }
        c.teams.push_back(t);
    }
    c.have_player = c.have_player && player_found;
    return true;
}

template <class F> void each_inst(const Team &t, F &&f)
{
    for (uint64_t k = 0; k < t.trees; ++k) {
        const UpgradeTree &tree = model_of(t.up)->trees[k];
        const UpgradeInstance *b = tree.instances_begin;
        const UpgradeInstance *e = tree.instances_end;
        if (!b || e < b || static_cast<size_t>(e - b) > kMaxInstances)
            continue;
        if (!ce::comp::readable(reinterpret_cast<uintptr_t>(b), static_cast<size_t>(e - b) * sizeof(UpgradeInstance)))
            continue;
        for (const UpgradeInstance *u = b; u < e; ++u)
            if (listed(reinterpret_cast<uintptr_t>(u)))
                f(reinterpret_cast<uintptr_t>(u), static_cast<uint8_t>(k));
    }
}

const Team *find_team(const Ctx &c, const ce::Json &args)
{
    const ce::Json &v = args["team"];
    uint32_t want = 0;
    if (v.type == ce::Json::Null || (v.type == ce::Json::Str && v.s == "player")) {
        if (!c.have_player)
            return nullptr;
        want = c.player_team;
    } else if (v.type == ce::Json::Num || v.type == ce::Json::Str) {
        long long n = v.num(-1);
        if (n < 0 || n > 0xffffffffll)
            return nullptr;
        want = static_cast<uint32_t>(n);
    } else {
        return nullptr;
    }
    for (auto &t : c.teams)
        if (t.key == want)
            return &t;
    return nullptr;
}

enum class Pick { Ok, BadArgs, NoUpgrade };

Pick pick(const Team &t, const ce::Json &args, std::vector<Inst> &out)
{
    const ce::Json &up = args["upgrade"];
    const ce::Json &nm = args["name"];
    const ce::Json &dp = args["dept"];
    bool all = args["all"].type == ce::Json::Bool && args["all"].b;
    long long hash = -1, dept = -2;
    std::string name;
    if (up.type == ce::Json::Num || up.type == ce::Json::Str) {
        hash = up.num(-1);
        if (hash < 0 || hash > 0xffffffffll)
            return Pick::BadArgs;
    }
    if (nm.type == ce::Json::Str)
        name = nm.s;
    if (dp.type == ce::Json::Num) {
        dept = dp.num(-2);
        if (dept < -1 || dept > kDeptTyres)
            return Pick::BadArgs;
    }
    if (hash < 0 && name.empty() && !(all && dept >= -1))
        return Pick::BadArgs;
    each_inst(t, [&](uintptr_t u, uint8_t k) {
        Inst i;
        i.at = u;
        i.hash = inst_of(u)->node_hash;
        i.dept = inst_of(u)->department;
        i.tree = k;
        if (hash >= 0 && i.hash != static_cast<uint32_t>(hash))
            return;
        if (hash < 0 && !name.empty()) {
            if (inst_name(u) != name)
                return;
        } else if (hash < 0 && dept >= 0 && i.dept != dept) {
            return;
        }
        i.name = inst_name(u);
        out.push_back(i);
    });
    return out.empty() ? Pick::NoUpgrade : Pick::Ok;
}

uint32_t owner_for(const Team &t, uintptr_t inst)
{
    Last l = last_of(inst);
    if (l.count && owner_valid(l.owner))
        return l.owner;
    return t.owner;
}

bool develop_lifecycle() { return *g_Career_UpgradeLifecycle == 2; }

void refresh_team(const Team &t, uint16_t day)
{
    uint8_t mode = model_of(t.up)->mode;
    if (mode == 1 || mode == 2) {
        Career_Refresh_UpgradeAvailability(reinterpret_cast<void *>(t.up), day);
    } else if (mode == 0) {
        if (owner_valid(t.owner)) {
            Career_Set_UpgradesOwner(reinterpret_cast<void *>(t.up), t.owner);
        } else {
            model_of(t.up)->owner_cache = kOwnerNone;
        }
    }
}

void after_change(const Team &t, uint16_t day)
{
    if (model_of(t.up)->mode == 0)
        refresh_team(t, day);
}

void start_research(const Team &t, uintptr_t inst, uint16_t day)
{
    std::memset(g_rng, 0, sizeof g_rng);
    Career_Start_UpgradeResearch(reinterpret_cast<void *>(t.up), owner_for(t, inst), reinterpret_cast<void *>(inst),
                                 day, day, 0, 0, g_rng, 1);
}

void start_develop(const Team &t, uintptr_t inst, uint16_t day)
{
    std::memset(g_rng, 0, sizeof g_rng);
    Career_Start_UpgradeDevelopment(reinterpret_cast<void *>(t.up), owner_for(t, inst), reinterpret_cast<void *>(inst),
                                    day, day, 0, 0, 0, g_rng, 1);
}

uint64_t purchase_request(const Ctx &c, const Team &t, uintptr_t inst, bool cancel)
{
    auto *f = reinterpret_cast<PurchaseRequestFn *>(site_Career_RnDPurchaseRequestEntry.address());
    return f(reinterpret_cast<void *>(c.comp), owner_for(t, inst), reinterpret_cast<void *>(inst), 0, 0,
             cancel ? 1 : 0);
}

const char *develop_now(const Team &t, uintptr_t inst, uint16_t day)
{
    start_develop(t, inst, day);
    if (last_of(inst).state != InDevelopment)
        return nullptr;
    Career_Cancel_UpgradeDevelopment(reinterpret_cast<void *>(t.up), owner_for(t, inst), reinterpret_cast<void *>(inst),
                                     day);
    return "game_refused";
}

enum class Stage { Research, Develop, Full };

bool done_for(uint8_t s, Stage st, bool fit)
{
    if (!develop_lifecycle())
        return s == Completed;
    switch (st) {
    case Stage::Research:
        return s == Completed || s == PendingDevelopment || s == InDevelopment || s == PendingFitting;
    case Stage::Develop:
        return s == Completed || s == PendingFitting;
    case Stage::Full:
        return s == Completed || (!fit && s == PendingFitting);
    }
    return true;
}

const char *step(const Ctx &c, const Team &t, uintptr_t inst, Stage st, bool fit)
{
    Last l = last_of(inst);
    if (!l.ok || !l.count)
        return "no_state";
    if (done_for(l.state, st, fit))
        return nullptr;
    void *U = reinterpret_cast<void *>(t.up);
    void *I = reinterpret_cast<void *>(inst);
    switch (l.state) {
    case Unavailable:
        return l.locked ? "facility_locked" : "prerequisites";
    case Invalid:
        return "invalid";
    case Available:
        start_research(t, inst, c.day);
        return nullptr;
    case InProgress:
        Career_Cancel_UpgradeResearch(U, owner_for(t, inst), I, c.day);
        if (last_of(inst).state == InProgress)
            return "game_refused";
        start_research(t, inst, c.day);
        return nullptr;
    case Failed:
        if (develop_lifecycle() && l.dev_history)
            return develop_now(t, inst, c.day);
        start_research(t, inst, c.day);
        return nullptr;
    case PendingDevelopment:
        if (!develop_lifecycle())
            return "lifecycle";
        return develop_now(t, inst, c.day);
    case InDevelopment:
        if (!develop_lifecycle())
            return "lifecycle";
        Career_Cancel_UpgradeDevelopment(U, owner_for(t, inst), I, c.day);
        if (last_of(inst).state == InDevelopment)
            return "game_refused";
        return develop_now(t, inst, c.day);
    case PendingFitting:
        if (l.fitted == kAdaptNeeded || l.fitted == kAdapting)
            return "adaptation_pending";
        if (!purchase_request(c, t, inst, false))
            return "game_refused";
        return nullptr;
    default:
        return "unknown_state";
    }
}

std::string hex32(uint32_t v)
{
    char b[16];
    std::snprintf(b, sizeof b, "0x%08x", v);
    return b;
}

void write_list(ce::Writer &w, const Team &t, const std::vector<Inst> &is, const std::vector<std::string> *why)
{
    w.begin_object();
    w.key("team");
    w.integer(t.key);
    w.key("upgrades");
    w.begin_array();
    for (size_t n = 0; n < is.size(); ++n) {
        Last l = last_of(is[n].at);
        w.begin_object();
        w.key("hash");
        w.integer(is[n].hash);
        w.key("name");
        w.str(is[n].name);
        w.key("tree");
        w.integer(is[n].tree);
        w.key("state");
        w.integer(l.count ? l.state : Invalid);
        w.key("state_name");
        w.str(state_name(l.count ? l.state : Invalid));
        w.key("facility_locked");
        w.boolean(l.locked);
        if (why && n < why->size() && !(*why)[n].empty()) {
            w.key("reason");
            w.str((*why)[n]);
        }
        w.end_object();
    }
    w.end_array();
    w.end_object();
}

std::string list_json(const Team &t, const std::vector<Inst> &is, const std::vector<std::string> *why = nullptr)
{
    ce::Writer w;
    write_list(w, t, is, why);
    return w.text();
}

ce::OpResult fail(const char *code, const std::string &after = {})
{
    ce::OpResult r;
    r.error = code;
    r.after = after;
    return r;
}

struct Prep {
    Ctx c;
    const Team *t = nullptr;
    std::vector<Inst> is;
};

const char *prepare(const ce::Json &args, Prep &p, bool need_pick = true)
{
    if (!load(p.c))
        return "not_ready";
    p.t = find_team(p.c, args);
    if (!p.t)
        return args["team"].type == ce::Json::Null || args["team"].str() == "player" ? "no_team" : "bad_args";
    if (!need_pick)
        return nullptr;
    switch (pick(*p.t, args, p.is)) {
    case Pick::BadArgs:
        return "bad_args";
    case Pick::NoUpgrade:
        return "no_upgrade";
    case Pick::Ok:
        break;
    }
    return nullptr;
}

void log_line(const char *op, const Team &t, const Inst &i, uint8_t from, uint8_t to, const char *note)
{
    ce::log("rnd.%s team=%s \"%s\" %s dept=%s %s -> %s%s%s", op, hex32(t.key).c_str(), i.name.c_str(),
            hex32(i.hash).c_str(), dept_name(i.dept), state_name(from), state_name(to), note ? " " : "",
            note ? note : "");
}

ce::OpResult op_complete(const ce::Json &args)
{
    Prep p;
    if (const char *e = prepare(args, p))
        return fail(e);
    std::string stage = args["stage"].str("full");
    Stage st = stage == "research" ? Stage::Research : stage == "develop" ? Stage::Develop : Stage::Full;
    if (stage != "research" && stage != "develop" && stage != "full")
        return fail("bad_args");
    bool fit = !(args["fit"].type == ce::Json::Bool && !args["fit"].b);
    bool all = args["all"].type == ce::Json::Bool && args["all"].b;
    std::vector<uint8_t> from(p.is.size());
    for (size_t n = 0; n < p.is.size(); ++n)
        from[n] = last_of(p.is[n].at).state;
    std::string before = list_json(*p.t, p.is);
    std::vector<std::string> why(p.is.size());
    int passes = all ? 32 : 1;
    for (int pass = 0; pass < passes; ++pass) {
        bool moved = false;
        for (size_t n = 0; n < p.is.size(); ++n) {
            for (int k = 0; k < 6; ++k) {
                Last a = last_of(p.is[n].at);
                if (!a.count || done_for(a.state, st, fit))
                    break;
                const char *r = step(p.c, *p.t, p.is[n].at, st, fit);
                Last b = last_of(p.is[n].at);
                if (r) {
                    why[n] = r;
                    break;
                }
                if (b.state == a.state && b.count == a.count) {
                    why[n] = p.is[n].dept == kDeptTyres ? "dept_unsupported" : "game_refused";
                    break;
                }
                why[n].clear();
                moved = true;
            }
        }
        if (moved)
            after_change(*p.t, p.c.day);
        if (!moved)
            break;
    }
    size_t changed = 0;
    for (size_t n = 0; n < p.is.size(); ++n) {
        uint8_t to = last_of(p.is[n].at).state;
        if (to != from[n]) {
            ++changed;
            log_line("complete", *p.t, p.is[n], from[n], to, nullptr);
        } else if (!all && !why[n].empty()) {
            log_line("complete", *p.t, p.is[n], from[n], to, why[n].c_str());
        }
    }
    ce::OpResult r;
    r.before = before;
    r.after = list_json(*p.t, p.is, &why);
    r.ok = changed > 0 || (!all && done_for(from[0], st, fit));
    if (!r.ok) {
        const std::string &w0 = why[0];
        bool game = !all && (w0 == "game_refused" || w0 == "dept_unsupported");
        r.error = game ? w0 : "wrong_state";
    }
    ce::log("rnd.complete team=%s stage=%s fit=%d all=%d: %zu of %zu upgrades moved",
            hex32(p.t->key).c_str(), stage.c_str(), fit ? 1 : 0, all ? 1 : 0, changed, p.is.size());
    return r;
}

ce::OpResult op_unlock(const ce::Json &args)
{
    Prep p;
    if (const char *e = prepare(args, p))
        return fail(e);
    std::vector<uint8_t> from(p.is.size());
    std::vector<bool> was_locked(p.is.size());
    for (size_t n = 0; n < p.is.size(); ++n) {
        Last l = last_of(p.is[n].at);
        from[n] = l.state;
        was_locked[n] = l.locked;
    }
    std::string before = list_json(*p.t, p.is);
    size_t cleared = 0;
    for (size_t n = 0; n < p.is.size(); ++n)
        if (was_locked[n]) {
            Career_Set_UpgradeFacilityLocked(reinterpret_cast<void *>(p.is[n].at), 0, p.c.day);
            ++cleared;
        }
    if (cleared)
        refresh_team(*p.t, p.c.day);
    std::vector<std::string> why(p.is.size());
    size_t ok = 0;
    for (size_t n = 0; n < p.is.size(); ++n) {
        Last l = last_of(p.is[n].at);
        if (l.locked)
            why[n] = p.is[n].dept == kDeptTyres ? "dept_unsupported" : p.t->mode == 0 ? "derived_lock" : "game_refused";
        else if (l.state == Unavailable)
            why[n] = "prerequisites";
        if (!l.locked)
            ++ok;
        log_line("unlock", *p.t, p.is[n], from[n], l.state,
                 was_locked[n] ? (l.locked ? "lock kept" : "lock cleared") : "was not locked");
    }
    ce::OpResult r;
    r.before = before;
    r.after = list_json(*p.t, p.is, &why);
    r.ok = ok == p.is.size();
    if (!r.ok)
        r.error = p.is.size() == 1 && p.is[0].dept == kDeptTyres ? "dept_unsupported"
                  : p.t->mode == 0                               ? "derived_lock"
                                                                 : "game_refused";
    return r;
}

ce::OpResult op_reset(const ce::Json &args)
{
    Prep p;
    if (const char *e = prepare(args, p))
        return fail(e);
    std::vector<uint8_t> from(p.is.size());
    for (size_t n = 0; n < p.is.size(); ++n)
        from[n] = last_of(p.is[n].at).state;
    std::string before = list_json(*p.t, p.is);
    std::vector<std::string> why(p.is.size());
    bool any_fitted = false;
    for (size_t n = 0; n < p.is.size(); ++n) {
        Last l = last_of(p.is[n].at);
        if (!l.ok)
            continue;
        if (p.is[n].dept >= kDeptTyres) {
            why[n] = "dept_unsupported";
            continue;
        }
        Car_Reset_LastFittedUpgrade(reinterpret_cast<void *>(p.is[n].at));
        if (l.state == Completed || l.fitted)
            any_fitted = true;
    }
    refresh_team(*p.t, p.c.day);
    if (any_fitted)
        for (size_t n = 0; n < p.is.size(); ++n)
            if (from[n] == Completed) {
                Career_Post_UpgradeFittedRatio(reinterpret_cast<void *>(p.t->up), reinterpret_cast<void *>(p.is[n].at), 0);
                break;
            }
    size_t ok = 0;
    for (size_t n = 0; n < p.is.size(); ++n) {
        Last l = last_of(p.is[n].at);
        if (why[n].empty() && l.count && l.state <= Available)
            ++ok;
        log_line("reset", *p.t, p.is[n], from[n], l.state, why[n].empty() ? nullptr : why[n].c_str());
    }
    ce::OpResult r;
    r.before = before;
    r.after = list_json(*p.t, p.is, &why);
    r.ok = ok > 0;
    if (!r.ok)
        r.error = "game_refused";
    return r;
}

ce::OpResult op_refund(const ce::Json &args)
{
    Prep p;
    if (const char *e = prepare(args, p))
        return fail(e);
    if (p.is.size() != 1)
        return fail("bad_args");
    Inst &i = p.is[0];
    Last a = last_of(i.at);
    std::string before = list_json(*p.t, p.is);
    if (a.state != InProgress && a.state != InDevelopment)
        return fail("wrong_state", before);
    uint64_t rc = purchase_request(p.c, *p.t, i.at, true);
    after_change(*p.t, p.c.day);
    Last b = last_of(i.at);
    log_line("refund", *p.t, i, a.state, b.state, rc ? "cancelled with refund" : "handler refused");
    ce::OpResult r;
    r.before = before;
    ce::Writer w;
    w.begin_object();
    w.key("team");
    w.integer(p.t->key);
    w.key("rp_refunded");
    w.boolean(rc != 0 && b.state != a.state);
    w.key("upgrades");
    w.begin_array();
    w.begin_object();
    w.key("hash");
    w.integer(i.hash);
    w.key("name");
    w.str(i.name);
    w.key("state");
    w.integer(b.state);
    w.key("state_name");
    w.str(state_name(b.state));
    w.end_object();
    w.end_array();
    w.end_object();
    r.after = w.text();
    r.ok = rc != 0 && b.state != a.state;
    if (!r.ok)
        r.error = "game_refused";
    return r;
}

ce::OpResult op_fit(const ce::Json &args)
{
    Prep p;
    if (const char *e = prepare(args, p))
        return fail(e);
    if (p.is.size() != 1)
        return fail("bad_args");
    bool want = !(args["fitted"].type == ce::Json::Bool && !args["fitted"].b);
    Inst &i = p.is[0];
    Last a = last_of(i.at);
    std::string before = list_json(*p.t, p.is);
    if (want ? a.state != PendingFitting : (a.state != Completed || !develop_lifecycle()))
        return fail("wrong_state", before);
    if (a.fitted == kAdaptNeeded || a.fitted == kAdapting)
        return fail("wrong_state", before);
    uint64_t rc = purchase_request(p.c, *p.t, i.at, false);
    after_change(*p.t, p.c.day);
    Last b = last_of(i.at);
    log_line("fit", *p.t, i, a.state, b.state, want ? "fit" : "unfit");
    ce::OpResult r;
    r.before = before;
    r.after = list_json(*p.t, p.is);
    r.ok = rc != 0 && b.state == (want ? Completed : PendingFitting);
    if (!r.ok)
        r.error = i.dept == kDeptTyres ? "dept_unsupported" : "game_refused";
    return r;
}

struct Counts {
    unsigned total = 0, unavailable = 0, available = 0, researching = 0, researched = 0, developing = 0,
             pending_fitting = 0, fitted = 0, failed = 0, locked = 0;
    void add(const Last &l)
    {
        ++total;
        if (l.locked)
            ++locked;
        switch (l.state) {
        case Unavailable: ++unavailable; break;
        case Available: ++available; break;
        case InProgress: ++researching; break;
        case PendingDevelopment: ++researched; break;
        case InDevelopment: ++developing; break;
        case PendingFitting: ++pending_fitting; break;
        case Completed: ++fitted; break;
        case Failed: ++failed; break;
        default: break;
        }
    }
};

void write_counts(ce::Writer &w, const Counts &c)
{
    w.key("counts");
    w.begin_object();
    w.key("total");
    w.integer(c.total);
    w.key("unavailable");
    w.integer(c.unavailable);
    w.key("available");
    w.integer(c.available);
    w.key("researching");
    w.integer(c.researching);
    w.key("researched");
    w.integer(c.researched);
    w.key("developing");
    w.integer(c.developing);
    w.key("pending_fitting");
    w.integer(c.pending_fitting);
    w.key("fitted");
    w.integer(c.fitted);
    w.key("failed");
    w.integer(c.failed);
    w.key("locked");
    w.integer(c.locked);
    w.end_object();
}

void write_depts(ce::Writer &w, const Counts (&per)[5])
{
    w.key("depts");
    w.begin_array();
    for (unsigned d = 0; d < 5; ++d) {
        if (!per[d].total)
            continue;
        w.begin_object();
        w.key("d");
        w.integer(d);
        w.key("t");
        w.integer(per[d].total);
        w.key("f");
        w.integer(per[d].fitted);
        w.key("p");
        w.integer(per[d].researching + per[d].developing);
        w.key("a");
        w.integer(per[d].available);
        w.key("l");
        w.integer(per[d].locked);
        w.end_object();
    }
    w.end_array();
}

const char *blocked_reason(const Last &l)
{
    if (l.fitted == kAdaptNeeded || l.fitted == kAdapting)
        return "adaptation_pending";
    if (l.state == Unavailable)
        return l.locked ? "facility_locked" : "prerequisites";
    return nullptr;
}

void write_upgrade(ce::Writer &w, uintptr_t u, uint8_t k, const Last &l)
{
    w.begin_object();
    w.key("h");
    w.integer(inst_of(u)->node_hash);
    w.key("n");
    w.str(inst_name(u));
    w.key("d");
    w.integer(inst_of(u)->department);
    w.key("s");
    w.integer(l.state);
    w.key("c");
    w.integer(inst_of(u)->cost);
    w.key("t");
    w.integer(inst_of(u)->num_days_to_research);
    int32_t dc = inst_of(u)->cost_to_develop, dt = inst_of(u)->num_days_to_develop;
    if (dc) {
        w.key("dc");
        w.integer(dc);
    }
    if (dt) {
        w.key("dt");
        w.integer(dt);
    }
    uint16_t b = kNoDay, e = kNoDay;
    if (l.state == InProgress) {
        b = inst_of(u)->research_start_day;
        e = inst_of(u)->research_end_day;
    } else if (l.state == InDevelopment) {
        b = inst_of(u)->develop_start_day;
        e = inst_of(u)->develop_end_day;
    }
    if (b != kNoDay) {
        w.key("b");
        w.integer(b);
    }
    if (e != kNoDay) {
        w.key("e");
        w.integer(e);
    }
    if (l.locked) {
        w.key("l");
        w.integer(1);
    }
    if (l.fitted) {
        w.key("ad");
        w.integer(l.fitted);
    }
    if (const char *r = blocked_reason(l)) {
        w.key("r");
        w.str(r);
    }
    int32_t lv = inst_of(u)->unlock_at_facility_level;
    if (lv) {
        w.key("ul");
        w.integer(lv);
    }
    if (inst_of(u)->row_flag != kTeamRow) {
        w.key("sp");
        w.integer(1);
    }
    if (k) {
        w.key("tr");
        w.integer(k);
    }
    w.end_object();
}

void tally(const Team &t, Counts &all, Counts (&per)[5])
{
    each_inst(t, [&](uintptr_t u, uint8_t) {
        Last l = last_of(u);
        if (!l.count)
            return;
        all.add(l);
        uint8_t dept = inst_of(u)->department;
        if (dept < 5)
            per[dept].add(l);
    });
}

void write_team_name(ce::Writer &w, uint32_t stat_id)
{
    const std::string *n = ce::cached_team_name(stat_id);
    if (n)
        w.str(*n);
    else
        w.null();
}

void write_team_detail(ce::Writer &w, const Team &t)
{
    Counts all;
    Counts per[5];
    tally(t, all, per);
    w.begin_object();
    w.key("team");
    w.integer(t.key);
    w.key("team_stat_id");
    w.integer(t.key >> 16);
    w.key("team_name");
    write_team_name(w, t.key >> 16);
    w.key("player");
    w.boolean(t.player);
    w.key("mode");
    w.integer(t.mode);
    w.key("owner");
    w.integer(t.owner);
    write_counts(w, all);
    write_depts(w, per);
    w.key("u");
    w.begin_array();
    each_inst(t, [&](uintptr_t u, uint8_t k) {
        Last l = last_of(u);
        if (l.count)
            write_upgrade(w, u, k, l);
    });
    w.end_array();
    w.end_object();
}

void section_rnd(ce::Writer &w)
{
    w.begin_object();
    Ctx c;
    if (!ce::in_career() || !load(c)) {
        w.key("ready");
        w.boolean(false);
        w.key("reason");
        w.str(ce::in_career() ? "R&D component not found" : "no career loaded");
        w.end_object();
        return;
    }
    w.key("ready");
    w.boolean(true);
    w.key("day");
    w.integer(c.day);
    w.key("player_team");
    if (c.have_player)
        w.integer(c.player_team);
    else
        w.null();
    w.key("lifecycle");
    w.str(develop_lifecycle() ? "research_develop_fit" : "research_only");
    w.key("player");
    const Team *mine = nullptr;
    for (auto &t : c.teams)
        if (t.player)
            mine = &t;
    if (mine)
        write_team_detail(w, *mine);
    else
        w.null();
    w.key("teams");
    w.begin_array();
    for (auto &t : c.teams) {
        Counts all;
        Counts per[5];
        tally(t, all, per);
        w.begin_object();
        w.key("team");
        w.integer(t.key);
        w.key("team_stat_id");
        w.integer(t.key >> 16);
        w.key("team_name");
        write_team_name(w, t.key >> 16);
        w.key("player");
        w.boolean(t.player);
        w.key("fitted_ratio");
        w.num(all.total ? static_cast<double>(all.fitted) / all.total : 0.0);
        write_counts(w, all);
        write_depts(w, per);
        w.end_object();
    }
    w.end_array();
    w.end_object();
}

ce::OpResult op_team_detail(const ce::Json &args)
{
    Ctx c;
    if (!load(c))
        return fail("not_ready");
    const Team *t = find_team(c, args);
    if (!t)
        return fail("no_team");
    ce::Writer w;
    write_team_detail(w, *t);
    ce::OpResult r;
    r.ok = true;
    r.after = w.text();
    ce::log("rnd.team_detail team=%s (%zu bytes)", hex32(t->key).c_str(), r.after.size());
    return r;
}

} // namespace

CE_SECTION("rnd", section_rnd);
CE_OP("rnd.complete", op_complete);
CE_OP("rnd.unlock", op_unlock);
CE_OP("rnd.reset", op_reset);
CE_OP("rnd.refund", op_refund);
CE_OP("rnd.fit", op_fit);
CE_OP_ANYTIME("rnd.team_detail", op_team_detail);
