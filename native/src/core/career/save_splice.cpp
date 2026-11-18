#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/resolve.hpp"

#include <windows.h>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <set>
#include <string>
#include <vector>

#include "core/career/career_saves_logic.hpp"
#include "core/career/save_diag_logic.hpp"
#include "core/career/save_heap_logic.hpp"
#include "core/runtime/layout.hpp"
#include "core/career/save_splice_logic.hpp"
#include "core/career/save_data.hpp"

// Save splice: keeps the save containers of absent addons through a profile save
// (re/db/subsystems/game.md#save-containers, #save-splice).
//
// The profile save is one tree of save-data nodes under the root save_game. Every game mode with
// register_save_container hangs its container, named by save_data.name, under the root, and that
// container holds every career of the mode. A career content addon with its own mode container
// (driver_career_progressive, driver_career_1990) is a top-level child of the root, so in a session
// without the addon nobody claims it: the load drops it and the next profile save writes the root
// without it.
//
// Load (site_Game_SaveData_UnclaimedChild, inside Game_SaveData_DeserialiseContainer): for every child
// bag of the root that matched no registered child, the core writes the bag with the game's own bag
// writer (Game_SaveData_WriteBag) into memory the core owns, first through a counting stream for the
// size, then through a memory write stream, the way the game stores a career in a slot. Nothing goes
// into the game's SAVE_DATA heap. The wrap of Game_SaveData_DeserialiseContainer empties the store
// when a root load starts.
//
// Save (Game_SaveData_SerialiseContainer_2, the inline profile save): site_Game_SaveData_ContainersCount
// adds the kept containers to the root's containers count before it is written, and
// site_Game_SaveData_ContainersChildren writes each kept container right after it: its name (stream
// write_length and write, as the string writer does), the bag type id the block uses and the captured
// bytes. The reader reads them back as the same bags, so a mode that comes back with its addon loads
// its careers again. A failed write stores the error in the result and continues at
// site_Game_SaveData_ContainersDone, the game's own path for a failed write in that block.
//
// Career saves: the splice also fires career.saving at site_Game_SaveData_ContainersCount and career.saved at
// site_Game_SaveData_ContainersDone (a separate mixin; a good result in [r12]) for the root (career_saves.cpp).
//
// SWOFTY_SAVE_SPLICE=0 turns it off.

namespace save_data {

Bindings b;

}

using namespace save_data;

