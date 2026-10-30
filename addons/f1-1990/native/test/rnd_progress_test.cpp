// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cmath>
#include <cstdio>
#include <vector>

#include "rnd_progress.hpp"

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

struct Inst {
    std::vector<uint8_t> raw = std::vector<uint8_t>(0xe0);
    std::vector<uint8_t> states = std::vector<uint8_t>(0x20);
    Inst(uint8_t type, float weight, uint8_t last_state, uint32_t hash, const char *name)
    {
        put<uint8_t>(raw.data(), 0x70, type);
        put<float>(raw.data(), 0x64, weight);
        put<uint32_t>(raw.data(), 0x58, hash);
        put<const char *>(raw.data(), 0x10, name);
        states[0x08] = 1;
        states[0x18] = last_state;
        put<const uint8_t *>(raw.data(), 0xc0, states.data());
        put<const uint8_t *>(raw.data(), 0xc8, states.data() + states.size());
    }
};

static bool near(float a, float b)
{
    return std::fabs(a - b) < 1e-5f;
}

int main()
{
    Inst ice(0, 1.f, 3, 0x11111111, "f1990_ice_1"), rev(0, 1.f, 3, f1990::kQualiRevLimit, "f1990_quali_rev_limit"),
        qeng(0, 2.f, 3, 0, "f1990_quali_engine"), cool(0, 2.f, 1, 0x22222222, "f1990_cooling"),
        aero(4, 5.f, 3, 0x33333333, "f1990_wing");
    const void *list[] = {ice.raw.data(), rev.raw.data(), qeng.raw.data(), cool.raw.data(), aero.raw.data()};
    std::vector<uint8_t> meta(0x40);
    meta[0] = 4;
    put<float>(meta.data(), 0x1c, 0.5f);
    meta[0x20] = 0;
    put<float>(meta.data(), 0x3c, 1.f);
    auto none = [](const void *) { return false; };

    float all = f1990::type_progress(list, 5, 0, 3, meta.data(), meta.data() + meta.size(), none);
    float race = f1990::type_progress(list, 5, 0, 3, meta.data(), meta.data() + meta.size(), f1990::is_quali_upgrade);
    check("stock formula: completed weighting over all (4 of 6)", near(all, 4.f / 6.f));
    check("qualifying lines leave the numerator only (1 of 6)", near(race, 1.f / 6.f));
    check("the type's scale applies", near(f1990::type_progress(list, 5, 4, 3, meta.data(), meta.data() + meta.size(), none), 0.5f));
    check("no instances of a type gives 0", f1990::type_progress(list, 5, 7, 3, meta.data(), meta.data() + meta.size(), none) == 0.f);
    put<float>(meta.data(), 0x3c, 3.f);
    check("clamps to 1", f1990::type_progress(list, 5, 0, 3, meta.data(), meta.data() + meta.size(), none) == 1.f);
    check("finds the qualifying upgrades by hash or name",
          f1990::is_quali_upgrade(rev.raw.data()) && f1990::is_quali_upgrade(qeng.raw.data()) &&
              !f1990::is_quali_upgrade(ice.raw.data()));

    std::vector<uint8_t> weekend(0x240), sessions(3 * 0xa0);
    sessions[0x98] = 0;
    sessions[0xa0 + 0x98] = f1990::kShootoutGroup;
    sessions[0x140 + 0x98] = f1990::kQualifyingGroup;
    put<const uint8_t *>(weekend.data(), 0x220, sessions.data());
    put<const uint8_t *>(weekend.data(), 0x228, sessions.data() + sessions.size());
    check("reads a session's group", f1990::session_group(weekend.data(), 1) == f1990::kShootoutGroup);
    check("rejects an index past the list", f1990::session_group(weekend.data(), 3) == -1);
    check("Friday and Saturday qualifying count as qualifying",
          f1990::is_qualifying_group(f1990::kShootoutGroup) && f1990::is_qualifying_group(f1990::kQualifyingGroup) &&
              !f1990::is_qualifying_group(0));

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
