#pragma once

#include "core/diagnostics/crash/stall_order_logic.hpp"
#include "core/runtime/build_proof.hpp"

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
// On Steam 1.0.145 and EA App 1.0.145 the section also decodes the state 5 unload tick (Sess_Module_OnSessionUnloaded, 0x142d4d3b0): block A
// (+0x1c == 5, every module in [+0x200,+0x208) answers vcall +0x30), block B (+0x24 == 2, the +0x1f0 task) and block C
// (+0xc, +0x14, +0x1c, +0x10, +0x110 all 0 and every sub-task in [+0x1d8,+0x1e0) answers +0x30). It prints the gate fields,
// each module's vtable and unload-ready target, and for the vehicle spawning sub-task (+0x1f0, else the first module with
// its vtable) the id list at [task+0x88,+0x90), per id the spawn request state in the spawning system's map at [task+0x60]
// and whether the id is in the system's active list [sys+0x28,+0x30), stride 0x6880.
//
// Every offset is read by the same instruction in the same function on the KB build, EA App (KB build), Steam 1.0.145
// and EA App 1.0.145 (re/tools/sigport.py --witness of 0x2d4b6a0, 0x2d4a320 and 0x2e39bb0; re/research/
// loading_pipeline.md §1.9). On another build only the raw bytes are dumped.
//
// The unload tick decode holds on EA App 1.0.145 too (re/research/ea-1-0-145.md, "Session loader unload gates"): the
// state 5 tick is equal to the KB's over all 392 instructions with every gate and list offset used at the same
// instruction, the vehicle spawning sub-task's constructor is equal over 541, and the unload-ready function and its
// active-record helper differ from Steam's only by protector filler. The sub-task's vtable is at RVA 0x58628b0 on Steam
// and 0x5974bd0 on EA, both the vtable the constructor stores.

// On Steam 1.0.145 the section also decodes each module of the +0x200 list that is a world task (Ne_WorldLoadTask,
// vtables +0x57e8ba0 primary and +0x5a8f650 secondary): busy byte +0x88 (cleared only by the world stop callback,
// 0x14442f3b0), kind +0xc8, stop flag +0xcc, the world name +0xa0 (a C string, compared case-insensitively), the task's
// module +0x60 with its primary world +0xe8 and its world lists +0xf0 (active) and +0xf8 (stopping), and per world its
// name +0xa0, state +0x200 and stop-pending byte +0x218. The world stop (0x153daf400) refuses with 0x25, and never calls
// back, when the world's state is 0x7d4..0x7d6 or a stop is already pending. entering_line() is the same data as one
// line per module for the log, written at every StateUnloadSession Entering.

