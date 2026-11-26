#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/hook_set.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/resolve.hpp"

#include <windows.h>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <vector>

#include "core/career/career_saves_logic.hpp"
#include "core/career/save_diag_logic.hpp"
#include "core/career/save_heap_logic.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/layout.hpp"
#include "core/career/save_splice_logic.hpp"
#include "core/career/save_data.hpp"
#include "core/career/bag_scan_logic.hpp"
#include "core/career/plain_saves.hpp"

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
// Only the profile's root: the game's other saves (the race highlights, savegame@highlights000) have a root of the same
// class. A load or save of another root gets no kept containers, no career events and no plain store generation, and leaves
// the kept store as it is (sp::ProfileRoot).
//
// SWOFTY_SAVE_SPLICE=0 turns it off.

namespace career::save_data {

Bindings b;

}

using namespace career::save_data;

namespace {

namespace mem = swofty::mem;
namespace g = swofty::game;
namespace sp = career::save_splice;
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

constinit swofty_core::guards::LogFirst<1> pool_line;
sp::Store store;
sp::Owed owed;
sp::ProfileRoot profile_root;
uint32_t loads = 0;

// ---- W4 career snapshots (native/src/core/career/snapshots/): the core's own swofty_registry entry rides in
// this store like any unclaimed container. The game never claims it, so a load keeps it and a save
// writes it back; the hooks hand its body to the snapshots at each root load and take a new one at
// each profile save. ----
career::save_splice::EntryHooks entry_hooks;
bool entry_put = false;
// ---- end W4 ----

using Write_t = int (*)(void *stream, const void *data, uint64_t size);
using WriteLength_t = int (*)(void *stream, uint32_t length);
using Deserialise_t = int *(void *self, int *result, void *bag, uint8_t reset_on_error, uint64_t groups);
using SlotsLoad_t = int(void *iface, void *bag, void *ctx);

// The save splice set: the root container's deserialise and, when this build has it, the slot managers' load.
struct SpliceHooks : runtime::loader::HookSet<> {
    runtime::loader::Original<Deserialise_t> deserialise_container;
    runtime::loader::Original<SlotsLoad_t> load_slot_manager;
};

constinit SpliceHooks hooks;
namespace cs = career::saves;
std::vector<cs::SlotRead> slot_reads;
struct Manager {
    uintptr_t iface = 0, vtable = 0;
    size_t count = 0;
    std::string container;
};
std::vector<Manager> managers;
bool in_root_load = false, slots_clean = true;

// The plain store (plain_saves.cpp): what every root child of the last load held, so the store has registered
// containers too. A registered child's bag is encoded from the recursive deserialise call the root's loop makes for it
// (depth 2 below the root's own call). SWOFTY_PLAIN_REGISTERED=0 leaves registered children out.
namespace plain = career::plain_saves;
struct Captured {
    std::string name;
    std::shared_ptr<const std::vector<uint8_t>> bytes;
};
std::vector<Captured> captured;
std::set<std::string> seen_children, injected_names;
std::string current_child;
thread_local int load_depth = 0;
thread_local bool in_fix = false;
plain::Plan plan;
std::map<std::string, size_t> career_tally;
std::vector<std::string> registered_now;
bool registered_now_ok = false;
std::vector<Captured> saved_caps;
bool saved_caps_ok = false;
std::vector<plain::Swap> swaps;
plain::ReadOnce &read_once()
{
    static plain::ReadOnce r;
    return r;
}
void *load_root = nullptr;
// A bag the core made for a mode to read in place of its own: {the game's allocator, then what the bag reader fills}, the layout
// of the root bag {pool, 0, 0}. Kept until the next root load, past the pool the game frees after the root deserialise.
struct FreshBag {
    alignas(16) std::array<uint8_t, 0x40> bag{};
    std::vector<uint8_t> bytes;
};
std::map<std::string, std::unique_ptr<FreshBag>> swap_bags;

// The bag stream and root child table layouts the plain store reads and writes through (the root's child table at +0x160,
// each node's name at +0x10 and child at +0x58, the bag reader's use of stream slots +0x28/+0x30/+0x38 and bag+0, the save
// stream's write slots +0x48/+0x50) were compared instruction by instruction on the KB build and on Steam 1.0.145 and are
// identical there. The EA builds are packed and were not read, so the registered capture, the save capture and the registered
// put-back stay off on them.
constexpr runtime::build_proof::Proof BAG_LAYOUT{"the save root's child table and bag stream layouts",
                                                 runtime::build_proof::bit(runtime::build_proof::Build::kb) |
                                                     runtime::build_proof::bit(runtime::build_proof::Build::steam_1_0_145)};
constexpr runtime::build_proof::Gate BAG_GATE{"the plain store's registered containers", &BAG_LAYOUT};

bool bag_proven() { return runtime::holds(BAG_GATE); }

bool profile_load(uintptr_t self)
{
    bool has = career::save_data::registered_child(self, sp::PROFILE_CHILD) != 0;
    bool profile;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        profile = profile_root.at_load(self, has);
    }
    if (!profile)
        swofty::log("[saves] a save other than the profile loads (its root has no %s); the save splice and the plain store "
                    "leave it alone",
                    sp::PROFILE_CHILD);
    return profile;
}

bool profile_save(uintptr_t self, bool names_ok, const std::vector<std::string> &registered)
{
    bool has = names_ok ? std::find(registered.begin(), registered.end(), sp::PROFILE_CHILD) != registered.end()
                        : career::save_data::registered_child(self, sp::PROFILE_CHILD) != 0;
    bool profile;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        profile = profile_root.at_save(self, has);
    }
    if (!profile) {
        std::string what = names_ok && !registered.empty() ? "its root registers " + std::to_string(registered.size()) +
                                                                 " container(s): " + registered.front()
                                                           : std::string("its root's children are not readable");
        swofty::log("[saves] a save other than the profile (%s): no kept save containers, career events or plain store "
                    "generation for it",
                    what.c_str());
    }
    return profile;
}

