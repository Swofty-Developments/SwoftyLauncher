#include "core/career/hub_state.hpp"
#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <cstring>

// Driver Career Team HQ (screen team_hq_container): its tabs are built by the Team HQ container's own
// builder (0x1422cf7a0..0x1422d5e43), not the laptop's. Its primary tabs are 0xc08-byte Team HQ tabs:
// allocated through container +0x2c0, constructed by 0x1414bf8c0 (tab, container+0x28,
// container+0x268, container, "team_hq_tab_secondary_controller", "team_hq_secondary_tab", lng name,
// "", 1), given the class vtables (0x145787460, +0x260 0x145787540, +0x268 0x145787570) and the
// 0x80-byte field block from 0x14577c048 at +0xb78 (+0xbf8 and +0xc00 zero), then pushed onto
// container +0x2c8 (begin +0x2d0, end +0x2d8) by 0x1423ac4d0. A page is allocated through the tab's
// allocator +0x668 (0x660 bytes, align 0x10), constructed by the same tab view model constructor, and
// appended to the tab's page vector object +0x670 {allocator, begin +0x678, end +0x680, capacity}
// ({0, allocator, page} entries, the shape Career_Push_HubLaptopTab pushes); then bit 0x8 of page
// +0x628 is cleared and, when bit 0x4 is set, it is cleared and the page's vt+0xc8 runs. STANDINGS
// (0x1422d4d3e..) is the pattern, and the new tab is added at 0x1422d5cce, where the builder joins and
// starts registering every tab (container at [rsp+0x50]). The R&D tab block (0x1422d23e5..0x1422d2ce0,
// key 0xf4482186) has no gate in this builder: while hide_rnd is on, execution resumes at the VEHICLE
// block (0x1422d2ce1), which reloads everything it reads from the container. The other users of the
// R&D key (0x1422d6132 search, 0x1422d65f1 availability, 0x1422d6822 / 0x1422d6fce tab jumps) are the
// same null-tolerant lookups as the laptop's.

namespace career::hub {

namespace {

void add_team_hq_tab(uintptr_t hq, HubTab &t)
{
    constexpr uintptr_t HQ_ARG2 = 0x28, HQ_ALLOC = 0x2c0, HQ_ARG3 = 0x268, HQ_TABS = 0x2c8, HQ_TABS_END = 0x2d8,
                        TQ_PAGE_ALLOC = 0x668, TQ_PAGES = 0x670, TQ_FIELDS = 0xb78, TQ_FIELDS_END = 0xbf8, TQ_FLAG = 0xc00,
                        PAGE_FLAGS = 0x628, PAGE_VT_REFRESH = 0xc8;
    constexpr size_t TQ_SIZE = 0xc08;
    void *allocator = alloc_through(hq + HQ_ALLOC);
    if (!allocator) {
        note("[hub] %s: the Team HQ container has no allocator; tab not added", t.name.c_str());
        return;
    }
    auto *tab = static_cast<uint8_t *>(allocate(allocator, TQ_SIZE, TAB_ALIGN));
    if (!tab)
        return;
    void *a2 = at<void *>(hq + HQ_ARG2), *a3 = at<void *>(hq + HQ_ARG3);
    reinterpret_cast<TabCtor_t *>(a_tq_ctor)(tab, a2, a3, reinterpret_cast<void *>(hq), "team_hq_tab_secondary_controller",
                                             "team_hq_secondary_tab", t.lng.c_str(), "", 1);
    *reinterpret_cast<uintptr_t *>(tab) = a_tq_vt0;
    *reinterpret_cast<uintptr_t *>(tab + 0x260) = a_tq_vt260;
    *reinterpret_cast<uintptr_t *>(tab + 0x268) = a_tq_vt268;
    std::memcpy(tab + TQ_FIELDS, reinterpret_cast<const void *>(a_tq_init - 0x10), TQ_FIELDS_END - TQ_FIELDS);
    *reinterpret_cast<uint64_t *>(tab + TQ_FIELDS_END) = 0;
    tab[TQ_FLAG] = 0;
    struct {
        void *allocator, *obj;
    } owned{allocator, tab};
    uint32_t key = t.key;
    reinterpret_cast<Push_t *>(a_tq_push)(reinterpret_cast<void *>(hq + HQ_TABS), &key, &owned);
    uintptr_t end = at<uintptr_t>(hq + HQ_TABS_END);
    if (!end || at<void *>(end - 8) != tab) {
        note("[hub] %s: the tab is not Team HQ's last tab after the push; no page added", t.name.c_str());
        return;
    }
    void *page_alloc = alloc_through(reinterpret_cast<uintptr_t>(tab) + TQ_PAGE_ALLOC);
    auto *page = static_cast<uint8_t *>(page_alloc ? allocate(page_alloc, PAGE_SIZE, TAB_ALIGN) : nullptr);
    if (!page) {
        note("[hub] %s: no page allocated", t.name.c_str());
        return;
    }
    reinterpret_cast<PageCtor_t *>(a_page_ctor)(page, a2, a3, reinterpret_cast<void *>(hq), t.content.c_str(), t.lng.c_str(),
                                                nullptr, "", PAGE_TYPE, 0);
    bool mine = own_vtable(page);
    struct {
        void *allocator, *obj;
    } page_owned{page_alloc, page};
    uint32_t zero_key = 0;
    reinterpret_cast<Push_t *>(a_push)(tab + TQ_PAGES, &zero_key, &page_owned);
    uint8_t flags = page[PAGE_FLAGS];
    page[PAGE_FLAGS] = flags & ~0x8;
    if (flags & 0x4) {
        page[PAGE_FLAGS] &= ~0x4;
        using Refresh_t = void(void *);
        reinterpret_cast<Refresh_t *>(at<uintptr_t>(at<uintptr_t>(reinterpret_cast<uintptr_t>(page)) + PAGE_VT_REFRESH))(page);
    }
    publish(t);
    swofty::log("[hub] %s: Team HQ tab '%s' added after STANDINGS (key 0x%08x, page %s%s, data %s)", t.name.c_str(),
                t.title.c_str(), t.key, t.content.c_str(), mine ? "" : ", its commands stay the game's", t.data.c_str());
}

}

void team_hq_end(safetyhook::Context &c)
{
    uintptr_t hq = at<uintptr_t>(c.rsp + 0x50);
    if (!hq || hq != c.rsi || !swofty::mem::object_like(hq)) {
        note("[hub] Team HQ: the container is not where the builder keeps it; no tab");
        return;
    }
    char group[64] = {};
    if (runtime::career_content_table()->data_group(nullptr, group, sizeof group) != SWOFTY_ASSET_OK)
        return;
    for (auto &t : tabs)
        if (t.group == group)
            add_team_hq_tab(hq, t);
}

}
