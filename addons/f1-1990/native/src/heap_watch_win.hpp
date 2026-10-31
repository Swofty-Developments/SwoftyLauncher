#pragma once

#include <windows.h>
#include <tlhelp32.h>

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "heap_watch.hpp"

namespace f1990::watch {

inline constexpr uint32_t kMaxHits = 50000;
inline constexpr int kLogHeapHits = 60;
inline constexpr int kLogOtherHits = 100;
inline constexpr size_t kMaxThreads = 8192;

struct Watch {
    uint64_t addr[kSlots]{};
    uint64_t shadow[kSlots]{};
    int n = 0;
    uint64_t start = 0;
    std::atomic<bool> live{false};
    std::atomic<bool> trap_seen{false};
    std::atomic<bool> stop{false};
    std::atomic<uint32_t> hits{0};
    uint32_t done[kMaxThreads]{};
    size_t done_n = 0;
    int armed = 0, failed = 0;
    int heap_logged = 0, other_logged = 0;
    uint32_t heap_total = 0, other_total = 0;
};

inline Watch g_watch;
inline Ring g_ring;

inline bool read_word(uint64_t a, uint64_t &v)
{
    if (!swofty::mem::readable(reinterpret_cast<const void *>(a), 8))
        return false;
    std::memcpy(&v, reinterpret_cast<const void *>(a), 8);
    return true;
}

inline LONG CALLBACK on_trap(PEXCEPTION_POINTERS p)
{
    if (p->ExceptionRecord->ExceptionCode != EXCEPTION_SINGLE_STEP || !g_watch.trap_seen.load(std::memory_order_relaxed))
        return EXCEPTION_CONTINUE_SEARCH;
    CONTEXT *c = p->ContextRecord;
    if (c->EFlags & 0x100)
        return EXCEPTION_CONTINUE_SEARCH;
    if (g_watch.live.load(std::memory_order_relaxed)) {
        if (g_watch.hits.fetch_add(1) >= kMaxHits)
            g_watch.stop.store(true);
        else if (Hit *h = g_ring.begin()) {
            uint64_t now[kSlots] = {};
            for (int i = 0; i < g_watch.n; i++)
                if (!read_word(g_watch.addr[i], now[i]))
                    now[i] = g_watch.shadow[i];
            h->tid = GetCurrentThreadId();
            h->tick = GetTickCount64();
            h->rip = c->Rip;
            h->rsp = c->Rsp;
            h->dr6 = c->Dr6;
            h->armed = uint32_t(g_watch.n);
            h->changed = changed_mask(g_watch.shadow, now, g_watch.n);
            std::memcpy(h->value, now, sizeof now);
            std::memcpy(g_watch.shadow, now, sizeof now);
            h->reg[0] = c->Rax;
            h->reg[1] = c->Rcx;
            h->reg[2] = c->Rdx;
            h->reg[3] = c->Rbx;
            h->reg[4] = c->Rsi;
            h->reg[5] = c->Rdi;
            h->reg[6] = c->R8;
            h->reg[7] = c->R9;
            auto *tib = reinterpret_cast<NT_TIB *>(NtCurrentTeb());
            uint64_t top = reinterpret_cast<uint64_t>(tib->StackBase), bottom = reinterpret_cast<uint64_t>(tib->StackLimit);
            h->stack_words = 0;
            if (c->Rsp >= bottom && c->Rsp < top) {
                uint64_t end = c->Rsp + 8 * kStackWords;
                if (end > top)
                    end = top;
                h->stack_words = uint32_t((end - c->Rsp) / 8);
            } else if (swofty::mem::readable(reinterpret_cast<const void *>(c->Rsp), 8 * kStackWords)) {
                h->stack_words = kStackWords;
            }
            if (h->stack_words)
                std::memcpy(h->stack, reinterpret_cast<const void *>(c->Rsp), 8 * h->stack_words);
            if (swofty::mem::readable(reinterpret_cast<const void *>(c->Rip - sizeof h->code), sizeof h->code))
                std::memcpy(h->code, reinterpret_cast<const void *>(c->Rip - sizeof h->code), sizeof h->code);
            else
                std::memset(h->code, 0, sizeof h->code);
            Ring::publish(h);
        }
    }
    c->Dr6 = 0;
    return EXCEPTION_CONTINUE_EXECUTION;
}

inline bool set_debug_registers(DWORD tid, bool on)
{
    HANDLE t = OpenThread(THREAD_GET_CONTEXT | THREAD_SET_CONTEXT | THREAD_SUSPEND_RESUME, FALSE, tid);
    if (!t)
        return false;
    bool ok = false;
    if (SuspendThread(t) != DWORD(-1)) {
        CONTEXT c{};
        c.ContextFlags = CONTEXT_DEBUG_REGISTERS;
        if (GetThreadContext(t, &c)) {
            c.Dr0 = on && g_watch.n > 0 ? g_watch.addr[0] : 0;
            c.Dr1 = on && g_watch.n > 1 ? g_watch.addr[1] : 0;
            c.Dr2 = on && g_watch.n > 2 ? g_watch.addr[2] : 0;
            c.Dr3 = on && g_watch.n > 3 ? g_watch.addr[3] : 0;
            c.Dr6 = 0;
            c.Dr7 = on ? dr7(g_watch.n) : 0;
            if (SetThreadContext(t, &c)) {
                CONTEXT r{};
                r.ContextFlags = CONTEXT_DEBUG_REGISTERS;
                ok = GetThreadContext(t, &r) && (r.Dr7 & 0xff) == (c.Dr7 & 0xff) && r.Dr0 == c.Dr0;
            }
        }
        ResumeThread(t);
    }
    CloseHandle(t);
    return ok;
}

inline bool seen(uint32_t tid)
{
    for (size_t i = 0; i < g_watch.done_n; i++)
        if (g_watch.done[i] == tid)
            return true;
    return false;
}

inline void each_thread(bool on, bool only_new)
{
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snap == INVALID_HANDLE_VALUE)
        return;
    THREADENTRY32 e{};
    e.dwSize = sizeof e;
    DWORD self = GetCurrentThreadId(), pid = GetCurrentProcessId();
    for (BOOL more = Thread32First(snap, &e); more; more = Thread32Next(snap, &e)) {
        if (e.th32OwnerProcessID != pid || e.th32ThreadID == self)
            continue;
        if (only_new && seen(e.th32ThreadID))
            continue;
        if (only_new && g_watch.done_n < kMaxThreads)
            g_watch.done[g_watch.done_n++] = e.th32ThreadID;
        bool ok = set_debug_registers(e.th32ThreadID, on);
        if (!on)
            continue;
        if (ok) {
            g_watch.armed++;
            g_watch.trap_seen.store(true);
        } else {
            g_watch.failed++;
        }
    }
    CloseHandle(snap);
}

