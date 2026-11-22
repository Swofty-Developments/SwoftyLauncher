#include "core/diagnostics/perf.hpp"

#include "core/diagnostics/crash/blackbox.hpp"
#include "core/diagnostics/perf/profiler.hpp"
#include "core/diagnostics/perf/system.hpp"
#include "core/diagnostics/perf_logic.hpp"
#include "core/runtime/imports.hpp"

#include <swofty/mixin.hpp>

#include <dxgi1_4.h>

#include <cmath>
#include <cstring>

namespace perf {

namespace detail {

void Profiler::hitch(uint64_t ended, double ms, double swofty_ms)
{
    std::vector<std::pair<uint64_t, uint32_t>> in;
    uint32_t n = probes.load();
    std::vector<uint64_t> per(n);
    each_local([&](Local &l) {
        if (&l == &overflow || !l.game.load(rlx))
            return;
        size_t k = ended & 1;
        for (uint32_t i = 0; i < n; i++)
            if (l.frame_of[k][i].load(rlx) == ended)
                per[i] += l.frame_ticks[k][i].load(rlx);
    });
    for (uint32_t i = 0; i < n; i++)
        if (per[i])
            in.push_back({per[i], i});
    Hitch h{seconds_now(), ms, swofty_ms, during(), top_list(in, 3, 0.01)};
    bool heavy = logic::heavy_hitch(ms, swofty_ms);
    if ((heavy || verbose) && hitch_lines.fetch_add(1, rlx) < 20)
        swofty::log("%s", logic::hitch_line(ms, h.during, swofty_ms, h.top).c_str());
    std::lock_guard<std::mutex> g(rings_lock);
    hitch_count++;
    hitch_heavy += swofty_ms >= 5.0;
    hitches.push_back(std::move(h));
    if (hitches.size() > HITCHES)
        hitches.pop_front();
}

void Profiler::frame(uint64_t now, bool present)
{
    if (!live()) {
        epoch.fetch_add(1, rlx);
        last_frame.store(now, rlx);
        return;
    }
    AcquireSRWLockExclusive(&frame_lock);
    uint64_t prev = last_frame.exchange(now, rlx);
    uint64_t ended = epoch.fetch_add(1, rlx);
    bool switched = present_mode.exchange(present, rlx) != present;
    (present ? present_frames : flow_frames).fetch_add(1, rlx);
    if (!prev || now <= prev || switched) {
        ReleaseSRWLockExclusive(&frame_lock);
        return;
    }
    uint64_t sw = 0;
    each_local([&](Local &l) {
        if (&l != &overflow && l.game.load(rlx) && l.top_frame_of[ended & 1].load(rlx) == ended)
            sw += l.top_frame[ended & 1].load(rlx);
    });
    uint64_t dt = now - prev;
    double ms = ms_of(dt), sw_ms = ms_of(sw), at = ms_of(now - tsc0) / 1000.0;
    session_pace.add(ms, at);
    minute_pace.add(ms, at);
    frames++;
    seg_frames++;
    slow33 += ms > 33.4;
    slow50 += ms > 50;
    slow100 += ms > 100;
    slow250 += ms > 250;
    seg_slow += ms > 50;
    frames_over_1ms += sw_ms > 1;
    frames_over_4ms += sw_ms > 4;
    frame_ticks_max = std::max(frame_ticks_max, dt);
    seg_worst = std::max(seg_worst, dt);
    swofty_frame_max = std::max(swofty_frame_max, sw);
    ReleaseSRWLockExclusive(&frame_lock);
    uint64_t h = hitch_ticks.load(rlx);
    if (h && dt >= h)
        hitch(ended, ms, sw_ms);
}

int Profiler::frame_token(void **token, const uint32_t *index)
{
    uint64_t now = tsc();
    uint32_t i = index ? *index : 0, seen = frame_index.load(rlx);
    if (i > seen && frame_index.compare_exchange_strong(seen, i, rlx)) {
        last_present.store(now, rlx);
        frame(now, true);
    } else if (i + 100000 < seen) {
        frame_index.store(i, rlx);
    }
    return frame_token_orig(token, index);
}

bool Profiler::hook_import(const char *dll, const char *fn, void *repl, void **orig)
{
    pe_imports it;
    pe_import e, first{};
    if (!pe_imports_open(&it, uintptr_t(GetModuleHandleW(nullptr)), dll, runtime::import_readable, nullptr))
        return false;
    int skipped = 0;
    bool done = false;
    for (int r; !done && (r = pe_imports_next(&it, &e)) != PE_IMPORT_END;) {
        if (r == PE_IMPORT_SKIP) {
            if (!skipped++)
                first = e;
            continue;
        }
        if (e.kind != PE_IMPORT_BY_NAME || strcmp(e.name, fn))
            continue;
        void *was = reinterpret_cast<void *>(*e.slot);
        if (!runtime::write_slot(reinterpret_cast<ULONG_PTR *>(e.slot), repl))
            break;
        *orig = was;
        done = true;
    }
    if (skipped)
        vq::note_import_skip("game exe", first, skipped);
    return done;
}

void Profiler::flow_tick()
{
    uint64_t now = tsc();
    uint64_t prev = last_flow.exchange(now, rlx);
    uint64_t shown = last_present.load(rlx);
    double r = ticks_per_ms.load(rlx);
    if (r <= 0 || (shown && now - shown < uint64_t(PRESENT_GAP_MS * r)))
        return;
    if (!prev || now - prev > uint64_t(2 * r))
        frame(now, false);
}

bool Profiler::frame_claim()
{
    uint64_t e = epoch.load(rlx), now = tsc();
    uint64_t late = uint64_t(CLAIM_MS * ticks_per_ms.load(rlx));
    auto due = [&] { return claimed_epoch.load(rlx) != e || now - claimed_at.load(rlx) >= late; };
    if (!due() || claiming.exchange(true, std::memory_order_acquire))
        return false;
    if (!due()) {
        claiming.store(false, std::memory_order_release);
        return false;
    }
    claimed_epoch.store(e, rlx);
    claimed_at.store(now, rlx);
    return true;
}

void Profiler::frame_release() { claiming.store(false, std::memory_order_release); }

void Profiler::presented(void *swapchain, unsigned sync, unsigned flags, long hr, uint64_t entry, uint64_t before, uint64_t after)
{
    if (!live())
        return;
    double r = ticks_per_ms.load(rlx);
    if (r <= 0)
        return;
    bool fresh = false;
    AcquireSRWLockExclusive(&present_lock);
    if (swapchain != main_chain) {
        bool stale = !main_chain || (entry > main_seen && entry - main_seen >= uint64_t(1000 * r));
        if (!stale) {
            minute_presents.other++;
            session_presents.other++;
            ReleaseSRWLockExclusive(&present_lock);
            return;
        }
        main_chain = swapchain, prev_present = 0, chain_learned = 0;
    }
    if (!chain_learned || entry - chain_learned >= uint64_t(10000 * r))
        chain_learned = entry, fresh = true;
    main_seen = entry;
    double interval = prev_present && entry > prev_present ? double(entry - prev_present) / r : 0;
    prev_present = entry;
    double inside = after > before ? double(after - before) / r : 0, at = entry > tsc0 ? double(entry - tsc0) / r / 1000 : 0;
    bool tear = (flags & DXGI_PRESENT_ALLOW_TEARING) != 0;
    minute_presents.add(interval, inside, sync, tear, uint32_t(hr), at);
    session_presents.add(interval, inside, sync, tear, uint32_t(hr), at);
    ReleaseSRWLockExclusive(&present_lock);
    if (fresh)
        sys::learn_chain(swapchain);
}

void Profiler::mark(const char *kind, const std::string &what)
{
    blackbox::note(kind, what.c_str());
    if (!live())
        return;
    uint64_t now = tsc();
    std::string label = std::string(kind) + " " + what;
    {
        std::lock_guard<std::mutex> g(rings_lock);
        AcquireSRWLockShared(&frame_lock);
        uint64_t t0 = seg_t0;
        ReleaseSRWLockShared(&frame_lock);
        if (now - t0 < merge_ticks.load(rlx)) {
            if (seg_what.size() < 240)
                seg_what += "; " + label;
            else if (seg_what.back() != '.')
                seg_what += "; ...";
            return;
        }
    }
    std::vector<uint64_t> base = probe_ticks(true);
    uint64_t top = top_ticks(1);
    std::lock_guard<std::mutex> g(rings_lock);
    AcquireSRWLockExclusive(&frame_lock);
    uint64_t fr = seg_frames, sl = seg_slow, worst = seg_worst, t0 = seg_t0;
    seg_frames = seg_slow = seg_worst = 0;
    seg_t0 = now;
    ReleaseSRWLockExclusive(&frame_lock);
    std::vector<std::pair<uint64_t, uint32_t>> in;
    for (uint32_t i = 0; i < base.size(); i++) {
        uint64_t was = i < seg_base.size() ? seg_base[i] : 0;
        if (base[i] > was)
            in.push_back({base[i] - was, i});
    }
    double took = ms_of(now - t0);
    timeline.push_back({ms_of(t0 - tsc0) / 1000, took, fr, sl, ms_of(worst), ms_of(top - seg_top), seg_what,
                        took >= 250 ? top_list(in, 3, 0.05) : std::string()});
    if (timeline.size() > TIMELINE)
        timeline.pop_front();
    seg_base = std::move(base);
    seg_top = top;
    seg_what = label;
}

int frame_token_hook(void **token, const uint32_t *index) { return profiler.frame_token(token, index); }

}

void flow_tick() { detail::profiler.flow_tick(); }

bool frame_claim() { return detail::profiler.frame_claim(); }

void frame_release() { detail::profiler.frame_release(); }

void presented(void *swapchain, unsigned sync, unsigned flags, long hr, uint64_t entry, uint64_t before, uint64_t after)
{
    detail::profiler.presented(swapchain, sync, flags, hr, entry, before, after);
}

void mark(const char *kind, const std::string &what) { detail::profiler.mark(kind, what); }

}
