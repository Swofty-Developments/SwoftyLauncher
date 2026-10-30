// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - dry tyres in Friday qualifying (its own DLL, so a binding it can't resolve drops only this hook).
//
// Friday qualifying runs as a sprint-shootout session (ids 10..14). In tyre allocation mode 0 the
// "set usable now" check (0x142e88730) has no case for those sessions, so only untagged sets
// (inters and wets) can be fitted, and the session start (Sess_TyreAllocation_OnSessionStart) hands
// sets back. After that session start, on a 1990 weekend, every unused and undamaged dry set of
// every car loses its handed-back flag and its session tag (tag 0 is usable in any session), so
// Friday and Saturday qualifying share the unused dry sets. The 1990 weekend structure exists only in
// the f1_1990 setup mode, so 2025/2026 sessions are untouched. Setting fq_tyre_fix turns it off.
//
// Tyre fallback (setting tyre_fallback): some cars of a 26-car 1990 session found no compound to fit
// ("Unable to get Closest Available Compound for vehicle %u") and ran without tyres, although their
// weekend allocations were full. Sess_Tyres_ClosestAvailableCompound reads the allocation from the
// car's tyre component (+0x9c68), which the game sets only while it is empty, so a car can keep an
// allocation other than the one the weekend list holds for its driver. At every 1990 session start,
// after the game's own session start, every car's tyre component is pointed at its driver's list
// allocation, and an allocation with no sets gets a copy of the fullest one's sets. When the lookup
// still finds nothing for a car of a 1990 session, the car's allocation is repaired (reassigned,
// refilled, worn fresh sets cleared, tags of fresh sets dropped) and the lookup runs again; if it
// still fails, the first preferred slot with a fresh set is returned. Each step is logged.
//
// Tyre class: at every 1990 session start one line tells whether the cars' tyre components hold the
// compound class the game picked for the session's allocations (f1_1990_tyreclass.dll gives it to them
// when they are created).

#include <atomic>
#include <bitset>
#include <cstdio>
#include <iterator>
#include <mutex>
#include <string>

#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "fq_tyres.hpp"
#include "weekend.hpp"

SWOFTY_SETTING(bool, fq_tyre_fix, true);
SWOFTY_SETTING(bool, tyre_fallback, true);

namespace {

using f1990::at;
namespace t = f1990::tyres;

constexpr uint32_t kNoCompound = 0xffffffff;
constexpr size_t kEntrantSize = 0x88;
constexpr size_t kEntrantMax = 256;

std::mutex g_lock;
void *g_component;
bool g_1990;
uint32_t g_session;
std::bitset<256> g_reported;
int g_lines;

void *call_getter(void *obj, size_t slot)
{
    auto **vt = *static_cast<void ***>(obj);
    return vt && vt[slot] ? reinterpret_cast<void *(*)(void *)>(vt[slot])(obj) : nullptr;
}

void *heap_alloc(void *allocator, size_t bytes)
{
    auto **vt = *static_cast<void ***>(allocator);
    return reinterpret_cast<void *(*)(void *, size_t, size_t)>(vt[0x18 / 8])(allocator, bytes, 4);
}

void heap_release(void *allocator, void *p)
{
    auto **vt = *static_cast<void ***>(allocator);
    reinterpret_cast<void (*)(void *, void *)>(vt[0x38 / 8])(allocator, p);
}

const t::Heap kHeap{heap_alloc, heap_release};

unsigned key(uint32_t id)
{
    return ((id & 0xff) << 14 | id >> 16) & 0xffff;
}

bool sentinel(uint32_t id)
{
    return (id & 0xffff0000) == 0x3fff0000;
}

struct Cars {
    uint8_t *b = nullptr, *e = nullptr;

