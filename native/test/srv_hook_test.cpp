// Host-native unit test for the addon view callbacks on CreateShaderResourceView
// (native/src/core/render/srv_hook_logic.hpp). Built and run by native/test/run_tests.sh srv_hook.

#include "core/render/srv_hook_logic.hpp"

#include <cstdint>
#include <cstdio>
#include <vector>

namespace sh = render::srv_hook;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

using Fn = void (*)(void *user, int value);

static void add_value(void *user, int value) { static_cast<std::vector<int> *>(user)->push_back(value); }
static void add_twice(void *user, int value) { static_cast<std::vector<int> *>(user)->push_back(2 * value); }

int main()
{
    CHECK(!sh::wanted(false, false));
    CHECK(sh::wanted(true, false));
    CHECK(sh::wanted(false, true));

    sh::Callbacks<Fn> cbs;
    CHECK(!cbs.any());
    CHECK(cbs.add(nullptr, nullptr) == 0);

    std::vector<int> a, b;
    uint32_t ta = cbs.add(&add_value, &a);
    uint32_t tb = cbs.add(&add_twice, &b);
    CHECK(ta && tb && ta != tb);
    CHECK(cbs.any());
    cbs.each([](Fn fn, void *user) { fn(user, 7); });
    CHECK(a.size() == 1 && a[0] == 7);
    CHECK(b.size() == 1 && b[0] == 14);

    // A removed callback no longer runs; its stale token removes nothing, even once the slot is reused.
    CHECK(cbs.remove(ta));
    CHECK(!cbs.remove(ta));
    cbs.each([](Fn fn, void *user) { fn(user, 1); });
    CHECK(a.size() == 1 && b.size() == 2);
    uint32_t tc = cbs.add(&add_value, &a);
    CHECK(tc && tc != ta && (tc & 0xffu) == (ta & 0xffu));
    CHECK(!cbs.remove(ta));
    cbs.each([](Fn fn, void *user) { fn(user, 3); });
    CHECK(a.size() == 2 && a[1] == 3 && b.size() == 3);

    CHECK(!cbs.remove(0));
    CHECK(!cbs.remove(0xffu));
    CHECK(cbs.remove(tb) && cbs.remove(tc));
    CHECK(!cbs.any());

    // The table holds kMax callbacks; the next one is refused.
    std::vector<uint32_t> tokens;
    for (uint32_t i = 0; i < sh::kMax; i++)
        tokens.push_back(cbs.add(&add_value, &a));
    bool all = true;
    for (uint32_t t : tokens)
        all = all && t != 0;
    CHECK(all);
    CHECK(cbs.add(&add_value, &a) == 0);
    CHECK(cbs.remove(tokens[5]));
    CHECK(cbs.add(&add_value, &a) != 0);

    if (g_fail) {
        std::printf("srv_hook: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("srv_hook: ok\n");
    return 0;
}