bool saving_profile(uintptr_t self)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    return profile_root.is(self);
}

bool env_on(const char *name)
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return !(n && n < sizeof v && !strcmp(v, "0"));
}

namespace bs = career::bag_scan;

using ReadBag_t = int (*)(void *codec, void *bag);

// Reads our bytes into a bag object the game made, through the game's own bag reader and a read stream the core owns. The
// reader clears the bag first, so this is only called on a bag whose content the game has already used or never had.
bool read_into_bag(void *bag, const std::vector<uint8_t> &bytes, std::string &why)
{
    if (!bs::whole_bag(bytes.data(), bytes.size()))
        return why = "the stored bytes are not one whole bag", false;
    if (!career::save_data::bag_object(bag))
        return why = "the target bag has no game allocator", false;
    uintptr_t fn = 0;
    if (!b.codec_vt || !mem::read(b.codec_vt + 0x10, fn) || !mem::in_image(fn))
        return why = "the bag reader is not readable", false;
    bs::ReadStream stream;
    bs::init_read(stream, bytes.data(), bytes.size());
    alignas(16) std::array<uint8_t, 0x10> codec{};
    auto *c = reinterpret_cast<g::NeBagCodec *>(codec.data());
    c->vtable = reinterpret_cast<void **>(b.codec_vt);
    c->stream = &stream;
    int r = reinterpret_cast<ReadBag_t>(fn)(c, bag);
    if (r)
        return why = "the bag reader failed with code " + std::to_string(r), false;
    if (stream.pos != stream.size || stream.bad_slot_calls)
        return why = "the bag reader stopped at " + std::to_string(stream.pos) + " of " + std::to_string(stream.size) +
                     " bytes (" + std::to_string(stream.bad_slot_calls) + " unsupported stream call(s))",
               false;
    return true;
}

size_t tally_of(const std::string &name)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    auto it = career_tally.find(name);
    return it == career_tally.end() ? 0 : it->second;
}

void set_captured(const std::string &name, std::vector<uint8_t> bytes)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    auto body = std::make_shared<const std::vector<uint8_t>>(std::move(bytes));
    for (auto &c : captured)
        if (c.name == name) {
            c.bytes = body;
            injected_names.insert(name);
            return;
        }
    captured.push_back({name, body});
    injected_names.insert(name);
}

bool registered_capture_on()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_PLAIN_REGISTERED", v, sizeof v);
    return !(n && n < sizeof v && !strcmp(v, "0"));
}

plain::Slot plain_slot(const cs::SlotRead &r)
{
    plain::Slot s;
    s.slot = r.id, s.guid = r.guid, s.name = r.name;
    s.season = r.season == cs::NO_INDEX ? -1 : int64_t(r.season);
    s.round = r.weekend == cs::NO_INDEX ? -1 : int64_t(r.weekend);
    s.team = r.team, s.driver = r.driver, s.last_played = r.date, s.flags = r.flags;
    return s;
}

std::vector<plain::Body> plain_bodies(bool at_save, const std::map<std::string, std::vector<plain::Slot>> &slots)
{
    std::vector<plain::Body> out;
    auto slots_of = [&](const std::string &name) {
        auto it = slots.find(name);
        return it == slots.end() ? std::vector<plain::Slot>() : it->second;
    };
    const std::vector<Captured> &game = at_save && saved_caps_ok ? saved_caps : captured;
    for (const Captured &c : game) {
        plain::Body b;
        b.name = c.name, b.bytes = c.bytes, b.registered = true, b.source = "game", b.slots = slots_of(c.name);
        if (at_save && !saved_caps_ok)
            b.as_of = "load";
        if (!at_save && injected_names.count(c.name))
            b.source = "injected";
        out.push_back(std::move(b));
    }
    for (const sp::Kept &k : store.kept()) {
        plain::Body b;
        b.name = k.name, b.bytes = std::make_shared<const std::vector<uint8_t>>(k.body);
        b.source = injected_names.count(k.name) ? "injected" : "kept";
        b.slots = slots_of(k.name);
        out.push_back(std::move(b));
    }
    return out;
}

void log_load(const std::vector<cs::SlotRead> &reads, const std::vector<Manager> &seen, const int *result)
{
    const bool ok = result && *result == 0;
    unsigned n;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        n = loads;
    }
    std::string failed = "FAILED to read (the game's load returned " + (result ? std::to_string(*result) : "nothing") + ")";
    swofty::log("[saves] profile load %u: %s from %s; %s", n, ok ? "read" : failed.c_str(),
                runtime::save_sandbox_source().c_str(), runtime::save_sandbox_file().c_str());
    if (!hooks.load_slot_manager) {
        swofty::log("[saves] profile load %u: career slots are not read on this build", n);
        return;
    }
    uintptr_t image = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
    size_t at = 0;
    for (size_t m = 0; m < seen.size(); m++) {
        swofty::log("[saves] %s", career::save_diag::manager_line(n, m, seen[m].count, seen[m].vtable - image).c_str());
        for (size_t i = 0; i < seen[m].count && at < reads.size(); i++, at++)
            swofty::log("[saves] %s",
                        career::save_diag::slot_line(
                            n, m, i, reads[at], runtime::career_saves_id_for_slot(cs::slot_key(reads[at].id, reads[at].guid)))
                            .c_str());
    }
    if (seen.empty())
        swofty::log("[saves] profile load %u: no save slot manager was read (no career containers in this save)", n);
}

struct ChildInfo {
    void *self = nullptr, *bag = nullptr;
    uint8_t reset = 0;
    uint64_t groups = 0;
};
std::map<std::string, ChildInfo> child_info;

