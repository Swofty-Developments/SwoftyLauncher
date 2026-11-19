#pragma once

#include "core/apply/content/lower.hpp"

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <optional>
#include <string>
#include <utility>
#include <vector>

// A box's fuel rules (content.package rules.refuelling, race_fuel_kg; 1.5 refuel_stint_kg, quali_fuel_kg,
// race_start_fuel_from_quali). The game has no refuel rule and its pit stop has no fuel service (no "refuel" string in
// F1_25.exe; the only refills are the garage refuel 0x1436ae7b0 and the FuelTank Reset flag 0x40), so the core plays
// the rule itself: it writes the FuelTank's fuel_mass (CarLive+0x540 -> +0x100, kg, the car_live field fuel_mass,
// clamped to FuelCapacity +0x98) from the flow thread. The pit stop trigger is the PitLaneCarState pit_stops counter
// (+0x14, L2) the live snapshot already reads; no new hook. Pure, for the host tests.

namespace swofty_core::apply::content {

struct FuelRules {
    std::string package;
    bool refuelling = false;
    std::optional<float> race_fuel_kg, refuel_stint_kg, quali_fuel_kg;
    bool start_from_quali = false;
    bool race_tyre_change = true; // 1.5 rules.race_tyre_change: false keeps a car's tyres through its race pit stops
    std::string line;

