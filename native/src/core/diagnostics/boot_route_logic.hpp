#pragma once

#include "core/diagnostics/crash/stall_order_logic.hpp"

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>

// The front end boot route section of the load-stall report (boot_route.cpp). 1.10.26 Steam boots, with frame
// generation on and off, stalled with StateFTUE (#04fc8ffa) in Updating on splash_transition and never reached
// StateFrontend.
// FeFlow_StateFTUE_OnUpdate (KB 0x1436d80e0) returns g_Game_FlowLinkStay while the byte at +0xa8 of the front end's
// boot route record is 0, the record's job (+0x8) is set and that job's u32 +0x48 is not 0; otherwise it leaves.
// StateIntro fills the record when the title's press interrupt (kind 1) fires the scene route 'enter_boot': the
// route's name is copied into the record's buffer +0x28 (pointer +0x20, capacity +0x18) and the route's +0xa8 into
// the record's +0xa8. Ne_AsyncJob_StopAndNotify (KB 0x14416cf60), which StateFTUE's interrupt handler calls on the
// job, reads the job's status at +0x48 (2 and 5 run; it writes 3 to +0x48 and 2 to +0x4c) and calls each listener
// {+0x0 function, +0x8 argument} of the 16-byte list at +0x100..+0x108.
//
// The section reads the two globals the update reads, decoded from its own two rip-relative operands, the record's
// route name, the job, its two status words and its listeners, and dumps the record and the job raw. The signature
// spans the whole update (its reads at +0x48 included) and matches once on the KB build, EA App (KB build), Steam
// 1.0.145 and EA App 1.0.145; StateIntro's record store (record +0x20, +0x28, +0xa8) and StopAndNotify's job fields
// (+0x48, +0x4c, +0x100, +0x108) match once on the same four. On another build only the update's own reads and the
// raw bytes are shown.

namespace runtime::boot_route {

constexpr const char *kPattern =
    "56 48 83 EC 20 48 89 D6 8B 05 ?? ?? ?? ?? 89 02 80 3D ?? ?? ?? ?? 00 74 42 48 8B 81 F8 0B 00 00 48 "
    "8B 50 18 4C 8D 40 30 48 39 50 20 4C 0F 45 C2 41 83 78 10 0E 75 09 48 8D 05 ?? ?? ?? ?? EB "
    "0E 31 D2 E8 ?? ?? ?? ?? 48 8D 05 ?? ?? ?? ?? 8B 00 89 06 48 89 F0 48 83 C4 20 5E C3 48 8B "
    "05 ?? ?? ?? ?? 48 85 C0 74 B2 83 78 48 00 75 E5 EB AA";
constexpr uintptr_t kKbRva = 0x36d80e0;

constexpr uint64_t kFlagInsn = 0x10;
constexpr uint64_t kFlagDisp = 0x12;
constexpr uint64_t kFlagLen = 7;
constexpr uint64_t kJobInsn = 0x5b;
constexpr uint64_t kJobDisp = 0x5e;
constexpr uint64_t kJobLen = 7;

constexpr uint64_t kRecordJob = 0x8;
constexpr uint64_t kNameCap = 0x18;
constexpr uint64_t kName = 0x20;
constexpr uint64_t kNameBuf = 0x28;
constexpr uint64_t kFlag = 0xa8;

constexpr uint64_t kStatus = 0x48;
constexpr uint64_t kSubStatus = 0x4c;
constexpr uint64_t kListeners = 0x100;
constexpr uint64_t kListenersEnd = 0x108;
constexpr uint64_t kListener = 16;

constexpr size_t kNameMax = 0x80;
constexpr size_t kMaxListeners = 16;
constexpr size_t kRow = 16;
constexpr size_t kRecordRaw = 0xd0;
constexpr size_t kJobRaw = 0x200;

template <class T, class Read> bool fetch(Read &read, uint64_t at, T &v)
{
    return at && at + sizeof(T) > at && read(at, &v, sizeof(T));
}

struct Where {
    uint64_t fn = 0, record = 0, flag = 0;
};

template <class Read> bool locate(Read &&read, uint64_t fn, Where &w)
{
    int32_t flag = 0, job = 0;
    if (!fetch(read, fn + kFlagDisp, flag) || !fetch(read, fn + kJobDisp, job))
        return false;
    w.fn = fn;
    w.flag = uint64_t(int64_t(fn + kFlagInsn + kFlagLen) + flag);
    w.record = uint64_t(int64_t(fn + kJobInsn + kJobLen) + job) - kRecordJob;
    return true;
}

struct Snapshot {
    bool off = false;
    char why[256] = {};
    uint64_t base = 0;
    Where at;
    bool proven = false;
    char proof[320] = {};