    explicit Cars(void *component)
    {
        if (!swofty::mem::readable(static_cast<uint8_t *>(component) + 0x2b0, 0x10))
            return;
        auto *pb = at<uint8_t *>(component, 0x2b0), *pe = at<uint8_t *>(component, 0x2b8);
        if (pb && pe > pb && size_t(pe - pb) % 0x10 == 0 && swofty::mem::readable(pb, size_t(pe - pb)))
            b = pb, e = pe;
    }

    template <class F> void each(F f) const
    {
        for (uint8_t *c = b; c < e; c += 0x10) {
            auto *alloc = at<uint8_t *>(c, 0x08);
            if (alloc && swofty::mem::readable(alloc, 0xf0) && !sentinel(at<uint32_t>(alloc, 0)))
                f(alloc);
        }
    }

    uint8_t *find(uint32_t id) const
    {
        uint8_t *found = nullptr;
        each([&](uint8_t *alloc) {
            uint32_t a = at<uint32_t>(alloc, 0);
            if (!found && uint8_t(a) == uint8_t(id) && uint16_t(a >> 16) == uint16_t(id >> 16))
                found = alloc;
        });
        return found;
    }

    bool contains(const uint8_t *alloc) const
    {
        bool found = false;
        each([&](uint8_t *a) { found |= a == alloc; });
        return found;
    }

    uint8_t *donor(size_t *sets) const
    {
        uint8_t *best = nullptr;
        *sets = 0;
        each([&](uint8_t *alloc) {
            size_t n = t::census(alloc).total;
            if (n > *sets)
                best = alloc, *sets = n;
        });
        return best;
    }
};

// The tyre component of vehicle slot v of a car list (car list +0x28 slots, entity +0x10), found the
// way Sess_Tyres_ClosestAvailableCompound finds it.
uint8_t *tyre_component(void *car_list, uint8_t v)
{
    namespace g = swofty::game;
    if (!car_list || !swofty::mem::readable(static_cast<uint8_t *>(car_list) + 0x28, 0x10))
        return nullptr;
    auto **b = at<uint8_t **>(car_list, 0x28), **e = at<uint8_t **>(car_list, 0x30);
    if (!b || e <= b || size_t(e - b) <= v || !swofty::mem::readable(b + v, sizeof *b))
        return nullptr;
    uint8_t *slot = b[v];
    if (!slot || !swofty::mem::readable(slot + 0x10, 8))
        return nullptr;
    void *obj = at<void *>(slot, 0x10);
    auto type = reinterpret_cast<const uint8_t *>(g::k_Sess_TyreComponentType.address());
    auto base = reinterpret_cast<const uint8_t *>(g::k_Ne_TypeRoot.address());
    auto end = reinterpret_cast<const uint8_t *>(g::k_Ne_TypeChainEnd.address());
    bool component = type == base || type == end;
    for (const uint8_t *n = type + 8; !component && n && swofty::mem::readable(n + 0x10, 8);) {
        n = at<const uint8_t *>(n, 0x10);
        component = n == base + 8;
        if (n == end + 8)
            break;
    }
    if (component)
        obj = obj ? g::Ne_Entity_FindComponent(obj, type) : nullptr;
    return obj && swofty::mem::readable(static_cast<uint8_t *>(obj) + 0x9c68, 8) ? static_cast<uint8_t *>(obj)
                                                                               : nullptr;
}

struct Entrants {
    const uint8_t *b = nullptr, *e = nullptr;

    explicit Entrants(void *component)
    {
        if (!swofty::mem::readable(static_cast<uint8_t *>(component) + 0x278, 0x08))
            return;
        auto *career = at<uint8_t *>(component, 0x278);
        if (!career || !swofty::mem::readable(career + 0x2e0, 0x10))
            return;
        auto *pb = at<const uint8_t *>(career, 0x2e0), *pe = at<const uint8_t *>(career, 0x2e8);
        if (pb && pe > pb && size_t(pe - pb) % kEntrantSize == 0 && size_t(pe - pb) / kEntrantSize <= kEntrantMax &&
            swofty::mem::readable(pb, size_t(pe - pb)))
            b = pb, e = pe;
    }

