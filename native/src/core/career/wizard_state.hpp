#pragma once

#include <windows.h>

#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>
#include <vector>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

// What the career wizard's entries, API and core mixin sets (career_wizard.cpp) and its game hooks (wizard_hooks.cpp)
// share: the entry kinds, the wizard and header layout, the wrapped functions' originals, the entries and their
// effective list, the last tile's group, the hidden driver tabs, and the capped note line.
namespace career::wizard {

namespace mem = swofty::mem;
constexpr uintptr_t NESTR_CHARS = 0x09;
constexpr uintptr_t WIZARD_GROUP = 0x128, STEPS = 0xe0, OPTIONS = 0x100, VM_WIZARD = 0x270, HEADER_BUILD = 0x270,
                    HEADER_TABS = 0x2b8;
constexpr size_t TAB_ITEM = 0x70, CONTENT_TAB = 0x30, MAX_TABS = 64;

using StartsInF1 = bool(void *entrants, void *wizard_data, const char *group);
using PushStep = void(void *wizard);
using BuildTabs = int64_t(void *tabs, void *alloc, void *node, const void *group);
using SetTabState = void(void *header, uint64_t tab, uint8_t content_visible, uint8_t enabled);

enum Kind { START, TILE_ROUTE, STEP, OPTION, CONFIRM_ROUTE, TAB };

struct Entry {
    Kind kind;
    std::string group, from, to;
    uint32_t a, b;
    HMODULE owner;
    std::string addon;
};

extern StartsInF1 *orig_starts;
extern PushStep *orig_push;
extern BuildTabs *orig_tabs;
extern SetTabState *orig_tab_state;

extern std::mutex lock;
extern std::vector<Entry> entries, effective;
extern std::string last_tile;
extern std::atomic<uintptr_t> hidden_header;
extern std::atomic<uint64_t> hidden_tabs;
extern std::atomic<bool> any;

// Rebuilds the effective list from the registry's wizard records, in load order; the caller holds lock.
void rebuild_effective();
// A copy of s that lives as long as the core, for a route name the game keeps a pointer to.
const char *intern(const std::string &s);
// Logs fmt with up to three strings while note line `which` has logged fewer than 6 times.
void note(int which, const char *fmt, const char *a = "", const char *b = "", const char *c = "");
std::string read_cstr(uintptr_t p);
std::string nestr(uintptr_t p);
// The data group the wizard state carries at +0x128.
std::string wizard_group(const void *wizard);

// Whether an effective entry of `kind` for `group` matches, newest first.
template <class F> bool find(Kind kind, const std::string &group, F &&match)
{
    if (!any.load() || group.empty())
        return false;
    std::lock_guard<std::mutex> g(lock);
    rebuild_effective();
    for (auto it = effective.rbegin(); it != effective.rend(); ++it)
        if (it->kind == kind && it->group == group && match(*it))
            return true;
    return false;
}

// The wraps and the site the wizard's mixin sets install (wizard_hooks.cpp).
bool starts_in_f1(void *entrants, void *wizard_data, const char *group);
void push_step(void *wizard);
void confirm_route(SwoftyContext *c);
int64_t build_tabs(void *tabs, void *alloc, void *node, const void *group);
void tab_state(void *header, uint64_t tab, uint8_t content_visible, uint8_t enabled);

}
