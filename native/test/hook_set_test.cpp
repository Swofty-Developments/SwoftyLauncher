// Host-native unit test for a core mixin set's shared state (native/src/core/runtime/loader/hook_set.hpp): the bind
// step's resolution and its reason, an original's slot and call, turning the hooks off, and a set's separately wanted
// parts.
// Built and run by native/test/run_tests.sh hook_set.

#include "core/runtime/loader/hook_set.hpp"

#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

using runtime::loader::Binding;
using runtime::loader::HookSet;
using runtime::loader::MixinPart;
using runtime::loader::Original;

static uintptr_t fake_resolve(const char *id)
{
    if (!std::strcmp(id, "Game_Present"))
        return 0x1000;
    if (!std::strcmp(id, "g_Game_Manager"))
        return 0x2000;
    return 0;
}

static void bind_step()
{
    uintptr_t a = 7, b = 7, c = 7, d = 7;
    std::string why;
    CHECK(HookSet<>::bind({{"Game_Present", &a}, {"g_Game_Manager", &b}}, why));
    CHECK(a == 0x1000 && b == 0x2000 && why.empty());

    CHECK(!HookSet<>::bind({{"Game_Present", &a}, {"Game_Missing", &b}, {"g_Game_Manager", &c}, {"k_Missing", &d}}, why));
    CHECK(why == "needs Game_Missing, k_Missing, which this build does not resolve");
    CHECK(a == 0x1000 && b == 0 && c == 0x2000 && d == 0);

    why.clear();
    a = b = 7;
    CHECK(HookSet<>::bind({{nullptr, &a}, {"Game_Optional", &b, false}}, why));
    CHECK(a == 0 && b == 0 && why.empty());
}

static int doubled(int v) { return v * 2; }

static void original()
{
    Original<int(int)> o;
    CHECK(!o);
    *o.slot() = reinterpret_cast<void *>(&doubled);
    CHECK(bool(o));
    CHECK(o(21) == 42);
}

static void turn_off()
{
    struct Hooks : HookSet<std::recursive_mutex> {
        Original<int(int)> scale;
    } hooks;
    CHECK(!hooks.live() && !hooks.off() && hooks.why().empty());
    CHECK(hooks.gate.offer() && !hooks.gate.offer());
    hooks.set_live();
    CHECK(hooks.live());
    {
        std::lock_guard<std::recursive_mutex> g(hooks.lock);
        std::lock_guard<std::recursive_mutex> again(hooks.lock);
        hooks.turn_off("the hooks did not install");
    }
    CHECK(hooks.off() && hooks.why() == "the hooks did not install");
}

static void mixin_part()
{
    MixinPart idle{"idle mixin", "idle"}, used{"used mixin", "used"}, failed{"failed mixin", "failed"};
    CHECK(!idle.offer() && !idle.gate.offered());
    CHECK(idle.usable(false) && !idle.usable(true));
    CHECK(!idle.settle([] { return true; }) && !idle.live);
    CHECK(std::strcmp(idle.state(), "not used") == 0);

    used.wanted = true;
    CHECK(used.offer() && !used.offer());
    int asked = 0;
    CHECK(used.settle([&] { return ++asked, true; }));
    CHECK(!used.settle([&] { return ++asked, false; }));
    CHECK(asked == 1 && used.live && used.usable(true));
    CHECK(std::strcmp(used.state(), "armed") == 0);

    failed.wanted = true;
    CHECK(failed.offer());
    CHECK(failed.settle([] { return false; }));
    CHECK(!failed.live && !failed.usable(false));
    CHECK(std::strcmp(failed.state(), "off") == 0);

    CHECK(failed.first_late() && !failed.first_late());
}

int main()
{
    SwoftyApi api{};
    api.resolve = fake_resolve;
    swofty::api = &api;
    bind_step();
    original();
    turn_off();
    mixin_part();
    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("hook_set: all checks passed\n");
    return 0;
}