    uint32_t id(const uint8_t *r) const { return at<uint8_t>(r, 0x50) | uint32_t(at<uint16_t>(r, 0x52)) << 16; }

    const uint8_t *vehicle(uint8_t v) const
    {
        for (const uint8_t *r = b; r < e; r += kEntrantSize)
            if (at<uint8_t>(r, 0x82) == v)
                return r;
        return nullptr;
    }
};

void describe(const t::Census &c, char *out, size_t n)
{
    std::snprintf(out, n,
                  "%zu set(s), %zu fresh, %zu fresh tagged (tags 0x%x), %zu fresh worn; per slot S %zu/%zu M %zu/%zu "
                  "H %zu/%zu I %zu/%zu W %zu/%zu",
                  c.total, c.fresh, c.tagged, c.tags, c.worn, c.fresh_sets[0], c.sets[0], c.fresh_sets[1], c.sets[1],
                  c.fresh_sets[2], c.sets[2], c.fresh_sets[3], c.sets[3], c.fresh_sets[4], c.sets[4]);
}

// The allocation a tyre component holds, described for the log: its key when it is in the weekend
// list, else its address.
void holder(const Cars &cars, const uint8_t *alloc, char *out, size_t n)
{
    bool listed = cars.contains(alloc);
    if (!alloc)
        std::snprintf(out, n, "none");
    else if (listed)
        std::snprintf(out, n, "%04x", key(at<uint32_t>(alloc, 0)));
    else
        std::snprintf(out, n, "unlisted %p", static_cast<const void *>(alloc));
}

// Points the car's tyre component at its driver's list allocation. Returns 1 when it changed it.
int reassign(const Cars &cars, uint8_t *tyres, uint8_t *alloc, uint32_t session, uint8_t v, uint32_t driver)
{
    auto *held = at<uint8_t *>(tyres, 0x9c68);
    if (!alloc || held == alloc)
        return 0;
    char was[48];
    holder(cars, held, was, sizeof was);
    std::memcpy(tyres + 0x9c68, &alloc, sizeof alloc);
    swofty::log("[f1-1990] tyres: session %u: V%02u (driver %04x) held allocation %s; now %04x", session, v, key(driver),
                was, key(at<uint32_t>(alloc, 0)));
    return 1;
}

int refill(const Cars &cars, uint8_t *alloc, const char *why)
{
    size_t donor_sets;
    uint8_t *donor = cars.donor(&donor_sets);
    if (!donor) {
        swofty::log("[f1-1990] tyres: allocation %04x (%s) has no sets and no allocation has any to copy",
                    key(at<uint32_t>(alloc, 0)), why);
        return 0;
    }
    int copied = t::copy_sets(alloc, donor, kHeap);
    swofty::log("[f1-1990] tyres: allocation %04x (preset %u, %s) had no sets: %d set(s) copied from %04x (%zu set(s))%s",
                key(at<uint32_t>(alloc, 0)), at<uint32_t>(alloc, 4), why, copied < 0 ? 0 : copied,
                key(at<uint32_t>(donor, 0)), donor_sets, copied < 0 ? "; an allocation failed" : "");
    return copied;
}

void audit(void *component, uint32_t session)
{
    Cars cars(component);
    Entrants entrants(component);
    size_t count = 0, lo = SIZE_MAX, hi = 0;
    uint8_t *empty[32];
    size_t empties = 0;
    cars.each([&](uint8_t *alloc) {
        size_t n = t::census(alloc).total;
        count++;
        lo = std::min(lo, n);
        hi = std::max(hi, n);
        if (!n && empties < std::size(empty))
            empty[empties++] = alloc;
    });
    for (size_t i = 0; i < empties; i++)
        refill(cars, empty[i], "session start");
    size_t with_car = 0, missing = 0, unreachable = 0, moved = 0;
    void *car_list = swofty::mem::readable(static_cast<uint8_t *>(component) + 0x2d8, 8) ? at<void *>(component, 0x2d8)
                                                                                          : nullptr;
    for (const uint8_t *r = entrants.b; r < entrants.e; r += kEntrantSize) {
        uint8_t v = at<uint8_t>(r, 0x82);
        if (v == 0xff)
            continue;
        with_car++;
        uint8_t *alloc = cars.find(entrants.id(r));
        if (!alloc) {
            if (missing++ < 8)
                swofty::log("[f1-1990] tyres: session %u: V%02u (driver %04x) has no weekend allocation", session, v,
                            key(entrants.id(r)));
            continue;
        }
        uint8_t *tyres = tyre_component(car_list, v);
        if (!tyres) {
            unreachable++;
            continue;
        }
        moved += reassign(cars, tyres, alloc, session, v, entrants.id(r));
    }
    swofty::log("[f1-1990] tyres: session %u: %zu car allocation(s) for %zu car(s), %zu set(s) per car at least, %zu at "
                "most; %zu empty, %zu car(s) without one, %zu tyre component(s) not found, %zu reassigned",
                session, count, with_car, count ? lo : 0, hi, empties, missing, unreachable, moved);
}

// The class setPresetChoice (0x140bcb9a0) gives the session's allocations: the first compound class
// (component+0x2e8, +0xf0..+0xf8) whose masks match the championship. Null when the game takes each
// car's own class there instead (career+0x2b0, +0x284 set).
const uint8_t *session_class(void *component, const uint8_t **base)
{
    auto *c = static_cast<uint8_t *>(component);
    if (!swofty::mem::readable(c + 0x278, 8) || !swofty::mem::readable(c + 0x2e8, 8))
        return nullptr;
    auto *career = at<uint8_t *>(c, 0x278), *classes = at<uint8_t *>(c, 0x2e8);
    if (!career || !classes || !swofty::mem::readable(career, 0x2b8) || !swofty::mem::readable(classes + 0xf0, 0x10))
        return nullptr;
    auto *rules = at<uint8_t *>(career, 0x2b0);
    if (rules && swofty::mem::readable(rules + 0x284, 1) && rules[0x284])
        return nullptr;
    auto **vt = at<void **>(career, 0);
    if (!vt || !swofty::mem::readable(vt + 200 / 8, 8) || !vt[200 / 8])
        return nullptr;
    alignas(8) uint8_t raw[16] = {};
    reinterpret_cast<void (*)(void *, uint8_t *)>(vt[200 / 8])(career, raw);
    t::Championship champ;
    std::memcpy(&champ, raw, sizeof champ);
    auto *b = at<uint8_t *>(classes, 0xf0), *e = at<uint8_t *>(classes, 0xf8);
    if (!b || e < b || size_t(e - b) % t::kClassSize || !swofty::mem::readable(b, size_t(e - b)))
        return nullptr;
    *base = b;
    for (; b < e; b += t::kClassSize)
        if (t::class_matches(b, champ))
            return b;
    return nullptr;
}

void report_classes(void *component, uint32_t session)
{
    const uint8_t *base = nullptr, *cls = session_class(component, &base);
    if (!cls) {
        swofty::log("[f1-1990] tyres: session %u: no session compound class", session);
        return;
    }
    Entrants entrants(component);
    void *car_list = swofty::mem::readable(static_cast<uint8_t *>(component) + 0x2d8, 8) ? at<void *>(component, 0x2d8)
                                                                                          : nullptr;
    size_t had = 0, other = 0, unreachable = 0;
    long other_index = -1;
    for (const uint8_t *r = entrants.b; r < entrants.e; r += kEntrantSize) {
        uint8_t v = at<uint8_t>(r, 0x82);
        if (v == 0xff)
            continue;
        uint8_t *tyres = tyre_component(car_list, v);
        if (!tyres) {
            unreachable++;
            continue;
        }
        auto *own = at<const uint8_t *>(tyres, 0x9bc0);
        if (own == cls) {
            had++;
            continue;
        }
        other++;
        if (own >= base && size_t(own - base) % t::kClassSize == 0)
            other_index = long((own - base) / t::kClassSize);
    }
    swofty::log("[f1-1990] tyres: session %u: compound class %ld on %zu car(s), %zu on another class (%ld), %zu tyre "
                "component(s) not found",
                session, long((cls - base) / t::kClassSize), had, other, other_index, unreachable);
}
}

