#include "core/career/hub_state.hpp"

#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>

// The R&D tab of the Team HQ hub tabs (hub_tab.cpp builds the tabs and sets the hooks up).
//
// The R&D tab block is skipped by the builder when the laptop's r_and_d_visible property (laptop
// +0x920, read through [rsp+0x168]) does not hold a true bool: `cmp byte [rax], 0; je 0x141c41cf2`
// at 0x141c41bac. The laptop's other R&D lookups by the tab key 0xb88a669 (0x141c43aa5 availability,
// 0x141c44d79 / 0x141c44dd9 / 0x141c44df0 the tab jump) already handle the tab being absent, which is
// the stock state when r_and_d_visible is false.
//
// hide_rnd hides the R&D tab through the tab model's own visibility flag, so it follows live. The builders
// register every tab with a tab model (laptop +0x2d0, Team HQ container +0x2e8) through 0x14431d720, which
// gives each tab a VM entry whose header_states byte holds bit 0x1 (disabled) and bit 0x2 (hidden).
// Vm_Set_TabEnabled (0x14431f470, body .data1 0x153c60e40; despite the KB name it writes the hidden bit)
// rewrites bit 0x2 of one tab's entry; the UI tab controller watches header_states (Vm_UiTabs_OnMessage
// 0x144264d50 subscribes, 0x144267740 applies: hidden -> header invisible, disabled -> header disabled)
// and, when a header's visibility changes, recounts the visible headers and lays the bar out again
// (0x144266490). Tab selection goes by the entry node (current_tab), not by index, and refuses a hidden or
// disabled tab (0x14431f6b0), so hiding the R&D tab, which sits before the hub tab, keeps the selected tab
// and its content, and the stock R&D jumps (0x141871390 -> 0x14431f6b0) refuse a hidden tab. Stock Team HQ
// hides and shows all its tabs the same way (0x1422d7bb0 / 0x1422d7a20, onboarding), and the laptop hides
// single tabs (0x141c45340). So each builder now builds the R&D tab as stock does and, right after the
// registration loop (site_Game_HubLaptopTabsRegistered 0x141c42689, site_Game_TeamHqTabsRegistered
// 0x1422d5d3b), hides it while hide_rnd is on; the container and its R&D tab are recorded there, and a
// later hide_rnd change is applied on the next flow tick to the recorded container, when its vtable and its
// R&D tab entry are still the ones recorded. Without the visibility signatures the builders fall back to
// leaving the R&D tab out (laptop: rax points at a zero byte at the gate; Team HQ: resume at the VEHICLE
// block), which only changes at the next Team HQ entry.
//
// The game also shows the tabs again on its own: Team HQ's command handler (0x1422d6d40) answers one
// message (0xcc3dd6bb) by showing every tab and raising enable_navigation (0x1422d7a20) and another
// (0x3cc065a8) by hiding every tab (0x1422d7bb0), most likely what undid the hide on the Overview tab. So a
// hook at the show-all tail (0x1422d7b2b, before enable_navigation, rsi = the container) hides the R&D tab
// again while hide_rnd is on, and the flow tick re-hides it about twice a second while Team HQ stays open,
// which covers any other path that rewrites header_states. A hide_rnd(false) is never undone: both only act
// while it is on.

