#pragma once

#include "core/guards/skips.hpp"
#include "core/runtime/build_proof.hpp"

#include <emmintrin.h>

#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

namespace runtime::phys_nonfinite {

namespace bp = build_proof;

// Ne_PhysUpdateStage_FinishStepNotifyBodies and every function that reads or writes these offsets (the step state
// machine, the body list's link, cursor, walk and teardown, the body's transform and velocity writers) have the same
// instruction stream on the four builds (re/bindings/core/Ne_PhysUpdateStage_FinishStepNotifyBodies.json).
inline constexpr bp::Proof BODY_LIST{"the physics world's body list and body state (world +0xcf0, +0x48, body +0x330, "
                                     "+0x1c0, +0x210, +0x220)",
                                     bp::bit(bp::Build::kb) | bp::bit(bp::Build::ea) | bp::bit(bp::Build::steam_1_0_145) |
                                         bp::bit(bp::Build::ea_1_0_145)};

constexpr uintptr_t STAGE_WORLD = 0x8;
constexpr uintptr_t WORLD_BODIES = 0x48, WORLD_STATE = 0xcf0, WORLD_STEP_TIME = 0xcf4;
constexpr uintptr_t BODY_POSITION = 0x1c0, BODY_ANGULAR = 0x210, BODY_LINEAR = 0x220, BODY_NEXT = 0x330;
constexpr uintptr_t BODY_INV_MASS = 0x438, BODY_HANDLE = 0x458, HANDLE_BODY = 0x18;

constexpr uint32_t MAX_BODIES = 1u << 16;
constexpr int LOG_BODIES = 16;
constexpr int LOG_WORLDS = 8;
constexpr uint64_t REPORT_CHECK_STEPS = 256;
constexpr double REPORT_SECONDS = 60;

enum Field : uint8_t { linear = 1, angular = 2, position = 4 };

inline __m128i exponent_mask() { return _mm_set1_epi32(0x7f800000); }

inline __m128i nonfinite_lanes(uintptr_t at, __m128i e)
{
    __m128i v = _mm_and_si128(_mm_load_si128(reinterpret_cast<const __m128i *>(at)), e);
    return _mm_cmpeq_epi32(v, e);
}

inline bool any_xyz(__m128i lanes) { return (_mm_movemask_ps(_mm_castsi128_ps(lanes)) & 7) != 0; }

inline uint8_t fields(uintptr_t body)
{
    const __m128i e = exponent_mask();
    return uint8_t((any_xyz(nonfinite_lanes(body + BODY_LINEAR, e)) ? linear : 0) |
                   (any_xyz(nonfinite_lanes(body + BODY_ANGULAR, e)) ? angular : 0) |
                   (any_xyz(nonfinite_lanes(body + BODY_POSITION, e)) ? position : 0));
}

struct Walk {
    uint32_t bodies = 0;
    bool nonfinite = false;
    bool unended = false;
};

inline Walk walk(uintptr_t head)
{
    const __m128i e = exponent_mask();
    __m128i seen = _mm_setzero_si128();
    uint32_t n = 0;
    uintptr_t body = head;
    for (; body && n < MAX_BODIES; n++) {
        __m128i bad = _mm_or_si128(nonfinite_lanes(body + BODY_POSITION, e),
                                   _mm_or_si128(nonfinite_lanes(body + BODY_ANGULAR, e), nonfinite_lanes(body + BODY_LINEAR, e)));
        seen = _mm_or_si128(seen, bad);
        body = *reinterpret_cast<const uintptr_t *>(body + BODY_NEXT);
    }
    return {n, any_xyz(seen), body != 0};
}

struct Body {
    uintptr_t at = 0, handle = 0;
    uint32_t index = 0;
    uint8_t bad = 0;
    float position[3] = {}, linear[3] = {}, angular[3] = {};
    float inv_mass = 0;
};

inline Body read_body(uintptr_t body, uint32_t index)
{
    Body b;
    b.at = body;
    b.index = index;
    b.bad = fields(body);
    std::memcpy(b.position, reinterpret_cast<const void *>(body + BODY_POSITION), sizeof b.position);
    std::memcpy(b.linear, reinterpret_cast<const void *>(body + BODY_LINEAR), sizeof b.linear);
    std::memcpy(b.angular, reinterpret_cast<const void *>(body + BODY_ANGULAR), sizeof b.angular);
    std::memcpy(&b.inv_mass, reinterpret_cast<const void *>(body + BODY_INV_MASS), sizeof b.inv_mass);
    std::memcpy(&b.handle, reinterpret_cast<const void *>(body + BODY_HANDLE), sizeof b.handle);
    return b;
}

inline std::string value(float f)
{
    char buf[40];
    if (std::isfinite(f)) {
        std::snprintf(buf, sizeof buf, "%.6g", double(f));
        return buf;
    }
    uint32_t bits;
    std::memcpy(&bits, &f, sizeof bits);
    if (std::isinf(f))
        return f < 0 ? "-inf" : "inf";
    std::snprintf(buf, sizeof buf, "nan(0x%08x)", unsigned(bits));
    return buf;
}

inline std::string vec(const float (&v)[3]) { return "(" + value(v[0]) + ", " + value(v[1]) + ", " + value(v[2]) + ")"; }

inline std::string field_names(uint8_t bad)
{
    std::string out;
    for (auto [bit, name] :
         {std::pair{linear, "linear velocity"}, std::pair{angular, "angular velocity"}, std::pair{position, "position"}})
        if (bad & bit)
            out += (out.empty() ? "" : " and ") + std::string(name);
    return out;
}

// One body's line: which of its fields are not finite and all three, and who owns it (`owner`, from the core's car
// table read).
inline std::string body_line(uint64_t step, uintptr_t world, float step_time, uint32_t bodies, uint32_t bad_bodies, const Body &b,
                             const std::string &owner, const char *tail)
{
    char head[256];
    std::snprintf(head, sizeof head,
                  "[phys] step %llu: body %u of %u (0x%llx, handle 0x%llx, world 0x%llx, step %s s): ", (unsigned long long)step,
                  unsigned(b.index), unsigned(bodies), (unsigned long long)b.at, (unsigned long long)b.handle,
                  (unsigned long long)world, value(step_time).c_str());
    char count[96];
    std::snprintf(count, sizeof count, "; %u bod%s in this step %s not finite", unsigned(bad_bodies),
                  bad_bodies == 1 ? "y" : "ies", bad_bodies == 1 ? "is" : "are");
    bool two = b.bad & (b.bad - 1);
    return head + field_names(b.bad) + (two ? " are" : " is") + " not finite; position " + vec(b.position) +
           ", linear velocity " + vec(b.linear) + ", angular velocity " + vec(b.angular) + ", inverse mass " + value(b.inv_mass) +
           "; " + owner + count + tail;
}

// The four wheels in the car's suspension order (CarSuspension +0x70): rear left, rear right, front left, front right
// (render/postfx/signal_logic.hpp wheels_contract_order).
inline constexpr const char *CORNERS[4] = {"RL", "RR", "FL", "FR"};

struct Corner {
    bool read = false;
    float spin = 0, slip_angle = 0, slip_ratio = 0;
};

struct Owner {
    int slot = -1;
    unsigned vehicle_id = 0xff;
    bool ai = false;
    bool table = false;
    bool wheels = false;
    Corner corners[4];
};

inline std::string owner_text(const Owner &o)
{
    if (!o.table)
        return "the car table is not read on this build";
    if (o.slot < 0)
        return "no car's chassis";
    char buf[96];
    std::snprintf(buf, sizeof buf, "the chassis of vehicle %u (table slot %d, %s)", o.vehicle_id, o.slot, o.ai ? "AI" : "player");
    std::string out = buf;
    if (!o.wheels)
        return out;
    out += ", wheel spin/slip angle/slip ratio";
    for (int i = 0; i < 4; i++) {
        out += std::string(i ? ", " : " ") + CORNERS[i] + " ";
        const Corner &c = o.corners[i];
        out += c.read ? value(c.spin) + "/" + value(c.slip_angle) + "/" + value(c.slip_ratio) : "unread";
    }
    return out;
}

inline std::string world_line(uintptr_t world, uint64_t step, uint32_t bodies, bool unended, const char *tail)
{
    char buf[256];
    std::snprintf(buf, sizeof buf, "[phys] non-finite log: world 0x%llx finished its first step (step %llu) with %u bod%s%s%s",
                  (unsigned long long)world, (unsigned long long)step, unsigned(bodies), bodies == 1 ? "y" : "ies",
                  unended ? "; its body list did not end there, so each step reads that many" : "", tail);
    return buf;
}

// The steps since the last report: how many, the bodies they read, the time their reads took (timestamp counter
// ticks), and the steps that found a body that is not finite.
struct Window {
    uint64_t steps = 0, bodies = 0, ticks = 0, max_ticks = 0, nonfinite_steps = 0;
    uint32_t max_bodies = 0;
};

struct Rates {
    double ns_per_step = 0, max_ns = 0, bodies_per_step = 0, steps_per_s = 0;
    double us_per_s_60 = 0, us_per_s_240 = 0;
};

inline Rates rates(const Window &w, double seconds, double ns_per_tick)
{
    Rates r;
    if (!w.steps)
        return r;
    r.ns_per_step = double(w.ticks) * ns_per_tick / double(w.steps);
    r.max_ns = double(w.max_ticks) * ns_per_tick;
    r.bodies_per_step = double(w.bodies) / double(w.steps);
    r.steps_per_s = seconds > 0 ? double(w.steps) / seconds : 0;
    r.us_per_s_60 = r.ns_per_step * 60 / 1000;
    r.us_per_s_240 = r.ns_per_step * 240 / 1000;
    return r;
}

inline std::string perf_line(const Window &w, double seconds, double ns_per_tick)
{
    Rates r = rates(w, seconds, ns_per_tick);
    char buf[512];
    std::snprintf(buf, sizeof buf,
                  "[perf] physics non-finite check: last %.0f s: %llu steps (%.1f per s), %.0f bodies per step (max %u), "
                  "%.0f ns per step (max %.0f ns): %.1f us per s at 60 steps/s (%.4f%% of a 16.7 ms frame), %.1f us per s at "
                  "240 steps/s (%.4f%% of a 4.2 ms frame); %llu step%s found a body that is not finite",
                  seconds, (unsigned long long)w.steps, r.steps_per_s, r.bodies_per_step, unsigned(w.max_bodies), r.ns_per_step,
                  r.max_ns, r.us_per_s_60, r.ns_per_step / 16666666.7 * 100, r.us_per_s_240, r.ns_per_step / 4166666.7 * 100,
                  (unsigned long long)w.nonfinite_steps, w.nonfinite_steps == 1 ? "" : "s");
    return buf;
}

// What the log keeps between steps: the bodies and worlds it logged, the step count and the report window. A world's
// steps run on one thread; the counters take relaxed loads and stores (no read-modify-write), as the profiler's do.
class Checker {
public:
    using Log = void (*)(const std::string &line);
    using OwnerOf = Owner (*)(uintptr_t body);

