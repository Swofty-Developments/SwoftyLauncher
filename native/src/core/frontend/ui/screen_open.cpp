#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <cstring>
#include <string>

#include "core/diagnostics/perf.hpp"
#include "core/frontend/main_menu.hpp"
#include "core/frontend/ui/screen_loader.hpp"
#include "core/runtime/runtime.hpp"
#include "core/services/lap_host.hpp"

namespace frontend::ui {

namespace {

double perf_ms(const LARGE_INTEGER &a, const LARGE_INTEGER &b)
{
    static LARGE_INTEGER f = [] {
        LARGE_INTEGER q;
        QueryPerformanceFrequency(&q);
        return q;
    }();
    return double(b.QuadPart - a.QuadPart) * 1000.0 / double(f.QuadPart);
}

void perf_probe(const char *what, const void *p)
{
    MEMORY_BASIC_INFORMATION m{};
    if (!p || !VirtualQuery(p, &m, sizeof m))
        return;
    LARGE_INTEGER a, b;
    QueryPerformanceCounter(&a);
    for (int i = 0; i < 100; i++)
        VirtualQuery(p, &m, sizeof m);
    QueryPerformanceCounter(&b);
    swofty::log("[perf] VirtualQuery on %s %p: allocation base %p, region %p + %llu MB, %.2f us per call", what, p,
                m.AllocationBase, m.BaseAddress, (unsigned long long)(m.RegionSize >> 20), perf_ms(a, b) * 10.0);
}

bool is_open(void *scene, void *screen)
{
    auto *sc = static_cast<uint8_t *>(scene);
    if (!readable(sc + 0x130, 0x10))
        return true;
    uint8_t *n = *reinterpret_cast<uint8_t **>(sc + 0x138);
    for (unsigned guard = 0; n && n != sc + 0x130 && guard < 256; guard++) {
        if (!readable(n, 0x18))
            return true;
        if (*reinterpret_cast<void **>(n + 0x10) == screen)
            return true;
        n = *reinterpret_cast<uint8_t **>(n + 8);
    }
    return false;
}

}

uint64_t open_hook(void *scene, void *screen, void *a3, void *a4) { return loader.opened(scene, screen, a3, a4); }

NeStrHdr **resolve_hook(void *self, NeStrHdr **err, void *cmd) { return loader.route_resolved(self, err, cmd); }

NeStrHdr **openexec_hook(void *self, NeStrHdr **err, void *cmd) { return loader.route_opened(self, err, cmd); }

uint64_t ScreenLoader::opened(void *scene, void *screen, void *a3, void *a4)
{
    static const uint32_t probe = perf::probe("core", "ui screen open");
    perf::Scope p(probe);
    if (readable(screen, 0x70) && readable(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68), 10)) {
        const char *opened = chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68));
        runtime::load_stall_screen(opened);
        if (perf::live())
            perf::mark("screen", opened);
    }
    LARGE_INTEGER t0, t1, t2;
    QueryPerformanceCounter(&t0);
    AcquireSRWLockExclusive(&lock);
    register_pending("screen open");
    ReleaseSRWLockExclusive(&lock);
    QueryPerformanceCounter(&t1);
    uint64_t r;
    {
        perf::Pause game;
        r = h_open.call<uint64_t>(scene, screen, a3, a4);
    }
    QueryPerformanceCounter(&t2);
    double total = perf_ms(t0, t2);
    if (perf || total >= 50.0) {
        const char *name =
            readable(screen, 0x70) && readable(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68), 10)
                ? chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68))
                : "?";
        swofty::log("[perf] screen open '%.64s': %.2f ms (addon screen registration %.2f ms, engine open %.2f ms)", name, total,
                    perf_ms(t0, t1), perf_ms(t1, t2));
        if (perf && !perf_probed) {
            perf_probed = true;
            perf_probe("the screen object", screen);
            perf_probe("the scene object", scene);
        }
    }
    if (readable(screen, 0x70) && readable(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68), 10)) {
        runtime::session_host_screen(chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68)));
        frontend::main_menu::screen(chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68)));
        runtime::lap_host::screen(chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68)));
        runtime::testagent_screen(chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68)));
        runtime::direct_start_screen(chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(screen) + 0x68)));
    }
    if (screen != last_screen) {
        prev_scene = last_scene;
        prev_screen = last_screen;
        last_scene = scene;
        last_screen = screen;
    }
    return r;
}

bool ScreenLoader::guarded(const std::string &name)
{
    if (strict)
        return true;
    if (name.find("swofty") != std::string::npos)
        return true;
    for (auto &s : screens)
        if (s.name == name)
            return true;
    return false;
}

NeStrHdr **ScreenLoader::cancel_missing(NeStrHdr **err, const char *step)
{
    if (!err || !*err || !readable(*err, 16))
        return err;
    std::string text = chars(*err);
    const char *p = "No screen called ";
    if (text.rfind(p, 0) != 0)
        return err;
    std::string name = text.substr(strlen(p));
    name = name.substr(0, name.find(' '));
    if (!guarded(name)) {
        swofty::log("[ui] route step %s -> screen '%s' is not registered; not an addon screen, the engine handles it "
                    "(strict mode off)",
                    step, name.c_str());
        return err;
    }
    *err = nullptr;
    swofty::log("[ui] route -> screen '%s' is not registered; route cancelled (%s), front end kept", name.c_str(), step);
    AcquireSRWLockExclusive(&lock);
    for (auto &s : screens)
        if (s.name == name && s.state == State::idle && s.resid)
            s.state = State::armed;
    ReleaseSRWLockExclusive(&lock);
    if (recover && h_open && last_screen && last_scene) {
        bool ok = false;
        const char *cur =
            readable(last_screen, 0x70) ? chars(*reinterpret_cast<void **>(static_cast<uint8_t *>(last_screen) + 0x68)) : "?";
        void *scr = screen_lookup(djb2(cur), &ok);
        if (scr == last_screen && is_open(last_scene, last_screen))
            swofty::log("[ui] last screen '%s' is still open; staying on it", cur);
        else if (scr == last_screen) {
            swofty::log("[ui] re-opening the last screen '%s' after the cancelled route", cur);
            h_open.call<uint64_t>(last_scene, last_screen, nullptr, nullptr);
        }
    }
    return err;
}

NeStrHdr **ScreenLoader::route_resolved(void *self, NeStrHdr **err, void *cmd)
{
    static const uint32_t probe = perf::probe("core", "ui route guard");
    NeStrHdr **r = h_resolve.call<NeStrHdr **>(self, err, cmd);
    perf::Scope p(probe);
    return cancel_missing(r, "resolve-target");
}

NeStrHdr **ScreenLoader::route_opened(void *self, NeStrHdr **err, void *cmd)
{
    static const uint32_t probe = perf::probe("core", "ui route guard");
    NeStrHdr **r = h_openexec.call<NeStrHdr **>(self, err, cmd);
    perf::Scope p(probe);
    return cancel_missing(r, "open-screen");
}

}