namespace career::hub {

std::atomic<bool> hide{false}, pending{false}, laptop_flag{false}, team_hq_flag{false};
uintptr_t a_set_visible, a_tq_vehicle;

namespace {

using SetVisible_t = void(void *model, void *tab, uint8_t visible);

struct Bar {
    const char *name;
    uintptr_t begin, end, model;
    uint32_t rnd_key;
    uintptr_t owner = 0, owner_vt = 0, rnd = 0, rnd_vt = 0;
    bool told = false;
};

struct Target {
    uintptr_t model = 0, tab = 0;
};

const uint8_t zero_byte = 0;
SRWLOCK bars_lock = SRWLOCK_INIT;
Bar laptop_bar{"laptop", 0x2b8, 0x2c0, 0x2d0, 0xb88a669}, team_hq_bar{"Team HQ", 0x2d0, 0x2d8, 0x2e8, 0xf4482186};

uintptr_t rnd_tab(const Bar &b, uintptr_t owner)
{
    uintptr_t p = at<uintptr_t>(owner + b.begin), e = at<uintptr_t>(owner + b.end);
    if (!p || e < p || (e - p) % 0x18 || (e - p) / 0x18 > 64)
        return 0;
    for (; p != e; p += 0x18)
        if (at<uint32_t>(p) == b.rnd_key)
            return at<uintptr_t>(p + 0x10);
    return 0;
}

void set_visible(const Target &t, bool visible)
{
    reinterpret_cast<SetVisible_t *>(a_set_visible)(reinterpret_cast<void *>(t.model), reinterpret_cast<void *>(t.tab),
                                                    visible ? 1 : 0);
}

Target target(const Bar &b) { return {b.owner + b.model, b.rnd + 0x268}; }

bool still_open(const Bar &b)
{
    return b.owner && swofty::mem::object_like(b.owner) && at<uintptr_t>(b.owner) == b.owner_vt && rnd_tab(b, b.owner) == b.rnd &&
           swofty::mem::object_like(b.rnd) && at<uintptr_t>(b.rnd) == b.rnd_vt;
}

void registered(Bar &b, uintptr_t owner)
{
    if (!owner || !swofty::mem::object_like(owner))
        return;
    uintptr_t rnd = rnd_tab(b, owner);
    Target t;
    bool hidden = false;
    AcquireSRWLockExclusive(&bars_lock);
    b.owner = 0;
    b.told = false;
    if (rnd && swofty::mem::object_like(rnd)) {
        b.owner = owner;
        b.owner_vt = at<uintptr_t>(owner);
        b.rnd = rnd;
        b.rnd_vt = at<uintptr_t>(rnd);
        hidden = hide.load();
        t = target(b);
    }
    ReleaseSRWLockExclusive(&bars_lock);
    if (hidden) {
        set_visible(t, false);
        swofty::log("[hub] %s: the R&D tab is built hidden (hide_rnd); it follows live from here", b.name);
    }
}

void reassert(Bar &b, uintptr_t owner, const char *who)
{
    if (!hide.load())
        return;
    Target t;
    bool open = false, first = false;
    AcquireSRWLockExclusive(&bars_lock);
    if (b.owner && b.owner == owner && still_open(b)) {
        open = true;
        first = !b.told;
        b.told = true;
        t = target(b);
    }
    ReleaseSRWLockExclusive(&bars_lock);
    if (!open)
        return;
    set_visible(t, false);
    if (first)
        swofty::log("[hub] %s: the game showed the R&D tab again (%s); hidden again", b.name, who);
}

void keep(Bar &b, bool change, bool on)
{
    Target t;
    bool open = false, closed = false;
    AcquireSRWLockExclusive(&bars_lock);
    if (b.owner) {
        open = still_open(b);
        closed = !open;
        if (open)
            t = target(b);
        else
            b.owner = 0;
    }
    ReleaseSRWLockExclusive(&bars_lock);
    if (open) {
        set_visible(t, !on);
        if (change)
            swofty::log("[hub] %s: R&D tab %s live", b.name, on ? "hidden" : "shown");
    } else if (closed && change)
        swofty::log("[hub] %s: no longer open; its R&D tab follows at the next build", b.name);
}

}

void team_hq_rnd(safetyhook::Context &c)
{
    if (!hide.load() || team_hq_flag.load())
        return;
    c.rip = a_tq_vehicle;
    note("[hub] Team HQ: the R&D tab is left out (hide_rnd)");
}

void team_hq_registered(safetyhook::Context &c) { registered(team_hq_bar, at<uintptr_t>(c.rsp + 0x50)); }

void laptop_registered(safetyhook::Context &c) { registered(laptop_bar, c.rsi); }

void team_hq_show_all(safetyhook::Context &c) { reassert(team_hq_bar, c.rsi, "Team HQ showed all its tabs, enable_navigation"); }

void rnd_gate(safetyhook::Context &c)
{
    if (!hide.load() || laptop_flag.load())
        return;
    c.rax = reinterpret_cast<uintptr_t>(&zero_byte);
    note("[hub] laptop: the R&D tab is left out (hide_rnd)");
}

int api_hide_rnd(int on)
{
    if (hide.exchange(on != 0) != (on != 0)) {
        pending = true;
        swofty::log("[hub] R&D tab %s (hide_rnd %s)%s", on ? "hidden" : "shown", on ? "on" : "off",
                    team_hq_flag || laptop_flag ? "; an open Team HQ follows on the next frame" : " from the next Team HQ entry");
    }
    return SWOFTY_ASSET_OK;
}

}

namespace runtime {

void hub_tick()
{
    static uint32_t ticks;
    bool change = career::hub::pending.load() && career::hub::pending.exchange(false), on = career::hub::hide.load();
    if (!change && !(on && ++ticks % 5 == 0))
        return;
    if (career::hub::team_hq_flag.load())
        career::hub::keep(career::hub::team_hq_bar, change, on);
    if (career::hub::laptop_flag.load())
        career::hub::keep(career::hub::laptop_bar, change, on);
}

}
