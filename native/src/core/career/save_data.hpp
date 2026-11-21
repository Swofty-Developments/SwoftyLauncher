#pragma once

#include <swofty/game.hpp>

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "core/career/career_saves_logic.hpp"

// The save splice's reads of the game's save-data objects (save_data.cpp): the root container, a child's
// property and bag, the bag codec, the string table, the save slot managers and the root's registered children.
namespace career::save_data {

struct Bindings {
    uintptr_t root_vt = 0, write_bag = 0, lookup = 0, codec_vt = 0, counting_vt = 0, memory_vt = 0, table = 0, bag_type = 0,
              done = 0;
};

// Resolved by save_splice_mixins (save_splice.cpp).
extern Bindings b;

using Id_t = int (*)(void *self);

// The function at offset in obj's vtable, or null when the vtable or the slot does not read inside the image.
void *method(const void *obj, size_t offset);
// Whether self is a save's root container: the profile's or another save's (the race highlights), which share the class.
// save_splice.cpp tells the profile's apart (career::save_splice::ProfileRoot).
bool is_root(uintptr_t self);
// A child's key hash and bag value; false with why when it is not a readable bag.
template <class PV = swofty::game::NePropertyVtbl, class TV = swofty::game::NePropertyTypeVtbl>
bool read_property(void *prop, uint64_t &hash, void *&bag, std::string &why);
// The bag's encoded body (a size pass, then a write pass into out); false with why on any failure.
template <class C = swofty::game::NeBagCodec, class CS = swofty::game::NeCountingStream,
          class MS = swofty::game::NeMemWriteStream, class SV = swofty::game::NeStreamVtbl>
bool encode(void *bag, std::vector<uint8_t> &out, std::string &why);
// The string table's name for a key hash, or empty.
std::string name_of(uint64_t hash);
// Every save slot of the manager at base; false when the list or a slot does not read.
bool read_manager(uintptr_t base, std::vector<career::saves::SlotRead> &out);
// The container's name (the field the game's logs print); false when it does not read.
bool container_name(uintptr_t self, std::string &out);
// The registered child of the root container named name: found in the root's child table and checked against the child's own
// name; 0 when it is not there or does not check out.
uintptr_t registered_child(uintptr_t root, const std::string &name);
// Whether obj starts with a vtable pointer into the game image, the cheapest check that a pointer is a live game object.
bool game_object(const void *obj);
bool bag_object(const void *bag);
// The names of the root's registered children between begin and end; false when one does not read.
template <class S = swofty::game::SaveDataContainer>
bool child_names(uintptr_t begin, uintptr_t end, std::vector<std::string> &out);

}
