#include "core/career/save_data.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "core/career/save_splice_logic.hpp"
#include "core/runtime/layout.hpp"

namespace career::save_data {

namespace {

namespace mem = swofty::mem;
namespace g = swofty::game;
namespace sp = career::save_splice;
namespace layout = runtime::layout;
namespace cs = career::saves;

using WriteBag_t = int (*)(void *codec, void *bag);
using Lookup_t = const char *(*)(void *table, uint64_t hash);
using Get_t = void *(*)(void *self);
using Offset_t = size_t (*)(void *self);

constexpr size_t MAX_CHILDREN = 4096;
constexpr size_t SLOTS_BEGIN = 0x250;
constexpr size_t SLOTS_END = 0x258;
constexpr size_t MAX_SLOTS = 4096;

template <class S = g::SaveSlotInfo> bool read_slot(uintptr_t slot, cs::SlotRead &out)
{
    if (!slot || !mem::read(slot, out.id) || !mem::read(slot + 8, out.guid))
        return false;
    if constexpr (layout::slot_details<S>) {
        bool ok = mem::read(slot + offsetof(S, game_setup_mode), out.setup_mode) &&
                  mem::read(slot + offsetof(S, info_season), out.season) &&
                  mem::read(slot + offsetof(S, info_weekend), out.weekend) &&
                  mem::read(slot + offsetof(S, info_team), out.team) && mem::read(slot + offsetof(S, info_driver), out.driver) &&
                  mem::read(slot + offsetof(S, info_vehicle), out.vehicle) &&
                  mem::read(slot + offsetof(S, info_date), out.date) && mem::read(slot + offsetof(S, type_flags), out.flags);
        out.details = ok;
        if constexpr (layout::slot_name<S>) {
            uintptr_t name = 0;
            if (ok && mem::read(slot + offsetof(S, name), name) && name)
                mem::c_string(reinterpret_cast<const char *>(name), out.name, 129);
        }
    }
    return true;
}

}

void *method(const void *obj, size_t offset)
{
    uintptr_t vt = 0, fn = 0;
    if (!obj || !mem::read(reinterpret_cast<uintptr_t>(obj), vt) || !mem::in_image(vt) || !mem::read(vt + offset, fn) ||
        !mem::in_image(fn))
        return nullptr;
    return reinterpret_cast<void *>(fn);
}

bool is_root(uintptr_t self)
{
    uintptr_t vt = 0;
    return b.root_vt && self && mem::read(self, vt) && vt == b.root_vt;
}

template <class PV, class TV> bool read_property(void *prop, uint64_t &hash, void *&bag, std::string &why)
{
    if constexpr (layout::property<PV, TV>) {
        auto type_of = reinterpret_cast<Get_t>(method(prop, offsetof(PV, type)));
        auto value_of = reinterpret_cast<Get_t>(method(prop, offsetof(PV, value)));
        if (!type_of || !value_of)
            return why = "its property has no readable vtable", false;
        void *type = type_of(prop);
        auto key_offset = reinterpret_cast<Offset_t>(method(type, offsetof(TV, key_offset)));
        auto id = reinterpret_cast<Id_t>(method(type, offsetof(TV, id)));
        auto bag_id = reinterpret_cast<Id_t>(method(reinterpret_cast<void *>(b.bag_type), offsetof(TV, id)));
        if (!key_offset || !id || !bag_id)
            return why = "its value type has no readable vtable", false;
        size_t at = key_offset(type);
        if (at > 0x1000 || !mem::read(reinterpret_cast<uintptr_t>(prop) + at, hash))
            return why = "its key hash is not readable", false;
        if (id(type) != bag_id(reinterpret_cast<void *>(b.bag_type)))
            return why = "it is not a bag", false;
        bag = value_of(prop);
        return bag ? true : (why = "it has no bag", false);
    } else {
        (void)prop, (void)hash, (void)bag;
        return why = "this core lacks the property layout", false;
    }
}

template bool read_property<>(void *prop, uint64_t &hash, void *&bag, std::string &why);

template <class C, class CS, class MS, class SV> bool encode(void *bag, std::vector<uint8_t> &out, std::string &why)
{
    if constexpr (layout::save_streams<C, CS, MS, SV>) {
        static_assert(sizeof(CS) <= 0x30 && sizeof(MS) <= 0x28 && sizeof(C) <= 0x10);
        auto write_bag = reinterpret_cast<WriteBag_t>(b.write_bag);
        alignas(16) std::array<uint8_t, 0x30> counting{};
        alignas(16) std::array<uint8_t, 0x10> codec{};
        auto *cs = reinterpret_cast<CS *>(counting.data());
        auto *c = reinterpret_cast<C *>(codec.data());
        cs->vtable = reinterpret_cast<void **>(b.counting_vt);
        c->vtable = reinterpret_cast<void **>(b.codec_vt);
        c->stream = cs;
        if (int r = write_bag(c, bag))
            return why = "the size pass failed with code " + std::to_string(r), false;
        uint64_t size = cs->count;
        if (size < 16 || size > sp::MAX_BODY)
            return why = "its size " + std::to_string(size) + " is out of range", false;
        out.assign(size_t(size), 0);
        alignas(16) std::array<uint8_t, 0x28> memory{};
        auto *ms = reinterpret_cast<MS *>(memory.data());
        ms->vtable = reinterpret_cast<void **>(b.memory_vt);
        ms->data = out.data();
        ms->capacity = size;
        c->stream = ms;
        if (int r = write_bag(c, bag))
            return why = "the write pass failed with code " + std::to_string(r), false;
        if (ms->position != size)
            return why = "the write pass wrote " + std::to_string(ms->position) + " of " + std::to_string(size) + " bytes", false;
        return true;
    } else {
        (void)bag, (void)out;
        return why = "this core lacks the stream layout", false;
    }
}

template bool encode<>(void *bag, std::vector<uint8_t> &out, std::string &why);

std::string name_of(uint64_t hash)
{
    uintptr_t table = 0;
    if (!mem::read(b.table, table) || !table)
        return {};
    const char *s = reinterpret_cast<Lookup_t>(b.lookup)(reinterpret_cast<void *>(table), hash);
    std::string out;
    return s && mem::c_string(s, out, sp::NAME_LIMIT + 1) ? out : std::string();
}

bool read_manager(uintptr_t base, std::vector<career::saves::SlotRead> &out)
{
    uintptr_t begin = 0, end = 0;
    if (!(mem::read(base + SLOTS_BEGIN, begin) && mem::read(base + SLOTS_END, end) && begin <= end &&
          (end - begin) / 8 <= MAX_SLOTS))
        return false;
    for (uintptr_t at = begin; at < end; at += 8) {
        uintptr_t slot = 0;
        cs::SlotRead r;
        if (!mem::read(at, slot) || !read_slot(slot, r))
            return false;
        out.push_back(std::move(r));
    }
    return true;
}

template <class S> bool child_names(uintptr_t begin, uintptr_t end, std::vector<std::string> &out)
{
    if constexpr (layout::named_container<S>) {
        if (end < begin || (end - begin) % 8 || (end - begin) / 8 > MAX_CHILDREN)
            return false;
        for (uintptr_t at = begin; at < end; at += 8) {
            uintptr_t child = 0, name = 0;
            std::string n;
            if (!mem::read(at, child) || !mem::read(child + offsetof(S, name), name) || !mem::c_string(name, n, 256))
                return false;
            out.push_back(n);
        }
        return true;
    } else {
        (void)begin, (void)end, (void)out;
        return false;
    }
}

bool container_name(uintptr_t self, std::string &out)
{
    if constexpr (layout::named_container<g::SaveDataContainer>) {
        uintptr_t name = 0;
        return self && mem::read(self + offsetof(g::SaveDataContainer, name), name) && name && mem::c_string(name, out, 256) &&
               !out.empty();
    } else {
        (void)self, (void)out;
        return false;
    }
}

uintptr_t registered_child(uintptr_t root, const std::string &name)
{
    constexpr size_t TABLE_BEGIN = 0x160, TABLE_END = 0x168, NODE_NAME = 0x10, NODE_CHILD = 0x58;
    uintptr_t begin = 0, end = 0;
    if (!root || !mem::read(root + TABLE_BEGIN, begin) || !mem::read(root + TABLE_END, end) || begin > end ||
        (end - begin) / 8 > MAX_CHILDREN * 4 || (begin & 7))
        return 0;
    for (uintptr_t at = begin; at < end; at += 8) {
        uintptr_t node = 0, name_ptr = 0, child = 0;
        std::string n, cn;
        if (!mem::read(at, node) || node < 0x10000 || !mem::read(node + NODE_NAME, name_ptr) || !name_ptr ||
            !mem::c_string(name_ptr, n, 64) || n != name || !mem::read(node + NODE_CHILD, child) || !child ||
            !container_name(child, cn) || cn != name || !game_object(reinterpret_cast<const void *>(child)))
            continue;
        return child;
    }
    return 0;
}

bool game_object(const void *obj)
{
    uintptr_t vt = 0;
    return obj && mem::read(reinterpret_cast<uintptr_t>(obj), vt) && mem::in_image(vt);
}

bool bag_object(const void *bag)
{
    uintptr_t allocator = 0;
    return bag && mem::read(reinterpret_cast<uintptr_t>(bag), allocator) &&
           game_object(reinterpret_cast<const void *>(allocator));
}

template bool child_names<>(uintptr_t begin, uintptr_t end, std::vector<std::string> &out);

}