    constexpr Checker(Log log, OwnerOf owner) : log_(log), owner_(owner) {}

    // After a step finished in `world`: one pass over its body list, then the cold path only when the world is new or a
    // body is not finite. Returns the pass, and the ticks it took in `ticks`.
    Walk step(uintptr_t world, uint64_t &ticks)
    {
        uint64_t t0 = __builtin_ia32_rdtsc();
        Walk w = walk(*reinterpret_cast<const uintptr_t *>(world + WORLD_BODIES));
        ticks = __builtin_ia32_rdtsc() - t0;
        uint64_t n = steps_.load(rlx) + 1;
        steps_.store(n, rlx);
        add(window_.steps, 1);
        add(window_.bodies, w.bodies);
        add(window_.ticks, ticks);
        raise(window_.max_ticks, ticks);
        raise(window_.max_bodies, w.bodies);
        if (last_world_.load(rlx) != world) {
            last_world_.store(world, rlx);
            if (claim(worlds_seen_, world))
                first_step(world, n, w);
        }
        if (w.nonfinite) {
            add(window_.nonfinite_steps, 1);
            nonfinite(world, n, w);
        }
        return w;
    }

    uint64_t steps() const { return steps_.load(rlx); }

    // The steps since the last call, and a new window.
    Window take()
    {
        Window w;
        w.steps = window_.steps.exchange(0, rlx);
        w.bodies = window_.bodies.exchange(0, rlx);
        w.ticks = window_.ticks.exchange(0, rlx);
        w.max_ticks = window_.max_ticks.exchange(0, rlx);
        w.nonfinite_steps = window_.nonfinite_steps.exchange(0, rlx);
        w.max_bodies = window_.max_bodies.exchange(0, rlx);
        return w;
    }

