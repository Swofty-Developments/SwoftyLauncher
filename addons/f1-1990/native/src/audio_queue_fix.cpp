// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the audio job worker queue fix (its own DLL).
//
// Wwise asks the game for job workers through Snd_Request_JobWorker. The game gives each request to an idle one of its
// 12 audio worker threads, or queues it in g_Snd_JobWorkerOverflow at the first free of 24 slots, but the workers only
// take queued requests from slots 0-11. A request queued in slots 12-23 never runs, every worker spins on the pending
// count, the Wwise audio thread waits for that job forever holding the Wwise main lock, and the game freezes until the
// watchdog ends it 30 s later (seen on leaving a replay in a race). The wrap hands Wwise a thunk in place of each
// worker function; after every request it runs, and after every Snd_Request_JobWorker call, queued requests in slots
// 12-23 move to free slots 0-11 under g_Snd_JobWorkerLock, and the pending count stays as it is. Option audio_queue_fix
// off passes every request through unchanged.

#include <windows.h>

#include <array>
#include <atomic>
#include <cstdint>
#include <utility>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "audio_queue.hpp"

SWOFTY_SETTING(bool, audio_queue_fix, true);

namespace {

namespace aq = f1990::audio;

using Worker = void (*)(uint32_t job_type, uint32_t usec);

constexpr size_t kThunks = 8;
constexpr int kLogMoves = 16;

aq::Workers<kThunks> g_workers;
std::atomic<int> g_table_state{0};
std::atomic<int> g_logged{0};
std::atomic<uint64_t> g_moved{0};
std::atomic<bool> g_first{true};

uint8_t *table()
{
    int state = g_table_state.load(std::memory_order_acquire);
    auto *t = reinterpret_cast<uint8_t *>(swofty::game::g_Snd_JobWorkerOverflow.get());
    if (state == 0) {
        bool ok = t && swofty::mem::readable(t, aq::kTableBytes);
        g_table_state.store(ok ? 1 : 2, std::memory_order_release);
        if (!ok)
            swofty::log("[f1-1990] audio queue fix: the overflow queue binding is not readable (%p); requests pass "
                        "through", static_cast<void *>(t));
        return ok ? t : nullptr;
    }
    return state == 1 ? t : nullptr;
}

CRITICAL_SECTION *lock()
{
    void **p = swofty::game::g_Snd_JobWorkerLock.get();
    return p && swofty::mem::readable(p, sizeof(void *)) ? static_cast<CRITICAL_SECTION *>(*p) : nullptr;
}

void sweep(const char *where)
{
    uint8_t *t = table();
    if (!t || !aq::stranded(t))
        return;
    CRITICAL_SECTION *cs = lock();
    if (!cs)
        return;
    EnterCriticalSection(cs);
    size_t moved = aq::move_stranded(t);
    bool left = aq::stranded(t);
    LeaveCriticalSection(cs);
    if (!moved)
        return;
    uint64_t total = g_moved.fetch_add(moved) + moved;
    if (g_logged.fetch_add(1) < kLogMoves)
        swofty::log("[f1-1990] audio queue fix: moved %zu queued worker request(s) from slots 12-23 to slots 0-11 %s "
                    "(%llu in all%s)",
                    moved, where, static_cast<unsigned long long>(total), left ? "; more wait for a free slot" : "");
}

template <size_t I> void thunk(uint32_t job_type, uint32_t usec)
{
    reinterpret_cast<Worker>(g_workers.at(I))(job_type, usec);
    sweep("after a worker request ran");
}

template <size_t... I> constexpr std::array<Worker, sizeof...(I)> make_thunks(std::index_sequence<I...>)
{
    return {&thunk<I>...};
}

constexpr std::array<Worker, kThunks> kThunkFns = make_thunks(std::make_index_sequence<kThunks>{});

bool is_thunk(void *fn)
{
    for (Worker w : kThunkFns)
        if (reinterpret_cast<void *>(w) == fn)
            return true;
    return false;
}

}

SWOFTY_WRAP(Snd_Request_JobWorker, void, void *worker, uint32_t job_type, uint32_t count)
{
    if (!audio_queue_fix || !worker || is_thunk(worker) || !table()) {
        orig(worker, job_type, count);
        return;
    }
    size_t i = g_workers.index(worker);
    void *pass = i < kThunks ? reinterpret_cast<void *>(kThunkFns[i]) : worker;
    if (g_first.exchange(false))
        swofty::log("[f1-1990] audio queue fix: first worker request (worker %p, job type %u, %u worker(s)) runs "
                    "through thunk %zu",
                    worker, job_type, count, i);
    else if (i == kThunks && g_logged.fetch_add(1) < kLogMoves)
        swofty::log("[f1-1990] audio queue fix: no thunk left for worker %p; its requests pass through", worker);
    orig(pass, job_type, count);
    sweep("after a request was queued");
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] audio queue fix: %s", audio_queue_fix
                                                   ? "on (queued audio worker requests in slots 12-23 move to 0-11)"
                                                   : "off (audio_queue_fix)");
}

SWOFTY_ADDON()