void capture_child(void *self, void *bag, uint8_t reset_on_error, uint64_t groups)
{
    std::string name, why;
    std::vector<uint8_t> bytes;
    if (!career::save_data::container_name(reinterpret_cast<uintptr_t>(self), name) || !sp::name_ok(name))
        return;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        seen_children.insert(name);
        current_child = name;
        child_info[name] = {self, bag, reset_on_error, groups};
    }
    if (!registered_capture_on() || !plain::enabled() || !bag_proven())
        return;
    if (!career::save_data::bag_object(bag)) {
        swofty::log("[saves] plain store: the bag of %s has no game allocator; it is left out of this load's generation",
                    name.c_str());
        return;
    }
    if (!encode(bag, bytes, why)) {
        swofty::log("[saves] plain store: could not capture %s: %s", name.c_str(), why.c_str());
        return;
    }
    std::lock_guard<std::mutex> g(hooks.lock);
    captured.push_back({name, std::make_shared<const std::vector<uint8_t>>(std::move(bytes))});
}

// A registered mode whose container the file does not hold at all: the game never called the mode to read one. A bag made from
// our bytes is handed to the registered child, found in the root's child table. An object a read has reached is never read
// again: its tables would keep what the first read put there and take ours after it.
bool place_registered(void *root, void *root_bag, const std::string &name, const std::vector<uint8_t> &bytes,
                      uint8_t reset_on_error, uint64_t groups, std::string &why)
{
    uintptr_t child = career::save_data::registered_child(reinterpret_cast<uintptr_t>(root), name);
    if (!child)
        return why = "no registered mode container of that name", false;
    if (!read_once().may_read(child))
        return why = "the game has read " + name + " in this session already", false;
    void *allocator = nullptr;
    if (!mem::read(reinterpret_cast<uintptr_t>(root_bag), allocator) || !career::save_data::game_object(allocator))
        return why = "the root bag's allocator is not readable", false;
    alignas(16) std::array<uint8_t, 0x40> fresh{};
    *reinterpret_cast<void **>(fresh.data()) = allocator;
    if (!read_into_bag(fresh.data(), bytes, why))
        return false;
    alignas(16) std::array<uint8_t, 0x80> res{};
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        career_tally[name] = 0;
        current_child = name;
    }
    read_once().mark(child);
    hooks.deserialise_container(reinterpret_cast<void *>(child), reinterpret_cast<int *>(res.data()), fresh.data(),
                                reset_on_error, groups);
    int code = *reinterpret_cast<int *>(res.data());
    if (code != 0)
        return why = "the mode failed to read it (code " + std::to_string(code) + ")", false;
    return true;
}

std::string captured_sha(const std::string &name)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    for (const Captured &c : captured)
        if (c.name == name)
            return plain::Sha256::of(c.bytes->data(), c.bytes->size());
    return {};
}

bool registered_any(void *root, const std::vector<std::string> &names)
{
    for (const std::string &n : names)
        if (career::save_data::registered_child(reinterpret_cast<uintptr_t>(root), n))
            return true;
    return false;
}

// The first half of a replaced mode the root's loop reaches: its base is checked, and every half of the request's generation
// is read into a fresh bag with the game's allocator, so the mode's halves are replaced together or not at all.
std::string prepare_swap(plain::Swap &s, void *bag, const std::string &name)
{
    std::string why = plain::swap_refusal(plan.requests, s, name, captured_sha(name));
    if (!why.empty())
        return why;
    auto parts = plain::fetch_group(s.generation, plan.requests[s.requests.front()].container);
    if (!parts)
        return "generation " + std::to_string(s.generation) + " has no intact copy of every half of it";
    void *allocator = nullptr;
    if (!mem::read(reinterpret_cast<uintptr_t>(bag), allocator) || !career::save_data::game_object(allocator))
        return "the game's bag of " + name + " has no allocator";
    for (const plain::Part &part : *parts)
        if (!career::save_data::registered_child(reinterpret_cast<uintptr_t>(load_root), part.name))
            return "the game has no registered " + part.name;
    std::map<std::string, std::unique_ptr<FreshBag>> made;
    for (plain::Part &part : *parts) {
        auto fresh = std::make_unique<FreshBag>();
        *reinterpret_cast<void **>(fresh->bag.data()) = allocator;
        if (!read_into_bag(fresh->bag.data(), part.bytes, why))
            return part.name + ": " + why;
        fresh->bytes = std::move(part.bytes);
        made[part.name] = std::move(fresh);
    }
    for (auto &[n, fresh] : made) {
        s.halves.push_back(n);
        swap_bags[n] = std::move(fresh);
    }
    return {};
}

// The bag the root's loop hands a registered child, or ours when a replace request names its mode. The mode reads its container
// once, from whichever bag it gets here.
void *swap_child(void *self, void *bag)
{
    std::string name;
    if (swaps.empty() || !career::save_data::container_name(reinterpret_cast<uintptr_t>(self), name))
        return bag;
    plain::Swap *s = plain::swap_for(swaps, name);
    if (!s || s->state == plain::Swap::State::refused)
        return bag;
    if (s->state == plain::Swap::State::pending) {
        std::string why = prepare_swap(*s, bag, name);
        if (!why.empty()) {
            s->state = plain::Swap::State::refused;
            s->why = why;
            swofty::log("[saves] did not replace %s: %s; the game reads its own containers", s->key.c_str(), why.c_str());
            return bag;
        }
        s->state = plain::Swap::State::applying;
    } else if (std::string why = plain::swap_refusal(plan.requests, *s, name, captured_sha(name)); !why.empty())
        swofty::log("[saves] %s: %s; generation %llu's copy is read anyway, so the pair stays from one generation", name.c_str(),
                    why.c_str(), (unsigned long long)s->generation);
    auto it = swap_bags.find(name);
    if (it == swap_bags.end())
        return bag;
    s->read.insert(name);
    set_captured(name, it->second->bytes);
    swofty::log("[saves] %s: the game reads generation %llu's copy in place of the loaded one", name.c_str(),
                (unsigned long long)s->generation);
    return it->second->bag.data();
}

