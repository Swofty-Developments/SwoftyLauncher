// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <vector>

#include "career_group.hpp"
#include "content_entry.hpp"

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

static std::vector<uint8_t> entries(std::initializer_list<std::pair<uint32_t, uint8_t>> rows)
{
    std::vector<uint8_t> v(rows.size() * f1990::kContentEntrySize);
    size_t i = 0;
    for (auto [id, dlc] : rows) {
        put(v.data() + i * f1990::kContentEntrySize, 4, id);
        v[i * f1990::kContentEntrySize + 0x58] = dlc;
        i++;
    }
    return v;
}

static std::map<void *, void *> g_components;
static std::map<void *, uint8_t> g_index;

static void *component(void *champ)
{
    auto it = g_components.find(champ);
    return it == g_components.end() ? nullptr : it->second;
}

static uint8_t index_of(void *comp)
{
    return g_index[comp];
}

int main()
{
    auto stock = entries({{0x00000000, 0}, {0x00010000, 1}});
    auto pick = f1990::pick_content(stock.data(), stock.data() + stock.size(), 2);
    check("no 1990 car group falls back to the 2026 (dlc) group", pick.kind == f1990::ContentPick::dlc && pick.id == 0x00010000);
    auto v2 = entries({{0x00000000, 0}, {0x00010000, 1}, {0x00020000, 1}});
    pick = f1990::pick_content(v2.data(), v2.data() + v2.size(), 2);
    check("the 1990 car group is picked by stat id", pick.kind == f1990::ContentPick::exact && pick.id == 0x00020000);
    auto none = entries({{0x00000000, 0}});
    pick = f1990::pick_content(none.data(), none.data() + none.size(), 2);
    check("no dlc group and no 1990 group is no content", pick.kind == f1990::ContentPick::none && pick.id == f1990::kNoContent);
    pick = f1990::pick_content(nullptr, nullptr, 2);
    check("empty list is no content", pick.kind == f1990::ContentPick::none);

    const char *names[] = {"f1_career_2025", "f1_career_2026", "f1_career_1990"};
    std::vector<uint8_t> named[3], group[3];
    std::vector<uint8_t> list(3 * 0x10), src(0x600), comp(0x300), champ(0x300), career(0x1100), champs(0x10);
    for (int i = 0; i < 3; i++) {
        named[i].resize(0x20);
        put(named[i].data(), 0x10, names[i]);
        group[i].resize(0x30);
        put(group[i].data(), 0x28, named[i].data());
        put(list.data() + i * 0x10, 8, group[i].data());
    }
    put(src.data(), 0x560, list.data());
    put(src.data(), 0x568, list.data() + list.size());
    put(comp.data(), 0x290, src.data());
    put(champs.data(), 8, champ.data());
    put(career.data(), 0x1040, champs.data());
    put(career.data(), 0x1048, champs.data() + champs.size());
    g_components[champ.data()] = comp.data();
    f1990::CareerGroupAccess access{component, index_of};

    g_index[comp.data()] = 2;
    check("a career on data group 2 is the 1990 career", f1990::career_data_group_is(career.data(), "f1_career_1990", access));
    g_index[comp.data()] = 1;
    check("a 2026 career is not", !f1990::career_data_group_is(career.data(), "f1_career_1990", access));
    f1990::CareerGroupSeen seen = f1990::career_data_group_seen(career.data(), access);
    check("the walk reports index, count and name", seen.index == 1 && seen.count == 3 && seen.name &&
                                                       !std::strcmp(seen.name, "f1_career_2026"));
    g_index[comp.data()] = 7;
    check("an index past the group list is not (stock list without the 1990 row)",
          !f1990::career_data_group_is(career.data(), "f1_career_1990", access));
    seen = f1990::career_data_group_seen(career.data(), access);
    check("the walk reports an index past the list without a name", seen.index == 7 && seen.count == 3 && !seen.name);
    g_components.clear();
    check("a championship without a setup component is not", !f1990::career_data_group_is(career.data(), "f1_career_1990", access));
    check("no career is not", !f1990::career_data_group_is(nullptr, "f1_career_1990", access));

    if (failures)
        std::printf("%d failure(s)\n", failures);
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
