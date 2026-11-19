#include "core/frontend/name_hash.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <string>

// The race MFD (GameMFDSystem). The game opens a page by name hashes: Game_Mfd_Open(system, djb2(mfd name),
// djb2(panel group), djb2(panel) or -1, activate), as the damage auto-open does for car_damage_panel. The
// system's per-frame update takes the system as its only argument, so a wrap on it records the system and
// runs development opens on the update's own thread.

namespace frontend {

namespace {

const runtime::Sig S_UPDATE{"Game_MfdSystem_Update",
                            "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 88 00 00 00 C5 F9 29 7C 24 70 C5 F9 29 74 24 60 "
                            "49 89 CD 48 8B 81 D0 00 00 00 83 78 20 01",
                            0x2a8e070};
const runtime::Sig S_OPEN{"Game_Mfd_Open call (damage auto-open)",
                          "B8 A8 2C 12 01 41 B9 ED AC C0 5B 44 0F 44 C8 48 8B 8E 88 02 00 00 C6 44 24 20 01 E8 ?? ?? ?? ??",
                          0x2a8cca0,
                          runtime::Sig::rel32,
                          27,
                          28,
                          5};

using frontend::name_hash::djb2;

constexpr uint32_t OSD_MFD = djb2("osd_mfd"), OSD_MAIN = djb2("osd_main");

using Open_t = uint64_t(void *system, uint32_t mfd, uint32_t group, uint32_t panel, uint8_t activate);

class MfdTest {
public:
    void setup();
    void tick();
    void update(void *system);

private:
    uint32_t test_panel = djb2("swofty_addons_panel");
    std::string test_panel_name = "swofty_addons_panel";
    uintptr_t a_open = 0;
    safetyhook::InlineHook h_update;
    std::atomic<void *> system_seen{nullptr};
    std::string test_mode;
    unsigned test_every = 6;
    uint64_t next_open = 0, next_step = 0;
    unsigned steps = 0;
    int told = 0;
};

// Not constinit: safetyhook's hooks hold a std::recursive_mutex, whose constructor is not constexpr.
MfdTest mfd_test;

void update_hook(void *system) { mfd_test.update(system); }

void MfdTest::update(void *system)
{
    if (system_seen.exchange(system) != system)
        swofty::log("[mfd] MFD system %p", system);
    if (!test_mode.empty()) {
        uint64_t now = GetTickCount64();
        if (now >= next_open) {
            next_open = now + 1000ull * test_every;
            uint64_t r = reinterpret_cast<Open_t *>(a_open)(system, OSD_MFD, OSD_MAIN, test_panel, 1);
            if (told++ < 4)
                swofty::log("[mfd] SWOFTY_MFD_TEST: opened %s in osd_main of osd_mfd (result %llu)", test_panel_name.c_str(),
                            (unsigned long long)r);
        }
    }
    h_update.call<void>(system);
}

void MfdTest::setup()
{
    char mode[32] = {};
    if (GetEnvironmentVariableA("SWOFTY_MFD_TEST", mode, sizeof mode) && mode[0])
        test_mode = mode;
    char every[16] = {};
    if (GetEnvironmentVariableA("SWOFTY_MFD_TEST_EVERY", every, sizeof every) && atoi(every) > 0)
        test_every = unsigned(atoi(every));
    char panel[64] = {};
    if (GetEnvironmentVariableA("SWOFTY_MFD_TEST_PANEL", panel, sizeof panel) && panel[0]) {
        test_panel_name = panel;
        test_panel = djb2(panel);
    }
    if (test_mode.empty())
        return;
    uintptr_t base = swofty::api->module_base();
    uintptr_t update = runtime::find(S_UPDATE, "MFD system", "mfd");
    a_open = runtime::find(S_OPEN, "MFD open", "mfd");
    if (!update || !a_open || !swofty::mem::in_image(a_open)) {
        swofty::log("[mfd] SWOFTY_MFD_TEST: the MFD system did not resolve (update %s, open %s); test off",
                    update ? "found" : "missing", a_open ? "found" : "missing");
        return;
    }
    auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(update), reinterpret_cast<void *>(&update_hook));
    if (!r) {
        swofty::log("[mfd] SWOFTY_MFD_TEST: hook on the MFD update at base+0x%llx failed; test off",
                    (unsigned long long)(update - base));
        return;
    }
    h_update = std::move(*r);
    swofty::log("[mfd] SWOFTY_MFD_TEST=%s: update base+0x%llx, open base+0x%llx; opens the ADDONS page every %us%s",
                test_mode.c_str(), (unsigned long long)(update - base), (unsigned long long)(a_open - base), test_every,
                test_mode == "next" ? " and steps its first row" : "");
}

void MfdTest::tick()
{
    if (test_mode != "next" || !system_seen.load(std::memory_order_relaxed) || !settings_mfd_rows())
        return;
    uint64_t now = GetTickCount64();
    if (now < next_step)
        return;
    next_step = now + 1000ull * test_every;
    if (!next_step || steps++ == 0)
        return;
    bool ok = settings_mfd_step(0, 1);
    swofty::log("[mfd] SWOFTY_MFD_TEST=next: row 0 %s", ok ? "stepped" : "did not step");
}

}

}

namespace runtime {

void mfd_setup() { frontend::mfd_test.setup(); }

void mfd_tick() { frontend::mfd_test.tick(); }

}
