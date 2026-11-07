#pragma once

#include <cstdint>
#include <cstring>
#include <string>

namespace runtime::entitlements {

enum State : int { UNKNOWN = 0, OWNED = 1, NOT_OWNED = 2 };

constexpr const char *F1_2026 = "f1_2026";
constexpr uint64_t CTX_ENTITIES = 0x228;
constexpr uint64_t CTX_UNLOCKS = 0x238;
constexpr uint64_t ENTITIES_LISTS_BEGIN = 0x560;
constexpr uint64_t ENTITIES_LISTS_END = 0x568;
// The check collapses "locked" and "not loaded yet" into one 0: an item's state in the unlock system is 0
// from registration until its unlock task completes, and no pending state or loaded flag was found. So
// not_owned needs a long unbroken run of zeros (about a minute of flow ticks) once the context is wired.
constexpr int NOT_OWNED_AFTER = 3600;

inline bool tracked(const char *name)
{
    return name && std::strcmp(name, F1_2026) == 0;
}

template <class Mem> bool ctx_ready(Mem &mem, uint64_t ctx)
{
    uint64_t entities = 0, unlocks = 0, begin = 0, end = 0;
    if (!ctx || !mem.u64(ctx + CTX_ENTITIES, entities) || !entities)
        return false;
    if (!mem.u64(ctx + CTX_UNLOCKS, unlocks) || !unlocks)
        return false;
    if (!mem.u64(entities + ENTITIES_LISTS_BEGIN, begin) || !mem.u64(entities + ENTITIES_LISTS_END, end))
        return false;
    return begin && end > begin;
}

class Settle {
    int state_ = UNKNOWN;
    int zeros_ = 0;

public:
    int state() const { return state_; }

    bool read(bool usable)
    {
        if (state_ == OWNED)
            return false;
        if (usable) {
            state_ = OWNED;
            return true;
        }
        if (state_ == NOT_OWNED)
            return false;
        if (++zeros_ >= NOT_OWNED_AFTER) {
            state_ = NOT_OWNED;
            return true;
        }
        return false;
    }
};

inline std::string json(int state)
{
    if (state == UNKNOWN)
        return {};
    return std::string("{\"") + F1_2026 + "\": " + (state == OWNED ? "true" : "false") + "}\n";
}

inline const char *word(int state)
{
    return state == OWNED ? "owned" : state == NOT_OWNED ? "not owned" : "unknown";
}

}