namespace {

namespace mem = swofty::mem;
namespace g = swofty::game;
namespace sp = runtime::save_splice;
namespace layout = runtime::layout;

constexpr const char *MIXIN = "swofty_core.dll (save splice)";
constexpr const char *SLOT_MIXIN = "swofty_core.dll (save slot)";
constexpr const char *SLOTS_MIXIN = "swofty_core.dll (career slots)";
constexpr const char *SAVED_MIXIN = "swofty_core.dll (career saved)";
constexpr size_t CHILD_SLOT = 0x88;
constexpr size_t FRAME_SELF = 0x30;
constexpr size_t FRAME_CHILDREN = 0x38;
constexpr size_t FRAME_STREAM = 0x50;
constexpr size_t FRAME_BAG_TYPE = 0x58;
constexpr size_t FRAME_COUNT = 0x90;
constexpr int WRITE_FAILED = 0x58;

runtime::loader::MixinGate mixin_gate;
constinit swofty_core::guards::LogFirst<1> pool_line;
bool live = false;
std::mutex lock;
sp::Store store;
sp::Owed owed;
uint32_t loads = 0;

// ---- W4 career snapshots (native/src/core/career/snapshots/): the core's own swofty_registry entry rides in
// this store like any unclaimed container. The game never claims it, so a load keeps it and a save
// writes it back; the hooks hand its body to the snapshots at each root load and take a new one at
// each profile save. ----
runtime::save_splice::EntryHooks entry_hooks;
bool entry_put = false;
// ---- end W4 ----

using Write_t = int (*)(void *stream, const void *data, uint64_t size);
using WriteLength_t = int (*)(void *stream, uint32_t length);
using Deserialise_t = int *(*)(void *self, int *result, void *bag, uint8_t reset_on_error, uint64_t groups);

Deserialise_t deserialise_orig = nullptr;

using SlotsLoad_t = int (*)(void *iface, void *bag, void *ctx);
SlotsLoad_t slots_orig = nullptr;
namespace cs = runtime::career_saves;
std::vector<cs::SlotRead> slot_reads;
struct Manager {
    uintptr_t iface = 0, vtable = 0;
    size_t count = 0;
};
std::vector<Manager> managers;
bool in_root_load = false, slots_clean = true;

void log_load(const std::vector<cs::SlotRead> &reads, const std::vector<Manager> &seen, bool ok)
{
    unsigned n;
    {
        std::lock_guard<std::mutex> g(lock);
        n = loads;
    }
    std::string file = runtime::save_sandbox_file();
    swofty::log("[saves] profile load %u: %s from %s; %s", n, ok ? "read" : "FAILED to read", runtime::save_sandbox_source().c_str(),
                file.empty() ? "save file not observed (Steam reads it natively)" : file.c_str());
    if (!slots_orig) {
        swofty::log("[saves] profile load %u: career slots are not read on this build", n);
        return;
    }
    uintptr_t image = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
    size_t at = 0;
    for (size_t m = 0; m < seen.size(); m++) {
        swofty::log("[saves] %s", runtime::save_diag::manager_line(n, m, seen[m].count, seen[m].vtable - image).c_str());
        for (size_t i = 0; i < seen[m].count && at < reads.size(); i++, at++)
            swofty::log("[saves] %s",
                        runtime::save_diag::slot_line(n, m, i, reads[at],
                                                      runtime::career_saves_id_for_slot(cs::slot_key(reads[at].id, reads[at].guid)))
                            .c_str());
    }
    if (seen.empty())
        swofty::log("[saves] profile load %u: no save slot manager was read (no career containers in this save)", n);
}

int *deserialise_entry(void *self, int *result, void *bag, uint8_t reset_on_error, uint64_t groups)
{
    bool root = is_root(reinterpret_cast<uintptr_t>(self));
    if (root) {
        std::lock_guard<std::mutex> g(lock);
        store.begin_load();
        ++loads;
        slot_reads.clear();
        managers.clear();
        slots_clean = true;
        in_root_load = true;
    }
    int *r = deserialise_orig(self, result, bag, reset_on_error, groups);
    if (root) {
        std::vector<cs::SlotRead> reads;
        bool report;
        std::vector<Manager> seen;
        {
            std::lock_guard<std::mutex> g(lock);
            in_root_load = false;
            seen = managers;
            report = slots_orig && slots_clean && r && *r == 0;
            reads.swap(slot_reads);
        }
        if (report)
            runtime::career_saves_listed(reads, true, true);
        log_load(reads, seen, r && *r == 0);
        runtime::direct_start_profile_loaded(r && *r == 0);
        // W4: the swofty_registry body this load kept, for the career snapshots.
        std::vector<uint8_t> entry;
        bool has_entry = false;
        {
            std::lock_guard<std::mutex> g(lock);
            if (store.size())
                swofty::log("[saves] profile load %u: kept %zu save container(s) no loaded game mode claims (%llu "
                            "bytes); the next profile save writes them back",
                            loads, store.size(), (unsigned long long)store.total());
            for (auto &k : store.kept())
                swofty::log("[saves] %s", runtime::save_diag::kept_line(loads, k.name, k.body.size()).c_str());
            for (auto &k : store.kept())
                if (k.name == sp::ENTRY_NAME)
                    entry = k.body, has_entry = true;
        }
        if (live && entry_hooks.loaded)
            entry_hooks.loaded(has_entry ? &entry : nullptr);
    }
    return r;
}

int slots_entry(void *iface, void *bag, void *ctx)
{
    int rc = slots_orig(iface, bag, ctx);
    std::vector<cs::SlotRead> reads;
    uintptr_t base = reinterpret_cast<uintptr_t>(iface), vt = 0;
    bool clean = rc == 0 && read_manager(base, reads) && mem::read(base, vt);
    std::lock_guard<std::mutex> g(lock);
    if (in_root_load) {
        if (clean) {
            managers.push_back({base, vt, reads.size()});
            for (auto &r : reads)
                slot_reads.push_back(std::move(r));
        } else
            slots_clean = false;
    }
    return rc;
}

void unclaimed_entry(SwoftyContext *c)
{
    if (!live || !is_root(c->rdi))
        return;
    uintptr_t slot = 0;
    void *prop = nullptr;
    std::string why, name;
    uint64_t hash = 0;
    void *bag = nullptr;
    std::vector<uint8_t> bytes;
    bool ok = mem::read(c->rsp + CHILD_SLOT, slot) && mem::read(slot, prop) && prop;
    if (!ok)
        why = "the current child is not readable";
    ok = ok && read_property(prop, hash, bag, why);
    if (ok) {
        name = name_of(hash);
        if (!sp::name_ok(name)) {
            why = "its name does not resolve";
            ok = false;
        }
    }
    ok = ok && encode(bag, bytes, why);
    std::lock_guard<std::mutex> g(lock);
    if (!ok) {
        swofty::log("[saves] profile load %u: could not keep the unclaimed save container %s (hash %016llx): %s; the "
                    "game drops it",
                    loads, name.empty() ? "?" : name.c_str(), (unsigned long long)hash, why.c_str());
        return;
    }
    size_t size = bytes.size();
    switch (store.add(name, std::move(bytes))) {
    case sp::Add::kept:
    case sp::Add::replaced:
        swofty::log("[saves] profile load %u: kept %s (%zu bytes), which no loaded game mode claims", loads, name.c_str(),
                    size);
        break;
    default:
        swofty::log("[saves] profile load %u: could not keep %s (%zu bytes): the store is full; the game drops it", loads,
                    name.c_str(), size);
    }
}

// W4b: Game_SaveSlotManager_SelectSlot stored the slot it loads at manager+0x280; rbx = that SaveSlotInfo
// (slot_id +0x0, slot_guid +0x8), rsi = the manager (re/db/structs/SaveSlotInfo.h, SaveSlotManager.h).
void slot_selected(SwoftyContext *c)
{
    uint64_t id = 0, guid = 0;
    if (c->rbx && mem::read(c->rbx, id) && mem::read(c->rbx + 8, guid))
        runtime::snapshots_slot_selected(c->rsi, id, guid);
}

void count_entry(SwoftyContext *c)
{
    uintptr_t self = 0;
    if (!live || !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self))
        return;
    runtime::career_saves_saving();
    // W4: the career snapshots' new swofty_registry body, when they changed since the last write.
    std::vector<uint8_t> entry;
    bool has_entry = entry_hooks.saving && entry_hooks.saving(entry);
    std::lock_guard<std::mutex> g(lock);
    owed.take(nullptr);
    if (entry_put && entry_hooks.written)
        entry_hooks.written(false);
    entry_put = false;
    if (has_entry) {
        size_t size = entry.size();
        sp::Add a = store.add(sp::ENTRY_NAME, std::move(entry));
        entry_put = a == sp::Add::kept || a == sp::Add::replaced;
        if (!entry_put) {
            swofty::log("[saves] profile save: the career snapshots' %s entry (%zu bytes) does not fit the save "
                        "splice's store; not written this time",
                        sp::ENTRY_NAME, size);
            if (entry_hooks.written)
                entry_hooks.written(false);
        }
    }
    uint64_t count = 0;
    uintptr_t begin = 0;
    if (!store.size() || !mem::read(c->rsp + FRAME_COUNT, count) || !mem::read(c->rsp + FRAME_CHILDREN, begin))
        return;
    std::vector<std::string> registered;
    if (!child_names(begin, c->r15, registered) || registered.size() != count) {
        swofty::log("[saves] profile save: the root's children are not readable; the kept save containers are not "
                    "written back this time");
        return;
    }
    std::vector<size_t> picked = sp::pick(store.kept(), registered);
    if (picked.size() != store.size())
        swofty::log("[saves] profile save: %zu kept save container(s) now have a registered container of the same name; "
                    "those are not written back",
                    store.size() - picked.size());
    if (picked.empty())
        return;
    if (pool_line.note()) {
        uint64_t heap_bytes = 0;
        bool raised = runtime::save_heap_raised(heap_bytes);
        uint64_t bytes = 0;
        for (size_t i : picked)
            bytes += store.kept()[i].body.size();
        swofty::log("%s", runtime::save_heap::session_line(heap_bytes, raised, picked.size(), bytes).c_str());
    }
    *reinterpret_cast<uint64_t *>(c->rsp + FRAME_COUNT) =
        owed.bump(reinterpret_cast<const void *>(c->r12), count, std::move(picked));
}