// A swapped half the mode failed to read: its mode's requests are answered as failed.
void swap_read_failed(void *self, int code)
{
    std::string name;
    if (!career::save_data::container_name(reinterpret_cast<uintptr_t>(self), name))
        return;
    plain::Swap *s = plain::swap_for(swaps, name);
    if (!s)
        return;
    s->state = plain::Swap::State::refused;
    s->why = name + ": the mode failed to read generation " + std::to_string(s->generation) + "'s copy (code " +
             std::to_string(code) + ")";
    swofty::log("[saves] %s", s->why.c_str());
}

size_t careers_of(const std::string &key)
{
    std::lock_guard<std::mutex> g(hooks.lock);
    size_t now = 0;
    for (auto &[n, c] : career_tally)
        if (plain::mode_key(n) == key)
            now += c;
    return now;
}

// After the root load: the halves of a replaced mode the file lacked are read by their registered children, and every request
// of the mode is answered. A mode whose containers the game does not register is left to plain::at_load (the kept store).
void finish_swap(plain::Swap &s, void *root, void *root_bag, uint8_t reset_on_error, uint64_t groups)
{
    std::vector<std::string> named;
    for (size_t i : s.requests)
        named.push_back(plan.requests[i].container);
    auto answer = [&](bool ok, const std::string &detail) {
        for (size_t i : s.requests)
            if (!plan.handled[i])
                plain::request_done(plan, int(i), ok, detail);
    };
    if (s.state == plain::Swap::State::refused) {
        answer(false, s.why);
        return;
    }
    std::map<std::string, std::vector<uint8_t>> absent_bytes;
    if (s.state == plain::Swap::State::pending) {
        if (!registered_any(root, named))
            return;
        auto parts = plain::fetch_group(s.generation, named.front());
        if (!parts) {
            s.why = "generation " + std::to_string(s.generation) + " has no intact copy of every half of it";
            swofty::log("[saves] could not put back %s: %s", s.key.c_str(), s.why.c_str());
            answer(false, s.why);
            return;
        }
        for (plain::Part &part : *parts) {
            s.halves.push_back(part.name);
            absent_bytes[part.name] = std::move(part.bytes);
        }
    } else
        for (const std::string &h : plain::swap_absent(s))
            if (auto it = swap_bags.find(h); it != swap_bags.end())
                absent_bytes[h] = it->second->bytes;
    std::string why;
    std::vector<std::string> absent = plain::swap_absent(s);
    for (const std::string &h : absent) {
        uintptr_t child = career::save_data::registered_child(reinterpret_cast<uintptr_t>(root), h);
        if (!absent_bytes.count(h) || !bs::whole_bag(absent_bytes[h].data(), absent_bytes[h].size()))
            why = h + " is not one whole bag";
        else if (!child)
            why = "the game has no registered " + h;
        else if (!read_once().may_read(child))
            why = "the game has read " + h + " in this session already";
        if (!why.empty())
            break;
    }
    for (const std::string &h : absent) {
        if (!why.empty())
            break;
        if (!place_registered(root, root_bag, h, absent_bytes[h], reset_on_error, groups, why)) {
            why = h + ": " + why;
            break;
        }
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            seen_children.insert(h);
        }
        set_captured(h, std::move(absent_bytes[h]));
        s.read.insert(h);
    }
    if (!why.empty()) {
        if (!s.read.empty())
            why += "; its pair may now come from different generations";
        swofty::log("[saves] could not put back %s: %s", s.key.c_str(), why.c_str());
        answer(false, why);
        return;
    }
    size_t now = careers_of(s.key);
    for (const std::string &h : s.halves)
        swofty::log("%s", plain::put_back_line(h, now, s.generation).c_str());
    answer(true, (absent.size() == s.halves.size() ? "added " : "replaced ") + s.key + " with generation " +
                     std::to_string(s.generation));
}

