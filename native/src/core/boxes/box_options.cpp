#include "core/boxes/box_options.hpp"

#include "core/runtime/runtime.hpp"

#include <algorithm>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <array>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#include "core/apply/content/box_options.hpp"
#include "core/boxes/car_category_logic.hpp"
#include "core/boxes/options_store.hpp"

// Box rules that are game options (content.package 1.5: rules.safety_car, rules.formation_lap, rules.penalties,
// rules.corner_cutting; apply/content/box_options.hpp). The era picker calls box_option_apply with the pressed box's
// setting for each rule it carries and box_option_put_back for each rule it does not (and for all of them when
// another box or a stock box is pressed). Each option is found and set through the options store reader
// (options_store.hpp) that car_category.cpp uses for the category option. Every step checks what it reads; a shape it
// does not know, a missing option or a value it cannot match sets nothing and says why.

namespace {

namespace mem = swofty::mem;
namespace content = swofty_core::apply::content;
namespace store = runtime::options_store;

std::mutex lock;
std::array<content::OptionForce, content::BOX_OPTION_COUNT> forced;

void *find_option(const char *name, std::string &why)
{
    uintptr_t find = swofty::api ? swofty::api->resolve("Game_OptionsStore_FindOption") : 0;
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_OptionsStore") : 0;
    if (!find || !slot) {
        why = "this build does not resolve Game_OptionsStore_FindOption and g_Game_OptionsStore";
        return nullptr;
    }
    void *store = mem::readable(reinterpret_cast<void *>(slot), 8) ? *reinterpret_cast<void **>(slot) : nullptr;
    void *option = nullptr;
    if (store::find(find, store, name, option) == store::Found::store_unreadable)
        why = "the options store is not readable";
    return option;
}

std::vector<std::string> value_names(void *option)
{
    uintptr_t key_at = swofty::api ? swofty::api->resolve("k_Gdb_StringXorKey") : 0;
    std::vector<std::string> out;
    for (const auto &v : store::values(option, reinterpret_cast<const uint8_t *>(key_at)))
        out.push_back(v.name);
    return out;
}

// The stock value labels of the box rule options in the order the database lists them (options_system.options.game
// options_folder_0: option_21 rules_and_penalties_mode, _22 corner_cutting_mode, _30 safety_car_frequency, _32
// formation_lap). In game (F1 25 build 25251167) every value name holder read empty while the value count matched, so
// a list whose names all read empty and whose count matches takes these labels (the option's values are built from the
// rows in this order).
struct StockValues {
    const char *option;
    std::array<const char *, 4> values;
    size_t count;
};
constexpr StockValues STOCK_VALUES[] = {
    {"rules_and_penalties_mode", {"off", "corner_cutting_only", "eSports", "on"}, 4},
    {"corner_cutting_mode", {"regular", "strict", nullptr, nullptr}, 2},
    {"safety_car_frequency", {"off", "reduced", "standard", "increased"}, 4},
    {"formation_lap", {"off", "on", nullptr, nullptr}, 2},
};

// Fills unreadable names from STOCK_VALUES. Returns true when it did.
bool stock_names(const char *option, std::vector<std::string> &names)
{
    if (names.empty() || std::any_of(names.begin(), names.end(), [](const std::string &n) { return !n.empty(); }))
        return false;
    for (const auto &s : STOCK_VALUES)
        if (!strcmp(s.option, option) && s.count == names.size()) {
            for (size_t i = 0; i < s.count; ++i)
                names[i] = s.values[i];
            return true;
        }
    return false;
}

int get_index(const void *option)
{
    int index = -1;
    store::get_index(option, index);
    return index;
}

void set_index(const void *option, int index) { store::set_index(option, index); }

std::string joined(const std::vector<std::string> &names)
{
    std::string s;
    for (const auto &n : names)
        s += (s.empty() ? "" : ", ") + (n.empty() ? std::string("?") : n);
    return s.empty() ? "none" : s;
}

}
namespace runtime {

std::string box_option_apply(size_t rule, const std::string &setting)
{
    if (rule >= content::BOX_OPTION_COUNT)
        return "not applied: unknown rule";
    const auto &r = content::BOX_OPTION_RULES[rule];
    std::lock_guard<std::mutex> g(lock);
    std::string why, tried;
    void *option = nullptr;
    const char *name = nullptr;
    for (const char *n : r.options) {
        if (!n)
            break;
        tried += (tried.empty() ? "" : ", ") + std::string(n);
        if ((option = find_option(n, why)) != nullptr) {
            name = n;
            break;
        }
        if (!why.empty())
            return "not applied: " + why;
    }
    if (!option)
        return "not applied: the options store has no option named " + tried;
    std::vector<std::string> names = value_names(option);
    bool stock = stock_names(name, names);
    int index = content::box_option_value(names, r.rule, setting);
    if (index < 0)
        return "not applied: option " + std::string(name) + " has no value for " + setting + " (values: " + joined(names) + ")";
    if (!store::vfunc<int(const void *, void *)>(option, 2) || !store::vfunc<void(const void *, int)>(option, 3))
        return "not applied: option " + std::string(name) + " has no index get/set in its vtable";
    int before = get_index(option);
    forced[rule].force(option, index, get_index, set_index);
    int now = get_index(option);
    return "option " + std::string(name) + " index " + std::to_string(before) + " -> " + std::to_string(now) + " (" +
           names[size_t(index)] + (stock ? ", stock value order" : "") + ")" +
           (now == index ? "" : "; the option did not take the index");
}

int box_option_put_back(size_t rule)
{
    if (rule >= content::BOX_OPTION_COUNT)
        return 0;
    std::lock_guard<std::mutex> g(lock);
    if (!forced[rule].active())
        return 0;
    std::string why;
    void *option = nullptr;
    for (const char *n : content::BOX_OPTION_RULES[rule].options) {
        if (!n)
            break;
        if ((option = find_option(n, why)) != nullptr)
            break;
    }
    return forced[rule].put_back(option, get_index, set_index);
}

void box_options_forget()
{
    std::lock_guard<std::mutex> g(lock);
    for (auto &f : forced)
        f.forget();
}

}