namespace runtime::session_loader {

constexpr uint64_t kSpawnVtableSteam = 0x58628b0;
constexpr uint64_t kSpawnVtableEa = 0x5974bd0;

constexpr uint64_t kWorldVtableSteam[2] = {0x57e8ba0, 0x5a8f650};

constexpr bool world_task_vtables(build_proof::Build b, uint64_t (&out)[2])
{
    if (b != build_proof::Build::steam_1_0_145)
        return false;
    out[0] = kWorldVtableSteam[0];
    out[1] = kWorldVtableSteam[1];
    return true;
}

constexpr uint64_t spawn_vtable_rva(build_proof::Build b)
{
    switch (b) {
    case build_proof::Build::steam_1_0_145:
        return kSpawnVtableSteam;
    case build_proof::Build::ea_1_0_145:
        return kSpawnVtableEa;
    default:
        return 0;
    }
}

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

constexpr uint64_t kGate0c = 0xc;
constexpr uint64_t kGate10 = 0x10;
constexpr uint64_t kGate14 = 0x14;
constexpr uint64_t kGate1c = 0x1c;
constexpr uint64_t kGate24 = 0x24;
constexpr uint64_t kGate110 = 0x110;
constexpr uint64_t kListCBegin = 0x1d8;
constexpr uint64_t kListCEnd = 0x1e0;
constexpr uint64_t kVehicleTask = 0x1f0;
constexpr uint64_t kListABegin = 0x200;
constexpr uint64_t kListAEnd = 0x208;
constexpr uint64_t kListCStride = 16;
constexpr uint64_t kListAStride = 8;
constexpr uint64_t kModuleVtable = 0x0;
constexpr uint64_t kModuleUnloadReady = 0x30;
constexpr uint64_t kTaskSystem = 0x60;
constexpr uint64_t kTaskIdsBegin = 0x88;
constexpr uint64_t kTaskIdsEnd = 0x90;
constexpr uint64_t kSysActiveBegin = 0x28;
constexpr uint64_t kSysActiveEnd = 0x30;
constexpr uint64_t kActiveStride = 0x6880;
constexpr uint64_t kNodeRequest = 0x8;
constexpr uint64_t kTaskBusy = 0x88;
constexpr uint64_t kTaskModule = 0x60;
constexpr uint64_t kTaskWorldName = 0xa0;
constexpr uint64_t kTaskKind = 0xc8;
constexpr uint64_t kTaskStopFlag = 0xcc;
constexpr uint64_t kWorldModulePrimary = 0xe8;
constexpr uint64_t kWorldModuleActive = 0xf0;
constexpr uint64_t kWorldModuleStopping = 0xf8;
constexpr uint64_t kVecBegin = 0x8;
constexpr uint64_t kVecEnd = 0x10;
constexpr uint64_t kWorldName = 0xa0;
constexpr uint64_t kWorldState = 0x200;
constexpr uint64_t kWorldStopPending = 0x218;
constexpr uint32_t kWorldStateStopFirst = 0x7d4;
constexpr uint32_t kWorldStateStopLast = 0x7d6;
constexpr size_t kMaxWorldTasks = 8;
constexpr size_t kMaxWorlds = 8;
constexpr size_t kNameMax = 48;
constexpr size_t kMaxModules = 32;
constexpr size_t kMaxVehicles = 64;
constexpr size_t kMaxActive = 128;

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

struct MapLayout {
    uint64_t mult, slots, info, mask, inc, shift;
};

constexpr MapLayout kIdMap{kMultiplier, kSlots, kInfo, kMask, kInfoInc, kInfoShift};
constexpr MapLayout kSpawnMap{0x60, 0x68, 0x70, 0x80, 0x90, 0x94};

template <class Read> Where find_node(Read &&read, const MapLayout &m, uint64_t sys, uint32_t id, uint64_t &node)
{
    node = 0;
    uint64_t mult = 0, slots = 0, info = 0, mask = 0;
    uint32_t inc = 0;
    uint8_t shift = 0;
    if (!fetch(read, sys + m.mult, mult) || !fetch(read, sys + m.slots, slots) || !fetch(read, sys + m.info, info) ||
        !fetch(read, sys + m.mask, mask) || !fetch(read, sys + m.inc, inc) || !fetch(read, sys + m.shift, shift))
        return Unread;
    uint64_t h = uint64_t(id) * mult;
    h ^= h >> 33;
    uint32_t want = inc + uint32_t((h & 0x1f) >> (shift & 0x3f));
    uint64_t idx = (h >> 5) & mask;
    uint8_t b = 0;
    if (!fetch(read, info + idx, b))
        return Unread;
    for (int step = 0; step < kProbes && !node; step += 2) {
        for (int k = 0; k < 2 && !node; k++) {
            if (k && !fetch(read, info + idx + 1, b))
                return Unread;
            if (want + uint32_t(k) * inc != b)
                continue;
            uint64_t p = 0;
            uint32_t pid = 0;
            if (!fetch(read, slots + (idx + uint64_t(k)) * 8, p) || !fetch(read, p + kNodeId, pid))
                return Unread;
            if (pid == id)
                node = p;
        }
        if (node)
            break;
        want += 2 * inc;
        idx += 2;
        if (!fetch(read, info + idx, b))
            return Unread;
        if (want > b)
            return Missing;
    }
    return node ? Found : Unread;
}

template <class Read> IdState lookup(Read &&read, uint64_t sys, uint32_t id)
{
    IdState r;
    r.id = id;
    r.where = Unread;
    uint64_t node = 0;
    Where w = find_node(read, kIdMap, sys, id, node);
    if (w != Found) {
        r.where = w;
        return r;
    }
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

struct Module {
    bool read = false;
    uint64_t object = 0, vtable = 0, unload_ready = 0;
    bool spawning = false;
};

enum Request : uint8_t { ReqUnread, ReqNoNode, ReqNull, ReqState };
enum Activity : uint8_t { ActUnread, ActNo, ActYes };

struct Vehicle {
    uint8_t id = 0;
    Request request = ReqUnread;
    uint8_t state = 0;
    Activity active = ActUnread;
    uint64_t request_at = 0;
};

struct Unload {
    bool read = false;
    uint32_t gate_0c = 0, gate_10 = 0, gate_14 = 0, gate_1c = 0, gate_24 = 0, gate_110 = 0;
    uint64_t list_c_begin = 0, list_c_end = 0, list_a_begin = 0, list_a_end = 0, vehicle_task = 0;
};

struct Spawning {
    bool found = false;
    uint64_t task = 0, vtable = 0, system = 0, ids_begin = 0, ids_end = 0, active_begin = 0, active_end = 0;
    bool task_read = false, system_read = false, ids_read_ok = false, active_read_ok = false;
    size_t ids_total = 0, vehicles = 0, active_total = 0, active_read = 0;
    Vehicle vehicle[kMaxVehicles];
    uint8_t active[kMaxActive] = {};
};

enum GateWhere : uint8_t { GateUnread, BlockA, BlockB, BlockCFields, BlockCTasks };

inline GateWhere gate_where(const Unload &u)
{
    if (!u.read)
        return GateUnread;
    if (u.gate_1c == 5)
        return BlockA;
    if (u.gate_24 == 2)
        return BlockB;
    if (u.gate_0c || u.gate_10 || u.gate_14 || u.gate_110)
        return BlockCFields;
    return BlockCTasks;
}

struct World {
    bool read = false;
    uint64_t object = 0;
    uint32_t state = 0;
    uint8_t stop_pending = 0;
    bool state_read = false;
    char name[kNameMax] = {};

    bool stop_refused() const
    {
        return state_read && ((state >= kWorldStateStopFirst && state <= kWorldStateStopLast) || stop_pending);
    }
};

struct WorldList {
    bool read = false;
    uint64_t holder = 0, begin = 0, end = 0;
    size_t total = 0, got = 0;
    World world[kMaxWorlds];
};

struct WorldTask {
    bool read = false;
    size_t index = 0;
    uint64_t object = 0, vtable = 0;
    bool world_task = false;
    bool fields_read = false;
    uint8_t busy = 0, stop_flag = 0;
    uint32_t kind = 0;
    uint64_t module = 0, name_at = 0;
    bool name_read = false;
    char name[kNameMax] = {};
    bool module_read = false;
    World primary;
    WorldList active, stopping;
    size_t active_match = 0;
};

struct WorldSet {
    bool read = false;
    uint64_t loader = 0, base = 0, list_begin = 0, list_end = 0;
    size_t total = 0, got = 0;
    WorldTask task[kMaxWorldTasks];
};

inline bool name_equal(const char *a, const char *b)
{
    for (; *a && *b; a++, b++) {
        char x = *a, y = *b;
        if (x >= 'A' && x <= 'Z')
            x = char(x + 32);
        if (y >= 'A' && y <= 'Z')
            y = char(y + 32);
        if (x != y)
            return false;
    }
    return *a == *b;
}

template <class Read> bool read_cstr(Read &&read, uint64_t at, char *out, size_t cap)
{
    out[0] = 0;
    if (!at)
        return false;
    size_t n = 0;
    for (; n + 1 < cap; n++) {
        uint8_t c = 0;
        if (!fetch(read, at + n, c)) {
            if (!n)
                return false;
            break;
        }
        if (!c) {
            out[n] = 0;
            return true;
        }
        out[n] = c >= 0x20 && c < 0x7f ? char(c) : '?';
    }
    out[n] = 0;
    uint8_t more = 0;
    if (fetch(read, at + n, more) && more && cap >= 4) {
        out[cap - 4] = '.';
        out[cap - 3] = '.';
        out[cap - 2] = '.';
        out[cap - 1] = 0;
    }
    return true;
}

template <class Read> void read_world(Read &&read, uint64_t object, World &w)
{
    w = World{};
    w.object = object;
    uint64_t name_at = 0;
    w.read = fetch(read, object + kWorldState, w.state);
    w.state_read = w.read && fetch(read, object + kWorldStopPending, w.stop_pending);
    if (!w.read)
        return;
    if (!fetch(read, object + kWorldName, name_at) || !read_cstr(read, name_at, w.name, sizeof w.name))
        std::snprintf(w.name, sizeof w.name, "%s", name_at ? "(unreadable)" : "(null)");
}

template <class Read> void read_world_list(Read &&read, uint64_t module, uint64_t at, WorldList &l)
{
    l = WorldList{};
    if (!fetch(read, module + at, l.holder) || !fetch(read, l.holder + kVecBegin, l.begin) ||
        !fetch(read, l.holder + kVecEnd, l.end))
        return;
    if (l.end < l.begin || (l.end - l.begin) % 8 || (l.end - l.begin) / 8 > (uint64_t(1) << 16))
        return;
    l.read = true;
    l.total = size_t((l.end - l.begin) / 8);
    size_t n = l.total < kMaxWorlds ? l.total : kMaxWorlds;
    for (size_t i = 0; i < n; i++) {
        uint64_t object = 0;
        if (!fetch(read, l.begin + i * 8, object))
            break;
        read_world(read, object, l.world[i]);
        l.got = i + 1;
    }
}

template <class Read> void read_world_task(Read &&read, uint64_t object, const uint64_t (&vtables)[2], WorldTask &t)
{
    uint64_t keep_index = t.index;
    t = WorldTask{};
    t.index = keep_index;
    t.object = object;
    t.read = fetch(read, object + kModuleVtable, t.vtable);
    if (!t.read)
        return;
    t.world_task = t.vtable == vtables[0] || t.vtable == vtables[1];
    if (!t.world_task)
        return;
    t.fields_read = fetch(read, object + kTaskBusy, t.busy) && fetch(read, object + kTaskKind, t.kind) &&
                    fetch(read, object + kTaskStopFlag, t.stop_flag) && fetch(read, object + kTaskModule, t.module) &&
                    fetch(read, object + kTaskWorldName, t.name_at);
    if (!t.fields_read)
        return;
    t.name_read = read_cstr(read, t.name_at, t.name, sizeof t.name);
    if (!t.name_read)
        std::snprintf(t.name, sizeof t.name, "%s", t.name_at ? "(unreadable)" : "(null)");
    t.module_read = t.module != 0;
    if (!t.module_read)
        return;
    uint64_t primary = 0;
    if (fetch(read, t.module + kWorldModulePrimary, primary) && primary)
        read_world(read, primary, t.primary);
    read_world_list(read, t.module, kWorldModuleActive, t.active);
    read_world_list(read, t.module, kWorldModuleStopping, t.stopping);
    if (t.name_read)
        for (size_t i = 0; i < t.active.got; i++)
            if (t.active.world[i].read && name_equal(t.name, t.active.world[i].name))
                t.active_match++;
}

template <class Read>
void capture_worlds(Read &&read, uint64_t loader, uint64_t base, const uint64_t (&vtables)[2], WorldSet &set)
{
    set = WorldSet{};
    set.loader = loader;
    set.base = base;
    if (!loader || !fetch(read, loader + kListABegin, set.list_begin) || !fetch(read, loader + kListAEnd, set.list_end))
        return;
    if (set.list_end < set.list_begin || (set.list_end - set.list_begin) % kListAStride)
        return;
    set.read = true;
    set.total = size_t((set.list_end - set.list_begin) / kListAStride);
    size_t n = set.total < kMaxWorldTasks ? set.total : kMaxWorldTasks;
    for (size_t i = 0; i < n; i++) {
        uint64_t object = 0;
        if (!fetch(read, set.list_begin + i * kListAStride, object))
            break;
        set.task[i].index = i;
        read_world_task(read, object, vtables, set.task[i]);
        set.got = i + 1;
    }
}

inline void world_name_into(char *out, size_t cap, const char *name) { std::snprintf(out, cap, "\"%s\"", name); }

inline size_t same_name_tasks(const WorldSet &set, const WorldTask &t)
{
    size_t n = 0;
    if (!t.name_read)
        return 0;
    for (size_t i = 0; i < set.got; i++)
        if (set.task[i].name_read && name_equal(set.task[i].name, t.name))
            n++;
    return n;
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
    bool unload_proven = false;
    char unload_proof[320] = {};
    uint64_t spawn_vtable = 0;
    bool world_proven = false;
    char world_proof[320] = {};
    uint64_t world_vtable[2] = {};
    uint64_t image_base = 0;
    WorldSet worlds;
    Unload unload_gates;
    size_t list_a_total = 0, list_c_total = 0, list_a_read = 0, list_c_read = 0;
    Module list_a[kMaxModules], list_c[kMaxModules];
    Spawning spawning;
};

inline uint64_t pick(const Seen &update, const Seen &unload, uint64_t &other)
{
    const Seen *a = update.ms >= unload.ms ? &update : &unload, *b = a == &update ? &unload : &update;
    other = b->at && b->at != a->at ? b->at : 0;
    return a->at;
}

template <class Read> void read_module(Read &read, uint64_t object, uint64_t spawn_vtable, Module &m)
{
    m = Module{};
    m.object = object;
    m.read = fetch(read, object + kModuleVtable, m.vtable);
    if (!m.read)
        return;
    if (!fetch(read, m.vtable + kModuleUnloadReady, m.unload_ready))
        m.unload_ready = 0;
    m.spawning = spawn_vtable && m.vtable == spawn_vtable;
}

template <class Read>
size_t read_list(Read &read, uint64_t begin, uint64_t end, uint64_t stride, uint64_t object_at, uint64_t spawn_vtable,
                 Module *out, size_t &total)
{
    total = 0;
    if (!begin || end < begin || (end - begin) % stride)
        return 0;
    total = size_t(end - begin) / stride;
    size_t n = total < kMaxModules ? total : kMaxModules, got = 0;
    for (size_t i = 0; i < n; i++) {
        uint64_t object = 0;
        if (!fetch(read, begin + i * stride + object_at, object))
            break;
        read_module(read, object, spawn_vtable, out[i]);
        got = i + 1;
    }
    return got;
}

template <class Read> void read_spawning(Read &read, uint64_t task, Spawning &sp)
{
    sp = Spawning{};
    sp.found = true;
    sp.task = task;
    sp.task_read = fetch(read, task + kModuleVtable, sp.vtable) && fetch(read, task + kTaskSystem, sp.system) &&
                   fetch(read, task + kTaskIdsBegin, sp.ids_begin) && fetch(read, task + kTaskIdsEnd, sp.ids_end);
    if (!sp.task_read)
        return;
    sp.ids_read_ok = sp.ids_end >= sp.ids_begin && sp.ids_end - sp.ids_begin <= (uint64_t(1) << 20);
    if (sp.ids_read_ok)
        sp.ids_total = size_t(sp.ids_end - sp.ids_begin);
    sp.system_read = sp.system != 0 && fetch(read, sp.system + kSysActiveBegin, sp.active_begin) &&
                     fetch(read, sp.system + kSysActiveEnd, sp.active_end);
    if (sp.system_read && sp.active_end >= sp.active_begin && (sp.active_end - sp.active_begin) % kActiveStride == 0) {
        sp.active_read_ok = true;
        sp.active_total = size_t((sp.active_end - sp.active_begin) / kActiveStride);
        size_t n = sp.active_total < kMaxActive ? sp.active_total : kMaxActive;
        for (size_t i = 0; i < n; i++) {
            if (!fetch(read, sp.active_begin + i * kActiveStride, sp.active[i])) {
                sp.active_read_ok = false;
                break;
            }
            sp.active_read = i + 1;
        }
    }
    size_t n = sp.ids_total < kMaxVehicles ? sp.ids_total : kMaxVehicles;
    for (size_t i = 0; i < n; i++) {
        Vehicle &v = sp.vehicle[i];
        if (!fetch(read, sp.ids_begin + i, v.id))
            break;
        sp.vehicles = i + 1;
        if (!sp.system_read)
            continue;
        uint64_t node = 0;
        Where w = find_node(read, kSpawnMap, sp.system, v.id, node);
        if (w == Missing)
            v.request = ReqNoNode;
        else if (w == Found && fetch(read, node + kNodeRequest, v.request_at)) {
            v.request = v.request_at ? ReqState : ReqNull;
            if (v.request_at && !fetch(read, v.request_at, v.state))
                v.request = ReqUnread;
        }
        for (size_t j = 0; j < sp.active_read; j++)
            if (sp.active[j] == v.id)
                v.active = ActYes;
        if (v.active != ActYes && sp.active_read_ok && sp.active_read == sp.active_total)
            v.active = ActNo;
    }
}

template <class Read> void capture_unload(Read &read, Snapshot &s)
{
    Unload &u = s.unload_gates;
    u = Unload{};
    s.list_a_total = s.list_c_total = s.list_a_read = s.list_c_read = 0;
    s.spawning = Spawning{};
    s.worlds = WorldSet{};
    if (s.world_proven)
        capture_worlds(read, s.at, s.image_base, s.world_vtable, s.worlds);
    if (!s.unload_proven)
        return;
    u.read = fetch(read, s.at + kGate0c, u.gate_0c) && fetch(read, s.at + kGate10, u.gate_10) &&
             fetch(read, s.at + kGate14, u.gate_14) && fetch(read, s.at + kGate1c, u.gate_1c) &&
             fetch(read, s.at + kGate24, u.gate_24) && fetch(read, s.at + kGate110, u.gate_110) &&
             fetch(read, s.at + kListCBegin, u.list_c_begin) && fetch(read, s.at + kListCEnd, u.list_c_end) &&
             fetch(read, s.at + kListABegin, u.list_a_begin) && fetch(read, s.at + kListAEnd, u.list_a_end) &&
             fetch(read, s.at + kVehicleTask, u.vehicle_task);
    if (!u.read)
        return;
    s.list_a_read = read_list(read, u.list_a_begin, u.list_a_end, kListAStride, 0, s.spawn_vtable, s.list_a, s.list_a_total);
    s.list_c_read = read_list(read, u.list_c_begin, u.list_c_end, kListCStride, 8, s.spawn_vtable, s.list_c, s.list_c_total);
    uint64_t task = u.vehicle_task;
    for (size_t i = 0; !task && i < s.list_a_read; i++)
        if (s.list_a[i].spawning)
            task = s.list_a[i].object;
    for (size_t i = 0; !task && i < s.list_c_read; i++)
        if (s.list_c[i].spawning)
            task = s.list_c[i].object;
    if (task)
        read_spawning(read, task, s.spawning);
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
    capture_unload(read, s);
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

inline const char *request_state_text(uint8_t st)
{
    switch (st) {
    case 0:
        return "Invalid";
    case 1:
        return "Begin";
    case 2:
        return "Spawning";
    case 3:
        return "Spawned";
    case 4:
        return "Generating";
    case 5:
        return "Ready";
    case 6:
        return "Despawning";
    case 7:
        return "Despawned";
    default:
        return "unknown";
    }
}

inline const char *gate_text(GateWhere w)
{
    switch (w) {
    case BlockA:
        return "block A: +0x1c is 5, so the tick waits for every module in the +0x200 list to answer unload-ready";
    case BlockB:
        return "block B: +0x24 is 2, so the tick waits for the +0x1f0 task to answer unload-ready";
    case BlockCFields:
        return "block C, a gate field is not 0 (+0xc, +0x10, +0x14 or +0x110)";
    case BlockCTasks:
        return "block C, every gate field is 0: the wait is on the sub-tasks in the +0x1d8 list";
    default:
        return "the gate fields are not readable";
    }
}

inline void name_into(char *w, size_t cap, Namer name, uint64_t at)
{
    w[0] = 0;
    if (name && at)
        name(w, cap, at);
}

inline void module_text(stall_order::Out &o, const char *list, size_t i, const Module &m, Namer name)
{
    char v[256], f[256];
    if (!m.read) {
        o.put("    %s #%zu object 0x%llx: not readable\n", list, i, (unsigned long long)m.object);
        return;
    }
    name_into(v, sizeof v, name, m.vtable);
    name_into(f, sizeof f, name, m.unload_ready);
    o.put("    %s #%zu object 0x%llx, vtable 0x%llx%s%s%s, unload-ready (+0x30) 0x%llx%s%s%s%s\n", list, i,
          (unsigned long long)m.object, (unsigned long long)m.vtable, v[0] ? " (" : "", v, v[0] ? ")" : "",
          (unsigned long long)m.unload_ready, f[0] ? " (" : "", f, f[0] ? ")" : "", m.spawning ? " [vehicle spawning]" : "");
}

inline void modules_text(stall_order::Out &o, const char *list, const Module *m, size_t read, size_t total, Namer name)
{
    for (size_t i = 0; i < read; i++)
        module_text(o, list, i, m[i], name);
    if (read < total)
        o.put("    %s: %zu more not shown\n", list, total - read);
}

inline void spawning_text(stall_order::Out &o, const Snapshot &s, Namer name)
{
    const Spawning &sp = s.spawning;
    if (!sp.found) {
        o.put("  vehicle spawning sub-task: +0x1f0 is %s and no module in either list has its vtable\n",
              s.unload_gates.vehicle_task ? "set" : "0");
        return;
    }
    char w[256];
    name_into(w, sizeof w, name, sp.vtable);
    o.put("  vehicle spawning sub-task 0x%llx (%s)\n", (unsigned long long)sp.task,
          sp.task == s.unload_gates.vehicle_task ? "from +0x1f0" : "found in a module list, +0x1f0 is not it");
    if (!sp.task_read) {
        o.put("    its fields are not readable\n");
        return;
    }
    o.put("    vtable 0x%llx%s%s%s, +0x60 spawning system 0x%llx, ids 0x%llx..0x%llx\n", (unsigned long long)sp.vtable,
          w[0] ? " (" : "", w, w[0] ? ")" : "", (unsigned long long)sp.system, (unsigned long long)sp.ids_begin,
          (unsigned long long)sp.ids_end);
    if (!sp.ids_read_ok) {
        o.put("    the id range is not a readable u8 list\n");
        return;
    }
    if (!sp.system_read)
        o.put("    the spawning system's active list is not readable\n");
    else if (!sp.active_read_ok)
        o.put("    active list 0x%llx..0x%llx is not a whole number of 0x%llx records or not readable\n",
              (unsigned long long)sp.active_begin, (unsigned long long)sp.active_end, (unsigned long long)kActiveStride);
    else {
        o.put("    active list: %zu record(s)", sp.active_total);
        for (size_t i = 0; i < sp.active_read; i++)
            o.put("%s%u", i ? " " : " ids ", unsigned(sp.active[i]));
        if (sp.active_read < sp.active_total)
            o.put(" (the first %zu read)", sp.active_read);
        o.put("\n");
    }
    size_t live = 0, not_ready = 0, active = 0;
    for (size_t i = 0; i < sp.vehicles; i++) {
        const Vehicle &v = sp.vehicle[i];
        o.put("    vehicle %-3u request ", unsigned(v.id));
        switch (v.request) {
        case ReqState:
            o.put("%u %s", unsigned(v.state), request_state_text(v.state));
            live++;
            if (v.state != 5)
                not_ready++;
            break;
        case ReqNull:
            o.put("none (node with a null request)");
            break;
        case ReqNoNode:
            o.put("none (not in the request map)");
            break;
        default:
            o.put("not readable");
            break;
        }
        o.put(", %s\n", v.active == ActYes  ? "in the active list"
                        : v.active == ActNo ? "not in the active list"
                                            : "active list unknown");
        active += v.active == ActYes;
    }
    if (sp.vehicles < sp.ids_total)
        o.put("    %zu more vehicle id(s) not shown\n", sp.ids_total - sp.vehicles);
    o.put("    %zu id(s): %zu with a live request, %zu of those not Ready, %zu in the active list; unload-ready is false while "
          "any id has a live request or an active record\n",
          sp.ids_total, live, not_ready, active);
}

inline const char *kind_text(uint32_t k) { return k == 0 ? "primary world" : k == 1 ? "secondary world" : "unknown kind"; }

inline void rva_into(char *out, size_t cap, uint64_t at, uint64_t base)
{
    if (base && at >= base)
        std::snprintf(out, cap, "F1_25.exe+0x%llx", (unsigned long long)(at - base));
    else
        std::snprintf(out, cap, "0x%llx", (unsigned long long)at);
}

inline void world_text(stall_order::Out &o, const char *label, const World &w)
{
    char n[kNameMax + 4];
    world_name_into(n, sizeof n, w.name);
    if (!w.read) {
        o.put("      %s 0x%llx: not readable\n", label, (unsigned long long)w.object);
        return;
    }
    o.put("      %s 0x%llx %s state +0x200 0x%x, stop pending +0x218 %u%s\n", label, (unsigned long long)w.object, n, w.state,
          w.stop_pending, w.stop_refused() ? ", a stop request would be refused (0x25)" : "");
}

inline void world_list_text(stall_order::Out &o, const char *label, const WorldList &l)
{
    if (!l.read) {
        o.put("    module %s: not readable\n", label);
        return;
    }
    o.put("    module %s: %zu world(s)\n", label, l.total);
    for (size_t i = 0; i < l.got; i++) {
        char b[24];
        std::snprintf(b, sizeof b, "#%zu", i);
        world_text(o, b, l.world[i]);
    }
    if (l.got < l.total)
        o.put("      %zu more not shown\n", l.total - l.got);
}

inline void world_tasks_text(stall_order::Out &o, const Snapshot &s)
{
    if (!s.world_proven) {
        o.put("  world tasks not decoded: %s\n", s.world_proof);
        return;
    }
    const WorldSet &w = s.worlds;
    if (!w.read) {
        o.put("  world tasks: the +0x200 list is not readable\n");
        return;
    }
    o.put("  world tasks in the +0x200 list (the busy byte +0x88 is cleared only by the world stop callback):\n");
    for (size_t i = 0; i < w.got; i++) {
        const WorldTask &t = w.task[i];
        char v[48], n[kNameMax + 4];
        if (!t.read) {
            o.put("    #%zu object 0x%llx: not readable\n", i, (unsigned long long)t.object);
            continue;
        }
        rva_into(v, sizeof v, t.vtable, w.base);
        if (!t.world_task) {
            o.put("    #%zu object 0x%llx vtable %s: not a world task\n", i, (unsigned long long)t.object, v);
            continue;
        }
        if (!t.fields_read) {
            o.put("    #%zu object 0x%llx vtable %s: fields not readable\n", i, (unsigned long long)t.object, v);
            continue;
        }
        world_name_into(n, sizeof n, t.name);
        o.put("    #%zu object 0x%llx vtable %s: busy +0x88 %u, kind +0xc8 %u (%s), stop flag +0xcc %u, name +0xa0 %s, "
              "module +0x60 0x%llx\n",
              i, (unsigned long long)t.object, v, t.busy, t.kind, kind_text(t.kind), t.stop_flag, n,
              (unsigned long long)t.module);
        if (!t.module_read) {
            o.put("    module not set\n");
            continue;
        }
        if (t.primary.object)
            world_text(o, "module +0xe8 primary world", t.primary);
        else
            o.put("      module +0xe8 primary world: none\n");
        world_list_text(o, "+0xf0 active worlds", t.active);
        world_list_text(o, "+0xf8 stopping worlds", t.stopping);
        if (t.kind == 1 && t.name_read)
            o.put("    this name is in the +0xf0 list %zu time(s), %zu task(s) in the +0x200 list carry it\n", t.active_match,
                  same_name_tasks(w, t));
    }
    if (w.got < w.total)
        o.put("  %zu more task(s) not shown\n", w.total - w.got);
}

inline void unload_text(stall_order::Out &o, const Snapshot &s, Namer name)
{
    if (!s.unload_proven) {
        o.put("  unload tick gates not decoded: %s\n", s.unload_proof);
        return;
    }
    const Unload &u = s.unload_gates;
    if (!u.read) {
        o.put("  unload tick gates (Sess_Module_OnSessionUnloaded): not readable\n");
        return;
    }
    o.put("  unload tick gates (Sess_Module_OnSessionUnloaded, state 5):\n");
    o.put("    +0x1c  block A gate %u (A runs while 5)\n", u.gate_1c);
    o.put("    +0x24  block B gate %u (B runs while 2)\n", u.gate_24);
    o.put("    +0xc   block C gate %u (must be 0)\n", u.gate_0c);
    o.put("    +0x14  block C gate %u (must be 0)\n", u.gate_14);
    o.put("    +0x10  block C gate %u (must be 0)\n", u.gate_10);
    o.put("    +0x110 block C gate %u (must be 0)\n", u.gate_110);
    o.put("    verdict: %s\n", gate_text(gate_where(u)));
    o.put("  +0x200 modules (block A): 0x%llx..0x%llx, %zu entr%s\n", (unsigned long long)u.list_a_begin,
          (unsigned long long)u.list_a_end, s.list_a_total, s.list_a_total == 1 ? "y" : "ies");
    modules_text(o, "A", s.list_a, s.list_a_read, s.list_a_total, name);
    o.put("  +0x1d8 sub-tasks (block C): 0x%llx..0x%llx, %zu entr%s\n", (unsigned long long)u.list_c_begin,
          (unsigned long long)u.list_c_end, s.list_c_total, s.list_c_total == 1 ? "y" : "ies");
    modules_text(o, "C", s.list_c, s.list_c_read, s.list_c_total, name);
    spawning_text(o, s, name);
    world_tasks_text(o, s);
}

inline size_t entering_line(const WorldSet &w, size_t i, char *out, size_t cap)
{
    const WorldTask &t = w.task[i];
    char v[48];
    if (!t.read)
        return size_t(
            std::snprintf(out, cap, "unload entering: module #%zu object 0x%llx not readable", i, (unsigned long long)t.object));
    rva_into(v, sizeof v, t.vtable, w.base);
    if (!t.world_task)
        return size_t(std::snprintf(out, cap, "unload entering: module #%zu vtable %s (not a world task)", i, v));
    if (!t.fields_read)
        return size_t(std::snprintf(out, cap, "unload entering: module #%zu vtable %s world task fields not readable", i, v));
    char n[kNameMax + 4];
    world_name_into(n, sizeof n, t.name);
    const World *own = t.kind == 0 ? &t.primary : nullptr;
    for (size_t k = 0; !own && k < t.active.got; k++)
        if (t.active.world[k].read && name_equal(t.active.world[k].name, t.name))
            own = &t.active.world[k];
    char st[96] = "no world found";
    if (own && own->read)
        std::snprintf(st, sizeof st, "world 0x%llx state 0x%x stop pending %u", (unsigned long long)own->object, own->state,
                      own->stop_pending);
    return size_t(std::snprintf(out, cap,
                                "unload entering: module #%zu vtable %s kind %u name %s busy %u, stop flag %u, %s, "
                                "%zu active and %zu stopping world(s)",
                                i, v, t.kind, n, t.busy, t.stop_flag, st, t.active.total, t.stopping.total));
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
    unload_text(o, s, name);
    o.put("  loader bytes +0x000..+0x%zx at 0x%llx:\n", kRaw, (unsigned long long)s.at);
    rows_text(o, s.raw, s.raw_row, kRaw);
    return o.size();
}

}