// The put-back for the registered modes, after the root has loaded. A mode is two root containers, gm_<mode> and
// <mode>_SaveSlotGameModeComponent, and they are added together from one generation, or not at all. The game's own containers
// are never read a second time: a replace request was applied as the root's loop reached the mode (swap_child), and a container
// the file holds is otherwise left as the game read it. From the newest save generation, ours goes in when the loaded tree
// lacks the mode, or lacks one half and the present half is the one in that generation.
void fix_registered(void *root, void *root_bag, uint8_t reset_on_error, uint64_t groups)
{
    if (!env_on("SWOFTY_PLAIN_PUTBACK") || !bag_proven())
        return;
    std::set<std::string> swapped;
    for (plain::Swap &s : swaps) {
        swapped.insert(s.key);
        finish_swap(s, root, root_bag, reset_on_error, groups);
    }
    std::map<std::string, std::vector<std::string>> groups_of;
    if (plan.newest)
        for (const plain::Entry &e : plan.newest->containers)
            groups_of[plain::mode_key(e.name)].push_back(e.name);
    for (const plain::Request &q : plan.requests)
        groups_of.emplace(plain::mode_key(q.container), std::vector<std::string>{q.container});
    for (auto &[key, halves] : groups_of) {
        if (swapped.count(key))
            continue;
        std::set<std::string> have;
        size_t tree_careers = 0;
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            for (auto &h : halves)
                if (seen_children.count(h) || child_info.count(h))
                    have.insert(h);
            for (auto &[n, c] : career_tally)
                if (plain::mode_key(n) == key)
                    tree_careers += c;
        }
        if (have.empty() && !registered_any(root, halves))
            continue;
        int req = -1;
        for (auto &h : halves)
            if (req < 0)
                req = plain::request_for(plan, h);
        uint64_t generation = 0;
        size_t ours = 0;
        std::optional<std::vector<plain::Part>> parts;
        if (req >= 0) {
            if (!have.empty()) {
                std::string why = "the loaded save already has " + *have.begin() + " and the request is not a replace";
                for (auto &h : halves)
                    for (int more = plain::request_for(plan, h); more >= 0; more = plain::request_for(plan, h))
                        plain::request_done(plan, more, false, why);
                continue;
            }
            generation = plan.requests[size_t(req)].generation;
            parts = plain::fetch_group(generation, plan.requests[size_t(req)].container);
        } else if (plan.newest) {
            generation = plan.newest->generation;
            for (auto &h : halves)
                if (const plain::Entry *e = plain::newest_entry(plan, h))
                    ours = std::max<size_t>(ours, e->careers);
            bs::Mode mode = bs::registered_decision(!have.empty(), tree_careers, ours);
            bool absent_only = mode == bs::Mode::leave && tree_careers && ours && have.size() < halves.size();
            if (mode == bs::Mode::replace_empty) {
                swofty::log("[saves] did not put back %s: the loaded save holds it without careers and the game has read it; a "
                            "Restore on the Career Saves page puts generation %llu's back at the next Play",
                            key.c_str(), (unsigned long long)generation);
                continue;
            }
            if (mode == bs::Mode::leave && !absent_only)
                continue;
            parts = plain::fetch_group(generation, halves.front());
            if (parts && parts->size() != halves.size())
                parts.reset();
            if (parts && absent_only) {
                bool same = true;
                for (auto &part : *parts)
                    if (have.count(part.name))
                        same &= captured_sha(part.name) == part.sha256;
                if (!same) {
                    swofty::log("[saves] did not put back %s: the half of its pair in the loaded save differs from "
                                "generation %llu's",
                                key.c_str(), (unsigned long long)generation);
                    continue;
                }
                parts->erase(
                    std::remove_if(parts->begin(), parts->end(), [&](const plain::Part &p) { return have.count(p.name); }),
                    parts->end());
            }
        } else
            continue;
        std::string why;
        if (!parts)
            why = "generation " + std::to_string(generation) + " has no intact copy of every half of it";
        for (size_t i = 0; why.empty() && i < parts->size(); i++) {
            const plain::Part &part = (*parts)[i];
            if (!bs::whole_bag(part.bytes.data(), part.bytes.size()))
                why = part.name + " is not one whole bag";
            else if (child_info.count(part.name))
                why = "the game has read " + part.name + " already";
            else if (!career::save_data::registered_child(reinterpret_cast<uintptr_t>(root), part.name))
                why = "the game has no registered " + part.name;
        }
        if (why.empty()) {
            for (plain::Part &part : *parts) {
                if (!place_registered(root, root_bag, part.name, part.bytes, reset_on_error, groups, why)) {
                    why = part.name + ": " + why + "; its pair may now come from different generations";
                    break;
                }
                {
                    std::lock_guard<std::mutex> g(hooks.lock);
                    seen_children.insert(part.name);
                }
                set_captured(part.name, std::move(part.bytes));
            }
        }
        if (!why.empty()) {
            swofty::log("[saves] could not put back %s: %s; the game's own containers stay", key.c_str(), why.c_str());
            if (req >= 0)
                plain::request_done(plan, req, false, why);
            continue;
        }
        size_t now = careers_of(key);
        for (auto &h : halves)
            swofty::log("%s", plain::put_back_line(h, now ? now : ours, generation).c_str());
        if (req >= 0) {
            plain::request_done(plan, req, true, "added " + key + " from generation " + std::to_string(generation));
            for (auto &h : halves)
                for (int more = plain::request_for(plan, h); more >= 0; more = plain::request_for(plan, h))
                    plan.handled[size_t(more)] = true;
        }
    }
}

