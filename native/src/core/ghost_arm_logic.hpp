#pragma once

#include <cstdint>

// When the supplied trace may be written into a telemetry lap. Pure: the game thread feeds it the six lap states it reads
// and acts on the decision; it never blocks and never loops.
namespace runtime::ghost_arm {

constexpr int SLOTS = 6, MAX_ATTEMPTS = 3;
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

// The window to write a trace is after the laps settle (the target lap left mode 0 for 1 or 2) and before or during the
// player's lap; the stock lap-start routine then puts the target into play mode. At session start every lap reads mode 0.
inline bool settled(const Views &v, int pick)
{
    const SlotView &p = v.s[0], &t = v.s[pick];
    return p.present && p.bound && p.error == 0 && t.present && t.bound && t.error == 0 && t.mode != 0;
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

    const char *why() const { return why_; }

    Decision decide(const Views &v, int env, int64_t now)
    {
        if (!gen_) {
            why_ = "no request";
            if (armed_ && slot_ == 3)
                return {Do::restore, 3};
            armed_ = false;
            return {};
        }
        if (armed_ && !rearm_) {
            why_ = "armed";
            return {};
        }
        if (attempts_ >= MAX_ATTEMPTS) {
            why_ = "gave up after three failures";
            return {};
        }
        if (now < retry_at_) {
            why_ = "waiting after a failure";
            return {};
        }
        int pick = pick_slot(v, env);
        const SlotView &t = v.s[pick];
        if (!t.present || !v.s[0].present) {
            why_ = "the laps do not exist yet";
            return {};
        }
        if (!t.bound) {
            why_ = "the target lap is not bound to a car";
            return {};
        }
        if (t.error != 0 || v.s[0].error != 0) {
            why_ = "a lap reports an error";
            return {};
        }
        if (!settled(v, pick)) {
            why_ = "the laps have not settled (target lap still in mode 0)";
            return {};
        }
        why_ = "ready";
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
    const char *why_ = "";
};

}
