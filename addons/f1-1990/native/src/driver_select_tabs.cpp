// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the Choose Driver tab strip of a new 1990 Driver Career shows only the F1 tab. Its own
// DLL, so a binding it can't resolve drops only this hook.
//
// The driver_career_driver_select_header view model builds its tabs with the shared tab builder
// 0x144251f40 from the content group driver_career_driver_select_header (header_f1,
// header_selected_team_icon, header_f2, header_icon), tab i from entry i. The screen group widget
// on the screen (UISG, 0x144253930) spawns one tab header per entry of the same stock group and
// binds it to headers.<name>, so the header must keep all four tabs: a header without header_f2 and
// header_icon leaves two unbound tab headers drawn as empty boxes with their notification marker.
// Instead, the redirect of the build (site_Game_DriverSelectHeader_BuildTabs) records which tabs are
// F2 and Icons in a 1990 wizard, and the wrap of Game_DriverSelectHeader_SetTabState turns those
// tabs off (content_visible and enabled 0), the state the view model gives a tab it doesn't offer,
// such as the selected team tab. The widget binds each tab's system_data.enabled to its tooltip
// refresh (0x144252a40), which hides the F5/F6 hints while fewer than two tabs are enabled. The wrap
// also skips an index past the header's tabs. A wizard is a 1990 one when the last career tile
// pressed was the 1990 tile (F1990_CareerWizard1990, exported by f1_1990_ui.dll) and no other career
// is loaded.

#include <swofty/mixin.hpp>

#ifdef SWOFTY_API_CAREER_WIZARD
// With a core that has the career wizard library the core owns the tab hooks and knows the last
// career tile pressed; the two tabs are hidden for the 1990 data group.
#include <swofty/career_wizard.hpp>

#include "career_group.hpp"

SWOFTY_ON_LOAD
{
    namespace wz = swofty::career::wizard;
    const char *group = f1990::kCareerGroup1990;
    wz::Result a = wz::hide_driver_tab(group, "header_f2"), b = wz::hide_driver_tab(group, "header_icon");
    swofty::log("[f1-1990] driver select: F2 and Icons tabs hidden in %s wizards through the core's wizard library "
                "(%s, %s)",
                group, wz::name(a), wz::name(b));
}
#else
#include <windows.h>

#include <atomic>

#include <swofty/career.hpp>
#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include "career_1990.hpp"
#include "content_tabs.hpp"

namespace {

using f1990::at;

constexpr size_t kTabSize = 0x70;
constexpr size_t kHeaderTabs = 0x270;
constexpr const char *kHidden[] = {"header_f2", "header_icon"};

std::atomic<void *> g_header;
std::atomic<uint64_t> g_hidden;
std::atomic<uint32_t> g_builds, g_skips, g_offs;

bool wizard_1990()
{
    static int (*pressed)() = [] {
        HMODULE ui = GetModuleHandleW(L"f1_1990_ui.dll");
        auto fn = ui ? reinterpret_cast<int (*)()>(GetProcAddress(ui, "F1990_CareerWizard1990")) : nullptr;
        if (!fn)
            swofty::log("[f1-1990] driver select: f1_1990_ui.dll is not loaded; the header keeps its tabs");
        return fn;
    }();
    if (!pressed || !pressed())
        return false;
    void *career = swofty::career::entity();
    return !career || f1990::is_1990_career(career);
}

}

SWOFTY_REDIRECT(site_Game_DriverSelectHeader_BuildTabs, int64_t, void *tabs, void *alloc, void *node, const void *group)
{
    void *header = tabs ? static_cast<uint8_t *>(tabs) - kHeaderTabs : nullptr;
    int64_t mask = group && wizard_1990() ? f1990::content_tab_mask(group, kHidden) : 0;
    if (mask > 0) {
        g_hidden = uint64_t(mask);
        g_header = header;
        if (g_builds++ < 4)
            swofty::log("[f1-1990] driver select: 1990 wizard; header tabs mask 0x%llx off (F2 and Icons)",
                        (unsigned long long)mask);
    } else {
        void *expected = header;
        g_header.compare_exchange_strong(expected, nullptr);
        if (mask < 0 && g_builds++ < 4)
            swofty::log("[f1-1990] driver select: the header group does not look like a content group; its tabs stay");
    }
    return orig(tabs, alloc, node, group);
}

SWOFTY_WRAP(Game_DriverSelectHeader_SetTabState, void, void *header, uint64_t tab, uint8_t content_visible,
            uint8_t enabled)
{
    auto *b = header ? at<const uint8_t *>(header, 0x2b8) : nullptr;
    auto *e = header ? at<const uint8_t *>(header, 0x2c0) : nullptr;
    if (!b || e <= b || tab >= size_t(e - b) / kTabSize) {
        if (g_skips++ < 8)
            swofty::log("[f1-1990] driver select: tab %llu state skipped, the header has %zu tab(s)",
                        (unsigned long long)tab, b && e > b ? size_t(e - b) / kTabSize : size_t(0));
        return;
    }
    f1990::TabState state{content_visible, enabled};
    if (header == g_header.load())
        state = f1990::header_tab_state(g_hidden, tab, state);
    if ((state.content_visible != content_visible || state.enabled != enabled) && g_offs++ < 8)
        swofty::log("[f1-1990] driver select: tab %llu kept off (asked content %u, enabled %u)",
                    (unsigned long long)tab, content_visible, enabled);
    orig(header, tab, state.content_visible, state.enabled);
}
#endif

SWOFTY_ADDON()