template <class SV = g::NeStreamVtbl>
int write_kept(void *stream, uint32_t bag_type, const std::vector<size_t> &picked, std::string &names)
{
    if constexpr (layout::save_streams<g::NeBagCodec, g::NeCountingStream, g::NeMemWriteStream, SV>) {
        auto write = reinterpret_cast<Write_t>(method(stream, offsetof(SV, write)));
        auto write_length = reinterpret_cast<WriteLength_t>(method(stream, offsetof(SV, write_length)));
        if (!write || !write_length)
            return WRITE_FAILED;
        for (size_t i : picked) {
            if (i >= store.size())
                return WRITE_FAILED;
            const sp::Kept &k = store.kept()[i];
            int r = write_length(stream, uint32_t(k.name.size()));
            if (!r)
                r = write(stream, k.name.data(), k.name.size());
            if (!r)
                r = write(stream, &bag_type, sizeof bag_type);
            if (!r)
                r = write(stream, k.body.data(), k.body.size());
            if (r)
                return r;
            names += (names.empty() ? "" : ", ") + k.name;
        }
        return 0;
    } else {
        (void)stream, (void)bag_type, (void)picked, (void)names;
        return WRITE_FAILED;
    }
}

void children_entry(SwoftyContext *c)
{
    uintptr_t self = 0;
    if (!live || !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self))
        return;
    std::lock_guard<std::mutex> g(lock);
    std::vector<size_t> picked = owed.take(reinterpret_cast<const void *>(c->r12));
    if (picked.empty())
        return;
    void *stream = nullptr;
    uint32_t bag_type = 0;
    mem::read(c->rsp + FRAME_STREAM, stream);
    mem::read(c->rsp + FRAME_BAG_TYPE, bag_type);
    std::string names;
    int r = write_kept(stream, bag_type, picked, names);
    // W4: whether the career snapshots' entry went out with this save.
    bool entry_written = false;
    if (entry_put) {
        for (size_t i : picked)
            entry_written |= i < store.size() && store.kept()[i].name == sp::ENTRY_NAME;
        entry_put = false;
        if (entry_hooks.written)
            entry_hooks.written(!r && entry_written);
    }
    if (r) {
        *reinterpret_cast<int *>(c->r12) = r;
        c->rip = b.done;
        swofty::log("[saves] profile save: writing back the kept save containers failed with code %d; the save fails "
                    "as on any write error",
                    r);
        return;
    }
    swofty::log("[saves] profile save: wrote back %zu save container(s) no loaded game mode claims: %s", picked.size(),
                names.c_str());
}

