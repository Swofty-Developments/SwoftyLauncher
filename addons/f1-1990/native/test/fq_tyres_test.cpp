// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <vector>

#include "fq_tyres.hpp"
#include "weekend.hpp"

using namespace f1990::tyres;

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

struct Set {
    uint32_t tag = 0;
    uint8_t flags = 0;
    float wear = 0.f;
};

struct Alloc {
    std::vector<uint8_t> mem = std::vector<uint8_t>(0x100);
    std::vector<std::vector<uint8_t>> sets;

    explicit Alloc(const std::vector<std::vector<Set>> &compounds)
    {
        uint64_t n = compounds.size();
        std::memcpy(mem.data() + 0xd8, &n, sizeof n);
        sets.resize(compounds.size());
        for (size_t i = 0; i < compounds.size(); i++) {
            auto &v = sets[i];
            v.resize(compounds[i].size() * kSetSize);
            for (size_t k = 0; k < compounds[i].size(); k++) {
                uint8_t *s = v.data() + k * kSetSize;
                std::memcpy(s, &compounds[i][k].tag, 4);
                s[9] = compounds[i][k].flags;
                std::memcpy(s + 0x18, &compounds[i][k].wear, 4);
            }
            uint8_t *b = v.data(), *e = v.data() + v.size();
            std::memcpy(mem.data() + 0x10 + i * 0x28 + 0x10, &b, sizeof b);
            std::memcpy(mem.data() + 0x10 + i * 0x28 + 0x18, &e, sizeof e);
        }
    }

    uint32_t tag(size_t c, size_t k) const
    {
        uint32_t t;
        std::memcpy(&t, sets[c].data() + k * kSetSize, 4);
        return t;
    }
    uint8_t flags(size_t c, size_t k) const { return sets[c][k * kSetSize + 9]; }
};

