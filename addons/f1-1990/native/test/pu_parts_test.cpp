// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <vector>

#include "pu_parts.hpp"

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

int main()
{
    using namespace f1990;
    check("lists the race engine", slot_listed(kSlotIce));
    check("lists the qualifying engine", slot_listed(kSlotMguK));
    check("lists the gearbox", slot_listed(kSlotGearbox));
    bool hidden = true;
    for (uint32_t s : {kSlotMguH, kSlotEs, kSlotTc, kSlotCe})
        hidden &= !slot_listed(s);
    check("hides MGU-H, ES, TC and CE", hidden);

    check("race engine wears in practice and the race", slot_wears(kSlotIce, 0) && slot_wears(kSlotIce, 3));
    check("race engine rests in both qualifying groups", !slot_wears(kSlotIce, 1) && !slot_wears(kSlotIce, 2));
    check("qualifying engine wears in both qualifying groups", slot_wears(kSlotMguK, 1) && slot_wears(kSlotMguK, 2));
    check("qualifying engine rests outside qualifying", !slot_wears(kSlotMguK, 0) && !slot_wears(kSlotMguK, 3));
    check("an unknown session wears the race engine", slot_wears(kSlotIce, -1) && !slot_wears(kSlotMguK, -1));
    check("gearbox wears in every session", slot_wears(kSlotGearbox, -1) && slot_wears(kSlotGearbox, 1));
    bool rest = true;
    for (uint32_t s : {kSlotMguH, kSlotEs, kSlotTc, kSlotCe})
        for (int g : {-1, 0, 1, 2, 3})
            rest &= !slot_wears(s, g);
    check("hidden slots never wear", rest);

    std::vector<uint8_t> records(3 * kPuRecordSize);
    uint32_t keys[] = {0x01230007, 0x01230008, 0x04560007};
    for (int i = 0; i < 3; i++)
        put<uint32_t>(records.data() + i * kPuRecordSize, 0, keys[i]);
    uint8_t *b = records.data(), *e = b + records.size();
    check("finds the first record", pu_record(b, e, 0x01230007) == b);
    check("finds the third record", pu_record(b, e, 0x04560007) == b + 2 * kPuRecordSize);
    check("ignores the key's middle byte", pu_record(b, e, 0x0123ff08) == b + kPuRecordSize);
    check("misses an unknown driver", pu_record(b, e, 0x01230009) == nullptr);
    check("rejects a partial vector", pu_record(b, e - 1, 0x01230007) == nullptr);
    check("rejects an empty vector", pu_record(b, b, 0x01230007) == nullptr);

    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures != 0;
}
