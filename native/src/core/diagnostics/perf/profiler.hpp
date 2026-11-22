#pragma once

#include "core/diagnostics/perf/counters.hpp"
#include "core/diagnostics/perf/pacing_logic.hpp"
#include "core/diagnostics/perf/vq.hpp"
#include "core/diagnostics/perf_logic.hpp"

#include <windows.h>

#include <algorithm>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <mutex>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace perf::detail {

constexpr uint32_t MAX_PROBES = 512;
constexpr int DEPTH = 32;
constexpr size_t LOCALS = 256;
constexpr size_t HITCHES = 64;
constexpr size_t SLOW_CALLS = 32;
constexpr size_t TIMELINE = 160;
constexpr int BUCKETS = pace::BUCKETS;
constexpr DWORD SAMPLE_MS = 5000, WRITE_MS = 15000;
constexpr unsigned WRITE_EVERY = WRITE_MS / SAMPLE_MS, LINE_EVERY = 60000 / SAMPLE_MS;
constexpr size_t ROWS = 30;
constexpr int SPIKES_SHOWN = 5;
constexpr double MERGE_MS = 100;
constexpr double PRESENT_GAP_MS = 250;
constexpr double CLAIM_MS = 33;

struct Local {
    U64 calls[MAX_PROBES], ticks[MAX_PROBES], self[MAX_PROBES], worst[MAX_PROBES];
    U64 minute_worst[MAX_PROBES], minute_of[MAX_PROBES];
    U64 frame_ticks[2][MAX_PROBES], frame_of[2][MAX_PROBES];
    U64 top, top_frame[2], top_frame_of[2];
    std::atomic<bool> game;
};

struct Open {
    uint32_t id;
    uint64_t t0, child, paused;
};

struct Names {
    std::string owner, what;
    bool api;
};

struct Hitch {
    double at, ms, swofty_ms;
    std::string during, top;
};

struct SlowCall {
    double at, ms;
    uint32_t probe;
    unsigned long thread;
    std::string during;
};

struct Segment {
    double at, ms;
    uint64_t frames, slow;
    double worst_ms, swofty_ms;
    std::string what, top;
};

// The profiler's state and the code that keeps it, one object (perf.cpp): the probe names and per-thread counters
// with the probe, slow call and owner code (perf/probes.cpp), the frame clock, its histograms and claim, the hitch
// ring and the timeline (perf/frames.cpp), the report, the minute lines and the crash text (perf/report.cpp), and
// the calibration and start-up (perf.cpp).
struct Profiler {
    Names names[MAX_PROBES];
    std::atomic<uint32_t> probes{0};
    std::mutex names_lock;
    std::unordered_map<std::string, uint32_t> name_index;

    Local *locals[LOCALS];
    std::atomic<size_t> local_count{0};
    Local overflow;
    static inline thread_local Local *mine = nullptr;
    static inline thread_local Open open_[DEPTH]{};
    static inline thread_local int depth_ = 0;
    static inline thread_local bool background_ = false;

    LARGE_INTEGER qpc0, qpf;
    uint64_t tsc0;
    std::atomic<double> ticks_per_ms{0};
    std::atomic<uint64_t> slow_call_ticks{0}, hitch_ticks{0}, merge_ticks{0};
    double hitch_ms = 50, slow_call_ms = 8;
    bool verbose;

    std::atomic<uint64_t> epoch{1}, minute{1};
    std::atomic<uint64_t> last_frame{0}, last_flow{0}, present_frames{0}, flow_frames{0};
    std::atomic<uint64_t> last_present{0};
    std::atomic<bool> present_mode{false};
    std::atomic<uint64_t> claimed_epoch{0}, claimed_at{0};
    std::atomic<bool> claiming{false};

    SRWLOCK frame_lock = SRWLOCK_INIT;
    pace::Pace session_pace, minute_pace;

    SRWLOCK present_lock = SRWLOCK_INIT;
    pace::Presents session_presents, minute_presents;
    void *main_chain;
    uint64_t main_seen, prev_present, chain_learned;

