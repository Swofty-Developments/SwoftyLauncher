// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#pragma once
#include "core/garage/steam_26566308.hpp"
#include "core/garage/garage_crew_pose.hpp"
#include "core/garage/garage_vehicle.hpp"
#include <swofty/mem.hpp>
#include <algorithm>
#include <array>
#include <atomic>
#include <cstring>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <windows.h>

// A garage bay's live placement edits (garage_placement.cpp tracks the bays and runs the visits), shared with the
// crew edits (garage_crew_edits.cpp), the mechanic edits (garage_member_edits.cpp) and the parked car edits
// (garage_car_edits.cpp).
namespace garage::edits {

namespace backend = garage::steam_26566308;
namespace sdk = swofty::garage_placement;
using R = sdk::Result;
using Play = uint64_t (*)(void *, const char *, int32_t, float, float);
using Update = void (*)(void *);
using Refresh = void (*)(void *, uint8_t);
using Node = uint32_t (*)(const char *);

extern Refresh refresh;
extern Node node;
extern garage::crew_pose::FindNode find_node;
extern std::atomic<unsigned> active_edits;

inline constexpr auto read_value = [](Address at, auto &out) { return swofty::mem::read(at, out); };
inline constexpr auto write_field = [](Address at, const auto &field) {
    MEMORY_BASIC_INFORMATION p{};
    if (!VirtualQuery(reinterpret_cast<void *>(at), &p, sizeof(p)) || p.State != MEM_COMMIT || (p.Protect & PAGE_GUARD) ||
        (p.Protect & 0xff) != PAGE_READWRITE || sizeof(field) > p.RegionSize - (at - reinterpret_cast<Address>(p.BaseAddress)))
        return false;
    std::memcpy(reinterpret_cast<void *>(at), &field, sizeof(field));
    return true;
};

struct SavedCrew {
    CrewPlacement placement;
    std::string track;
    backend::Bank bank;
};
struct Scope {
    std::string track;
    backend::Bank bank;
};
using CarStep = vehicle::Step;
using SavedCar = vehicle::Change;
struct Live {
    Owner owner;
    uint64_t generation;
    std::atomic<bool> idle{false};
    // Contending/reentrant callbacks skip this pass. Session unload only drops the
    // shared owner, and never waits for or dereferences a rig being torn down.
    std::mutex operation;
    std::atomic<bool> departure{false};
    std::optional<SavedCrew> crew;
    std::array<std::optional<members::Edit>, members::role_count> individual{};
    std::optional<SavedCar> vehicle;
    R car_result{R::unavailable};
    std::optional<Scope> edit_scope;
    bool counted{};
    bool has_individual() const
    {
        return std::any_of(individual.begin(), individual.end(), [](const auto &e) { return bool(e); });
    }
    void sync()
    {
        const bool extra = vehicle || has_individual();
        if (!extra)
            edit_scope.reset();
        const bool active = extra || crew;
        if (counted != active) {
            if (active)
                ++active_edits;
            else
                --active_edits;
            counted = active;
        }
    }
    void remember(SavedCrew saved)
    {
        crew = std::move(saved);
        sync();
    }
    void forget()
    {
        crew.reset();
        sync();
    }
    ~Live()
    {
        if (counted)
            --active_edits;
    }
};
using LivePtr = std::shared_ptr<Live>;
struct Visit {
    uint64_t ticket;
    LivePtr live;
    std::string track;
    backend::Bank bank;
};

bool current(const LivePtr &live);
bool same_track(const std::string &id);
bool scope(const LivePtr &live, const backend::Bank &bank);
Visit *context(uint64_t ticket);
bool page(Address at, bool code);
inline void copy(const Matrix &m, sdk::WorldMatrix &out) { std::copy(m.begin(), m.end(), out.values); }

R adjust(const LivePtr &live, const SavedCrew &saved, bool restore);
bool actor_pelvis(Address actor, sdk::Position &out);
R root(uint64_t t, sdk::WorldMatrix *out);
R car(uint64_t t, sdk::WorldMatrix *out);
R set_root(uint64_t t, const sdk::WorldMatrix *m);
R read_crew(uint64_t t, sdk::ActorRoots *out);
R read_pelvis(uint64_t t, sdk::CrewPositions *out);
R set_crew(uint64_t t, const sdk::Offsets *offsets);
R restore_crew(uint64_t t);

R restore_one(const LivePtr &live, members::Role role);
R list_members(uint64_t t, sdk::Mechanics *out);
R read_member(uint64_t t, members::Role role, sdk::WorldMatrix *out);
R read_member_pelvis(uint64_t t, members::Role role, sdk::Position *out);
R set_member(uint64_t t, members::Role role, const sdk::WorldMatrix *m);
R offset_member(uint64_t t, members::Role role, const sdk::Position *d);
R restore_member(uint64_t t, members::Role role);

R begin_car_restore(const LivePtr &live, bool rollback);
void check_car(const LivePtr &live);
R set_car(uint64_t t, const sdk::WorldMatrix *target);
R restore_car(uint64_t t);
R car_result(uint64_t t);

}
