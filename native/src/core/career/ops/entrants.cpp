#include "core/career/ops/entrants.hpp"
#include "core/career/ops/finalise_chunks.hpp"

#include "core/runtime/runtime.hpp"

#include <algorithm>

namespace career::ops::live {

uintptr_t bind(const char *id)
{
    std::lock_guard<std::mutex> l(bindings.lock);
    auto it = bindings.bound.find(id);
    if (it != bindings.bound.end())
        return it->second;
    uintptr_t a = swofty::api && swofty::api->resolve ? swofty::api->resolve(id) : 0;
    if (a || runtime::code_ready())
        bindings.bound[id] = a;
    return a;
}

constexpr const char *kLineupBindings[] = {"Career_Set_EntrantData_Thunk",
                                           "vt_Career_MessageDriverMoveConfirmed",
                                           "Career_Set_DriverContractState",
                                           "Career_Build_DriverContract",
                                           "vt_Career_MessageDriverContractChanged",
                                           "Career_Build_LineupChanges",
                                           "vt_Career_MessageDriverMovesFinalisedUpgrades",
                                           "vt_Career_MessageDriverMovesFinalisedRecognition",
                                           "vt_Career_MessageDriverMovesFinalised",
                                           "Career_DriverNumbers_CheckExisting",
                                           "k_Career_DriverCareerContractsManagerComponentDesc"};

bool lineup_bound(std::string &missing)
{
    Needs n;
    for (const char *id : kLineupBindings)
        n.ok({id});
    missing = n.missing;
    return missing.empty();
}

bool on_flow_thread()
{
    uint32_t t = flow_tid.load();
    return !t || t == GetCurrentThreadId();
}

void frame(double, void *) { flow_tid = GetCurrentThreadId(); }

bool writable(uintptr_t p, size_t n) { return runtime::pages::writable_in_region(p, n); }

int post(void *msg)
{
    const SwoftyCareer *c = runtime::career_table();
    return c && c->post ? c->post(msg) : SWOFTY_ASSET_UNAVAILABLE;
}

uintptr_t component(const char *desc_id)
{
    const SwoftyCareer *c = runtime::career_table();
    uintptr_t desc = bind(desc_id);
    auto p =
        desc && c && c->component_by_desc ? reinterpret_cast<uintptr_t>(c->component_by_desc(reinterpret_cast<void *>(desc))) : 0;
    return p && mem::object_like(p) ? p : 0;
}

uintptr_t entity()
{
    const SwoftyCareer *c = runtime::career_table();
    auto e = c && c->entity ? reinterpret_cast<uintptr_t>(c->entity()) : 0;
    g::CareerEntity ent{};
    if (!e || !mem::object_like(e) || !mem::read(e, ent) || !mem::in_image(reinterpret_cast<uintptr_t>(ent.component_registry)))
        return 0;
    return e;
}

uint32_t driver_of(const EntrantRecord &r)
{
    uint32_t v;
    std::memcpy(&v, &r.id_lo, sizeof v);
    return v;
}

bool entrant_rows(uintptr_t e, std::vector<Row> &out)
{
    out.clear();
    uintptr_t b = rdv<uintptr_t>(e + offsetof(g::CareerEntity, entrants_begin));
    uintptr_t en = rdv<uintptr_t>(e + offsetof(g::CareerEntity, entrants_end));
    if (!e || !b || en <= b || (en - b) % sizeof(EntrantRecord) || (en - b) / sizeof(EntrantRecord) > kMaxEntrants ||
        !mem::readable(b, en - b))
        return false;
    size_t n = (en - b) / sizeof(EntrantRecord);
    out.resize(n);
    for (size_t i = 0; i < n; ++i) {
        out[i].at = b + i * sizeof(EntrantRecord);
        std::memcpy(&out[i].rec, reinterpret_cast<const void *>(out[i].at), sizeof(EntrantRecord));
    }
    return true;
}

bool entity_busy(uintptr_t e) { return rdv<uint8_t>(e + offsetof(g::CareerEntity, entrant_update_busy)) != 0; }

bool unique_identity(uintptr_t e, const Row &id_row)
{
    std::vector<Row> rows;
    if (!entrant_rows(e, rows))
        return false;
    uint32_t id = id_row.rec.ordinal_flags;
    bool want_human = id & kHumanBit;
    size_t hits = 0;
    uintptr_t at = 0;
    for (auto &r : rows) {
        uint32_t v = r.rec.ordinal_flags;
        bool match = want_human ? (v & kHumanBit) != 0 : !(v & kHumanBit) && v == id;
        if (match) {
            ++hits;
            at = r.at;
        }
    }
    return hits == 1 && at == id_row.at;
}

EntrantWriteData seat_data(const Row &driver_row, const Row &seat_row)
{
    EntrantWriteData d{};
    d.driver = driver_of(driver_row.rec);
    d.driver_customisation = driver_row.rec.driver_customisation;
    d.team_stat_id = seat_row.rec.team_stat_id;
    d.vehicle_ref = seat_row.rec.vehicle_ref;
    d.driver_reg_value = seat_row.rec.driver_reg_value;
    d.vehicle_customisation = seat_row.rec.vehicle_customisation;
    d.badge_stat_id = seat_row.rec.badge_stat_id;
    d.seat_value = seat_row.rec.const_ffff0012;
    return d;
}

EntrantWriteData own_data(const Row &r) { return seat_data(r, r); }

EntrantWriteData released_data(const Row &r)
{
    EntrantWriteData d{};
    d.driver = driver_of(r.rec);
    d.driver_customisation = r.rec.driver_customisation;
    d.team_stat_id = kNoKey;
    d.vehicle_ref = kNoKey;
    d.driver_reg_value = kNoKey;
    d.vehicle_customisation = kNoKey;
    d.badge_stat_id = kNoKey;
    d.seat_value = 0xffff0012u;
    return d;
}

bool announce(uintptr_t manager, uint32_t driver, uint32_t old_team, uint32_t new_team, uint8_t reason)
{
    using M = g::DriverCareerContractsManagerComponent;
    uintptr_t vt_msg = bind("vt_Career_MessageDriverMoveConfirmed");
    if (!manager || !vt_msg)
        return false;
    uintptr_t src = rdv<uintptr_t>(manager + offsetof(M, weekend_source));
    uintptr_t time = rdv<uintptr_t>(manager + offsetof(M, career_time));
    uintptr_t vt = src ? rdv<uintptr_t>(src) : 0;
    uintptr_t fn = vt ? rdv<uintptr_t>(vt + kVtWeekendIndex) : 0;
    if (!mem::in_image(fn) || !time)
        return false;
    g::DriverMoveConfirmedMessage m{};
    m.vtable = reinterpret_cast<void **>(vt_msg);
    m.move.driver = driver;
    m.move.old_team = old_team;
    m.move.new_team = new_team;
    m.move.reason = reason;
    m.move.weekend_index = reinterpret_cast<WeekendFn>(fn)(reinterpret_cast<void *>(src));
    m.move.day_index =
        static_cast<uint16_t>(rdv<uint32_t>(time + offsetof(g::CareerTimeComponent, current_minutes)) / kMinutesPerDay);
    return post(&m) == SWOFTY_ASSET_OK;
}

bool write_row(uintptr_t e, const Row &id_row, const EntrantWriteData &data, uint16_t position, uint8_t number)
{
    auto set = reinterpret_cast<g::Career_Set_EntrantData_Thunk_t *>(bind("Career_Set_EntrantData_Thunk"));
    if (!set || entity_busy(e) || !unique_identity(e, id_row))
        return false;
    uint8_t id[kIdentitySize];
    std::memcpy(id, &id_row.rec, kIdentitySize);
    set(reinterpret_cast<void *>(e), id, reinterpret_cast<const uint32_t *>(&data), position, number);
    return !entity_busy(e);
}

bool same_key(uint32_t a, uint32_t b) { return ((a ^ b) & 0xffff00ffu) == 0; }

bool contract_clock(uintptr_t e, uintptr_t manager, uint16_t &day, uint32_t &value)
{
    uintptr_t time = rdv<uintptr_t>(manager + offsetof(g::DriverCareerContractsManagerComponent, career_time));
    uintptr_t registry = e + offsetof(g::CareerEntity, component_registry);
    uintptr_t vt = rdv<uintptr_t>(registry);
    uintptr_t fn = vt ? rdv<uintptr_t>(vt + kVtRegistryValue) : 0;
    if (!time || !mem::in_image(fn))
        return false;
    value = reinterpret_cast<RegistryValueFn>(fn)(reinterpret_cast<void *>(registry));
    day = static_cast<uint16_t>(rdv<uint32_t>(time + offsetof(g::CareerTimeComponent, current_minutes)) / kMinutesPerDay);
    return true;
}

bool contract_objects(uintptr_t manager, std::vector<uintptr_t> &out)
{
    out.clear();
    uintptr_t owner = manager ? rdv<uintptr_t>(manager + offsetof(g::DriverCareerContractsManagerComponent, contract_owner)) : 0;
    uintptr_t b = owner ? rdv<uintptr_t>(owner + offsetof(g::DriverCareerContractOwner, contracts_begin)) : 0;
    uintptr_t en = owner ? rdv<uintptr_t>(owner + offsetof(g::DriverCareerContractOwner, contracts_end)) : 0;
    if (!b || en < b || (en - b) % kContractSlot || (en - b) / kContractSlot > 4096 || !mem::readable(b, en - b))
        return false;
    for (uintptr_t p = b; p < en; p += kContractSlot)
        if (uintptr_t c = rdv<uintptr_t>(p + sizeof(void *)); c && mem::readable(c, sizeof(g::DriverContract)))
            out.push_back(c);
    return true;
}

uintptr_t contract_of(const std::vector<uintptr_t> &objects, uint32_t driver)
{
    for (uintptr_t c : objects)
        if (same_key(rdv<uint32_t>(c + offsetof(g::DriverContract, driver)), driver))
            return c;
    return 0;
}

bool contract_fits(uintptr_t contract, uint32_t team)
{
    return contract && rdv<uint8_t>(contract + offsetof(g::DriverContract, active)) &&
           same_key(rdv<uint32_t>(contract + offsetof(g::DriverContract, team)), team);
}

int32_t recommended_target(uintptr_t manager, int32_t base)
{
    struct Range {
        uint32_t min, max, minus, plus;
        int32_t recommended;
    };
    static_assert(sizeof(Range) == 20);
    constexpr uintptr_t kRatingRanges = 0x318;
    if (base < 0)
        base = 0;
    uintptr_t b = rdv<uintptr_t>(manager + kRatingRanges), e = rdv<uintptr_t>(manager + kRatingRanges + 8);
    if (b && e > b && (e - b) % sizeof(Range) == 0 && (e - b) / sizeof(Range) < 64 && mem::readable(b, e - b))
        for (uintptr_t p = b; p < e; p += sizeof(Range)) {
            Range r = rdv<Range>(p);
            if (uint32_t(base) < r.min || uint32_t(base) > r.max)
                continue;
            int64_t lo = std::max<int64_t>(0, int64_t(base) - r.minus);
            int64_t hi = std::min<int64_t>(100, int64_t(base) + r.plus);
            return int32_t(std::clamp<int64_t>(int64_t(base) + r.recommended, std::min(lo, hi), hi));
        }
    return std::min(base, 100);
}

const char *plan_signing(uintptr_t manager, const std::vector<uintptr_t> &objects, uint32_t driver, uint32_t team, Signing &s)
{
    constexpr uintptr_t kBuiltRating = 0x18;
    auto build = reinterpret_cast<g::Career_Build_DriverContract_t *>(bind("Career_Build_DriverContract"));
    s.driver = driver;
    s.contract = contract_of(objects, driver);
    if (!s.contract)
        return "the game keeps no contract for him";
    if (!writable(s.contract, offsetof(g::DriverContract, active) + 1))
        return "his contract is not writable";
    if (!manager || !build)
        return "no contracts manager to build the terms";
    alignas(16) uint8_t buf[0x80] = {};
    build(reinterpret_cast<void *>(manager), buf, driver, team, 0, 0, 1);
    int32_t base;
    std::memcpy(&base, buf + kBuiltRating, sizeof base);
    int32_t target = recommended_target(manager, base);
    std::memset(buf, 0, sizeof buf);
    build(reinterpret_cast<void *>(manager), buf, driver, team, 0, target, 1);
    uint32_t built;
    std::memcpy(&built, buf + offsetof(g::DriverContract, team), sizeof built);
    if (!same_key(built, team))
        return "the built terms name another team";
    std::memcpy(s.offer, buf, sizeof s.offer);
    return nullptr;
}

bool sign_contract(uintptr_t e, uintptr_t manager, const Signing &s)
{
    using C = g::DriverContract;
    auto set_state = reinterpret_cast<g::Career_Set_DriverContractState_t *>(bind("Career_Set_DriverContractState"));
    uintptr_t vt_changed = bind("vt_Career_MessageDriverContractChanged");
    uint16_t day;
    uint32_t value;
    if (!set_state || !vt_changed || !contract_clock(e, manager, day, value) || !writable(s.contract, offsetof(C, active) + 1))
        return false;
    uint8_t offer[kContractRecord];
    std::memcpy(offer, s.offer, sizeof offer);
    std::memcpy(offer + offsetof(C, signed_day), &day, sizeof day);
    uint32_t team;
    std::memcpy(&team, offer + offsetof(C, team), sizeof team);
    bool same = same_key(rdv<uint32_t>(s.contract + offsetof(C, team)), team);
    set_state(reinterpret_cast<void *>(s.contract), day, same ? kContractRenewed : kContractLeft, value);
    if (rdv<uint8_t>(s.contract + offsetof(C, active)))
        return false;
    g::DriverContractChangedMessage m{};
    m.vtable = reinterpret_cast<void **>(vt_changed);
    m.driver = rdv<uint32_t>(s.contract + offsetof(C, driver));
    std::memcpy(m.before, reinterpret_cast<const void *>(s.contract), sizeof m.before);
    std::memcpy(reinterpret_cast<void *>(s.contract), offer, sizeof offer);
    *reinterpret_cast<uint8_t *>(s.contract + offsetof(C, active)) = 1;
    std::memcpy(m.after, offer, sizeof m.after);
    m.kind = offer[offsetof(C, kind)];
    return post(&m) == SWOFTY_ASSET_OK;
}

bool release_contract(uintptr_t e, uintptr_t manager, uint32_t driver, bool &found)
{
    found = false;
    auto set_state = reinterpret_cast<g::Career_Set_DriverContractState_t *>(bind("Career_Set_DriverContractState"));
    std::vector<uintptr_t> objects;
    uint16_t day;
    uint32_t value;
    if (!set_state || !contract_objects(manager, objects) || !contract_clock(e, manager, day, value))
        return false;
    uintptr_t contract = contract_of(objects, driver);
    if (!contract)
        return true;
    set_state(reinterpret_cast<void *>(contract), day, kContractReleased, value);
    found = true;
    return true;
}

uintptr_t car_numbers(uintptr_t e, uintptr_t manager)
{
    uintptr_t c = manager ? rdv<uintptr_t>(manager + offsetof(g::DriverCareerContractsManagerComponent, car_numbers)) : 0;
    return c && mem::object_like(c) && rdv<uintptr_t>(c + offsetof(g::CarNumberComponent, entity)) == e ? c : 0;
}

void Baseline::settle(const std::vector<Row> &now, uint32_t driver)
{
    const Row *current = nullptr;
    for (auto &r : now)
        if (same_key(driver_of(r.rec), driver)) {
            current = &r;
            break;
        }
    if (!current)
        return;
    for (auto &b : rows_)
        if (same_key(driver_of(b), driver)) {
            b = current->rec;
            return;
        }
    rows_.push_back(current->rec);
    rebind();
}

bool finalise(uintptr_t e, const Baseline &baseline, uint64_t &changes)
{
    changes = 0;
    auto build = reinterpret_cast<g::Career_Build_LineupChanges_t *>(bind("Career_Build_LineupChanges"));
    if (!build)
        return false;
    std::vector<Row> now;
    if (!entrant_rows(e, now))
        return false;
    uintptr_t current = e + offsetof(g::CareerEntity, entrants_begin) - sizeof(void *);
    uintptr_t vts[] = {bind("vt_Career_MessageDriverMovesFinalisedUpgrades"),
                       bind("vt_Career_MessageDriverMovesFinalisedRecognition"), bind("vt_Career_MessageDriverMovesFinalised")};
    for (uintptr_t v : vts)
        if (!v)
            return false;
    constexpr size_t kCapacity = sizeof(g::DriverMovesFinalisedMessage::changes) / sizeof(g::LineupChange);
    static_assert(kCapacity == chunks::kMessageCapacity);
    Baseline settled(baseline.rows());
    auto result = chunks::post_all(
        [&] {
            g::DriverMovesFinalisedMessage m{};
            build(&m.changes, reinterpret_cast<void *>(current), settled.vector());
            size_t n = static_cast<size_t>(std::min<uint64_t>(m.count, kCapacity));
            return std::vector<g::LineupChange>(m.changes, m.changes + n);
        },
        [&](const std::vector<g::LineupChange> &chunk) {
            g::DriverMovesFinalisedMessage m{};
            std::copy(chunk.begin(), chunk.end(), m.changes);
            m.count = chunk.size();
            for (uintptr_t v : vts) {
                g::DriverMovesFinalisedMessage copy = m;
                copy.vtable = reinterpret_cast<void **>(v);
                copy.reserved = 0;
                if (post(&copy) != SWOFTY_ASSET_OK)
                    return false;
            }
            return true;
        },
        [&](const std::vector<g::LineupChange> &chunk) {
            for (auto &c : chunk)
                settled.settle(now, c.driver);
        });
    changes = result.changes;
    return result.ok;
}

}
