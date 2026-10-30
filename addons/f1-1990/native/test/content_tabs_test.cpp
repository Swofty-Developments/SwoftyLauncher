// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <string>
#include <vector>

#include "content_tabs.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

template <class T> static void put(void *p, size_t off, T v)
{
    std::memcpy(static_cast<uint8_t *>(p) + off, &v, sizeof v);
}

static bool same(f1990::TabState a, f1990::TabState b)
{
    return a.content_visible == b.content_visible && a.enabled == b.enabled;
}

int main()
{
    const char *names[] = {"header_f1", "header_selected_team_icon", "header_f2", "header_icon"};
    std::vector<uint8_t> strings[4], entries(4 * f1990::kContentTabSize), group(0x20);
    for (int i = 0; i < 4; i++) {
        strings[i].assign(9, 0);
        strings[i].insert(strings[i].end(), names[i], names[i] + std::strlen(names[i]) + 1);
        put(entries.data() + i * f1990::kContentTabSize, 0x08, strings[i].data());
    }
    put(group.data(), 0x08, entries.data());
    put(group.data(), 0x10, entries.data() + entries.size());

    const char *const hidden[] = {"header_f2", "header_icon"};
    int64_t mask = f1990::content_tab_mask(group.data(), hidden);
    check("marks the F2 and Icons tabs, entries 2 and 3", mask == 0b1100);

    const char *const none[] = {"header_mp"};
    check("marks nothing that isn't there", f1990::content_tab_mask(group.data(), none) == 0);

    put(entries.data() + f1990::kContentTabSize, 0x08, static_cast<const uint8_t *>(nullptr));
    check("skips an entry without a name", f1990::content_tab_mask(group.data(), hidden) == 0b1100);

    put(group.data(), 0x10, entries.data() + entries.size() - 1);
    check("rejects a group that is not whole entries", f1990::content_tab_mask(group.data(), hidden) == -1);
    put(group.data(), 0x10, entries.data());
    check("takes an empty group", f1990::content_tab_mask(group.data(), hidden) == 0);

    const f1990::TabState on{1, 1}, off{0, 0}, team{1, 0};
    check("turns the F2 tab off when the view model turns it on", same(f1990::header_tab_state(mask, 2, on), off));
    check("turns the Icons tab off when the view model turns it on", same(f1990::header_tab_state(mask, 3, on), off));
    check("keeps the F1 tab as asked", same(f1990::header_tab_state(mask, 0, on), on));
    check("keeps the selected team tab as asked", same(f1990::header_tab_state(mask, 1, team), team) &&
                                                       same(f1990::header_tab_state(mask, 1, off), off));
    check("keeps every tab as asked without a mask", same(f1990::header_tab_state(0, 2, on), on) &&
                                                         same(f1990::header_tab_state(0, 3, on), on));
    check("keeps a tab past the mask as asked", same(f1990::header_tab_state(~uint64_t(0), 64, on), on));

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
