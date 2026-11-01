// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: career components through the core's career library (swofty/career.hpp)
// and guarded reads through swofty/mem.hpp.
//
// Championship entity: swofty::career::entity() (the game's GameManager -> GameModeContext ->
// ChampionshipHolder -> CareerEntity chain, resolved by the core), accepted when its embedded
// component registry has a vtable in the image.
//
// Components (k_Career_<Class>Desc): swofty::career::component_by_desc() with the descriptor's KB
// binding, which calls the entity's own registry get (CareerEntity.component_registry, vtable slot
// +0x10), the call the game's components use to resolve their siblings (CareerBank's resolve slot
// 0x140c5d2c0, ResourcePoints' 0x1412256e0, the player card 0x1414c5f80). A component that is not
// found is logged once per entity with what the registry get itself returned before the object
// check, that pointer's vtable and the descriptor its GetType slot (+0x28) names, so a report tells
// an absent component from one the checks rejected.
//
// Entrants. CareerEntity.entrants_begin..entrants_end are EntrantRecords: ordinal_flags bit
// 0x10000000 marks a human player; the packed driver key is the dword at id_lo (u8 kind, u16 stat
// id in the high half, read whole as the game does), team_stat_id the packed team key. The game
// finds the human entrant of a team by that bit (RP generation handler 0x141229e00, driver-key
// resolver 0x1400fcbe0).
//
// Fields come from the KB structs (CareerEntity, EntrantRecord); the constants left are a flag bit
// and an element cap.
//
// Championship (vt_Career_Championship). The entity owns its championships (list +0x1040, ordered
// list +0x1068) and keeps the current one at +0x1058, written only by Career_Set_CurrentChampionship
// and read back by the game's own season code (0x1407c26f0, 0x1407bdfb0) in both career modes. The
// concrete object (type "Championship", k_Career_ChampionshipDesc) is not looked up by key anywhere
// in the game, so the registry get with that type is only a fallback. get(Id::Championship) takes
// the first of {entity+0x1058, the registry get} whose vtable is vt_Career_Championship, logs which
// one once, and returns 0 otherwise.
//
// Read only, apart from the registry get. Every pointer is region-checked.

#include <cstdint>
#include <cstring>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "ce.hpp"
#include "components.hpp"

using namespace swofty::game;

