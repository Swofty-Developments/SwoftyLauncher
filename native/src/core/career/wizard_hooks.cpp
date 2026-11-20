#include "core/career/career_ui.hpp"
#include "core/career/wizard_state.hpp"

#include <cstdio>

// The career wizard's wraps and sites, each in a core mixin set career_wizard.cpp offers:
// - Game_CareerWizard_StartsInF1(entrants, wizard data, data group): answers yes for a registered
//   group, so the wizard has no Skip to F1 / Start in F2 step and no team-icons step and the driver
//   select takes the F1 path.
// - Game_CareerWizard_PushNextStep(wizard): runs right after the add-option body pushed the
//   player's option byte on wizard +0x100..+0x108 and pushes the next step byte on +0xe0..+0xe8.
//   Before it runs, a forced option rewrites that last option byte; after it, a replaced step
//   rewrites the step it pushed.
// - site_Game_CareerWizardConfirm_NextRoute: rdx is the route the confirm step picked, rsi the view
//   model (+0x270 the wizard state); a registered route swap changes rdx.
// - site_Game_DriverSelectHeader_BuildTabs: the call to the shared tab builder for the Choose
//   Driver header (its first argument is header +0x270) records which entries of the content group
//   (+0x08..+0x10, 0x30 bytes each, tab name string +0x08 with its text at +0x09) are hidden tabs,
//   for the last tile's group while no career of another group is loaded. The header keeps every
//   tab: the screen spawns one tab box per entry of the stock group, so a header built without a tab
//   leaves an empty box with its notification marker. Game_DriverSelectHeader_SetTabState keeps a
//   hidden tab at content_visible 0 and enabled 0, the state the game gives a tab it doesn't offer,
//   and skips an index past the header's tabs (vector at header +0x2b8, 0x70 each), which it has no
//   bounds check for.