int *deserialise_entry(void *self, int *result, void *bag, uint8_t reset_on_error, uint64_t groups)
{
    bool root = is_root(reinterpret_cast<uintptr_t>(self)) && profile_load(reinterpret_cast<uintptr_t>(self));
    int depth = ++load_depth;
    if (root) {
        std::lock_guard<std::mutex> g(hooks.lock);
        store.begin_load();
        ++loads;
        slot_reads.clear();
        managers.clear();
        captured.clear();
        seen_children.clear();
        injected_names.clear();
        current_child.clear();
        career_tally.clear();
        child_info.clear();
        slots_clean = true;
        in_root_load = true;
    }
    if (root) {
        plan = plain::make_plan();
        load_root = self;
        swaps.clear();
        swap_bags.clear();
        if (plan.on && env_on("SWOFTY_PLAIN_PUTBACK") && bag_proven())
            swaps = plain::plan_swaps(plan.requests);
    }
    void *read_from = bag;
    if (!root && depth == 2 && !in_fix) {
        bool loading;
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            loading = in_root_load;
        }
        if (loading) {
            capture_child(self, bag, reset_on_error, groups);
            read_from = swap_child(self, bag);
            read_once().mark(reinterpret_cast<uintptr_t>(self));
        }
    }
    int *r = hooks.deserialise_container(self, result, read_from, reset_on_error, groups);
    if (read_from != bag && (!r || *r != 0))
        swap_read_failed(self, r ? *r : -1);
    --load_depth;
    if (root) {
        if (r && *r == 0 && plan.on) {
            in_fix = true;
            fix_registered(self, bag, reset_on_error, groups);
            in_fix = false;
        }
        if (r && *r == 0 && plain::enabled()) {
            std::map<std::string, std::string> registered, kept;
            {
                std::lock_guard<std::mutex> g(hooks.lock);
                for (auto &n : seen_children)
                    registered[n] = {};
                for (auto &c : captured)
                    registered[c.name] = plain::Sha256::of(c.bytes->data(), c.bytes->size());
                for (auto &k : store.kept())
                    kept[k.name] = plain::Sha256::of(k.body.data(), k.body.size());
            }
            std::vector<plain::Added> added = plain::at_load(plan, registered, kept);
            std::lock_guard<std::mutex> g(hooks.lock);
            for (auto &a : added) {
                size_t size = a.bytes.size();
                switch (store.add(a.name, std::move(a.bytes))) {
                case sp::Add::kept:
                case sp::Add::replaced:
                    injected_names.insert(a.name);
                    if (!a.injected_by_request)
                        swofty::log("%s", plain::put_back_line(a.name, a.careers, a.generation).c_str());
                    break;
                default:
                    swofty::log("[saves] could not put back %s (%zu bytes): the store is full", a.name.c_str(), size);
                }
            }
        }
        std::vector<cs::SlotRead> reads;
        bool report;
        std::vector<Manager> seen;
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            in_root_load = false;
            seen = managers;
            report = hooks.load_slot_manager && slots_clean && r && *r == 0;
            reads.swap(slot_reads);
        }
        if (report)
            runtime::career_saves_listed(reads, true, true);
        log_load(reads, seen, r);
        runtime::direct_start_profile_loaded(r && *r == 0);
        // W4: the swofty_registry body this load kept, for the career snapshots.
        std::vector<uint8_t> entry;
        bool has_entry = false;
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            if (store.size())
                swofty::log("[saves] profile load %u: kept %zu save container(s) no loaded game mode claims (%llu "
                            "bytes); the next profile save writes them back",
                            loads, store.size(), (unsigned long long)store.total());
            for (auto &k : store.kept())
                swofty::log("[saves] %s", career::save_diag::kept_line(loads, k.name, k.body.size()).c_str());
            for (auto &k : store.kept())
                if (k.name == sp::ENTRY_NAME)
                    entry = k.body, has_entry = true;
        }
        if (hooks.live() && entry_hooks.loaded)
            entry_hooks.loaded(has_entry ? &entry : nullptr);
        if (r && *r == 0 && plain::enabled()) {
            std::map<std::string, std::vector<plain::Slot>> slots;
            size_t at = 0;
            for (const Manager &m : seen)
                for (size_t i = 0; i < m.count && at < reads.size(); i++, at++)
                    if (!m.container.empty() && cs::career_slot(reads[at]))
                        slots[m.container].push_back(plain_slot(reads[at]));
            std::vector<plain::Body> bodies;
            {
                std::lock_guard<std::mutex> g(hooks.lock);
                bodies = plain_bodies(false, slots);
            }
            plain::write_async("load", std::move(bodies));
        }
    }
    return r;
}

int slots_entry(void *iface, void *bag, void *ctx)
{
    int rc = hooks.load_slot_manager(iface, bag, ctx);
    std::vector<cs::SlotRead> reads;
    uintptr_t base = reinterpret_cast<uintptr_t>(iface), vt = 0;
    bool clean = rc == 0 && read_manager(base, reads) && mem::read(base, vt);
    std::lock_guard<std::mutex> g(hooks.lock);
    if (in_root_load) {
        if (clean) {
            managers.push_back({base, vt, reads.size(), current_child});
            if (!current_child.empty())
                for (auto &r : reads)
                    if (cs::career_slot(r))
                        career_tally[current_child]++;
            for (auto &r : reads)
                slot_reads.push_back(std::move(r));
        } else
            slots_clean = false;
    }
    return rc;
}

void unclaimed_entry(SwoftyContext *c)
{
    if (!hooks.live() || !is_root(c->rdi))
        return;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        if (!in_root_load || !profile_root.is(c->rdi))
            return;
    }
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
    if (ok && name == sp::HIGHLIGHTS_CHILD) {
        std::lock_guard<std::mutex> g(hooks.lock);
        swofty::log("[saves] profile load %u: left out %s, the race highlights save's own container, which an older core "
                    "copied into the profile; the game drops it and the highlights save keeps its own",
                    loads, name.c_str());
        return;
    }
    ok = ok && encode(bag, bytes, why);
    std::lock_guard<std::mutex> g(hooks.lock);
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
        swofty::log("[saves] profile load %u: kept %s (%zu bytes), which no loaded game mode claims", loads, name.c_str(), size);
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

// ---- the plain store's save capture ----
// The inline profile save writes each registered child straight into the save stream, so no bag of it exists. From the end of
// the kept entries to the end of the containers block the core runs the stream through a copy of its vtable whose write slot
// also appends the bytes to a buffer (the game's own slot still does the write). At the end of the block the buffer is split
// by the bag format into the registered children, and used only when exactly the registered children came out, whole.
constexpr size_t TEE_SLOTS = 64;
constexpr size_t TEE_MAX = 192ull << 20;
struct Tee {
    bool on = false, overflow = false, in_length = false;
    void *stream = nullptr;
    void **original = nullptr;
    void *write = nullptr, *write_length = nullptr;
    sp::TeeBuffer buffer{TEE_MAX};
};
Tee tee;
void *tee_vtable[TEE_SLOTS];

int tee_write(void *stream, const void *data, uint64_t size)
{
    int r = reinterpret_cast<Write_t>(tee.write)(stream, data, size);
    if (!r && !tee.in_length)
        tee.buffer.append(static_cast<const uint8_t *>(data), size);
    return r;
}

int tee_write_length(void *stream, uint32_t length)
{
    tee.in_length = true;
    int r = reinterpret_cast<WriteLength_t>(tee.write_length)(stream, length);
    tee.in_length = false;
    if (!r)
        tee.buffer.push(uint8_t(length));
    return r;
}

void tee_stop()
{
    if (tee.on && tee.stream) {
        void **now = nullptr;
        if (mem::read(reinterpret_cast<uintptr_t>(tee.stream), now) && now == tee_vtable)
            *reinterpret_cast<void ***>(tee.stream) = tee.original;
    }
    tee.on = false;
}