    size_t logged() const { return size_t(logged_.load(rlx)); }

    uint64_t unlogged_steps() const { return unlogged_.load(rlx); }

private:
    static constexpr auto rlx = std::memory_order_relaxed;

    struct Counters {
        std::atomic<uint64_t> steps{0}, bodies{0}, ticks{0}, max_ticks{0}, nonfinite_steps{0};
        std::atomic<uint32_t> max_bodies{0};
    };

    static void add(std::atomic<uint64_t> &a, uint64_t v) { a.store(a.load(rlx) + v, rlx); }

    template <class T> static void raise(std::atomic<T> &a, T v)
    {
        if (v > a.load(rlx))
            a.store(v, rlx);
    }

    static uintptr_t next(uintptr_t body) { return *reinterpret_cast<const uintptr_t *>(body + BODY_NEXT); }

    // Whether `key` takes a free slot of `slots` (true once per key; false for a key already there or a full set).
    template <size_t N> static bool claim(std::atomic<uintptr_t> (&slots)[N], uintptr_t key)
    {
        for (auto &slot : slots) {
            uintptr_t v = slot.load(rlx);
            if (v == key)
                return false;
            if (!v) {
                if (slot.compare_exchange_strong(v, key, rlx))
                    return true;
                if (v == key)
                    return false;
            }
        }
        return false;
    }

