#include "core/guards/career/standings_drop_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/guards/lines.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <cstring>
#include <map>
#include <mutex>
#include <string>
#include <vector>

// Dropped scores (re/db/functions.tsv Career_Recompute_SeasonStandings, Career_Compare_StandingsEntries).
//
// The drivers' standings of a season (F1SeasonStandings, embedded in Season at +0x2a8) hold the points each session
// scored (+0x20: rounds of 0x20 bytes {allocator, sessions begin, end, capacity}, sessions of 0x28 bytes {allocator,
// records begin, end, capacity, session id at +0x20}, records {u32 key, i32 points}) and the cumulative table
// (+0x40, the same shape): the session's table is a copy of the previous one plus the session's points, entries
// {u32 key, i32 total}. Career_Recompute_SeasonStandings (0x140b2fa60, a jmp to the .data1 body 0x149618ee0) refreshes
// the points, adds every new session to the table and sorts every table session with Career_Compare_StandingsEntries
// (total, then the countback of finishing positions, then the key). The constructors' standings sum the drivers'
// points, not this table, so they keep counting every result.
//
// After the game's recompute, a data group with a rule has every table entry set to the sum of the best N
// per-round totals of the driver up to that session, taken from the points records, which stay gross, and each
// table session is sorted again with the game's own predicate. The totals are absolute, so a later recompute that
// adds a session on top of a counted table is corrected the same way.
//
// The rule is set by set_dropped_scores(data group, N) and applies while the loaded career is filed under that data
// group, to the seasons of the current championship. The hook is installed by the first set_dropped_scores call with a best above 0, so a launch with no
// such addon never runs through it. SWOFTY_STANDINGS_DROP=0 turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;
namespace sd = runtime::standings_drop;

// The recompute is a protector thunk whose filler differs per build: it is found from a season loop's call
// (0x140ae95a2, rcx = season + 0x2a8).
const runtime::Sig S_RECOMPUTE{"Career_Recompute_SeasonStandings",
                               "E8 ?? ?? ?? ?? 48 89 F9 31 D2 E8 ?? ?? ?? ?? 49 8B 8F 10 05 00 00",
                               0xb2fa60,
                               runtime::Sig::rel32,
                               0,
                               1,
                               5};
const runtime::Sig S_COMPARE{
    "Career_Compare_StandingsEntries", "41 56 56 57 53 48 81 EC 08 01 00 00 41 8B 40 04", 0xb317a0, runtime::Sig::addr, 0, 0, 0};