SWOFTY_WRAP(Sess_TyreAllocation_OnSessionStart, void, void *component)
{
    orig(component);
    if (!component)
        return;
    void *provider = at<void *>(component, 0x270);
    void *session = provider ? call_getter(provider, 0xa8 / 8) : nullptr;
    void *weekend = provider ? call_getter(provider, 0x98 / 8) : nullptr;
    bool is_1990 = session && weekend && f1990::is_1990_weekend(weekend);
    {
        std::lock_guard lock(g_lock);
        g_component = component;
        g_1990 = is_1990;
        g_session = session ? at<uint32_t>(session, 0x08) : 0;
        g_reported.reset();
        if (is_1990 && tyre_fallback)
            audit(component, g_session);
        if (is_1990)
            report_classes(component, g_session);
    }
    if (!is_1990)
        return;
    uint32_t id = at<uint32_t>(session, 0x08);
    bool free_dry = fq_tyre_fix && f1990::tyres::is_shootout_session(id);
    auto *b = at<uint8_t *>(component, 0x2b0), *e = at<uint8_t *>(component, 0x2b8);
    int cars = 0, sets = 0, capped = 0, capped_cars = 0;
    for (; b && b < e; b += 0x10) {
        auto *alloc = at<uint8_t *>(b, 0x08);
        if (!alloc)
            continue;
        cars++;
        if (free_dry)
            sets += f1990::tyres::free_unused_dry_sets(alloc);
        int n = f1990::tyres::cap_usable_sets(alloc);
        capped += n;
        capped_cars += n > 0;
    }
    if (free_dry)
        swofty::log("[f1-1990] FQ tyres: session %u: %d unused dry set(s) made usable on %d car allocation(s)", id, sets,
                    cars);
    if (capped)
        swofty::log("[f1-1990] tyres: session %u: %d set(s) on %d car(s) handed back: the game picks from at most %zu "
                    "usable sets of one compound",
                    id, capped, capped_cars, f1990::tyres::kCollectorSets);
}

