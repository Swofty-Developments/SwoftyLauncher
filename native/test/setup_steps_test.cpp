// Host-native unit test for core/apply/content/setup_steps.hpp.

#include "core/apply/content/setup_steps.hpp"

#include <cstdio>
#include <cstring>
#include <vector>

namespace c = swofty_core::apply::content;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    std::vector<uint8_t> image(0x400, 0);
    uintptr_t base = reinterpret_cast<uintptr_t>(image.data());
    auto read = [&](uintptr_t a, void *out, size_t n) {
        if (a < base || a + n > base + image.size())
            return false;
        std::memcpy(out, image.data() + (a - base), n);
        return true;
    };
    auto put = [&](uintptr_t off, int32_t v) { std::memcpy(image.data() + off, &v, sizeof v); };
    for (int i = 0; i < c::SETUP_SLIDER_COUNT; i++) {
        put(c::SETUP_BLOCK_STEPS + 4 * i, 100 + i);
        put(c::SETUP_BLOCK_STEPS_ALT + 4 * i, 200 + i);
    }
    c::SetupSteps steps{};
    expect(c::read_setup_steps(base, read, steps) && steps[0] == 100 && steps[37] == 137 && steps[41] == 141,
           "the plain block is read when the selector byte is 0");
    image[c::SETUP_BLOCK_SELECT] = 1;
    expect(c::read_setup_steps(base, read, steps) && steps[0] == 200 && steps[40] == 240, "the second block is read when the selector is set");
    c::SetupSteps keep = steps;
    expect(!c::read_setup_steps(0, read, steps) && steps == keep, "a null block reads nothing");
    expect(!c::read_setup_steps(base + 0x300, read, steps) && steps == keep, "an unreadable block leaves the steps alone");
    std::printf(failed ? "%d failed\n" : "all passed\n", failed);
    return failed ? 1 : 0;
}
