#include "core/career/career_entity.hpp"

#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstddef>
#include <cstdint>
#include <string>

#include "core/runtime/layout.hpp"
#include "core/guards/career/myteam_vehicle_heal_logic.hpp"
#include "core/diagnostics/perf.hpp"

// Career (g_Game_Manager, GameManager, GameModeContext, ChampionshipHolder, CareerEntity,
// EntrantRecord, k_Career_<Class>Desc). The championship entity is
// manager->mode_context->championship_holder->entity; its component registry is embedded at
// CareerEntity.component_registry and answers get(registry, &desc, search_parents) at vtable slot
// +0x10, the call the game's own components make. A CareerComponent means a loaded career;
// CareerBank or MyTeamFacilities means My Team, DriverCareerPlayerDriverStatistics Driver Career.
// The human entrant (EntrantRecord.ordinal_flags bit 0x10000000) gives the career key.

namespace career::game {

namespace {

namespace g = swofty::game;
namespace layout = runtime::layout;
namespace mem = swofty::mem;

constexpr uintptr_t REGISTRY_GET_SLOT = 0x10;
constexpr uint32_t HUMAN_FLAG = 0x10000000u;
constexpr size_t MAX_ENTRANTS = runtime::myteam_heal::MAX_ENTRANTS;

using RegistryGet = void *(void *registry, const void *desc, uint32_t search_parents);

template <class M = g::GameManager, class C = g::GameModeContext, class H = g::ChampionshipHolder, class E = g::CareerEntity>
uintptr_t championship_entity(runtime::myteam_heal::Load *trace = nullptr)
{
    if constexpr (layout::career_chain<M, C, H, E>) {
        runtime::myteam_heal::Load own;
        auto &t = trace ? *trace : own;
        uintptr_t slot = binding("g_Game_Manager"), mgr = 0, ctx = 0, holder = 0, e = 0;
        if (!slot)
            t.stop = "g_Game_Manager is not resolved";
        else if (!mem::read(slot, mgr) || !mgr)
            t.stop = "[g_Game_Manager] reads null";
        else if (!mem::read(mgr + offsetof(M, mode_context), ctx) || !ctx)
            t.stop = "the game manager's mode context reads null";
        else if (!mem::read(ctx + offsetof(C, championship_holder), holder) || !holder)
            t.stop = "the championship holder reads null";
        else if (!mem::read(holder + offsetof(H, entity), e) || !e)
            t.stop = "the championship entity reads null";
        else if (!mem::object_like(e) || !mem::object_like(e + offsetof(E, component_registry)))
            t.stop = "the championship entity is not a game object";
        t.manager = mgr, t.context = ctx, t.holder = holder, t.entity = e;
        return t.stop ? 0 : e;
    } else {
        (void)trace;
        return 0;
    }
}

template <class E = g::CareerEntity> uintptr_t registry_get(uintptr_t e, uintptr_t desc)
{
    if constexpr (layout::CAREER) {
        uintptr_t reg = e + offsetof(E, component_registry), vt = 0, fn = 0;
        if (!e || !desc || !mem::read(reg, vt) || !mem::in_image(vt) || !mem::read(vt + REGISTRY_GET_SLOT, fn) ||
            !mem::in_image(fn))
            return 0;
        static const uint32_t probe = perf::probe("core", "career RegistryGet (game call)");
        perf::Scope p(probe);
        auto c = reinterpret_cast<uintptr_t>(
            reinterpret_cast<RegistryGet *>(fn)(reinterpret_cast<void *>(reg), reinterpret_cast<const void *>(desc), 1));
        return mem::object_like(c) ? c : 0;
    } else {
        (void)e, (void)desc;
        return 0;
    }
}

std::string desc_id(const char *type)
{
    std::string t = type ? type : "";
    if (t.empty() || t.rfind("k_", 0) == 0)
        return t;
    return "k_Career_" + t + "Desc";
}

uintptr_t component_of(uintptr_t e, const char *type)
{
    std::string id = desc_id(type);
    return id.empty() ? 0 : registry_get(e, binding(id));
}

template <class E = g::CareerEntity, class R = g::EntrantRecord> void entrants(uintptr_t e, career::state::Components &out)
{
    if constexpr (layout::CAREER && layout::ENTRANT) {
        uintptr_t b = 0, en = 0;
        if (!mem::read(e + offsetof(E, entrants_begin), b) || !mem::read(e + offsetof(E, entrants_end), en) || !b || en < b ||
            (en - b) % sizeof(R) || (en - b) / sizeof(R) > MAX_ENTRANTS || (en != b && !mem::readable(b, en - b)))
            return;
        uint32_t humans = 0;
        for (uintptr_t p = b; p < en; p += sizeof(R)) {
            uint32_t flags = 0, driver = 0, team = 0;
            mem::read(p + offsetof(R, ordinal_flags), flags);
            if (!(flags & HUMAN_FLAG))
                continue;
            mem::read(p + offsetof(R, id_lo), driver);
            mem::read(p + offsetof(R, team_stat_id), team);
            out.driver = driver, out.team = team;
            ++humans;
        }
        out.humans = humans;
        out.player = humans == 1;
        if (!out.player)
            out.driver = out.team = 0;
    } else {
        (void)e, (void)out;
    }
}

template <class E = g::CareerEntity, class R = g::EntrantRecord> void heal_my_team_cars()
{
    if constexpr (layout::CAREER && layout::ENTRANT) {
        static_assert(offsetof(R, vehicle_ref) == runtime::myteam_heal::ENTRANT_VEHICLE &&
                      offsetof(R, ordinal_flags) == runtime::myteam_heal::ENTRANT_FLAGS &&
                      sizeof(R) == runtime::myteam_heal::ENTRANT_SIZE);
        mem::Trust trust(runtime::HEAP_TRUST_MS);
        runtime::myteam_heal::Load load;
        uintptr_t e = championship_entity(&load), b = 0, en = 0, es = 0;
        if (e) {
            bool read = mem::read(e + offsetof(E, entrants_begin), b) && mem::read(e + offsetof(E, entrants_end), en);
            load.first = b, load.end = en;
            load.stop = runtime::myteam_heal::range_stop(read, b, en, MAX_ENTRANTS,
                                                         [](uint64_t p, uint64_t n) { return mem::readable(p, n); });
        }
        load.offset = runtime::manager_entrants_system();
        if (load.manager && mem::read(load.manager + load.offset, es))
            load.system = es;
        if (!load.stop && !es)
            load.stop = "the entrants system reads null";
        runtime::myteam_vehicle_heal_entrants(load);
    }
}

}

uintptr_t entity() { return championship_entity(); }

uintptr_t component(uintptr_t e, const char *type) { return component_of(e, type); }

uintptr_t component_by_desc(uintptr_t e, uintptr_t desc) { return registry_get(e, desc); }

void heal_my_team() { heal_my_team_cars(); }

career::state::Components poll()
{
    static const uint32_t find = perf::probe("core", "career poll: entity");
    static const uint32_t lookup = perf::probe("core", "career poll: components");
    static const uint32_t walk = perf::probe("core", "career poll: entrants");
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    career::state::Components c;
    uintptr_t e = 0;
    {
        perf::Scope p(find);
        e = championship_entity();
    }
    c.entity = e != 0;
    if (!e)
        return c;
    {
        perf::Scope p(lookup);
        c.career = component_of(e, "CareerComponent");
        if (c.career) {
            if (component_of(e, "CareerBankComponent") || component_of(e, "MyTeamFacilitiesComponent"))
                c.mode = SWOFTY_CAREER_MY_TEAM;
            else if (component_of(e, "DriverCareerPlayerDriverStatisticsComponent"))
                c.mode = SWOFTY_CAREER_DRIVER;
        }
    }
    if (c.career) {
        perf::Scope p(walk);
        entrants(e, c);
    }
    return c;
}

}