    bool any() const { return refuelling || start_from_quali || quali_fuel_kg.has_value(); }
    bool tyres_kept() const { return !race_tyre_change; }
};

inline std::optional<float> fuel_kg(const Value &v)
{
    if (!v.is_number())
        return std::nullopt;
    double d = v.as_double();
    return d >= 0 && d < 1000 ? std::optional<float>(float(d)) : std::nullopt;
}

inline std::string kg_text(float kg)
{
    char b[32];
    std::snprintf(b, sizeof b, "%.1f kg", kg);
    return b;
}

// The fuel rules of the box whose entrant list was pressed: the last content.package (in the order given) with that
// entrant list.
inline FuelRules fuel_rules_pick(const std::string &entrant_list,
                                 const std::vector<std::pair<std::string, const Value *>> &packages)
{
    FuelRules r;
    const Value *found = nullptr;
    for (auto &[id, rec] : packages)
        if (rec && (*rec)["entrant_list"].is_string() && (*rec)["entrant_list"].as_string() == entrant_list) {
            r.package = id;
            found = rec;
        }
    if (!found) {
        r.line = "box " + entrant_list + ": no content.package; the game's own fuel";
        return r;
    }
    const Value &rules = (*found)["rules"];
    r.refuelling = rules["refuelling"].is_bool() && rules["refuelling"].as_bool();
    r.race_fuel_kg = fuel_kg(rules["race_fuel_kg"]);
    r.refuel_stint_kg = fuel_kg(rules["refuel_stint_kg"]);
    r.quali_fuel_kg = fuel_kg(rules["quali_fuel_kg"]);
    r.start_from_quali = rules["race_start_fuel_from_quali"].is_bool() && rules["race_start_fuel_from_quali"].as_bool();
    r.race_tyre_change = !(rules["race_tyre_change"].is_bool() && !rules["race_tyre_change"].as_bool());
    std::string head = "box " + entrant_list + " (" + r.package + "): ";
    if (!r.any() && !r.tyres_kept()) {
        r.line = head + "no fuel rule; the game's own fuel";
        return r;
    }
    std::string parts;
    auto add = [&](const std::string &s) { parts += (parts.empty() ? "" : ", ") + s; };
    if (r.quali_fuel_kg)
        add("qualifying starts on " + kg_text(*r.quali_fuel_kg));
    if (r.start_from_quali)
        add("race starts on the fuel left after qualifying");
    else if (r.refuelling && r.race_fuel_kg)
        add("race starts on " + kg_text(*r.race_fuel_kg));
    if (r.refuelling) {
        std::optional<float> stint = r.refuel_stint_kg ? r.refuel_stint_kg : r.race_fuel_kg;
        add("a pit stop refuels to " + (stint ? kg_text(*stint) : std::string("a full tank")));
    }
    if (r.tyres_kept())
        add("race pit stops keep the tyres (a change only for a puncture)");
    r.line = head + parts;
    return r;
}

// rules.race_tyre_change false (2005: one set for qualifying and race): the core wraps TyreComponent::changeToNextCompound
// (Car_Tyre_ChangeToNextCompound 0x142e94040, log PITSTOP_TYRES 'PITLANE CHANGEOVER'). Its pit stop caller
// Game_PitLane_ApplyPitstopTyres 0x1434bdd60 calls it with flag 1 and ignores its result (always 0); the garage actor
// (Game_PitBoxCarActor_Update) calls it with flag 0. In a race session a flag-1 call is skipped, so the fitted compound,
// its wear and temperatures stay; pitstopComplete still runs. A car with a punctured tyre keeps the game's change.
//
// The puncture state is CarWheel.tyre_flags (+0x20) of each of the car's four wheels: VehicleTyreComponent.physics
// (+0x20), CarPhysicsRecord.car (+0x58, the CarLive), CarLive.suspension (+0x580), CarSuspension.wheels (+0x70 RL,
// +0x78 RR, +0x80 FL, +0x88 FR), the chain 0x142e92f70 and 0x142ea2300 walk. Bit 1 is a puncture (every set goes through
// the tyre damage monitor's Car_Puncture_Add 0x14464e200, every clear through Car_Puncture_Remove 0x14464e3a0); bit 0 a
// burst, set only while bit 1 is (the tyre update 0x1446b5720 once the deflation +0x98 reaches 2.0, or 0x144656f80); bits
// 2 and 3 are pending hits. The pit stop's own change resets every wheel (0x142ea2300 -> 0x142e88840 -> the tyre reset
// 0x1446b6d10 with flags 0x1002, reset bit 0x1000: bit 0 cleared, a puncture removed, {0, 0, 1.0, 0} stored at
// +0x88..+0x94 and 0 at +0x98) and then adds a puncture back only where the fitted set has one, so the flags are read
// before the game's change runs (Game_PitLane_ApplyPitstopTyres calls Car_Tyre_ChangeToNextCompound first, and
// 0x142ea2300 is its last call). Dword +0x208 of the TyreComponent's
// tyre records is the tyre model's visibility mask (m_tyreModelVisibility, set per shown part by setTyreVisibility
// 0x142e83130), not a damage state.
constexpr uint64_t TYRE_PHYSICS = 0x20, PHYSICS_CAR = 0x58, CAR_SUSPENSION = 0x580, WHEEL_TYRE_FLAGS = 0x20;
constexpr uint64_t SUSPENSION_WHEELS[4] = {0x70, 0x78, 0x80, 0x88};
constexpr uint8_t WHEEL_PUNCTURED = 0x03;
inline constexpr const char *WHEEL_NAMES[4] = {"RL", "RR", "FL", "FR"};

// The punctured wheels of the TyreComponent's car, bit i for SUSPENSION_WHEELS[i]. `read(address, out)` copies an
// object of out's type from the game; a wheel whose state can't be read counts as not punctured.
template <class Read> uint8_t punctured_wheels(uint64_t tyre_component, Read read)
{
    uint64_t physics = 0, car = 0, suspension = 0;
    if (!read(tyre_component + TYRE_PHYSICS, physics) || !physics || !read(physics + PHYSICS_CAR, car) || !car ||
        !read(car + CAR_SUSPENSION, suspension) || !suspension)
        return 0;
    uint8_t out = 0;
    for (size_t i = 0; i < 4; i++) {
        uint64_t wheel = 0;
        uint8_t flags = 0;
        if (read(suspension + SUSPENSION_WHEELS[i], wheel) && wheel && read(wheel + WHEEL_TYRE_FLAGS, flags) &&
            (flags & WHEEL_PUNCTURED))
            out |= uint8_t(1u << i);
    }
    return out;
}

// The wheels of a punctured_wheels mask, for the log ("RL, FR").
inline std::string wheel_names(uint8_t wheels)
{
    std::string out;
    for (size_t i = 0; i < 4; i++)
        if (wheels & (1u << i))
            out += (out.empty() ? "" : ", ") + std::string(WHEEL_NAMES[i]);
    return out;
}

enum class TyreChange { game, kept, punctured };

// `punctured` is punctured_wheels' mask, 0 on a build without a witness for the wheel state.
inline TyreChange race_tyre_change_pick(bool kept_rule, uint32_t session_type, bool pit_stop_call, uint8_t punctured)
{
    if (!kept_rule || !pit_stop_call || !(session_type >= 15 && session_type <= 17))
        return TyreChange::game;
    return punctured ? TyreChange::punctured : TyreChange::kept;
}

// One car as the fuel plan sees it in a frame. pit_stops_ok: the live snapshot read the car's pit stop counter.
struct FuelCar {
    bool present = false;
    bool pit_stops_ok = false;
    uint8_t pit_stops = 0;
    int32_t lap = 0;
    float fuel = 0, capacity = 0;
};

struct FuelWrite {
    uint32_t car;
    float kg;
    const char *why;
};

// The game's session type numbers (swofty/hud_format.hpp): 5..14 qualifying (incl. sprint), 15..17 race.
inline bool fuel_quali_session(uint32_t t) { return t >= 5 && t <= 14; }
inline bool fuel_race_session(uint32_t t) { return t >= 15 && t <= 17; }

// Per-frame plan. A new session (type or generation change) starts over; the qualifying fuel each car had last is
// kept for the next race while the box does not change. Start fuel is re-asserted while the car has not burned any of
// it yet (the start sequence may load its own fuel after the session loaded); a pit stop counter step refills.
class FuelPlan {
public:
    static constexpr uint32_t MAX = 32;
    static constexpr float SLACK = 0.5f; // kg the game may differ from a written start fuel before it is written again