void tee_start(void *stream)
{
    tee_stop();
    tee.buffer.clear();
    tee.in_length = false;
    saved_caps.clear();
    saved_caps_ok = false;
    uintptr_t vt = 0;
    if (!stream || !mem::read(reinterpret_cast<uintptr_t>(stream), vt) || !mem::in_image(vt) || !mem::readable(vt, TEE_SLOTS * 8))
        return;
    uintptr_t w = 0, wl = 0;
    if (!mem::read(vt + offsetof(g::NeStreamVtbl, write), w) || !mem::in_image(w) ||
        !mem::read(vt + offsetof(g::NeStreamVtbl, write_length), wl) || !mem::in_image(wl))
        return;
    for (size_t i = 0; i < TEE_SLOTS; i++)
        mem::read(vt + i * 8, tee_vtable[i]);
    tee.original = reinterpret_cast<void **>(vt);
    tee.write = reinterpret_cast<void *>(w);
    tee.write_length = reinterpret_cast<void *>(wl);
    tee_vtable[offsetof(g::NeStreamVtbl, write) / 8] = reinterpret_cast<void *>(&tee_write);
    tee_vtable[offsetof(g::NeStreamVtbl, write_length) / 8] = reinterpret_cast<void *>(&tee_write_length);
    tee.stream = stream;
    *reinterpret_cast<void ***>(stream) = tee_vtable;
    tee.on = true;
}

void tee_finish()
{
    if (!tee.on)
        return;
    tee_stop();
    std::vector<std::string> names;
    bool known;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        names = registered_now;
        known = registered_now_ok;
    }
    std::vector<bs::Entry> parts;
    std::string why;
    if (tee.buffer.no_memory())
        why = "the process is out of memory";
    else if (tee.buffer.overflow())
        why = "the registered containers are over " + std::to_string(TEE_MAX >> 20) + " MB";
    else if (!known)
        why = "the root's children were not readable";
    else if (!bs::split_entries(tee.buffer.bytes().data(), tee.buffer.bytes().size(), names.size(), parts))
        why = "the bytes written for the root's children (" + std::to_string(tee.buffer.bytes().size()) + ") do not split into " +
              std::to_string(names.size()) + " whole containers";
    std::vector<Captured> got;
    if (why.empty()) {
        std::set<std::string> want(names.begin(), names.end());
        for (const bs::Entry &e : parts) {
            if (!want.erase(e.name)) {
                why = "the container " + e.name + " was not among the registered children";
                break;
            }
            got.push_back({e.name, std::make_shared<const std::vector<uint8_t>>(tee.buffer.bytes().begin() + long(e.begin),
                                                                                tee.buffer.bytes().begin() + long(e.end))});
        }
        if (why.empty() && !want.empty())
            why = "a registered child was not written";
    }
    tee.buffer.release();
    if (!why.empty()) {
        swofty::log("[saves] plain store: could not capture the registered containers at save: %s; this generation holds "
                    "their load-time bytes (as_of load)",
                    why.c_str());
        return;
    }
    std::lock_guard<std::mutex> g(hooks.lock);
    saved_caps = std::move(got);
    saved_caps_ok = true;
}
// ---- end plain store save capture ----

void count_entry(SwoftyContext *c)
{
    uintptr_t self = 0;
    if (!hooks.live() || !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self))
        return;
    uint64_t count = 0;
    uintptr_t begin = 0;
    bool have_children = mem::read(c->rsp + FRAME_COUNT, count) && mem::read(c->rsp + FRAME_CHILDREN, begin);
    std::vector<std::string> registered;
    bool names_ok = have_children && child_names(begin, c->r15, registered) && registered.size() == count;
    if (!profile_save(self, names_ok, registered))
        return;
    runtime::career_saves_saving();
    // W4: the career snapshots' new swofty_registry body, when they changed since the last write.
    std::vector<uint8_t> entry;
    bool has_entry = entry_hooks.saving && entry_hooks.saving(entry);
    std::lock_guard<std::mutex> g(hooks.lock);
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
    registered_now = names_ok ? registered : std::vector<std::string>();
    registered_now_ok = names_ok;
    tee_stop();
    if (!store.size() || !have_children)
        return;
    if (!names_ok) {
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
        swofty::log("%s", career::save_heap::session_line(heap_bytes, raised, picked.size(), bytes).c_str());
    }
    *reinterpret_cast<uint64_t *>(c->rsp + FRAME_COUNT) =
        owed.bump(reinterpret_cast<const void *>(c->r12), count, std::move(picked));
}

template <class SV = g::NeStreamVtbl>
int write_kept(void *stream, uint32_t bag_type, const std::vector<size_t> &picked, std::string &names, std::string &failed)
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
            if (r) {
                failed = k.name;
                return r;
            }
            names += (names.empty() ? "" : ", ") + k.name;
        }
        return 0;
    } else {
        (void)stream, (void)bag_type, (void)picked, (void)names, (void)failed;
        return WRITE_FAILED;
    }
}

void children_inner(SwoftyContext *c)
{
    uintptr_t self = 0;
    if (!hooks.live() || !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self) || !saving_profile(self))
        return;
    std::lock_guard<std::mutex> g(hooks.lock);
    std::vector<size_t> picked = owed.take(reinterpret_cast<const void *>(c->r12));
    if (picked.empty())
        return;
    void *stream = nullptr;
    uint32_t bag_type = 0;
    mem::read(c->rsp + FRAME_STREAM, stream);
    mem::read(c->rsp + FRAME_BAG_TYPE, bag_type);
    std::string names, failed;
    int r = write_kept(stream, bag_type, picked, names, failed);
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
        uint64_t bytes = 0;
        for (size_t i : picked)
            bytes += i < store.size() ? store.kept()[i].body.size() : 0;
        swofty::log("[saves] profile save: writing back the kept save containers failed with code %d at %s (%zu kept, %llu "
                    "bytes); the save fails as on any write error and the game writes no file",
                    r, failed.empty() ? "?" : failed.c_str(), picked.size(), (unsigned long long)bytes);
        return;
    }
    swofty::log("[saves] profile save: wrote back %zu save container(s) no loaded game mode claims: %s", picked.size(),
                names.c_str());
}

