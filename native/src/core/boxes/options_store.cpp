#include "core/boxes/options_store.hpp"

#include <cstring>

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

namespace runtime::options_store {

namespace {

namespace mem = swofty::mem;

template <class T> T at(const void *base, size_t off)
{
    T v{};
    memcpy(&v, static_cast<const uint8_t *>(base) + off, sizeof v);
    return v;
}

}

bool code_ptr(const void *p)
{
    auto a = reinterpret_cast<uintptr_t>(p);
    uintptr_t base = swofty::api->module_base();
    return p && a >= base && a < base + 0x20000000 && mem::readable(p, 1);
}

std::string text_at(const char *p)
{
    if (!p || !mem::readable(p, 1))
        return {};
    std::string out;
    for (size_t i = 0; i < car_category::VALUE_NAME_MAX && mem::readable(p + i, 1) && p[i]; ++i)
        out.push_back(p[i]);
    return out;
}

Found find(uintptr_t find_fn, void *store, const char *name, void *&option)
{
    option = nullptr;
    if (!mem::readable(store, STORE_END + 8))
        return Found::store_unreadable;
    uint64_t index = reinterpret_cast<swofty::game::Game_OptionsStore_FindOption_t *>(find_fn)(
        static_cast<uint8_t *>(store) + STORE_MAP, &name);
    auto *base = at<uint8_t *>(store, STORE_ENTRIES), *end = at<uint8_t *>(store, STORE_END);
    if (!base || end < base || index >= size_t(end - base) / STORE_ENTRY)
        return Found::no_option;
    uint8_t *entry = base + index * STORE_ENTRY;
    void *found = mem::readable(entry, STORE_ENTRY) ? at<void *>(entry, 8) : nullptr;
    if (!mem::readable(found, OPT_VALUES_END + 8))
        return Found::option_unreadable;
    option = found;
    return Found::ok;
}

std::vector<car_category::Value> values(void *option, const uint8_t *key)
{
    std::vector<car_category::Value> out;
    auto *v = at<uint8_t *>(option, OPT_VALUES), *e = at<uint8_t *>(option, OPT_VALUES_END);
    if (!v || e < v || size_t(e - v) % VALUE_BYTES || size_t(e - v) / VALUE_BYTES > MAX_VALUES ||
        !mem::readable(v, size_t(e - v)))
        return out;
    for (uint8_t *entry = v; entry < e; entry += VALUE_BYTES) {
        car_category::Value value;
        auto *display = at<uint8_t *>(entry, 0);
        if (mem::readable(display, DISPLAY_TEXT + 8))
            value.display = text_at(at<const char *>(display, DISPLAY_TEXT));
        auto *holder = at<uint8_t *>(entry, VALUE_NAME);
        auto *name = mem::readable(holder, 8) ? at<uint8_t *>(holder, 0) : nullptr;
        if (mem::readable(name, NAME_BUF + 8)) {
            if (at<int64_t>(name, NAME_PLAIN) == 1) {
                value.name = text_at(at<const char *>(name, NAME_BUF));
            } else {
                auto len = at<uint64_t>(name, NAME_LEN);
                auto *src = at<const uint8_t *>(name, NAME_SRC);
                if (key && len >= 2 && len <= car_category::VALUE_NAME_MAX && mem::readable(src, size_t(len)) &&
                    mem::readable(key, 4))
                    value.name = car_category::decode_name(src, size_t(len), key);
            }
        }
        out.push_back(value);
    }
    return out;
}

bool get_index(const void *option, int &index)
{
    auto *get = vfunc<int(const void *, void *)>(option, 2);
    if (!get)
        return false;
    uint64_t scratch[2] = {};
    index = get(option, scratch);
    return true;
}

bool set_index(const void *option, int index)
{
    auto *set = vfunc<void(const void *, int)>(option, 3);
    if (!set)
        return false;
    set(option, index);
    return true;
}

}
