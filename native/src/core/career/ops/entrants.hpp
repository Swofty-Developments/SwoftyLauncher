#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

// The Driver Career entrant rows as career_ops.cpp's operations read and write them: the bindings they resolve, the
// flow thread they run on, one write per row through Career_Set_EntrantData_Thunk, the contracts behind the seats and
// the one finalise of a batch (entrants.cpp).
namespace career::ops::live {

namespace g = swofty::game;
namespace mem = swofty::mem;

using g::EntrantRecord;
using g::EntrantWriteData;

constexpr uint32_t kNoKey = 0x3fff0000u;
constexpr uint32_t kHumanBit = 0x10000000u;
constexpr uint8_t kNoNumber = 0xff;
constexpr size_t kMaxEntrants = 256;
constexpr size_t kIdentitySize = offsetof(EntrantRecord, id_lo);
constexpr uint8_t kReasonSigned = 0;
constexpr uint8_t kReasonReleased = 1;
constexpr uintptr_t kVtWeekendIndex = 0x108;
constexpr uintptr_t kVtRegistryValue = 0x108;
constexpr uint32_t kMinutesPerDay = 1440;
constexpr uint8_t kContractReleased = 3;
constexpr uint8_t kContractRenewed = 4;
constexpr uint8_t kContractLeft = 5;
constexpr size_t kContractSlot = 16;
constexpr size_t kContractRecord = offsetof(g::DriverContract, active);
static_assert(sizeof(EntrantRecord) == 0x88 && sizeof(EntrantWriteData) == 0x2c);
static_assert(kContractRecord == 0x2c && sizeof(g::DriverContractChangedMessage) == 0x78);

using WeekendFn = uint32_t (*)(void *self);
using RegistryValueFn = uint32_t (*)(void *registry);

// Every binding an operation resolved, kept once it resolved or once the code reads decrypted.
struct Bindings {
    std::mutex lock;
    std::unordered_map<std::string, uintptr_t> bound;
};

extern Bindings bindings;
// The thread that last ran the frame callback; an operation runs on it or before the first frame.
extern std::atomic<uint32_t> flow_tid;

uintptr_t bind(const char *id);

struct Needs {
    std::string missing;
    bool ok(std::initializer_list<const char *> ids)
    {
        for (const char *id : ids)
            if (!bind(id))
                missing += (missing.empty() ? "" : ", ") + std::string(id);
        return missing.empty();
    }
};

template <class T> T rdv(uintptr_t a, T def = T{})
{
    T v{};
    return mem::read(a, v) ? v : def;
}

struct Row {
    uintptr_t at = 0;
    EntrantRecord rec{};
};

inline uint32_t stat_of(uint32_t key) { return key >> 16; }
inline bool none(uint32_t key) { return (key & 0xffff0000u) == kNoKey; }
inline bool has_car(const EntrantRecord &r) { return !none(r.vehicle_ref); }
inline bool teamless(const EntrantRecord &r) { return none(r.team_stat_id); }
inline bool human(const EntrantRecord &r) { return (r.ordinal_flags & kHumanBit) != 0; }
inline int position_of(const EntrantRecord &r) { return (r.team_position >> 8) ? (r.team_position & 0xff) : -1; }

struct Signing {
    uint32_t driver = 0;
    uintptr_t contract = 0;
    uint8_t offer[kContractRecord] = {};
};

class Baseline {
public:
    explicit Baseline(const std::vector<Row> &rows)
    {
        for (auto &r : rows)
            rows_.push_back(r.rec);
        vec_.begin = rows_.data();
        vec_.end = rows_.data() + rows_.size();
        vec_.capacity = vec_.end;
    }
    Baseline(const Baseline &) = delete;
    Baseline &operator=(const Baseline &) = delete;
    const void *vector() const { return &vec_; }

private:
    struct Vec {
        void *allocator = nullptr;
        const EntrantRecord *begin = nullptr;
        const EntrantRecord *end = nullptr;
        const EntrantRecord *capacity = nullptr;
    };
    std::vector<EntrantRecord> rows_;
    Vec vec_;
};

bool lineup_bound(std::string &missing);
bool on_flow_thread();
void frame(double, void *);
bool writable(uintptr_t p, size_t n);
int post(void *msg);
uintptr_t component(const char *desc_id);
uintptr_t entity();
uint32_t driver_of(const EntrantRecord &r);
bool entrant_rows(uintptr_t e, std::vector<Row> &out);
bool entity_busy(uintptr_t e);
bool unique_identity(uintptr_t e, const Row &id_row);
EntrantWriteData seat_data(const Row &driver_row, const Row &seat_row);
EntrantWriteData own_data(const Row &r);
EntrantWriteData released_data(const Row &r);
bool announce(uintptr_t manager, uint32_t driver, uint32_t old_team, uint32_t new_team, uint8_t reason);
bool write_row(uintptr_t e, const Row &id_row, const EntrantWriteData &data, uint16_t position, uint8_t number);
bool same_key(uint32_t a, uint32_t b);
bool contract_clock(uintptr_t e, uintptr_t manager, uint16_t &day, uint32_t &value);
bool contract_objects(uintptr_t manager, std::vector<uintptr_t> &out);
uintptr_t contract_of(const std::vector<uintptr_t> &objects, uint32_t driver);
bool contract_fits(uintptr_t contract, uint32_t team);
int32_t recommended_target(uintptr_t manager, int32_t base);
const char *plan_signing(uintptr_t manager, const std::vector<uintptr_t> &objects, uint32_t driver, uint32_t team, Signing &s);
bool sign_contract(uintptr_t e, uintptr_t manager, const Signing &s);
bool release_contract(uintptr_t e, uintptr_t manager, uint32_t driver, bool &found);
uintptr_t car_numbers(uintptr_t e, uintptr_t manager);
bool finalise(uintptr_t e, const Baseline &baseline, uint64_t &changes);

}
