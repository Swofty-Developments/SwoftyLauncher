#pragma once

#include "core/diagnostics/crash/stall_order_logic.hpp"

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>

// The session loader section of the load-stall report (session_loader.cpp). Every SWOFTY 2011 Grand Prix unload in
// 1.10.16-1.10.24 sat on the loading screen with StateUnloadSession in Updating: that state leaves only once its
// loader callback runs, and the session loader calls it from its completion delegate. The loader's per-frame tick
// (0x142d4a320, Game_GameModule_Update) switches on loader +0x8; in state 7 it runs Game_StreamingAssetLoadTask_Update
// (0x142d4b6a0) on the loader, which starts the asset load at +0x10 == 0 (and sets 1), and at +0x10 == 2 checks every
// u32 id in +0x188..+0x190 with the id system at +0xc0 (0x142e39bb0) and returns at the first one that does not report
// 2. Once every id reports 2 it calls the delegate {+0x220 function, +0x228 argument}, clears it and sets +0x8 to 2.
// The section names the loader the two entry probes saw, those fields, every id with the state the game's lookup gives
// it, the first id the update stops at, and the first 0x300 bytes of the loader raw.
//
// The id lookup is a robin-hood node map at the id system (+0x70 hash multiplier, +0x78 slots, +0x80 info bytes,
// +0x90 mask, +0xa0 info increment, +0xa4 info hash shift; a slot points at a node whose u32 +0x0 is the id and u32
// +0xc its row key) and an MSVC std::map of rows at [[system+0x20]+0xc8] (node +0x0 left, +0x8 parent, +0x10 right,
// +0x19 is-nil, u32 key +0x20, u32 state +0x28) searched for the lower bound of the row key. lookup() follows the same
// steps with bounded loops and a read that fails instead of faulting. The game answers 0 for an id that is not in the
// map and for one whose row is missing; lookup() tells the two apart.
//
// Every offset is read by the same instruction in the same function on the KB build, EA App (KB build), Steam 1.0.145
// and EA App 1.0.145 (re/tools/sigport.py --witness of 0x2d4b6a0, 0x2d4a320 and 0x2e39bb0; re/research/
// loading_pipeline.md §1.9). On another build only the raw bytes are dumped.

namespace runtime::session_loader {

constexpr uint64_t kState = 0x8;
constexpr uint64_t kAssets = 0x10;
constexpr uint64_t kSubtasks = 0x1c;
constexpr uint64_t kIdSystem = 0xc0;
constexpr uint64_t kBatch = 0x158;
constexpr uint64_t kIdsBegin = 0x188;
constexpr uint64_t kIdsEnd = 0x190;
constexpr uint64_t kDone = 0x220;
constexpr uint64_t kDoneArg = 0x228;

constexpr uint64_t kOwner = 0x20;
constexpr uint64_t kMultiplier = 0x70;
constexpr uint64_t kSlots = 0x78;
constexpr uint64_t kInfo = 0x80;
constexpr uint64_t kMask = 0x90;
constexpr uint64_t kInfoInc = 0xa0;
constexpr uint64_t kInfoShift = 0xa4;
constexpr uint64_t kRows = 0xc8;
constexpr uint64_t kNodeId = 0x0;
constexpr uint64_t kNodeKey = 0xc;
constexpr uint64_t kLeft = 0x0;
constexpr uint64_t kParent = 0x8;
constexpr uint64_t kRight = 0x10;
constexpr uint64_t kNil = 0x19;
constexpr uint64_t kRowKey = 0x20;
constexpr uint64_t kRowState = 0x28;

constexpr uint32_t kReady = 2;
constexpr uint32_t kTaskState = 7;
constexpr size_t kMaxIds = 4096;
constexpr size_t kRaw = 0x300;
constexpr size_t kBatchRaw = 0x30;
constexpr size_t kRow = 16;
constexpr int kProbes = 1024;
constexpr int kDepth = 96;
constexpr int kPerLine = 4;
constexpr int kDetailed = 64;

enum Where : uint8_t { Unread, Missing, RowUnread, NoRow, Found, NotLooked };

struct IdState {
    uint32_t id = 0;
    Where where = Unread;
    uint64_t node = 0;
    uint32_t key = 0;
    uint32_t state = 0;