inline void log_hit(const Hit &h)
{
    auto img = swofty::mem::detail::image();
    bool heap_code = in_allocator(h.rip, img.lo);
    if (heap_code) {
        g_watch.heap_total++;
        if (g_watch.heap_logged >= kLogHeapHits)
            return;
        g_watch.heap_logged++;
    } else {
        g_watch.other_total++;
        if (g_watch.other_logged >= kLogOtherHits)
            return;
        g_watch.other_logged++;
    }
    char where[32], values[kSlots * 40], stack[16 * 24], code[3 * 16 + 4];
    exe_or_raw(where, sizeof where, h.rip, img.lo, img.hi);
    size_t w = 0;
    values[0] = 0;
    for (uint32_t i = 0; i < h.armed && w + 40 < sizeof values; i++)
        w += size_t(std::snprintf(values + w, sizeof values - w, "%s%s%llx=%llx", w ? " " : "", (h.changed >> i) & 1 ? "*" : "",
                                  static_cast<unsigned long long>(g_watch.addr[i]), static_cast<unsigned long long>(h.value[i])));
    w = 0;
    for (size_t i = 0; i < sizeof h.code; i++)
        w += size_t(std::snprintf(code + w, sizeof code - w, "%02x", h.code[i]));
    unsigned long long ms = h.tick - g_watch.start;
    if (heap_code) {
        swofty::log("[f1-1990] heap watch: +%llu ms thread %u heap code %s wrote; %s", ms, h.tid, where, values);
        return;
    }
    exe_stack(h, img.lo, img.hi, stack, sizeof stack, 12);
    swofty::log("[f1-1990] heap watch: +%llu ms thread %u rip %s is outside the heap code and wrote a watched address "
                "(* = value changed): %s; dr6 %llx; code before rip %s; rax %llx rcx %llx rdx %llx rbx %llx rsi %llx rdi %llx "
                "r8 %llx r9 %llx; stack %s",
                ms, h.tid, where, values, static_cast<unsigned long long>(h.dr6), code,
                static_cast<unsigned long long>(h.reg[0]), static_cast<unsigned long long>(h.reg[1]),
                static_cast<unsigned long long>(h.reg[2]), static_cast<unsigned long long>(h.reg[3]),
                static_cast<unsigned long long>(h.reg[4]), static_cast<unsigned long long>(h.reg[5]),
                static_cast<unsigned long long>(h.reg[6]), static_cast<unsigned long long>(h.reg[7]), stack);
}