    template <size_t N> static bool holds(const std::atomic<uintptr_t> (&slots)[N], uintptr_t key)
    {
        for (const auto &slot : slots)
            if (slot.load(rlx) == key)
                return true;
        return false;
    }

    void first_step(uintptr_t world, uint64_t n, const Walk &w)
    {
        if (auto line = worlds_.note(world))
            log_(world_line(world, n, w.bodies, w.unended, line.tail));
    }

    // One pass counts the bodies that are not finite and keeps those not logged yet; only a new one takes the lock
    // of the logged set and logs. A body logged before costs a compare per logged body on each later step.
    void nonfinite(uintptr_t world, uint64_t n, const Walk &w)
    {
        if (logged_.load(rlx) >= LOG_BODIES) {
            unlogged_.store(unlogged_.load(rlx) + 1, rlx);
            return;
        }
        uintptr_t fresh[LOG_BODIES];
        uint32_t index[LOG_BODIES];
        int k = 0;
        uint32_t bad = 0;
        uintptr_t body = *reinterpret_cast<const uintptr_t *>(world + WORLD_BODIES);
        for (uint32_t i = 0; body && i < w.bodies; i++, body = next(body)) {
            if (!fields(body))
                continue;
            bad++;
            if (k < LOG_BODIES && !holds(logged_bodies_, body)) {
                fresh[k] = body;
                index[k++] = i;
            }
        }
        if (!k)
            return;
        float step_time = 0;
        std::memcpy(&step_time, reinterpret_cast<const void *>(world + WORLD_STEP_TIME), sizeof step_time);
        for (int j = 0; j < k; j++) {
            auto line = bodies_.note(fresh[j]);
            if (!line)
                continue;
            logged_bodies_[line.n - 1].store(fresh[j], rlx);
            if (line.n > logged_.load(rlx))
                logged_.store(line.n, rlx);
            log_(body_line(n, world, step_time, w.bodies, bad, read_body(fresh[j], index[j]), owner_text(owner_(fresh[j])),
                           line.tail));
        }
    }

    Log log_;
    OwnerOf owner_;
    swofty_core::guards::FirstKeys<uintptr_t, LOG_BODIES> bodies_{"non-finite bodies"};
    swofty_core::guards::FirstKeys<uintptr_t, LOG_WORLDS> worlds_{"worlds"};
    std::atomic<uint64_t> steps_{0}, unlogged_{0};
    std::atomic<uintptr_t> last_world_{0};
    std::atomic<uintptr_t> worlds_seen_[LOG_WORLDS] = {};
    std::atomic<uintptr_t> logged_bodies_[LOG_BODIES] = {};
    std::atomic<int> logged_{0};
    Counters window_;
};

}