namespace career::wizard {

namespace {

namespace cu = career::ui;

bool last_byte(const void *wizard, uintptr_t vec, uintptr_t &at, size_t &count)
{
    uintptr_t b = 0, e = 0;
    if (!mem::read(reinterpret_cast<uintptr_t>(wizard) + vec, b) ||
        !mem::read(reinterpret_cast<uintptr_t>(wizard) + vec + 8, e) || !b || e <= b || e - b > 256)
        return count = 0, false;
    count = e - b;
    at = e - 1;
    return true;
}

std::string tab_name(const uint8_t *entry)
{
    uintptr_t s = 0;
    return mem::read(reinterpret_cast<uintptr_t>(entry) + 0x08, s) && s ? read_cstr(s + 9) : std::string();
}

// Bit i set when entry i of the header's content group is a tab hidden for the last tile's group.
uint64_t hidden_mask(const void *group)
{
    std::string g;
    {
        std::lock_guard<std::mutex> lk(lock);
        g = last_tile;
    }
    if (!any.load() || !group || g.empty() || !find(TAB, g, [](const Entry &) { return true; }))
        return 0;
    std::string loaded = cu::loaded_group();
    if (!loaded.empty() && loaded != g)
        return 0;
    struct Group {
        uint8_t head[0x08];
        const uint8_t *begin, *end;
    } copy;
    if (!mem::read(group, copy) || !copy.begin || copy.end < copy.begin || size_t(copy.end - copy.begin) % CONTENT_TAB ||
        size_t(copy.end - copy.begin) / CONTENT_TAB > MAX_TABS) {
        note(5, "[wizard] %s: the Choose Driver header group does not look like a content group; its tabs stay", g.c_str());
        return 0;
    }
    std::lock_guard<std::mutex> lk(lock);
    uint64_t mask = 0;
    size_t i = 0, dropped = 0, total = size_t(copy.end - copy.begin) / CONTENT_TAB;
    for (const uint8_t *p = copy.begin; p < copy.end; p += CONTENT_TAB, i++) {
        std::string name = tab_name(p);
        bool hide = false;
        for (auto &e : effective)
            hide |= e.kind == TAB && e.group == g && e.from == name;
        if (hide)
            mask |= uint64_t(1) << i, dropped++;
    }
    if (mask) {
        char a[16], b[16];
        snprintf(a, sizeof a, "%zu", dropped), snprintf(b, sizeof b, "%zu", total);
        note(6, "[wizard] %s: Choose Driver header tabs: %s of %s off", g.c_str(), a, b);
    }
    return mask;
}

}

bool starts_in_f1(void *entrants, void *wizard_data, const char *group)
{
    bool stock = orig_starts(entrants, wizard_data, group);
    std::string g = read_cstr(reinterpret_cast<uintptr_t>(group));
    if (!stock && find(START, g, [](const Entry &) { return true; })) {
        note(0, "[wizard] %s starts in F1 (no F2 choice, no team-icons step)", g.c_str());
        return true;
    }
    return stock;
}

void push_step(void *wizard)
{
    if (!any.load() || !wizard)
        return orig_push(wizard);
    std::string g = wizard_group(wizard);
    uintptr_t at = 0;
    size_t before = 0;
    uint32_t with = 0;
    if (last_byte(wizard, OPTIONS, at, before)) {
        uint8_t option = *reinterpret_cast<uint8_t *>(at);
        if (find(OPTION, g, [&](const Entry &e) { return e.a == option && (with = e.b, true); }) && with != option) {
            *reinterpret_cast<uint8_t *>(at) = uint8_t(with);
            char a[8], b[8];
            snprintf(a, sizeof a, "%u", option), snprintf(b, sizeof b, "%u", with);
            note(2, "[wizard] %s: option %s recorded as %s", g.c_str(), a, b);
        }
    }
    last_byte(wizard, STEPS, at, before);
    orig_push(wizard);
    size_t after = 0;
    if (!last_byte(wizard, STEPS, at, after) || after != before + 1)
        return;
    uint8_t step = *reinterpret_cast<uint8_t *>(at);
    if (find(STEP, g, [&](const Entry &e) { return e.a == step && (with = e.b, true); }) && with != step) {
        *reinterpret_cast<uint8_t *>(at) = uint8_t(with);
        char a[8], b[8];
        snprintf(a, sizeof a, "%u", step), snprintf(b, sizeof b, "%u", with);
        note(3, "[wizard] %s: step %s replaced by step %s", g.c_str(), a, b);
    }
}

void confirm_route(SwoftyContext *c)
{
    if (!any.load() || !c->rdx || !c->rsi)
        return;
    uintptr_t wizard = 0;
    if (!mem::read(c->rsi + VM_WIZARD, wizard) || !wizard)
        return;
    std::string g = wizard_group(reinterpret_cast<void *>(wizard)), route = read_cstr(c->rdx);
    const char *to = nullptr;
    if (find(CONFIRM_ROUTE, g, [&](const Entry &e) {
            if (!e.from.empty() && e.from != route)
                return false;
            to = intern(e.to);
            return true;
        })) {
        c->rdx = reinterpret_cast<uintptr_t>(to);
        note(4, "[wizard] %s: confirm opens %s instead of %s", g.c_str(), to, route.c_str());
    }
}

int64_t build_tabs(void *tabs, void *alloc, void *node, const void *group)
{
    uintptr_t header = tabs ? reinterpret_cast<uintptr_t>(tabs) - HEADER_BUILD : 0;
    if (uint64_t mask = hidden_mask(group)) {
        hidden_tabs = mask;
        hidden_header = header;
    } else {
        uintptr_t expected = header;
        hidden_header.compare_exchange_strong(expected, 0);
    }
    return orig_tabs(tabs, alloc, node, group);
}

void tab_state(void *header, uint64_t tab, uint8_t content_visible, uint8_t enabled)
{
    uintptr_t b = 0, e = 0;
    if (header && mem::read(reinterpret_cast<uintptr_t>(header) + HEADER_TABS, b) &&
        mem::read(reinterpret_cast<uintptr_t>(header) + HEADER_TABS + 8, e) && b && e >= b && tab >= size_t(e - b) / TAB_ITEM) {
        char a[24];
        snprintf(a, sizeof a, "%llu", (unsigned long long)tab);
        note(7, "[wizard] Choose Driver tab %s state skipped, past the header's tabs", a);
        return;
    }
    if (header && reinterpret_cast<uintptr_t>(header) == hidden_header.load() && tab < MAX_TABS &&
        (hidden_tabs.load() >> tab & 1))
        content_visible = enabled = 0;
    orig_tab_state(header, tab, content_visible, enabled);
}

}