void saved_entry(SwoftyContext *c)
{
    uintptr_t self = 0;
    int result = 1;
    if (!live || !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self) || !mem::read(c->r12, result) || result)
        return;
    std::vector<cs::SlotRead> reads;
    std::vector<Manager> known;
    {
        std::lock_guard<std::mutex> g(lock);
        known = managers;
    }
    bool clean = !known.empty();
    for (const Manager &m : known) {
        uintptr_t vt = 0;
        if (!mem::read(m.iface, vt) || vt != m.vtable || !read_manager(m.iface, reads))
            clean = false;
    }
    if (clean)
        runtime::career_saves_listed(reads, true, false);
    runtime::career_saves_saved();
}

}

namespace runtime {

std::vector<CoreMixins> save_splice_mixins()
{
    std::vector<CoreMixins> out;
    if (mixin_gate.offered() || !code_ready())
        return out;
    mixin_gate.offer();
    // W4b: the career id's save slot, whether or not the splice runs.
    out.push_back({SLOT_MIXIN, {{"site_Game_SaveSlot_Selected", SWOFTY_MIXIN_SITE, 100, (void *)&slot_selected, nullptr}}});
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_SAVE_SPLICE", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[saves] save splice off (SWOFTY_SAVE_SPLICE=%s)", v);
        return out;
    }
    if (!layout::SAVE_SPLICE) {
        swofty::log("[saves] save splice off: this core was built without the save stream and property layouts");
        return out;
    }
    std::string missing;
    b.root_vt = runtime::resolve_or_note("vt_Game_SaveDataRootContainer", missing);
    b.write_bag = runtime::resolve_or_note("Game_SaveData_WriteBag", missing);
    b.lookup = runtime::resolve_or_note("Ne_StringTable_Lookup", missing);
    b.codec_vt = runtime::resolve_or_note("k_Ne_BagCodecVtbl", missing);
    b.counting_vt = runtime::resolve_or_note("k_Ne_CountingStreamVtbl", missing);
    b.memory_vt = runtime::resolve_or_note("k_Ne_MemWriteStreamVtbl", missing);
    b.table = runtime::resolve_or_note("g_Ne_StringTable", missing);
    b.bag_type = runtime::resolve_or_note("k_Ne_PropertyType_Bag", missing);
    b.done = runtime::resolve_or_note("site_Game_SaveData_ContainersDone", missing);
    if (!missing.empty()) {
        swofty::log("[saves] save splice off: needs %s, which this build does not resolve", missing.c_str());
        return out;
    }
    out.push_back({MIXIN,
                   {{"Game_SaveData_DeserialiseContainer", SWOFTY_MIXIN_WRAP, 100, (void *)&deserialise_entry,
                     (void **)&deserialise_orig},
                    {"site_Game_SaveData_UnclaimedChild", SWOFTY_MIXIN_SITE, 100, (void *)&unclaimed_entry, nullptr},
                    {"site_Game_SaveData_ContainersCount", SWOFTY_MIXIN_SITE, 100, (void *)&count_entry, nullptr},
                    {"site_Game_SaveData_ContainersChildren", SWOFTY_MIXIN_SITE, 100, (void *)&children_entry,
                     nullptr}}});
    if (swofty::api->resolve("Game_SaveSlotManager_Deserialise"))
        out.push_back({SLOTS_MIXIN,
                       {{"Game_SaveSlotManager_Deserialise", SWOFTY_MIXIN_WRAP, 100, (void *)&slots_entry,
                         (void **)&slots_orig}}});
    out.push_back({SAVED_MIXIN, {{"site_Game_SaveData_ContainersDone", SWOFTY_MIXIN_SITE, 100, (void *)&saved_entry, nullptr}}});
    return out;
}