inline void drain()
{
    while (const Hit *h = g_ring.next()) {
        log_hit(*h);
        g_ring.pop();
    }
    if (uint32_t d = g_ring.dropped.exchange(0))
        swofty::log("[f1-1990] heap watch: %u hit(s) dropped (ring full)", d);
}

inline DWORD WINAPI watch_worker(LPVOID)
{
    uint32_t pass = 0;
    bool reported = false;
    for (;;) {
        drain();
        if (pass % 5 == 0) {
            each_thread(true, true);
            if (g_watch.armed == 0 && g_watch.failed >= 3) {
                g_watch.live.store(false);
                swofty::log("[f1-1990] heap watch: debug registers were not honoured in %d thread(s); the watch is off",
                            g_watch.failed);
                return 0;
            }
            if (!reported && g_watch.armed > 0) {
                reported = true;
                swofty::log("[f1-1990] heap watch: armed %d thread(s), %d refused; write breakpoints on %d address(es)",
                            g_watch.armed, g_watch.failed, g_watch.n);
            }
        }
        if (g_watch.stop.load()) {
            g_watch.live.store(false);
            each_thread(false, false);
            drain();
            swofty::log("[f1-1990] heap watch: %u hits, the breakpoints are cleared; heap code %u, other %u", g_watch.hits.load(),
                        g_watch.heap_total, g_watch.other_total);
            return 0;
        }
        if (pass % 1200 == 1199)
            swofty::log("[f1-1990] heap watch: %u hits so far, heap code %u, other %u, %d thread(s) armed (the first %d and %d "
                        "are logged)",
                        g_watch.hits.load(), g_watch.heap_total, g_watch.other_total, g_watch.armed, kLogHeapHits, kLogOtherHits);
        pass++;
        Sleep(50);
    }
}

inline void start_watch()
{
    g_watch.n = parse(std::getenv("F1_1990_HEAP_WATCH"), g_watch.addr, kSlots);
    if (g_watch.n == 0) {
        for (int i = 0; i < kSlots; i++)
            g_watch.addr[i] = kDefault[i];
        g_watch.n = kSlots;
    }
    for (int i = 0; i < g_watch.n; i++)
        read_word(g_watch.addr[i], g_watch.shadow[i]);
    g_watch.start = GetTickCount64();
    g_watch.live.store(true);
    AddVectoredExceptionHandler(1, on_trap);
    if (HANDLE t = CreateThread(nullptr, 0, watch_worker, nullptr, 0, nullptr))
        CloseHandle(t);
    char list[kSlots * 20] = "";
    size_t w = 0;
    for (int i = 0; i < g_watch.n; i++)
        w += size_t(std::snprintf(list + w, sizeof list - w, "%s%llx", w ? " " : "", static_cast<unsigned long long>(g_watch.addr[i])));
    swofty::log("[f1-1990] heap watch: on (write breakpoints on %s in every thread)", list);
}

}
