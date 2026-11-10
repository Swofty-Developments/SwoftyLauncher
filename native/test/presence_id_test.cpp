#include "presence_id_logic.hpp"

#include <cstdio>

using namespace runtime::presence_id;

static int failures = 0;
#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

int main()
{
    Object o;
    o.at20 = 0x3f0ce3369ec6a9c6ull;
    o.at28 = 0x3e1ce337ac807cc1ull;
    Id id = from_presence_object(o);
    CHECK(id.type == 1 && id.value[0] == 76561198803768583ull && !id.flag39 && !id.flag38);

    auto bytes = serialise(id);
    CHECK(bytes.size() == 9);
    CHECK(bytes[0] == 0xe2);
    Id back;
    CHECK(deserialise(bytes, back) && same_user(id, back) && back.value[0] == id.value[0]);

    Id wrong_mode;
    std::vector<uint8_t> typed = bytes;
    typed[0] |= 1;
    CHECK(!deserialise(typed, wrong_mode));

    Id pair = id;
    pair.value[1] = 5;
    pair.value[2] = 7;
    auto longer = serialise(pair);
    CHECK(longer.size() == 25);
    Id back2;
    CHECK(deserialise(longer, back2) && back2.value[1] == 5 && back2.value[2] == 7 && same_user(id, back2));

    Id t3;
    t3.type = 3;
    t3.value[0] = 889856962207027981ull;
    t3.flag39 = true;
    auto s3 = serialise(t3);
    Id b3;
    CHECK(deserialise(s3, b3) && b3.type == 3 && b3.value[0] == t3.value[0] && b3.flag39);
    CHECK(!same_user(id, t3));

    std::vector<uint8_t> server_blob = {0xb3, 0x61, 0x42, 0xa8, 0x28, 0x06, 0x2d, 0x8b, 0x21, 0, 0, 0, 0, 0, 0, 0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    Id rejected;
    CHECK(!deserialise(server_blob, rejected));

    Id flag = id;
    flag.flag39 = true;
    CHECK(!same_user(id, flag));
    std::printf("presence stream %s (hex %s)\n", base64(bytes).c_str(), hex(bytes).c_str());
    if (failures)
        return 1;
    std::puts("presence id: ok");
    return 0;
}
