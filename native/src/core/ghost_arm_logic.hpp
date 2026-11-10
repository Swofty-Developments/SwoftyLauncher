#pragma once

#include <cstdint>

// When the supplied trace may be written into a telemetry lap. Pure: the game thread feeds it the six lap states it reads
// and acts on the decision; it never blocks and never loops.
namespace runtime::ghost_arm {

constexpr int SLOTS = 6, MAX_ATTEMPTS = 3, LIVE_RECORDS = 50;
constexpr int64_t RETRY_MS = 2000;

struct SlotView {
    bool present = false, bound = false;
    uint32_t mode = 0, want = 0, error = 0, count = 0;
};

struct Views {
    SlotView s[SLOTS];
};

enum class Do { none, arm, restore };

struct Decision {
    Do what = Do::none;
    int slot = -1;
};

// The session is live once the player's lap (slot 0, bound to the player's car) is recording: mode 3 with samples. At
// session start every lap reads mode 0 and count 0, and nothing is written then.
inline bool live(const Views &v)
{
    const SlotView &p = v.s[0];
    return p.present && p.bound && p.mode == 3 && p.error == 0 && p.count >= uint32_t(LIVE_RECORDS);
}

inline int pick_slot(const Views &v, int env)
{
    if (env >= 0 && env < SLOTS)
        return env;
    return v.s[4].present && v.s[4].bound ? 4 : 3;
}

class Arming {
public:
    void component(uintptr_t c)
    {
        if (c == comp_)
            return;
        comp_ = c;
        armed_ = false;
        attempts_ = 0;
        retry_at_ = 0;
        slot_ = -1;
    }

    void request(uint64_t generation)
    {
        if (generation == gen_)
            return;
        gen_ = generation;
        if (generation)
            rearm_ = true;
    }

    bool armed() const { return armed_; }
    int slot() const { return slot_; }

    Decision decide(const Views &v, int env, int64_t now)
    {
        if (!gen_) {
            if (armed_ && slot_ == 3)
                return {Do::restore, 3};
            armed_ = false;
            return {};
        }
        if (armed_ && !rearm_)
            return {};
        if (attempts_ >= MAX_ATTEMPTS || now < retry_at_ || !live(v))
            return {};
        int pick = pick_slot(v, env);
        const SlotView &t = v.s[pick];
        if (!t.present || !t.bound || t.error != 0)
            return {};
        return {Do::arm, pick};
    }

    void armed_ok(int slot)
    {
        armed_ = true;
        rearm_ = false;
        slot_ = slot;
    }

    void failed(int64_t now)
    {
        attempts_++;
        retry_at_ = now + RETRY_MS;
    }

    void restored()
    {
        armed_ = false;
        slot_ = -1;
    }

private:
    uintptr_t comp_ = 0;
    uint64_t gen_ = 0;
    bool armed_ = false, rearm_ = false;
    int attempts_ = 0, slot_ = -1;
    int64_t retry_at_ = 0;
};

}