    // Box change: forgets everything, including the qualifying fuel.
    void reset() { *this = FuelPlan{}; }

    std::vector<FuelWrite> tick(const FuelRules &r, uint32_t type, uint32_t generation, const FuelCar *cars, uint32_t n)
    {
        std::vector<FuelWrite> out;
        if (!r.any())
            return out;
        if (!seen_ || type != type_ || generation != generation_) {
            seen_ = true;
            type_ = type;
            generation_ = generation;
            bool new_quali = fuel_quali_session(type);
            for (auto &c : car_) {
                Car fresh;
                fresh.quali_left = c.quali_left;
                fresh.quali_ok = c.quali_ok && !new_quali;
                c = fresh;
            }
        }
        bool quali = fuel_quali_session(type), race = fuel_race_session(type);
        if (!quali && !race)
            return out;
        n = n < MAX ? n : MAX;
        for (uint32_t i = 0; i < n; i++) {
            const FuelCar &f = cars[i];
            Car &c = car_[i];
            if (!f.present || !(f.capacity > 0))
                continue;
            std::optional<float> start;
            const char *why = nullptr;
            if (quali && r.quali_fuel_kg) {
                start = *r.quali_fuel_kg;
                why = "qualifying fuel";
            } else if (race && r.start_from_quali) {
                start = c.quali_ok ? std::optional<float>(c.quali_left) : r.quali_fuel_kg;
                why = c.quali_ok ? "parc ferme: fuel left after qualifying" : "parc ferme: no qualifying run, qualifying fuel";
            } else if (race && r.refuelling && r.race_fuel_kg) {
                start = *r.race_fuel_kg;
                why = "race start fuel";
            }
            if (start && !c.start_done)
                start_step(c, i, f, std::min(*start, f.capacity), why, out);
            if (quali && (!start || c.written)) { // the fuel left at the end of this qualifying session
                c.quali_left = f.fuel;
                c.quali_ok = true;
            }
            if (c.refuel_pending) {
                c.refuel_pending = false;
                c.last_refuel = true;
                out.push_back({i, c.refuel_kg, "pit stop refuel (retry)"});
                continue;
            }
            if (race && r.refuelling && f.pit_stops_ok) {
                if (!c.stops_seen) {
                    c.stops_seen = true;
                    c.stops = f.pit_stops;
                } else if (f.pit_stops != c.stops) {
                    bool more = uint8_t(f.pit_stops - c.stops) < 128;
                    c.stops = f.pit_stops;
                    std::optional<float> stint = r.refuel_stint_kg ? r.refuel_stint_kg : r.race_fuel_kg;
                    float want = std::min(stint ? *stint : f.capacity, f.capacity);
                    if (more && want > f.fuel) {
                        out.push_back({i, want, "pit stop refuel"});
                        c.start_done = true;
                        c.last_refuel = true;
                        c.refuel_kg = want;
                    }
                }
            }
        }
        return out;
    }

    // The caller could not write the fuel planned for this car in the last tick: try again next frame.
    void failed(uint32_t car)
    {
        if (car >= MAX)
            return;
        Car &c = car_[car];
        if (c.last_refuel)
            c.refuel_pending = true;
        else if (!c.start_done)
            c.written = false;
    }

    bool quali_known(uint32_t car, float &kg) const
    {
        if (car >= MAX || !car_[car].quali_ok)
            return false;
        kg = car_[car].quali_left;
        return true;
    }

private:
    struct Car {
        bool written = false, start_done = false, stops_seen = false;
        float written_kg = 0;
        uint8_t stops = 0;
        float quali_left = 0;
        bool quali_ok = false;
        bool last_refuel = false, refuel_pending = false;
        float refuel_kg = 0;
    };

    // Writes the start fuel until the car burns some of it, its lap passes 1 or it stops; re-writes when the game
    // loaded its own fuel since (more than SLACK away from what was written).
    static void start_step(Car &c, uint32_t i, const FuelCar &f, float kg, const char *why, std::vector<FuelWrite> &out)
    {
        if (f.lap > 1 || (c.stops_seen && f.pit_stops != c.stops)) {
            c.start_done = true;
            return;
        }
        c.last_refuel = false;
        if (!c.written) {
            c.written = true;
            c.written_kg = kg;
            out.push_back({i, kg, why});
            return;
        }
        if (f.fuel < c.written_kg - SLACK) {
            c.start_done = true; // burning: the start fuel held
            return;
        }
        if (f.fuel > c.written_kg + SLACK) {
            c.written_kg = kg;
            out.push_back({i, kg, why});
        }
    }

    bool seen_ = false;
    uint32_t type_ = 0, generation_ = 0;
    Car car_[MAX];
};

}