void children_entry(SwoftyContext *c)
{
    auto rip = c->rip;
    children_inner(c);
    uintptr_t self = 0;
    if (c->rip != rip || !hooks.live() || !plain::enabled() || !env_on("SWOFTY_PLAIN_SAVE_CAPTURE") || !bag_proven() ||
        !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self) || !saving_profile(self))
        return;
    void *stream = nullptr;
    mem::read(c->rsp + FRAME_STREAM, stream);
    std::lock_guard<std::mutex> g(hooks.lock);
    tee_start(stream);
}

void saved_entry(SwoftyContext *c)
{
    uintptr_t self = 0;
    int result = 1;
    if (!hooks.live() || !mem::read(c->rsp + FRAME_SELF, self) || !is_root(self) || !saving_profile(self))
        return;
    tee_finish();
    if (!mem::read(c->r12, result) || result)
        return;
    std::vector<cs::SlotRead> reads;
    std::vector<Manager> known;
    {
        std::lock_guard<std::mutex> g(hooks.lock);
        known = managers;
    }
    bool clean = !known.empty();
    std::map<std::string, std::vector<plain::Slot>> slots;
    for (const Manager &m : known) {
        uintptr_t vt = 0;
        std::vector<cs::SlotRead> own;
        if (!mem::read(m.iface, vt) || vt != m.vtable || !read_manager(m.iface, own))
            clean = false;
        else if (!m.container.empty())
            for (const cs::SlotRead &r : own)
                if (cs::career_slot(r))
                    slots[m.container].push_back(plain_slot(r));
        for (auto &r : own)
            reads.push_back(std::move(r));
    }
    if (clean)
        runtime::career_saves_listed(reads, true, false);
    runtime::career_saves_saved();
    if (plain::enabled()) {
        std::vector<plain::Body> bodies;
        {
            std::lock_guard<std::mutex> g(hooks.lock);
            bodies = plain_bodies(true, slots);
        }
        plain::write_async("save", std::move(bodies));
    }
}

}

namespace runtime {

static std::vector<CoreMixins> save_splice_mixins()
{
    std::vector<CoreMixins> out;
    if (hooks.gate.offered() || !code_ready())
        return out;
    hooks.gate.offer();
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
    std::string why;
    if (!SpliceHooks::bind({{"vt_Game_SaveDataRootContainer", &b.root_vt},
                            {"Game_SaveData_WriteBag", &b.write_bag},
                            {"Ne_StringTable_Lookup", &b.lookup},
                            {"k_Ne_BagCodecVtbl", &b.codec_vt},
                            {"k_Ne_CountingStreamVtbl", &b.counting_vt},
                            {"k_Ne_MemWriteStreamVtbl", &b.memory_vt},
                            {"g_Ne_StringTable", &b.table},
                            {"k_Ne_PropertyType_Bag", &b.bag_type},
                            {"site_Game_SaveData_ContainersDone", &b.done}},
                           why)) {
        swofty::log("[saves] save splice off: %s", why.c_str());
        return out;
    }
    out.push_back({MIXIN,
                   {{"Game_SaveData_DeserialiseContainer", SWOFTY_MIXIN_WRAP, 100, (void *)&deserialise_entry,
                     hooks.deserialise_container.slot()},
                    {"site_Game_SaveData_UnclaimedChild", SWOFTY_MIXIN_SITE, 100, (void *)&unclaimed_entry, nullptr},
                    {"site_Game_SaveData_ContainersCount", SWOFTY_MIXIN_SITE, 100, (void *)&count_entry, nullptr},
                    {"site_Game_SaveData_ContainersChildren", SWOFTY_MIXIN_SITE, 100, (void *)&children_entry, nullptr}}});
    if (swofty::api->resolve("Game_SaveSlotManager_Deserialise"))
        out.push_back({SLOTS_MIXIN,
                       {{"Game_SaveSlotManager_Deserialise", SWOFTY_MIXIN_WRAP, 100, (void *)&slots_entry,
                         hooks.load_slot_manager.slot()}}});
    out.push_back({SAVED_MIXIN, {{"site_Game_SaveData_ContainersDone", SWOFTY_MIXIN_SITE, 100, (void *)&saved_entry, nullptr}}});
    return out;
}

static void save_splice_commit()
{
    career_saves_start();
    if (!layout::SLOT_DETAILS)
        swofty::log("[saves] career list details off: needs SaveSlotInfo.game_setup_mode, info_season, info_weekend, "
                    "info_team, info_driver, info_vehicle, info_date and type_flags (a verify agree row each); the "
                    "list still holds every save slot's key");
    if (!b.done || !hooks.gate.settle()) {
        snapshots_start(false);
        return;
    }
    if (core_mixins_active(MIXIN))
        hooks.set_live();
    swofty::log("[saves] save splice %s",
                loader::install_state(hooks.live(),
                                      "armed: save containers no loaded game mode claims are kept through each profile "
                                      "load and save"));
    snapshots_start(hooks.live());
}

static loader::FunctionSet save_splice_core_set{loader::CoreSetId::save_splice, save_splice_mixins, save_splice_commit};

}

// ---- W4 career snapshots ----
namespace career::save_splice {

void set_entry_hooks(const EntryHooks &h) { entry_hooks = h; }

std::string entry_blocked()
{
    if (!hooks.live())
        return "the save splice is not armed";
    if constexpr (layout::SAVE_SPLICE) {
        auto bag_id = reinterpret_cast<Id_t>(method(reinterpret_cast<void *>(b.bag_type), offsetof(g::NePropertyTypeVtbl, id)));
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

// ---- end W4 ----

}
