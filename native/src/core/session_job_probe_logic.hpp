#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <mutex>

// The wheel tyre instance probe (session_job_probe.cpp). Four 1.9.6-1.9.39 hangs entering the next session of a race
// weekend had a job thread inside game function 0x1445acb00 (Game_Vfx_UpdateWheelTyreInstance), which the renderer
// module's per-vehicle job 0x1401717b0 calls once per wheel, for three minutes until the game's watchdog killed the game. The function walks an outer list of
// 0x20-byte items whose count is (end - begin) >> 5, tested unsigned against the index, and for every outer index scans
// an inner list of entry pointers, each with a u32 list; every miss captures a stack trace. An outer list whose end is
// below its begin gives a count the unsigned test never reaches, and a huge one runs for hours. The probe reads both
// lists on entry and logs their sizes and pointers once per session per distinct value, with a [warning] line when a
// size is negative, huge or not a whole number of items.

namespace runtime::session_job_probe {

constexpr uint64_t kOuterItem = 0x20;
constexpr uint64_t kInnerItem = 8;
constexpr uint64_t kListItem = 4;
constexpr int64_t kHuge = int64_t(1) << 16;
constexpr int kPerSession = 8;
constexpr int kListsRead = 64;

enum Gate : uint32_t {
    Runs = 0,
    NoVehicle = 1,
    NoOwner = 2,
    NoHolder = 3,
    NoState = 4,
    FlagOff = 5,
    NoLists = 6,
    Pending = 7,
    OuterEmpty = 8,
    Unread = 9,
};

inline const char *gate_text(uint32_t g)
{
    switch (g) {
    case Runs:
        return "the nested loop runs";
    case NoVehicle:
        return "no loop: its vehicle id is 0xff";
    case NoOwner:
        return "no loop: its fifth argument is null";
    case NoHolder:
        return "no loop: the wheel's slot holds no object";
    case NoState:
        return "no loop: the object's +0x48 is null";
    case FlagOff:
        return "no loop: the +0x31 flag is off";
    case NoLists:
        return "no loop: the object's +0x40 is null";
    case Pending:
        return "no loop: its pending list is not empty";
    case OuterEmpty:
        return "no loop: the outer list is empty";
    default:
        return "not read: a pointer on the way is not readable";
    }
}

struct Span {
    uint64_t begin = 0, end = 0;
    bool read = false;

