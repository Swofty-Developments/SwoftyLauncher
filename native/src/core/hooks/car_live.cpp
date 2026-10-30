#include "../runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstring>

#include "car_live.hpp"

// The live car operations on the running game (car_live.hpp): g_Sess_VehicleTable and g_Sess_VehicleCount
// resolve once the code is ready, memory reads and writes are checked (swofty::mem), writes run only on the
// flow thread (the one that ticks on_frame), and nothing is handed over on an exe that isn't the KB build,
// whose field offsets these are.

namespace swofty_core::hooks {
namespace {

namespace mem = swofty::mem;

struct CoreTag {};
using Ops = car_live::Ops<CoreTag>;

std::atomic<uint32_t> g_flow_tid{0};

bool on_flow_thread()
{
    uint32_t t = g_flow_tid.load();
    return !t || t == GetCurrentThreadId();
}

void frame(double, void *) { g_flow_tid = GetCurrentThreadId(); }

void setup()
{
    car_live::Env e;
    e.read = [](uintptr_t a, void *out, size_t n) {
        if (!mem::readable(a, n))
            return false;
        std::memcpy(out, reinterpret_cast<const void *>(a), n);
        return true;
    };
    e.write = [](uintptr_t a, const void *in, size_t n) {
        if (!mem::readable(a, n))
            return false;
        std::memcpy(reinterpret_cast<void *>(a), in, n);
        return true;
    };
    e.flow_thread = on_flow_thread;
    e.enabled = [] { return runtime::enabled("events", nullptr); };
    if (!runtime::same_build()) {
        swofty::log("[cars] live car operations off: the field offsets are the KB build's and this is not that build");
    } else {
        e.table = swofty::api->resolve("g_Sess_VehicleTable");
        e.count = swofty::api->resolve("g_Sess_VehicleCount");
        if (!e.table || !e.count)
            swofty::log("[cars] live car operations off: g_Sess_VehicleTable or g_Sess_VehicleCount does not resolve");
        else
            swofty::log("[cars] live car operations on: %zu fields", sizeof car_live::FIELDS / sizeof car_live::FIELDS[0]);
    }
    Ops::bind(std::move(e));
    const SwoftyEvents *ev = runtime::events_table();
    uint32_t token = 0;
    if (ev && ev->on_frame)
        ev->on_frame(frame, nullptr, &token);
}

}
}

extern "C" __declspec(dllexport) const SwoftyCarOps *SwoftyCoreCarOps(void)
{
    return &swofty_core::hooks::Ops::table;
}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("live car operations", swofty_core::hooks::setup);
}