    bool ready() const { return where == Found && state == kReady; }
};

template <class T, class Read> bool fetch(Read &read, uint64_t at, T &v)
{
    return at && at + sizeof(T) > at && read(at, &v, sizeof(T));
}

template <class Read> IdState lookup(Read &&read, uint64_t sys, uint32_t id)
{
    IdState r;
    r.id = id;
    r.where = Unread;
    uint64_t mult = 0, slots = 0, info = 0, mask = 0;
    uint32_t inc = 0;
    uint8_t shift = 0;
    if (!fetch(read, sys + kMultiplier, mult) || !fetch(read, sys + kSlots, slots) || !fetch(read, sys + kInfo, info) ||
        !fetch(read, sys + kMask, mask) || !fetch(read, sys + kInfoInc, inc) || !fetch(read, sys + kInfoShift, shift))
        return r;
    uint64_t h = uint64_t(id) * mult;
    h ^= h >> 33;
    uint32_t want = inc + uint32_t((h & 0x1f) >> (shift & 0x3f));
    uint64_t idx = (h >> 5) & mask;
    uint8_t b = 0;
    if (!fetch(read, info + idx, b))
        return r;
    uint64_t node = 0;
    for (int step = 0; step < kProbes && !node; step += 2) {
        for (int k = 0; k < 2 && !node; k++) {
            if (k && !fetch(read, info + idx + 1, b))
                return r;
            if (want + uint32_t(k) * inc != b)
                continue;
            uint64_t p = 0;
            uint32_t pid = 0;
            if (!fetch(read, slots + (idx + uint64_t(k)) * 8, p) || !fetch(read, p + kNodeId, pid))
                return r;
            if (pid == id)
                node = p;
        }
        if (node)
            break;
        want += 2 * inc;
        idx += 2;
        if (!fetch(read, info + idx, b))
            return r;
        if (want > b) {
            r.where = Missing;
            return r;
        }
    }
    if (!node)
        return r;
    r.node = node;
    r.where = RowUnread;
    uint64_t owner = 0, head = 0, x = 0;
    uint8_t nil = 0;
    if (!fetch(read, node + kNodeKey, r.key) || !fetch(read, sys + kOwner, owner) || !fetch(read, owner + kRows, head) ||
        !fetch(read, head + kParent, x) || !fetch(read, x + kNil, nil))
        return r;
    uint64_t best = head;
    for (int depth = 0; !nil; depth++) {
        uint32_t k = 0;
        uint64_t next = 0;
        if (depth >= kDepth || !fetch(read, x + kRowKey, k))
            return r;
        if (k >= r.key)
            best = x;
        if (!fetch(read, x + (k >= r.key ? kLeft : kRight), next) || !fetch(read, next + kNil, nil))
            return r;
        x = next;
    }
    uint8_t best_nil = 0;
    uint32_t best_key = 0;
    if (!fetch(read, best + kNil, best_nil))
        return r;
    if (best_nil || best == head) {
        r.where = NoRow;
        return r;
    }
    if (!fetch(read, best + kRowKey, best_key))
        return r;
    if (r.key < best_key) {
        r.where = NoRow;
        return r;
    }
    if (!fetch(read, best + kRowState, r.state))
        return r;
    r.where = Found;
    return r;
}

struct Loader {
    bool read = false;
    uint32_t state = 0, assets = 0, subtasks = 0;
    uint64_t id_system = 0, batch = 0, ids_begin = 0, ids_end = 0, done = 0, done_arg = 0;
};

template <class Read> Loader read_loader(Read &&read, uint64_t at)
{
    Loader l;
    l.read = fetch(read, at + kState, l.state) && fetch(read, at + kAssets, l.assets) &&
             fetch(read, at + kSubtasks, l.subtasks) && fetch(read, at + kIdSystem, l.id_system) &&
             fetch(read, at + kBatch, l.batch) && fetch(read, at + kIdsBegin, l.ids_begin) &&
             fetch(read, at + kIdsEnd, l.ids_end) && fetch(read, at + kDone, l.done) && fetch(read, at + kDoneArg, l.done_arg);
    return l;
}

enum IdsVerdict : uint8_t { IdsUnread, IdsOk, IdsEmpty, IdsBackwards, IdsRagged, IdsHuge };

inline IdsVerdict ids_verdict(const Loader &l)
{
    if (l.ids_end == l.ids_begin)
        return IdsEmpty;
    if (l.ids_end < l.ids_begin)
        return IdsBackwards;
    if ((l.ids_end - l.ids_begin) % 4)
        return IdsRagged;
    if ((l.ids_end - l.ids_begin) / 4 > uint64_t(1) << 20)
        return IdsHuge;
    return IdsOk;
}

struct Seen {
    bool hooked = false;
    uint64_t at = 0, ms = 0, calls = 0;
};

struct Snapshot {
    bool off = false;
    char why[256] = {};
    Seen update, unload;
    uint64_t now = 0;
    uint64_t at = 0;
    uint64_t other = 0;
    bool proven = false;
    char proof[320] = {};
    Loader loader;
    IdsVerdict verdict = IdsUnread;
    size_t ids_total = 0, ids_read = 0;
    bool looked = false;
    IdState ids[kMaxIds];
    uint8_t raw[kRaw] = {};
    bool raw_row[kRaw / kRow] = {};
    uint8_t batch[kBatchRaw] = {};
    bool batch_row[kBatchRaw / kRow] = {};
};

inline uint64_t pick(const Seen &update, const Seen &unload, uint64_t &other)
{
    const Seen *a = update.ms >= unload.ms ? &update : &unload, *b = a == &update ? &unload : &update;
    other = b->at && b->at != a->at ? b->at : 0;
    return a->at;
}

template <class Read> void capture(Read &&read, Snapshot &s)
{
    s.ids_total = s.ids_read = 0;
    s.verdict = IdsUnread;
    s.looked = false;
    s.loader = Loader{};
    s.at = pick(s.update, s.unload, s.other);
    if (!s.at)
        return;
    for (size_t r = 0; r < kRaw / kRow; r++)
        s.raw_row[r] = read(s.at + r * kRow, s.raw + r * kRow, kRow);
    if (!s.proven)
        return;
    s.loader = read_loader(read, s.at);
    if (!s.loader.read)
        return;
    for (size_t r = 0; r < kBatchRaw / kRow; r++)
        s.batch_row[r] = s.loader.batch && read(s.loader.batch + r * kRow, s.batch + r * kRow, kRow);
    s.verdict = ids_verdict(s.loader);
    if (s.verdict != IdsOk)
        return;
    s.ids_total = size_t((s.loader.ids_end - s.loader.ids_begin) / 4);
    size_t n = s.ids_total < kMaxIds ? s.ids_total : kMaxIds;
    for (size_t i = 0; i < n; i++) {
        uint32_t id = 0;
        if (!fetch(read, s.loader.ids_begin + i * 4, id))
            break;
        s.ids[i] = IdState{};
        s.ids[i].id = id;
        s.ids[i].where = NotLooked;
        s.ids_read = i + 1;
    }
    s.looked = s.loader.id_system != 0;
    for (size_t i = 0; i < s.ids_read && s.looked; i++)
        s.ids[i] = lookup(read, s.loader.id_system, s.ids[i].id);
}

inline const char *state_text(uint32_t s)
{
    switch (s) {
    case 1:
        return "its tick runs Sess_Loader_Update";
    case 3:
        return "its tick waits for +0x1c == 1 and its sub-tasks, then calls the completion and sets +0x8 to 4";
    case 5:
        return "its tick runs Sess_Module_OnSessionUnloaded";
    case 6:
        return "its tick runs Sess_Loader_UpdateFrontendLoad";
    case 7:
        return "its tick runs Game_StreamingAssetLoadTask_Update";
    default:
        return "its tick does nothing";
    }
}

inline const char *assets_text(uint32_t a)
{
    switch (a) {
    case 0:
        return "the update starts the asset load and sets 1";
    case 2:
        return "the update waits until every id below reports 2";
    default:
        return "the update returns at once";
    }
}

inline const char *verdict_text(IdsVerdict v)
{
    switch (v) {
    case IdsEmpty:
        return "empty: the update asks for the thumbnails again (Sess_Loader_RequestLoadThumbnails) and completes if it "
               "stays empty";
    case IdsBackwards:
        return "the end is below the begin";
    case IdsRagged:
        return "not a whole number of u32 ids";
    case IdsHuge:
        return "more than 1048576 ids";
    case IdsUnread:
        return "not read";
    default:
        return "";
    }
}

inline void id_value(const IdState &e, char *out, size_t cap)
{
    switch (e.where) {
    case Found:
        std::snprintf(out, cap, "%u", e.state);
        break;
    case Missing:
        std::snprintf(out, cap, "none");
        break;
    case NoRow:
        std::snprintf(out, cap, "no-row");
        break;
    case RowUnread:
        std::snprintf(out, cap, "row?");
        break;
    case NotLooked:
        std::snprintf(out, cap, "-");
        break;
    default:
        std::snprintf(out, cap, "?");
        break;
    }
}

inline const char *where_text(Where w)
{
    switch (w) {
    case Found:
        return "its row's state";
    case Missing:
        return "not in the id map (the game's lookup answers 0)";
    case NoRow:
        return "in the id map with no state row (the game's lookup answers 0)";
    case RowUnread:
        return "in the id map, its state row not readable";
    case NotLooked:
        return "not looked up";
    default:
        return "the id map is not readable";
    }
}

struct Counts {
    size_t ready = 0, not_ready = 0, unread = 0;
    long long stop = -1;
};

inline Counts count(const Snapshot &s)
{
    Counts c;
    bool blind = false;
    for (size_t i = 0; i < s.ids_read; i++) {
        const IdState &e = s.ids[i];
        bool unknown = !s.looked || e.where == Unread || e.where == RowUnread;
        if (unknown)
            c.unread++;
        else if (e.ready())
            c.ready++;
        else
            c.not_ready++;
        if (c.stop < 0 && !blind && !unknown && !e.ready())
            c.stop = (long long)i;
        blind = blind || unknown;
    }
    return c;
}

using Namer = void (*)(char *out, size_t cap, uint64_t at);

inline void seen_text(stall_order::Out &o, const char *name, const Seen &v, uint64_t now)
{
    if (!v.hooked)
        o.put("  %s: not hooked\n", name);
    else if (!v.calls)
        o.put("  %s: not run since it was hooked\n", name);
    else
        o.put("  %s: last run %.1f s ago on 0x%llx, %llu call(s)\n", name, now >= v.ms ? double(now - v.ms) / 1000 : 0.0,
              (unsigned long long)v.at, (unsigned long long)v.calls);
}

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

inline size_t text(const Snapshot &s, char *out, size_t cap, Namer name)
{
    stall_order::Out o(out, cap);
    o.put("\n== Session loader (entry probes on Game_StreamingAssetLoadTask_Update and Sess_Loader_UnloadSession; read "
          "while the game runs)\n");
    if (s.off) {
        o.put("  off: %s\n", s.why);
        return o.size();
    }
    seen_text(o, "Game_StreamingAssetLoadTask_Update", s.update, s.now);
    seen_text(o, "Sess_Loader_UnloadSession", s.unload, s.now);
    if (!s.at) {
        o.put("  no session loader seen yet\n");
        return o.size();
    }
    o.put("loader 0x%llx", (unsigned long long)s.at);
    if (s.other)
        o.put(" (the other probe last saw 0x%llx)", (unsigned long long)s.other);
    o.put("\n");
    if (!s.proven)
        o.put("  fields not decoded: %s\n", s.proof);
    else if (!s.loader.read)
        o.put("  its fields are not readable\n");
    else {
        const Loader &l = s.loader;
        char w[512];
        o.put("  +0x8    state %u: %s\n", l.state, state_text(l.state));
        o.put("  +0x10   asset load %u: %s\n", l.assets, assets_text(l.assets));
        o.put("  +0x1c   sub-task state %u\n", l.subtasks);
        o.put("  +0x158  load batch 0x%llx\n", (unsigned long long)l.batch);
        if (l.done || l.done_arg) {
            w[0] = 0;
            if (name && l.done)
                name(w, sizeof w, l.done);
            o.put("  +0x220  completion 0x%llx%s%s%s, argument (+0x228) 0x%llx\n", (unsigned long long)l.done, w[0] ? " (" : "",
                  w, w[0] ? ")" : "", (unsigned long long)l.done_arg);
        } else
            o.put("  +0x220  completion none (+0x220 and +0x228 are 0)\n");
        if (l.id_system)
            o.put("  +0xc0   id system 0x%llx\n", (unsigned long long)l.id_system);
        else
            o.put("  +0xc0   id system 0: the update does not wait on the ids\n");
        o.put("  +0x188  ids 0x%llx..0x%llx", (unsigned long long)l.ids_begin, (unsigned long long)l.ids_end);
        if (s.verdict != IdsOk)
            o.put(": %s\n", verdict_text(s.verdict));
        else {
            Counts c = count(s);
            o.put(": %zu id(s)", s.ids_total);
            if (s.ids_read < s.ids_total)
                o.put(", the first %zu read", s.ids_read);
            if (s.looked)
                o.put("; %zu report 2, %zu do not, %zu not readable\n", c.ready, c.not_ready, c.unread);
            else
                o.put("; states not looked up\n");
            if (l.assets == kReady && l.id_system && c.stop >= 0) {
                const IdState &e = s.ids[c.stop];
                o.put("  the update stops at id #%lld 0x%08x: %s", c.stop, e.id, where_text(e.where));
                if (e.where == Found)
                    o.put(" %u", e.state);
                o.put("\n");
            } else if (l.assets == kReady && l.id_system && s.looked && !c.not_ready && !c.unread && s.ids_read == s.ids_total)
                o.put("  every id reports 2: the update calls the completion on its next run\n");
            for (size_t i = 0; i < s.ids_read; i++) {
                char v[16];
                id_value(s.ids[i], v, sizeof v);
                bool last = i % kPerLine == kPerLine - 1 || i + 1 == s.ids_read;
                o.put("%s#%-4zu %08x=%-*s%s", i % kPerLine ? " " : "    ", i, s.ids[i].id, last ? 0 : 6, v, last ? "\n" : "");
            }
            int shown = 0;
            for (size_t i = 0; i < s.ids_read && s.looked; i++) {
                const IdState &e = s.ids[i];
                if (e.ready() || e.where == Missing || e.where == Unread)
                    continue;
                if (shown++ == kDetailed) {
                    o.put("    (more ids that do not report 2 are not detailed)\n");
                    break;
                }
                o.put("    #%zu %08x: node 0x%llx, row key 0x%08x, %s", i, e.id, (unsigned long long)e.node, e.key,
                      where_text(e.where));
                if (e.where == Found)
                    o.put(" %u", e.state);
                o.put("\n");
            }
        }
        if (l.batch) {
            o.put("  load batch bytes at 0x%llx (layout not proven):\n", (unsigned long long)l.batch);
            rows_text(o, s.batch, s.batch_row, kBatchRaw);
        }
    }
    o.put("  loader bytes +0x000..+0x%zx at 0x%llx:\n", kRaw, (unsigned long long)s.at);
    rows_text(o, s.raw, s.raw_row, kRaw);
    return o.size();
}

}