SWOFTY_WRAP(Sess_Tyres_ClosestAvailableCompound, uint32_t, void *tyre_rules, uint8_t vehicle, const uint32_t *preference,
            float wear_scale)
{
    namespace g = swofty::game;
    uint32_t slot = orig(tyre_rules, vehicle, preference, wear_scale);
    if (slot != kNoCompound || !tyre_fallback)
        return slot;
    std::lock_guard lock(g_lock);
    if (!g_component || !g_1990)
        return slot;
    Cars cars(g_component);
    Entrants entrants(g_component);
    const uint8_t *entrant = entrants.vehicle(vehicle);
    uint32_t driver = entrant ? entrants.id(entrant) : 0xffffffff;
    uint8_t *listed = entrant ? cars.find(driver) : nullptr;
    void *car_list = swofty::mem::readable(static_cast<uint8_t *>(tyre_rules) + 0xd8, 8) ? at<void *>(tyre_rules, 0xd8)
                                                                                        : nullptr;
    uint8_t *tyres = tyre_component(car_list, vehicle);
    uint8_t *held = tyres ? at<uint8_t *>(tyres, 0x9c68) : nullptr;
    bool report = !g_reported.test(vehicle) && g_lines < 64;
    g_reported.set(vehicle);
    g_lines += report;

    char used[48], text[240], why[200];
    holder(cars, held, used, sizeof used);
    bool held_listed = held && cars.contains(held);
    t::Census before = held_listed ? t::census(held) : t::Census{};
    describe(before, text, sizeof text);
    float limit = 0.f;
    auto *rules_limit = swofty::mem::readable(static_cast<uint8_t *>(tyre_rules) + 0x108, 8)
                            ? at<uint8_t *>(tyre_rules, 0x108) : nullptr;
    if (rules_limit && swofty::mem::readable(rules_limit + 8, 8) && (rules_limit = at<uint8_t *>(rules_limit, 8)) &&
        swofty::mem::readable(rules_limit, 8) && (rules_limit = at<uint8_t *>(rules_limit, 0)) &&
        swofty::mem::readable(rules_limit + 8, 4))
        limit = at<float>(rules_limit, 8);
    std::snprintf(why, sizeof why,
                  "%s, flags +0x48 %02x +0x4b %02x, vehicle id %u; preference %u %u %u %u %u, wear scale %.2f, limit "
                  "%.2f; rules on %u mode %u session %u",
                  entrant && (at<uint8_t>(entrant, 0x4b) & 0x10) ? "human" : "AI", entrant ? at<uint8_t>(entrant, 0x48) : 0,
                  entrant ? at<uint8_t>(entrant, 0x4b) : 0, entrant ? at<uint32_t>(entrant, 0x5c) >> 16 : 0xffff,
                  preference[0], preference[1], preference[2], preference[3], preference[4], double(wear_scale),
                  double(limit), *g::g_Sess_TyreRulesActive, *g::g_Sess_TyreRulesMode, *g::g_Sess_TyreRulesSessionType);

    std::string done;
    uint32_t again = slot;
    auto retry = [&](const char *step) {
        done += done.empty() ? step : std::string(", ") + step;
        again = orig(tyre_rules, vehicle, preference, wear_scale);
        return again != kNoCompound;
    };
    if (tyres && listed && held != listed) {
        reassign(cars, tyres, listed, g_session, vehicle, driver);
        held = listed;
        held_listed = true;
        retry("reassigned");
    }
    if (again == kNoCompound && held_listed && !t::census(held).total && refill(cars, held, "no compound found") > 0)
        retry("refilled");
    if (again == kNoCompound && held_listed && t::clear_fresh_wear(held) > 0)
        retry("fresh wear cleared");
    if (again == kNoCompound && held_listed && t::untag_fresh_sets(held) > 0)
        retry("fresh sets untagged");
    int forced = -1;
    if (again == kNoCompound && held_listed && (forced = t::fresh_slot(held, preference)) >= 0) {
        again = uint32_t(forced);
        done += done.empty() ? "forced" : ", forced";
    }
    if (report)
        swofty::log("[f1-1990] tyres: session %u: V%02u (driver %04x, %s) found no compound in allocation %s (weekend "
                    "list %04x): %s; %s; %s; result: %s%d",
                    g_session, vehicle, key(driver), entrant ? "entrant" : "no entrant", used,
                    listed ? key(at<uint32_t>(listed, 0)) : 0xffff, text, why, done.empty() ? "nothing to change" : done.c_str(),
                    forced >= 0 ? "forced slot " : "", again == kNoCompound ? -1 : int(again));
    return again;
}

namespace {

void on_unloaded(const SwoftyEvent *, void *)
{
    std::lock_guard lock(g_lock);
    g_component = nullptr;
    g_1990 = false;
}

}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] FQ tyres loaded: Friday qualifying dry sets %s, tyre fallback %s",
                fq_tyre_fix ? "on" : "off (fq_tyre_fix)", tyre_fallback ? "on" : "off (tyre_fallback)");
    uint32_t token = 0;
    if (swofty::events::subscribe(SWOFTY_EVENT_CAREER_UNLOADED, on_unloaded, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] tyres: no career.unloaded event; the fallback keeps the last weekend's allocations");
}

SWOFTY_ADDON()