    std::mutex pace_lock;
    std::deque<pace::Row> pace_rows;
    pace::Tally tally;
    std::string last_verdict;
    ULARGE_INTEGER wall0;
    uint64_t frames, slow33, slow50, slow100, slow250, frames_over_1ms, frames_over_4ms, frame_ticks_max, swofty_frame_max;
    uint64_t seg_frames, seg_slow, seg_worst, seg_t0;

    std::mutex rings_lock;
    std::deque<Hitch> hitches;
    std::deque<SlowCall> slow_calls;
    uint64_t hitch_count, hitch_heavy;
    std::deque<Segment> timeline;
    std::string seg_what = "start";
    std::vector<uint64_t> seg_base;
    uint64_t seg_top;

    std::atomic<uint32_t> slow_logged[MAX_PROBES];
    std::atomic<uint32_t> hitch_lines{0};

    HANDLE wake;
    std::atomic<HANDLE> writer_thread{nullptr};
    std::atomic<DWORD> writer_id{0};
    std::mutex write_lock;
    std::atomic<bool> flushed{false};

    using FrameTokenFn = int (*)(void **, const uint32_t *);
    FrameTokenFn frame_token_orig;
    std::atomic<uint32_t> frame_index{0};

    struct Totals {
        std::vector<uint64_t> calls, ticks, self, worst;
    };

    struct Minute {
        uint64_t top = 0, frames = 0;
        std::vector<uint64_t> ticks, calls;
        double at = 0;
        vq::Last *vq = nullptr;
        vq::Cpu cpu;
    };

    double ms_of(uint64_t ticks) const
    {
        double r = ticks_per_ms.load(rlx);
        return r > 0 ? double(ticks) / r : 0;
    }

    double seconds_now() { return ms_of(tsc() - tsc0) / 1000.0; }

    template <class F> void each_local(F fn)
    {
        size_t n = std::min(local_count.load(), LOCALS);
        for (size_t i = 0; i < n; i++)
            if (Local *l = locals[i])
                fn(*l);
        fn(overflow);
    }

    void calibrate();
    void attach();
    void start();
    void flush(const char *why);
    void stop(DWORD wait_ms);

    uint32_t probe(const std::string &owner, const std::string &what);
    void enter(uint32_t id);
    void leave();
    void background();
    int open_probes(uint32_t *out, int max);
    Local *local();
    std::string during();
    void slow_call(uint32_t id, uint64_t own);
    void account(uint32_t id, uint64_t own, uint64_t self, bool top);
    std::vector<uint64_t> probe_ticks(bool game_only = false, U64 (Local::*field)[MAX_PROBES] = &Local::ticks);
    uint64_t top_ticks(int game = -1);
    std::string top_list(const std::vector<std::pair<uint64_t, uint32_t>> &in, size_t n, double floor_ms);

    void hitch(uint64_t ended, double ms, double swofty_ms);
    void frame(uint64_t now, bool present);
    int frame_token(void **token, const uint32_t *index);
    bool hook_import(const char *dll, const char *fn, void *repl, void **orig);
    void flow_tick();
    bool frame_claim();
    void frame_release();
    void presented(void *swapchain, unsigned sync, unsigned flags, long hr, uint64_t entry, uint64_t before, uint64_t after);
    void mark(const char *kind, const std::string &what);

    std::string fixed(std::string s, size_t w);
    std::string clock_text(double s);
    std::string wall_text(double at);
    std::string during_at(double at);
    std::vector<pace::SpikeRow> spike_rows(const pace::Pace &p, size_t n);
    Totals totals();
    double process_cores(double secs, double *user, double *kernel);
    void line(std::string &out, const char *fmt, ...);
    std::string report(const char *why);
    static const std::wstring &report_path();
    void write_report(const char *why);
    void pace_lines(double secs, const pace::Pace &mp, const pace::Presents &pp, uint64_t df, const logic::CpuMinute &pc);
    void minute_line(Minute &last);
    DWORD write_loop();
    size_t crash_text(char *out, size_t cap, const uint32_t *open, int depth);
};

extern Profiler profiler;

int frame_token_hook(void **token, const uint32_t *index);
DWORD WINAPI writer(LPVOID);

}
