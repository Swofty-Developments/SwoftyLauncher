#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include <swofty/mem.hpp>

#include "core/boxes/car_category_logic.hpp"

// The game's options store (g_Game_OptionsStore). Game_OptionsStore_FindOption on its map at +0x270 gives an index into
// the entries at +0x288..+0x290 (0x10 bytes each, the option at +8). An option's values run +0x80..+0x88, 0x50 bytes
// each, with the display text holder at +0 and the name holder at +0x38; its vtable +0x10/+0x18 get and set the
// selected index. car_category.cpp and box_options.cpp both read it this way.
namespace runtime::options_store {

constexpr size_t STORE_MAP = 0x270;
constexpr size_t STORE_ENTRIES = 0x288;
constexpr size_t STORE_END = 0x290;
constexpr size_t STORE_ENTRY = 0x10;
constexpr size_t OPT_VALUES = 0x80;
constexpr size_t OPT_VALUES_END = 0x88;
constexpr size_t VALUE_BYTES = 0x50;
constexpr size_t VALUE_NAME = 0x38;
constexpr size_t NAME_LEN = 0x10;
constexpr size_t NAME_SRC = 0x18;
constexpr size_t NAME_PLAIN = 0x40;
constexpr size_t NAME_BUF = 0x48;
constexpr size_t DISPLAY_TEXT = 0x10;
constexpr size_t MAX_VALUES = 64;

enum class Found { ok, store_unreadable, no_option, option_unreadable };

bool code_ptr(const void *p);

template <class F> F *vfunc(const void *object, size_t slot)
{
    if (!swofty::mem::readable(object, 8))
        return nullptr;
    auto **vt = *static_cast<void **const *>(object);
    if (!swofty::mem::readable(vt, (slot + 1) * 8) || !code_ptr(vt[slot]))
        return nullptr;
    return reinterpret_cast<F *>(vt[slot]);
}

std::string text_at(const char *p);
Found find(uintptr_t find_fn, void *store, const char *name, void *&option);
std::vector<car_category::Value> values(void *option, const uint8_t *key);
bool get_index(const void *option, int &index);
bool set_index(const void *option, int index);

}