// The layout above was checked on both 1.0.145 builds (EA 1.0.145: re/research/ea-1-0-145.md). Read by the same
// instruction in a body that is the same on each build (re/tools/sigport.py --witness at the KB RVA): Season +0x2a8 and
// the entry's season at +8 (the season loop 0xae9550), the championship +0x1058 and its seasons +0x548, +0x550
// (0x7b2140), entries {key, total} (the compare 0xb317a0). Used by the same instructions in the protector's relocated
// bodies (each use compared): the rounds +0x20 and +0x40, 0x20-byte rounds and 0x28-byte sessions (the recompute
// 0x9618ee0), the session id +0x20 and the 8-byte records (its callees 0x962eee0 and 0x962fff0).
namespace bp = runtime::build_proof;
constexpr bp::Proof STANDINGS_LAYOUT{"the season standings' layout (+0x2a8, +0x20, +0x40, sessions 0x28 with the id at "
                                     "+0x20; the championship +0x1058, +0x548)",
                                     bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate DROPPED_SCORES{"dropped scores", &STANDINGS_LAYOUT};

using Compare_t = bool(void *ctx, const void *a, const void *b);
using Recompute_t = void(void *standings, char mode);

std::mutex rules_lock;
std::map<std::string, uint32_t> rules;
safetyhook::InlineHook h_recompute;
Compare_t *compare = nullptr;
bool logged_apply = false;

bool is_current_season(uintptr_t standings)
{
    const SwoftyCareer *c = runtime::career_table();
    auto e = c && c->entity ? reinterpret_cast<uintptr_t>(c->entity()) : 0;
    return e && mem::object_like(e) && sd::in_championship(e, standings, guards::read_if_readable, nullptr);
}

uint32_t rule_for_loaded_career()
{
    {
        std::lock_guard<std::mutex> hold(rules_lock);
        if (rules.empty())
            return 0;
    }
    const SwoftyCareerContent *t = runtime::career_content_table();
    char group[128] = {};
    if (!t || !t->data_group || t->data_group(nullptr, group, sizeof group) != SWOFTY_ASSET_OK)
        return 0;
    std::lock_guard<std::mutex> hold(rules_lock);
    auto it = rules.find(group);
    return it == rules.end() ? 0 : it->second;
}

sd::TableEntry *game_entries(uint64_t at, size_t count, void *)
{
    return mem::readable(uintptr_t(at), count * sizeof(sd::TableEntry)) ? reinterpret_cast<sd::TableEntry *>(at) : nullptr;
}

bool game_less(const sd::TableEntry &a, const sd::TableEntry &b, void *ctx) { return compare(ctx, &a, &b); }

void recount(uintptr_t standings, uint32_t best)
{
    std::vector<sd::PointsRound> points;
    if (!sd::read_points(standings, guards::read_if_readable, nullptr, points) || points.empty())
        return;
    uint64_t packed = 0;
    bool have_packed = sd::table_key(standings, guards::read_if_readable, nullptr, packed);
    struct Ctx {
        uintptr_t standings;
        uint64_t *packed;
    } ctx{standings, &packed};
    sd::Recounted done = sd::recount_table(standings, points, best, guards::read_if_readable, nullptr, game_entries,
                                           compare && have_packed ? game_less : nullptr, &ctx);
    if (!logged_apply || done.changed) {
        swofty::log("[career] dropped scores: best %u results count; %zu table session(s) recounted, %zu total(s) changed", best,
                    done.sessions, done.changed);
        logged_apply = true;
    }
}

void recompute_hook(void *standings, char mode)
{
    h_recompute.call<void>(standings, mode);
    uint32_t best = rule_for_loaded_career();
    if (!best || !standings || !is_current_season(reinterpret_cast<uintptr_t>(standings)))
        return;
    recount(reinterpret_cast<uintptr_t>(standings), best);
}

class DropGuard final : public guards::OnDemandGuard {
protected:
    bool install() override
    {
        char v[16];
        if (guards::env_off("SWOFTY_STANDINGS_DROP", v)) {
            swofty::log("%s", guards::lines::off("[career] dropped scores", "SWOFTY_STANDINGS_DROP", v).c_str());
            return false;
        }
        if (!runtime::holds(DROPPED_SCORES))
            return false;
        uintptr_t recompute = runtime::find(S_RECOMPUTE, "career", "career"), cmp = runtime::find(S_COMPARE, "career", "career");
        if (!recompute || !cmp) {
            swofty::log("[career] dropped scores: not hooked: %s%s", recompute ? "" : "recompute ", cmp ? "" : "compare");
            return false;
        }
        compare = reinterpret_cast<Compare_t *>(cmp);
        auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(recompute), reinterpret_cast<void *>(&recompute_hook));
        if (!h) {
            swofty::log("[career] dropped scores: the hook failed");
            return false;
        }
        h_recompute = std::move(*h);
        swofty::log("[career] dropped scores armed (hook at base+0x%llx), first rule set",
                    (unsigned long long)(recompute - swofty::api->module_base()));
        return true;
    }
};

constinit DropGuard guard;

}

namespace runtime {

int standings_drop_set(const std::string &data_group, uint32_t best)
{
    if (best && !guard.arm())
        return SWOFTY_ASSET_LATE;
    std::lock_guard<std::mutex> hold(rules_lock);
    if (best)
        rules[data_group] = best;
    else
        rules.erase(data_group);
    swofty::log("[career] dropped scores: data group %s %s", data_group.c_str(),
                best ? ("counts the best " + std::to_string(best) + " results").c_str() : "counts every result");
    return SWOFTY_ASSET_OK;
}

}
