// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <vector>

#include "character_ref.hpp"

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
    const char *names[] = {"engineer", "elena", "flavio"};
    uint8_t ids[] = {12, 75, 105};
    std::vector<uint8_t> strings[3][5], records(3 * f1990::kCharacterSize), vector(0x20);
    for (int i = 0; i < 3; i++)
        for (int f = 0; f < 5; f++) {
            strings[i][f].resize(0x20);
            put(strings[i][f].data(), 0x10, f ? "text" : names[i]);
            put(records.data() + i * f1990::kCharacterSize, 0x08 + f * 8, strings[i][f].data());
            records[i * f1990::kCharacterSize] = ids[i];
        }
    put(vector.data(), 0x08, records.data());
    put(vector.data(), 0x10, records.data() + records.size());

    const uint8_t *elena = f1990::find_character(vector.data(), "elena");
    const uint8_t *flavio = f1990::find_character(vector.data(), "flavio");
    check("finds elena", elena == records.data() + f1990::kCharacterSize);
    check("finds flavio", flavio == records.data() + 2 * f1990::kCharacterSize);
    check("misses an unknown name", !f1990::find_character(vector.data(), "nobody"));
    check("reads the record name", elena && !std::strcmp(f1990::character_name(elena), "elena"));

    uint8_t stand_in[f1990::kCharacterSize];
    f1990::stand_in_character(stand_in, elena, flavio);
    check("stand-in keeps elena's stat_id", stand_in[0] == 75);
    check("stand-in carries flavio's name", !std::strcmp(f1990::character_name(stand_in), "flavio"));
    check("stand-in carries flavio's other strings",
          !std::memcmp(stand_in + 0x10, flavio + 0x10, f1990::kCharacterSize - 0x10));

    put(vector.data(), 0x10, records.data() + records.size() - 1);
    check("rejects a vector that is not whole records", !f1990::find_character(vector.data(), "elena"));
    put(vector.data(), 0x10, records.data() - f1990::kCharacterSize);
    check("rejects end before begin", !f1990::find_character(vector.data(), "elena"));
    check("rejects no vector", !f1990::find_character(nullptr, "elena"));

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
