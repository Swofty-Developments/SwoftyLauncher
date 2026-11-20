#include "wine_test.hpp"
#include <cmath>
#include <string_view>

SWOFTY_SETTING(bool, verbose, true);
SWOFTY_SETTING(int, greeting_count, 7);
SWOFTY_SETTING(double, offset, 0.5);
SWOFTY_SETTING(std::string_view, flavour, "plain");
SWOFTY_SETTING(bool, mismatched, true);
SWOFTY_SETTING(int, undeclared, 42);
SWOFTY_SETTING(bool, audi_name, false);

namespace {

long long seen(int slot, int64_t value)
{
    int64_t calls = probe()[16 + slot] + 1;
    probe()[8 + slot] = value;
    probe()[16 + slot] = calls;
    return calls;
}

}

SWOFTY_ON_CHANGE(verbose, bool v)
{
    swofty::log("[t_settings] on_change verbose -> %s (call %lld)", v ? "true" : "false", seen(0, v));
}

SWOFTY_ON_CHANGE(greeting_count, int v) { swofty::log("[t_settings] on_change greeting_count -> %d (call %lld)", v, seen(1, v)); }

SWOFTY_ON_CHANGE(offset, double v)
{
    swofty::log("[t_settings] on_change offset -> %.2f (call %lld)", v, seen(2, std::lround(v * 1000)));
}

SWOFTY_ON_CHANGE(flavour, std::string_view v)
{
    swofty::log("[t_settings] on_change flavour -> %s (call %lld)", v.data(), seen(3, reinterpret_cast<intptr_t>(v.data())));
}

SWOFTY_ON_CHANGE(mismatched, bool v) { swofty::log("[t_settings] on_change mismatched -> %d (call %lld)", v, seen(4, v)); }

extern "C" __declspec(dllexport) void t_settings_read()
{
    std::string_view f = flavour;
    volatile int64_t *p = probe() + 24;
    p[0] = verbose.get();
    p[1] = greeting_count.get();
    p[2] = std::lround(offset.get() * 1000);
    p[3] = reinterpret_cast<intptr_t>(f.data());
    p[4] = mismatched.get();
    p[5] = undeclared.get();
    p[6] = audi_name.get();
    swofty::log("[t_settings] read verbose=%d greeting_count=%d offset=%.2f flavour=%s mismatched=%d undeclared=%d "
                "audi_name=%d",
                verbose.get(), greeting_count.get(), offset.get(), f.data(), mismatched.get(), undeclared.get(), audi_name.get());
}

SWOFTY_ADDON()