int main()
{
    check("shootout sessions are 10..14",
          is_shootout_session(10) && is_shootout_session(13) && is_shootout_session(14) && !is_shootout_session(9) &&
              !is_shootout_session(15) && !is_shootout_session(8) && !is_shootout_session(0));

    Alloc a({{{10, kHandedBack}, {5, 0}, {1, kHandedBack | kUsed}, {11, kHandedBack, 0.2f}, {0, 0}},
             {{12, kHandedBack}},
             {},
             {{0, kHandedBack}},
             {{0, kHandedBack}}});
    int n = free_unused_dry_sets(a.mem.data());
    check("three unused dry sets changed", n == 3);
    check("handed-back shootout set freed", a.tag(0, 0) == 0 && !(a.flags(0, 0) & kHandedBack));
    check("tagged qualifying set untagged", a.tag(0, 1) == 0);
    check("used set left alone", a.tag(0, 2) == 1 && (a.flags(0, 2) & kHandedBack));
    check("worn set left alone", a.tag(0, 3) == 11 && (a.flags(0, 3) & kHandedBack));
    check("medium set freed", a.tag(1, 0) == 0 && !(a.flags(1, 0) & kHandedBack));
    check("inters and wets untouched", (a.flags(3, 0) & kHandedBack) && (a.flags(4, 0) & kHandedBack));
    check("allocation marked dirty", a.mem[0xe8] == 1);

    Alloc b({{{0, 0}, {3, kUsed}}});
    check("nothing to change keeps the allocation clean", free_unused_dry_sets(b.mem.data()) == 0 && b.mem[0xe8] == 0);

    Alloc c({{{0, kUsedOutsideGarage}}});
    check("set used outside the garage left alone", free_unused_dry_sets(c.mem.data()) == 0);

    Alloc donor({{{1, 0}, {2, kUsed, 0.4f}, {15, kHandedBack}}, {{10, 0}}, {}, {{0, 0}}, {{0, kUsedOutsideGarage}}});
    Census dc = census(donor.mem.data());
    check("census counts sets per slot", dc.total == 6 && dc.sets[0] == 3 && dc.sets[1] == 1 && dc.sets[2] == 0 &&
                                             dc.sets[3] == 1 && dc.sets[4] == 1);
    check("census counts fresh and tagged sets", dc.fresh == 3 && dc.tagged == 2 && dc.fresh_sets[0] == 1);

    std::vector<std::vector<Set>> ten(5);
    for (uint32_t k = 0; k < 10; k++)
        ten[1].push_back({k < 5 ? 13u : 14u, 0});
    ten[0] = {{0, kUsed}, {0, 0}, {0, kHandedBack}};
    Alloc capped(ten);
    check("ten usable ultrasoft sets: the tenth is handed back", cap_usable_sets(capped.mem.data()) == 1 &&
                                                                     (capped.flags(1, 9) & kHandedBack) &&
                                                                     !(capped.flags(1, 8) & kHandedBack));
    check("capping marks the allocation dirty", capped.mem[0xe8] == 1);
    check("used and handed-back sets don't count", !(capped.flags(0, 1) & kHandedBack));
    check("nine usable sets stay as they are", cap_usable_sets(capped.mem.data()) == 0);

    Alloc race({{{}}, {{}, {0, kUsed}, {0, kHandedBack}, {}}, {{}}});
    check("race sessions are 15 to 17", is_race_session(15) && is_race_session(17) && !is_race_session(14) &&
                                            !is_race_session(18) && !is_race_session(13));
    check("race: the two usable qualifiers are handed back",
          hand_back_usable_sets(race.mem.data(), kQualifierSlot) == 2 && (race.flags(1, 0) & kHandedBack) &&
              (race.flags(1, 3) & kHandedBack) && race.flags(1, 1) == kUsed);
    check("race: other compounds keep their sets", !(race.flags(0, 0) & kHandedBack) && !(race.flags(2, 0) & kHandedBack));
    check("race: handing back marks the allocation dirty", race.mem[0xe8] == 1);
    check("race: a missing slot changes nothing", hand_back_usable_sets(race.mem.data(), 7) == 0);

    Alloc empty({{}, {}, {}, {}, {}});
    check("empty allocation has no sets", census(empty.mem.data()).total == 0);
    static std::vector<void *> blocks;
    static int frees;
    Heap heap{[](void *, size_t n) -> void * { blocks.push_back(std::calloc(1, n)); return blocks.back(); },
              [](void *, void *) { frees++; }};
    int marker = 0;
    for (size_t i = 0; i < 5; i++) {
        void *allocator = &marker;
        std::memcpy(empty.mem.data() + 0x10 + i * 0x28 + 0x08, &allocator, sizeof allocator);
    }
    int copied = copy_sets(empty.mem.data(), donor.mem.data(), heap);
    Census ec = census(empty.mem.data());
    check("refill copies every donor set", copied == 6 && ec.total == 6 && ec.sets[0] == 3 && ec.sets[4] == 1);
    check("refill allocates one buffer per filled slot and frees none", blocks.size() == 4 && frees == 0);
    const uint8_t *s0 = sets_begin(block(empty.mem.data(), 0));
    float wear;
    std::memcpy(&wear, s0 + kSetSize + 0x18, sizeof wear);
    check("copied used set is new and unworn", !(s0[kSetSize + 9] & kUsed) && wear == 0.f);
    uint32_t tag;
    std::memcpy(&tag, s0 + 2 * kSetSize, sizeof tag);
    check("copied set keeps its tag and handed-back flag", tag == 15 && (s0[2 * kSetSize + 9] & kHandedBack));
    check("set used outside the garage copies as new", !(sets_begin(block(empty.mem.data(), 4))[9] & kUsedOutsideGarage));
    check("refilled allocation marked dirty", empty.mem[0xe8] == 1);
    uint64_t cap;
    std::memcpy(&cap, block(empty.mem.data(), 0) + 0x20, sizeof cap);
    check("refill records the capacity", cap == 3);
    check("donor untouched", census(donor.mem.data()).fresh == 3 && donor.flags(0, 1) == kUsed);
    size_t before = blocks.size();
    check("second refill reuses the buffers", copy_sets(empty.mem.data(), donor.mem.data(), heap) == 6 &&
                                                   blocks.size() == before && frees == 0);

    Alloc orphan({{}, {}, {}, {}, {}});
    check("refill without an allocator fails", copy_sets(orphan.mem.data(), donor.mem.data(), heap) == -1 &&
                                                   census(orphan.mem.data()).total == 0 && orphan.mem[0xe8] == 0);

    Alloc tagged({{{1, 0}, {2, kUsed}, {16, kHandedBack}}, {{5, 0}}, {}, {{0, 0}}});
    check("untag changes fresh tagged sets only", untag_fresh_sets(tagged.mem.data()) == 2);
    check("untagged fresh sets", tagged.tag(0, 0) == 0 && tagged.tag(1, 0) == 0);
    check("used and handed-back sets keep their tag", tagged.tag(0, 1) == 2 && tagged.tag(0, 2) == 16);
    check("untagged allocation marked dirty", tagged.mem[0xe8] == 1);
    for (void *b : blocks)
        std::free(b);

    float nan = std::numeric_limits<float>::quiet_NaN();
    Alloc wearing({{{0, 0, nan}, {0, kUsed, 2.f}}, {{5, 0, 1.f}, {0, 0, 0.5f}}, {}, {{0, 0}}});
    Census wc = census(wearing.mem.data());
    check("census counts worn fresh sets (NaN and 1.0)", wc.worn == 2 && wc.fresh == 4);
    check("census collects fresh tags", wc.tags == ((1u << 0) | (1u << 5)));
    check("clear wear on worn fresh sets only", clear_fresh_wear(wearing.mem.data()) == 2);
    float w;
    std::memcpy(&w, wearing.sets[0].data() + 0x18, sizeof w);
    check("NaN wear cleared", w == 0.f);
    std::memcpy(&w, wearing.sets[0].data() + kSetSize + 0x18, sizeof w);
    check("used set keeps its wear", w == 2.f);
    std::memcpy(&w, wearing.sets[1].data() + kSetSize + 0x18, sizeof w);
    check("usable wear kept", w == 0.5f);
    check("wear clear marks dirty", wearing.mem[0xe8] == 1 && census(wearing.mem.data()).worn == 0);

    Alloc slots({{{0, kUsed}}, {}, {{0, kHandedBack}}, {{0, 0}}, {{0, 0}}});
    uint32_t pref_hard[5] = {2, 1, 0, 4, 3}, pref_bad[5] = {7, 9, 0, 2, 1};
    check("fresh slot follows the preference", fresh_slot(slots.mem.data(), pref_hard) == 4);
    check("fresh slot skips out-of-range slots and finds none", fresh_slot(slots.mem.data(), pref_bad) == -1);

    std::vector<uint8_t> weekend(0x300), sessions(0xa0 * 5);
    auto session = [&](size_t i, uint32_t id, uint8_t group) {
        std::memcpy(sessions.data() + i * 0xa0 + 8, &id, 4);
        sessions[i * 0xa0 + 0x98] = group;
    };
    session(0, 1, 0);
    session(1, 2, 0);
    session(2, 13, 2);
    session(3, 8, 1);
    session(4, 15, 3);
    uint8_t *sb = sessions.data(), *se = sessions.data() + sessions.size();
    std::memcpy(weekend.data() + 0x220, &sb, sizeof sb);
    std::memcpy(weekend.data() + 0x228, &se, sizeof se);
    check("1990 weekend recognised", f1990::is_1990_weekend(weekend.data()));
    session(4, 16, 3);
    check("sprint weekend with race_2 rejected", !f1990::is_1990_weekend(weekend.data()));
    session(4, 15, 3);
    session(2, 3, 0);
    check("weekend without a shootout group rejected", !f1990::is_1990_weekend(weekend.data()));

    std::vector<uint8_t> classes(kClassSize * 3);
    auto masks = [&](size_t i, uint32_t setup, uint32_t season, uint32_t format) {
        uint32_t m[3] = {setup, season, format};
        std::memcpy(classes.data() + i * kClassSize + kClassMasks, m, sizeof m);
    };
    masks(0, 1u << 3, ~0u, 1);
    masks(1, 1u << 16, ~0u, 1);
    masks(2, 1u << 16, ~0u, 2);
    Championship f1990{16, 1, 7, 1, 0, 1}, stock{3, 1, 7, 1, 0, 1}, sprint{16, 1, 7, 1, 1, 1};
    auto first = [&](const Championship &c) {
        for (size_t i = 0; i < 3; i++)
            if (class_matches(classes.data() + i * kClassSize, c))
                return int(i);
        return -1;
    };
    check("class matches the 1990 setup mode", first(f1990) == 1);
    check("class matches the stock setup mode", first(stock) == 0);
    check("class matches the weekend format", first(sprint) == 2);
    check("class ignores masks that do not apply", first(Championship{40, 0, 40, 0, 1, 0}) == 0);
    check("class bit past its mask matches nothing", first(Championship{40, 1, 7, 1, 0, 1}) == -1);

    auto physical = [&](size_t i, std::vector<uint32_t> masks) {
        std::memcpy(classes.data() + i * kClassSize + kClassPhysicalMasks, masks.data(), kClassOptions * 4);
    };
    physical(0, {0x7f, 0x7f, 0x7f, 0x80, 0x100});
    physical(1, {0x7f, 0x7f, 0x7f, 0x80, 0x100});
    physical(2, {0x7f, 0x7f, 0x3f, 0x80, 0x100});
    check("same physical compounds per option", same_physical(classes.data(), classes.data() + kClassSize));
    check("other physical compounds in one option differ", !same_physical(classes.data(), classes.data() + 2 * kClassSize));

    if (failures)
        std::printf("%d failure(s)\n", failures);
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