    bool job_read = false, flag_read = false, status_read = false, vtable_read = false;
    uint64_t job = 0, vtable = 0;
    uint8_t flag = 0;
    uint32_t status = 0, sub = 0;

    bool name_read = false;
    uint64_t name_at = 0, name_cap = 0;
    char name[kNameMax + 1] = {};

    bool listeners_read = false;
    uint64_t listeners_begin = 0, listeners_end = 0;
    size_t listeners_total = 0, listeners_shown = 0;
    uint64_t listener_fn[kMaxListeners] = {}, listener_arg[kMaxListeners] = {};

    uint8_t record_raw[kRecordRaw] = {};
    bool record_row[kRecordRaw / kRow] = {};
    uint8_t job_raw[kJobRaw] = {};
    bool job_row[kJobRaw / kRow] = {};
};

template <class Read> void read_name(Read &read, Snapshot &s)
{
    s.name_read = false;
    s.name[0] = 0;
    if (!s.name_at)
        return;
    size_t n = 0;
    for (; n < kNameMax; n++) {
        char c = 0;
        if (!fetch(read, s.name_at + n, c))
            return;
        if (!c)
            break;
        s.name[n] = c >= 0x20 && c < 0x7f ? c : '?';
    }
    s.name[n] = 0;
    s.name_read = true;
}

template <class Read> void capture(Read &&read, Snapshot &s)
{
    s.job_read = fetch(read, s.at.record + kRecordJob, s.job);
    s.flag_read = fetch(read, s.at.flag, s.flag);
    s.status_read = s.job_read && s.job && fetch(read, s.job + kStatus, s.status) && fetch(read, s.job + kSubStatus, s.sub);
    for (size_t r = 0; r < kRecordRaw / kRow; r++)
        s.record_row[r] = read(s.at.record + r * kRow, s.record_raw + r * kRow, kRow);
    for (size_t r = 0; r < kJobRaw / kRow; r++)
        s.job_row[r] = s.job_read && s.job && read(s.job + r * kRow, s.job_raw + r * kRow, kRow);
    if (!s.proven)
        return;
    s.vtable_read = s.job_read && s.job && fetch(read, s.job, s.vtable);
    if (fetch(read, s.at.record + kName, s.name_at) && fetch(read, s.at.record + kNameCap, s.name_cap))
        read_name(read, s);
    s.listeners_read = s.job_read && s.job && fetch(read, s.job + kListeners, s.listeners_begin) &&
                       fetch(read, s.job + kListenersEnd, s.listeners_end);
    s.listeners_total = s.listeners_shown = 0;
    if (!s.listeners_read || s.listeners_end < s.listeners_begin || (s.listeners_end - s.listeners_begin) % kListener)
        return;
    s.listeners_total = size_t((s.listeners_end - s.listeners_begin) / kListener);
    for (size_t i = 0; i < s.listeners_total && i < kMaxListeners; i++) {
        uint64_t at = s.listeners_begin + i * kListener;
        if (!fetch(read, at, s.listener_fn[i]) || !fetch(read, at + 8, s.listener_arg[i]))
            break;
        s.listeners_shown = i + 1;
    }
}

enum Verdict : uint8_t { Unread, Leaves, LeavesNoJob, LeavesIdle, Stays };

inline Verdict verdict(const Snapshot &s)
{
    if (!s.flag_read)
        return Unread;
    if (s.flag)
        return Leaves;
    if (!s.job_read)
        return Unread;
    if (!s.job)
        return LeavesNoJob;
    if (!s.status_read)
        return Unread;
    return s.status ? Stays : LeavesIdle;
}

inline const char *verdict_text(Verdict v)
{
    switch (v) {
    case Leaves:
        return "the record's +0xa8 is set: StateFTUE leaves on its next update";
    case LeavesNoJob:
        return "the record has no job: StateFTUE leaves on its next update";
    case LeavesIdle:
        return "the job's +0x48 is 0: StateFTUE leaves on its next update";
    case Stays:
        return "StateFTUE stays in Updating while the job's +0x48 is not 0 and the record's +0xa8 is 0";
    default:
        return "not readable";
    }
}

using Namer = void (*)(char *out, size_t cap, uint64_t at);

inline void rows_text(stall_order::Out &o, const uint8_t *b, const bool *ok, size_t n)
{
    for (size_t r = 0; r < n / kRow; r++) {
        if (!ok[r]) {
            o.put("    +%03zx  not readable\n", r * kRow);
            continue;
        }
        uint64_t q[2];
        std::memcpy(q, b + r * kRow, sizeof q);
        o.put("    +%03zx  %016llx %016llx\n", r * kRow, (unsigned long long)q[0], (unsigned long long)q[1]);
    }
}

inline unsigned long long rva(const Snapshot &s, uint64_t at)
{
    return (unsigned long long)(s.base && at >= s.base ? at - s.base : at);
}

inline size_t text(const Snapshot &s, char *out, size_t cap, Namer name)
{
    stall_order::Out o(out, cap);
    o.put("\n== Front end boot route (what StateFTUE #04fc8ffa waits on before StateFrontend; read while the game runs)\n");
    if (s.off) {
        o.put("  off: %s\n", s.why);
        return o.size();
    }
    o.put("  FeFlow_StateFTUE_OnUpdate base+0x%llx reads the route record at base+0x%llx and its flag at base+0x%llx\n",
          rva(s, s.at.fn), rva(s, s.at.record), rva(s, s.at.flag));
    if (s.at.flag != s.at.record + kFlag)
        o.put("  the flag is not at the record's +0xa8 on this build\n");
    char w[512];
    if (!s.proven)
        o.put("  route name, job vtable and listeners not decoded: %s\n", s.proof);
    else if (s.name_read)
        o.put("  route '%s' (record +0x20 0x%llx, capacity %llu)\n", s.name, (unsigned long long)s.name_at,
              (unsigned long long)s.name_cap);
    else
        o.put("  route name not readable (record +0x20 0x%llx)\n", (unsigned long long)s.name_at);
    if (!s.job_read)
        o.put("  +0x8    job not readable\n");
    else if (!s.job)
        o.put("  +0x8    job none\n");
    else {
        w[0] = 0;
        if (name && s.vtable_read && s.vtable)
            name(w, sizeof w, s.vtable);
        o.put("  +0x8    job 0x%llx", (unsigned long long)s.job);
        if (s.vtable_read)
            o.put(", vtable 0x%llx%s%s%s", (unsigned long long)s.vtable, w[0] ? " (" : "", w, w[0] ? ")" : "");
        o.put("\n");
        if (s.status_read)
            o.put("  job +0x48 status %u, +0x4c %u\n", s.status, s.sub);
        else
            o.put("  job +0x48 not readable\n");
    }
    if (s.flag_read)
        o.put("  +0xa8   flag %u\n", s.flag);
    else
        o.put("  +0xa8   flag not readable\n");
    o.put("  %s\n", verdict_text(verdict(s)));
    if (s.proven && s.job_read && s.job) {
        if (!s.listeners_read)
            o.put("  job +0x100 listeners not readable\n");
        else if (s.listeners_end < s.listeners_begin || (s.listeners_end - s.listeners_begin) % kListener)
            o.put("  job +0x100 listeners 0x%llx..0x%llx: not a list of 16-byte entries\n", (unsigned long long)s.listeners_begin,
                  (unsigned long long)s.listeners_end);
        else {
            o.put("  job +0x100 listeners: %zu\n", s.listeners_total);
            for (size_t i = 0; i < s.listeners_shown; i++) {
                w[0] = 0;
                if (name && s.listener_fn[i])
                    name(w, sizeof w, s.listener_fn[i]);
                o.put("    #%zu 0x%llx%s%s%s, argument 0x%llx\n", i, (unsigned long long)s.listener_fn[i], w[0] ? " (" : "", w,
                      w[0] ? ")" : "", (unsigned long long)s.listener_arg[i]);
            }
            if (s.listeners_shown < s.listeners_total)
                o.put("    (%zu more not read)\n", s.listeners_total - s.listeners_shown);
        }
    }
    o.put("  record bytes +0x00..+0x%zx at 0x%llx:\n", kRecordRaw, (unsigned long long)s.at.record);
    rows_text(o, s.record_raw, s.record_row, kRecordRaw);
    if (s.job_read && s.job) {
        o.put("  job bytes +0x000..+0x%zx at 0x%llx:\n", kJobRaw, (unsigned long long)s.job);
        rows_text(o, s.job_raw, s.job_row, kJobRaw);
    }
    return o.size();
}

}