void save_splice_commit()
{
    career_saves_start();
    if (!layout::SLOT_DETAILS)
        swofty::log("[saves] career list details off: needs SaveSlotInfo.game_setup_mode, info_season, info_weekend, "
                    "info_team, info_driver, info_vehicle, info_date and type_flags (a verify agree row each); the "
                    "list still holds every save slot's key");
    if (!b.done || !mixin_gate.settle()) {
        snapshots_start(false);
        return;
    }
    live = core_mixins_active(MIXIN);
    swofty::log("[saves] save splice %s",
                loader::install_state(live, "armed: save containers no loaded game mode claims are kept through each profile "
                                            "load and save"));
    snapshots_start(live);
}

static loader::FunctionSet save_splice_core_set{loader::CoreSetId::save_splice, save_splice_mixins, save_splice_commit};

// ---- W4 career snapshots ----
namespace save_splice {

void set_entry_hooks(const EntryHooks &h) { entry_hooks = h; }

std::string entry_blocked()
{
    if (!live)
        return "the save splice is not armed";
    if constexpr (layout::SAVE_SPLICE) {
        auto bag_id = reinterpret_cast<Id_t>(
            method(reinterpret_cast<void *>(b.bag_type), offsetof(g::NePropertyTypeVtbl, id)));
        uint32_t id = bag_id ? uint32_t(bag_id(reinterpret_cast<void *>(b.bag_type))) : 0;
        if (id != ENTRY_BAG_TYPE) {
            char why[96];
            snprintf(why, sizeof why, "the game's bag type id is %08x, not 'pbag' (%08x)", id, ENTRY_BAG_TYPE);
            return why;
        }
        return {};
    } else {
        return "this core lacks the property layout";
    }
}

}
// ---- end W4 ----

}
