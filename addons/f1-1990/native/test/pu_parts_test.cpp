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
    bool listed = true;
    for (uint32_t s : {kSlotIce, kSlotMguK, kSlotEs, kSlotTc, kSlotCe, kSlotGearbox})
        listed &= slot_listed(s);
    check("lists engine, qualifying engine, clutch, fuel system, electronics and gearbox", listed);
    check("hides MGU-H", !slot_listed(kSlotMguH));
    check("lists no slot past the gearbox", !slot_listed(kSlotCount));

    check("race engine wears in practice and the race", slot_wears(kSlotIce, 0) && slot_wears(kSlotIce, 3));
    check("race engine rests in both qualifying groups", !slot_wears(kSlotIce, 1) && !slot_wears(kSlotIce, 2));
    check("qualifying engine wears in both qualifying groups", slot_wears(kSlotMguK, 1) && slot_wears(kSlotMguK, 2));
    check("qualifying engine rests outside qualifying", !slot_wears(kSlotMguK, 0) && !slot_wears(kSlotMguK, 3));
    check("an unknown session wears the race engine", slot_wears(kSlotIce, -1) && !slot_wears(kSlotMguK, -1));
    bool always = true, never = true;
    for (uint32_t s : {kSlotEs, kSlotTc, kSlotCe, kSlotGearbox})
        for (int g : {-1, 0, 1, 2, 3})
            always &= slot_wears(s, g);
    for (int g : {-1, 0, 1, 2, 3})
        never &= !slot_wears(kSlotMguH, g);
    check("clutch, fuel system, electronics and gearbox wear in every session", always);
    check("MGU-H never wears", never);

    check("clutch allocation 6", slot_allocation(kClutchSlot) == 6 && kClutchSlot == kSlotEs);
    check("fuel system allocation 5", slot_allocation(kFuelSystemSlot) == 5 && kFuelSystemSlot == kSlotTc);
    check("electronics allocation 5", slot_allocation(kElectronicsSlot) == 5 && kElectronicsSlot == kSlotCe);
    check("engine, qualifying engine and gearbox keep the set's allocation",
          !slot_allocation(kSlotIce) && !slot_allocation(kSlotMguK) && !slot_allocation(kSlotGearbox));
    check("wear scales 1.2, 0.9, 0.8", slot_wear_scale(kSlotEs) == 1.2f && slot_wear_scale(kSlotTc) == 0.9f &&
                                           slot_wear_scale(kSlotCe) == 0.8f);
    check("the engines and gearbox keep their own range",
          slot_wear_scale(kSlotIce) == 0.f && slot_wear_scale(kSlotMguK) == 0.f && slot_wear_scale(kSlotGearbox) == 0.f);

    std::vector<uint8_t> sets(2 * kSetSize);
    auto alloc = [&](int set, uint32_t slot) {
        return at<uint32_t>(sets.data() + set * kSetSize + kConfigsAt + slot * kConfigSize + kAllocationAt, 0);
    };
    for (int i = 0; i < 2; i++)
        for (uint32_t slot = 0; slot < kSlotCount; slot++)
            put<uint32_t>(sets.data() + i * kSetSize + kConfigsAt + slot * kConfigSize, kAllocationAt, 10 + slot);
    check("sets 3 allocations in each of 2 sets", apply_allocations(sets.data(), sets.data() + sets.size()) == 6);
    check("writes clutch, fuel system and electronics", alloc(0, kSlotEs) == 6 && alloc(1, kSlotTc) == 5 &&
                                                            alloc(1, kSlotCe) == 5);
    check("leaves the engine and gearbox", alloc(0, kSlotIce) == 10 && alloc(1, kSlotMguK) == 11 &&
                                               alloc(0, kSlotGearbox) == 16);
    check("a second pass changes nothing", apply_allocations(sets.data(), sets.data() + sets.size()) == 0);
    check("rejects a partial set vector", apply_allocations(sets.data(), sets.data() + sets.size() - 1) == 0);

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