namespace ce::comp {

namespace {

constexpr uint32_t kHumanFlag = 0x10000000u;

struct Entry {
    const char *name;
    uintptr_t (*desc)();
};

template <class B> uintptr_t addr_of(const B &b)
{
    return reinterpret_cast<uintptr_t>(b.get());
}

#define CE_DESC(cls) {#cls "Component", [] { return addr_of(k_Career_##cls##ComponentDesc); }}

const Entry kTable[] = {
    CE_DESC(Career),
    CE_DESC(CareerSetup),
    CE_DESC(CareerTime),
    CE_DESC(Schedule),
    {"Championship", [] { return addr_of(k_Career_ChampionshipDesc); }},
    CE_DESC(CareerBank),
    CE_DESC(CostCap),
    CE_DESC(CareerSponsors),
    CE_DESC(MyTeamRF),
    CE_DESC(MyTeamFacilities),
    CE_DESC(MyTeamStats),
    CE_DESC(MyTeamAttributes),
    CE_DESC(OwnerPerks),
    CE_DESC(OwnerSetup),
    CE_DESC(CareerFacilities),
    CE_DESC(ResourcePoints),
    CE_DESC(Upgrades),
    CE_DESC(EngineSuppliers),
    CE_DESC(DriverStatistics),
    CE_DESC(DriverCareerPlayerDriverStatistics),
    CE_DESC(DriverContractsMyTeam),
    CE_DESC(DriverCareerContractsManager),
    CE_DESC(CareerDriverMarket),
    CE_DESC(CareerDriverContractRenewal),
    CE_DESC(TeamHQController),
    CE_DESC(WeatherAndTimeOfDay),
};

#undef CE_DESC

static_assert(sizeof kTable / sizeof kTable[0] == static_cast<size_t>(Id::Count), "descriptor table out of sync");

} // namespace

bool object_like(uintptr_t obj)
{
    return swofty::mem::object_like(obj);
}

bool readable(uintptr_t p, size_t n)
{
    return swofty::mem::readable(p, n);
}

bool in_image(uintptr_t p)
{
    return swofty::mem::in_image(p);
}

uintptr_t entity()
{
    auto e = reinterpret_cast<uintptr_t>(swofty::career::entity());
    auto *ent = view<CareerEntity>(e);
    if (!ent || !object_like(e) || !in_image(reinterpret_cast<uintptr_t>(ent->component_registry)))
        return 0;
    return e;
}

const char *name(Id id)
{
    auto i = static_cast<size_t>(id);
    return i < static_cast<size_t>(Id::Count) ? kTable[i].name : "?";
}

namespace {

uintptr_t registry_get(uintptr_t desc)
{
    auto c = reinterpret_cast<uintptr_t>(swofty::career::component_by_desc(reinterpret_cast<const void *>(desc)));
    return c && object_like(c) ? c : 0;
}

constexpr uintptr_t kVtRegistryGet = 0x10; // CareerEntity.component_registry vtable slot get(registry, &desc, search)
constexpr uint32_t kSearchParents = 1;     // its search argument: this registry, then the parent's (the game's own lookups)
constexpr uintptr_t kVtGetType = 0x28;     // component vtable slot whose body is `lea rax, [k_Career_<Class>Desc]; ret`

using RegistryGetFn = void *(*)(void *registry, const void *desc, uint32_t search);

uintptr_t type_desc(uintptr_t obj)
{
    uintptr_t vt = 0, fn = 0;
    uint64_t code = 0;
    if (!object_like(obj) || !read(obj, vt) || !read(vt + kVtGetType, fn) || !in_image(fn) || !read(fn, code) ||
        (code & 0xffffff) != 0x058d48 || (code >> 56) != 0xc3)
        return 0;
    auto rel = static_cast<int32_t>(static_cast<uint32_t>(code >> 24));
    return fn + 7 + static_cast<intptr_t>(rel);
}

void log_missing(Id id, uintptr_t e, uintptr_t desc)
{
    static uintptr_t logged[static_cast<size_t>(Id::Count)];
    auto i = static_cast<size_t>(id);
    if (logged[i] == e)
        return;
    logged[i] = e;
    uintptr_t reg = e + offsetof(CareerEntity, component_registry), vt = 0, fn = 0, raw = 0, raw_vt = 0;
    if (desc && read(reg, vt) && in_image(vt) && read(vt + kVtRegistryGet, fn) && in_image(fn))
        raw = reinterpret_cast<uintptr_t>(reinterpret_cast<RegistryGetFn>(fn)(
            reinterpret_cast<void *>(reg), reinterpret_cast<const void *>(desc), kSearchParents));
    read(raw, raw_vt);
    ce::log("components: %s not found (entity 0x%llx, desc 0x%llx; registry get returned 0x%llx, vtable 0x%llx, "
            "type desc 0x%llx); logged once per entity",
            kTable[i].name, (unsigned long long)e, (unsigned long long)desc, (unsigned long long)raw,
            (unsigned long long)raw_vt, (unsigned long long)type_desc(raw));
}

uintptr_t championship(uintptr_t e)
{
    static int logged = 0;
    uintptr_t want = addr_of(vt_Career_Championship);
    auto *ent = view<CareerEntity>(e);
    uintptr_t current = ent ? reinterpret_cast<uintptr_t>(ent->current_championship) : 0;
    uintptr_t cand[2] = {current, registry_get(kTable[static_cast<size_t>(Id::Championship)].desc())};
    static const char *const kFrom[2] = {"entity+0x1058 (current championship)", "registry"};
    for (int i = 0; i < 2; ++i) {
        uintptr_t vt = 0;
        if (cand[i] && read(cand[i], vt) && vt == want) {
            if (logged != i + 1) {
                logged = i + 1;
                ce::log("components: Championship found as the %s (0x%llx)", kFrom[i], (unsigned long long)cand[i]);
            }
            return cand[i];
        }
    }
    if (logged != -1) {
        logged = -1;
        uintptr_t v[2] = {};
        for (int i = 0; i < 2; ++i)
            read(cand[i], v[i]);
        ce::log("components: Championship not found (entity 0x%llx, +0x1058 0x%llx vt 0x%llx, registry 0x%llx vt 0x%llx, "
                "want vt 0x%llx)",
                (unsigned long long)e, (unsigned long long)cand[0], (unsigned long long)v[0],
                (unsigned long long)cand[1], (unsigned long long)v[1], (unsigned long long)want);
    }
    return 0;
}

} // namespace

uintptr_t get(Id id)
{
    auto i = static_cast<size_t>(id);
    if (i >= static_cast<size_t>(Id::Count))
        return 0;
    uintptr_t e = entity();
    if (!e)
        return 0;
    if (id == Id::Championship)
        return championship(e);
    uintptr_t desc = kTable[i].desc();
    uintptr_t c = registry_get(desc);
    if (!c)
        log_missing(id, e, desc);
    return c;
}

std::vector<Entrant> entrants()
{
    std::vector<Entrant> out;
    auto *e = view<CareerEntity>(entity());
    size_t n = 0;
    if (!e || !e->entrants_begin || !array(e->entrants_begin, e->entrants_end, kMaxEntrants, n))
        return out;
    for (size_t i = 0; i < n; ++i) {
        const EntrantRecord &r = e->entrants_begin[i];
        Entrant x;
        x.at = reinterpret_cast<uintptr_t>(&r);
        std::memcpy(&x.driver, &r.id_lo, sizeof x.driver);
        x.team = r.team_stat_id;
        x.human = (r.ordinal_flags & kHumanFlag) != 0;
        out.push_back(x);
    }
    return out;
}

int humans()
{
    int n = 0;
    for (auto &x : entrants())
        n += x.human;
    return n;
}

bool player(Entrant &out)
{
    int n = 0;
    for (auto &x : entrants())
        if (x.human) {
            out = x;
            ++n;
        }
    return n == 1;
}

} // namespace ce::comp