    int64_t bytes() const
    {
        return int64_t(end - begin);
    }
};

enum Verdict : uint32_t { Ok = 0, Negative = 1, Huge = 2, Ragged = 3, NotRead = 4 };

inline Verdict verdict(const Span &s, uint64_t item)
{
    if (!s.read)
        return NotRead;
    int64_t b = s.bytes();
    if (b < 0)
        return Negative;
    if (b / int64_t(item) > kHuge)
        return Huge;
    if (b % int64_t(item))
        return Ragged;
    return Ok;
}

inline int64_t items(const Span &s, uint64_t item)
{
    return s.bytes() / int64_t(item);
}

inline uint64_t loop_count(const Span &s)
{
    return uint64_t(s.bytes() >> 5);
}

struct Entry {
    uint32_t vehicle = 0;
    uint32_t wheel = 0;
    uint32_t gate = Unread;
    uint64_t holder = 0;
    uint64_t lists = 0;
    Span outer;
    Span inner;
    int lists_read = 0;
    int64_t list_max = 0;
    int64_t list_min = 0;
    int lists_bad = 0;
};

inline uint64_t key(const Entry &e)
{
    uint64_t h = 1469598103934665603ull;
    auto mix = [&](uint64_t v) {
        for (int i = 0; i < 8; i++) {
            h ^= (v >> (i * 8)) & 0xff;
            h *= 1099511628211ull;
        }
    };
    mix(e.gate);
    mix(e.outer.read ? uint64_t(e.outer.bytes()) : ~0ull);
    mix(e.inner.read ? uint64_t(e.inner.bytes()) : ~0ull);
    return h | 1;
}

inline bool bad(const Entry &e)
{
    Verdict o = verdict(e.outer, kOuterItem), i = verdict(e.inner, kInnerItem);
    return (o != Ok && o != NotRead) || (i != Ok && i != NotRead) || e.lists_bad || e.list_max > kHuge;
}

enum Admit : uint32_t { Repeat = 0, First = 1, Last = 2 };

class Seen {
public:
    Admit admit(uint64_t serial, uint64_t k)
    {
        if (serial_.load(std::memory_order_acquire) == serial + 1) {
            int n = count_.load(std::memory_order_acquire);
            for (int i = 0; i < n && i < kPerSession; i++)
                if (keys_[i].load(std::memory_order_relaxed) == k)
                    return Repeat;
            if (n > kPerSession)
                return Repeat;
        }
        std::lock_guard<std::mutex> g(lock_);
        if (serial_.load(std::memory_order_relaxed) != serial + 1) {
            for (auto &x : keys_)
                x.store(0, std::memory_order_relaxed);
            count_.store(0, std::memory_order_relaxed);
            serial_.store(serial + 1, std::memory_order_release);
        }
        int n = count_.load(std::memory_order_relaxed);
        for (int i = 0; i < n && i < kPerSession; i++)
            if (keys_[i].load(std::memory_order_relaxed) == k)
                return Repeat;
        if (n > kPerSession)
            return Repeat;
        if (n == kPerSession) {
            count_.store(n + 1, std::memory_order_release);
            return Last;
        }
        keys_[n].store(k, std::memory_order_relaxed);
        count_.store(n + 1, std::memory_order_release);
        return First;
    }

private:
    std::mutex lock_;
    std::atomic<uint64_t> serial_{0};
    std::atomic<int> count_{0};
    std::atomic<uint64_t> keys_[kPerSession] = {};
};

inline size_t clip(int n, size_t cap)
{
    return n < 0 ? 0 : size_t(n) >= cap ? cap - 1 : size_t(n);
}

inline const char *verdict_text(Verdict v)
{
    switch (v) {
    case Negative:
        return "NEGATIVE";
    case Huge:
        return "HUGE";
    case Ragged:
        return "not a whole number of items";
    case NotRead:
        return "not read";
    default:
        return "ok";
    }
}

inline size_t line(const Entry &e, uint64_t serial, char *out, size_t cap)
{
    if (!out || !cap)
        return 0;
    if (e.gate != Runs && !e.outer.read)
        return clip(std::snprintf(out, cap,
                                  "[session-job] 0x1445acb00 entry, session %llu: vehicle %u, wheel %u; %s (object "
                                  "0x%llx, lists 0x%llx)",
                                  (unsigned long long)serial, e.vehicle, e.wheel, gate_text(e.gate),
                                  (unsigned long long)e.holder, (unsigned long long)e.lists),
                    cap);
    return clip(std::snprintf(out, cap,
                              "[session-job] 0x1445acb00 entry, session %llu: vehicle %u, wheel %u; %s; outer list "
                              "0x%llx..0x%llx (%lld bytes, %lld items, %s, loop count %llu); inner list 0x%llx..0x%llx "
                              "(%lld bytes, %lld entries, %s); %d entry lists read, %lld..%lld u32s, %d bad (object "
                              "0x%llx, lists 0x%llx)",
                              (unsigned long long)serial, e.vehicle, e.wheel, gate_text(e.gate),
                              (unsigned long long)e.outer.begin, (unsigned long long)e.outer.end,
                              (long long)e.outer.bytes(), (long long)items(e.outer, kOuterItem),
                              verdict_text(verdict(e.outer, kOuterItem)), (unsigned long long)loop_count(e.outer),
                              (unsigned long long)e.inner.begin, (unsigned long long)e.inner.end,
                              (long long)e.inner.bytes(), (long long)items(e.inner, kInnerItem),
                              verdict_text(verdict(e.inner, kInnerItem)), e.lists_read, (long long)e.list_min,
                              (long long)e.list_max, e.lists_bad, (unsigned long long)e.holder,
                              (unsigned long long)e.lists),
                cap);
}

inline size_t warning(const Entry &e, uint64_t serial, char *out, size_t cap)
{
    if (!out || !cap)
        return 0;
    Verdict o = verdict(e.outer, kOuterItem), i = verdict(e.inner, kInnerItem);
    const char *why = o == Negative ? "the outer list's end is below its begin, so its count is negative and the "
                                      "loop's unsigned exit test never ends it"
                      : o == Huge   ? "the outer list's count is huge, so the loop runs for hours"
                      : i == Negative ? "the inner list's end is below its begin, so its scan runs off the list"
                      : i == Huge     ? "the inner list's count is huge"
                      : e.lists_bad   ? "an entry's u32 list has its end below its begin or is not whole u32s"
                      : e.list_max > kHuge ? "an entry's u32 list is huge"
                      : o == Ragged   ? "the outer list is not a whole number of 0x20-byte items"
                                      : "the inner list is not a whole number of pointers";
    return clip(std::snprintf(out, cap,
                              "[session-job] [warning] 0x1445acb00 entry, session %llu, vehicle %u, wheel %u: %s (outer %lld "
                              "bytes at 0x%llx, inner %lld bytes at 0x%llx, longest entry list %lld u32s)",
                              (unsigned long long)serial, e.vehicle, e.wheel, why, (long long)e.outer.bytes(),
                              (unsigned long long)e.outer.begin, (long long)e.inner.bytes(),
                              (unsigned long long)e.inner.begin, (long long)e.list_max),
                cap);
}

inline size_t last_line(uint64_t serial, char *out, size_t cap)
{
    if (!out || !cap)
        return 0;
    return clip(std::snprintf(out, cap,
                              "[session-job] 0x1445acb00: %d distinct values logged in session %llu; further values "
                              "this session are not logged",
                              kPerSession, (unsigned long long)serial),
                cap);
}

}
