#include "runtime.hpp"
#include "telemetry_feed.hpp"
#include "udp_telemetry_logic.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <mutex>
#include <vector>

namespace runtime {
std::string hook_error(const safetyhook::MidHook::Error &e);
}

// Typed read access to live session and car state, and the shared-memory feed external tools
// read. The table stays unavailable until the telemetry feature is on (experimental) and a
// session is live. The per-frame tick, the session start and the results finalise are observed
// with mid hooks so the feed's frame counter and session clock advance in a running game. The
// car, participant and session fields come from the game's own UDP packets, decoded in the sendto
// hook (udp_ids.cpp, udp_telemetry_logic.hpp), so they fill only while the game's UDP telemetry
// output is on. Overtake, collision and retirement events are queued there and fired on the flow
// thread by the frame tick.

namespace {

namespace udp_tel = runtime::udp_tel;

using swofty::tel::feed_begin;
using swofty::tel::feed_copy;
using swofty::tel::feed_end;
using swofty::tel::feed_init;
using swofty::tel::feed_read;

const runtime::Sig S_FLOW{"Vm_Update_FlowStateMachine",
                          "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 58 02 00 00 C5 F8 29 B4 24 40 02 00 00 C5 F9 28 F2 48 89",
                          0x4423ce0};
const runtime::Sig S_ONSESS{"Game_GameModule_OnSessionEnter",
                            "41 57 41 56 41 54 56 57 53 48 81 EC 38 02 00 00 48 89 CE 80 3D AA F7 83 05 01 75 6D", 0x80c300};
const runtime::Sig S_RESULTS{"Game_SessionStateFinaliseResultsSync_OnResultsFinalised",
                             "56 57 53 48 83 EC 30 4C 89 C7 48 89 D0 48 89 CE 4C 8B 02 48 8D 15 5E C1 67 05", 0x9d2b20};

constexpr const char *FEED_NAME = "Local\\SwoftyTelemetry";

bool g_on;
std::atomic<bool> g_live;
std::mutex write_lock;
SwoftyTelemetryFeed master;

std::mutex map_lock;
std::atomic<bool> feed_on;
HANDLE map_handle;
SwoftyTelemetryFeed *view;

std::vector<udp_tel::Event> pending;
std::atomic<uint64_t> first_seen[3];

safetyhook::MidHook h_flow, h_onsess, h_results;
LARGE_INTEGER qpc_freq, qpc_last;

double tick_seconds()
{
    LARGE_INTEGER now;
    QueryPerformanceCounter(&now);
    double dt = qpc_freq.QuadPart ? double(now.QuadPart - qpc_last.QuadPart) / double(qpc_freq.QuadPart) : 0.0;
    qpc_last = now;
    return dt < 0 || dt > 1 ? 0.0 : dt;
}

void publish()
{
    if (feed_on.load(std::memory_order_acquire) && view)
        feed_copy(view, &master);
}

void on_flow(safetyhook::Context &)
{
    double dt = tick_seconds();
    std::vector<udp_tel::Event> fire;
    {
        std::lock_guard<std::mutex> g(write_lock);
        feed_begin(&master);
        master.frame++;
        if (g_live.load(std::memory_order_relaxed))
            master.session.session_time += float(dt);
        feed_end(&master);
        publish();
        fire.swap(pending);
    }
    for (auto &e : fire)
        runtime::events_fire(e.name, e.a, e.b, nullptr);
}

void set_live(bool live, const char *why)
{
    bool was = g_live.exchange(live);
    {
        std::lock_guard<std::mutex> g(write_lock);
        feed_begin(&master);
        if (!live) {
            uint64_t frame = master.frame;
            feed_init(&master);
            master.frame = frame;
        }
        feed_end(&master);
        publish();
    }
    if (!live) {
        std::lock_guard<std::mutex> g(write_lock);
        pending.clear();
    }
    if (was != live)
        swofty::log("[telemetry] session %s (%s); frame %llu", live ? "started" : "ended", why,
                    (unsigned long long)master.frame);
}

void on_sess(safetyhook::Context &)
{
    set_live(true, "Game_GameModule_OnSessionEnter");
}

void on_results(safetyhook::Context &)
{
    set_live(false, "Game_SessionStateFinaliseResultsSync_OnResultsFinalised");
}

int tel_available()
{
    return g_on && g_live.load(std::memory_order_acquire);
}

int tel_session(SwoftySessionState *out)
{
    if (!g_on || !out)
        return g_on ? SWOFTY_ASSET_INVALID : SWOFTY_ASSET_UNAVAILABLE;
    if (!g_live.load(std::memory_order_acquire))
        return SWOFTY_ASSET_UNAVAILABLE;
    SwoftyTelemetryFeed f;
    if (!feed_read(&master, &f))
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = f.session;
    return SWOFTY_ASSET_OK;
}

int tel_car(uint32_t index, SwoftyCarState *out)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!out || index >= swofty::tel::FEED_CARS)
        return SWOFTY_ASSET_INVALID;
    if (!g_live.load(std::memory_order_acquire))
        return SWOFTY_ASSET_UNAVAILABLE;
    SwoftyTelemetryFeed f;
    if (!feed_read(&master, &f))
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = f.cars[index];
    return SWOFTY_ASSET_OK;
}

int tel_participant(uint32_t index, SwoftyParticipant *out)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!out || index >= swofty::tel::FEED_PARTICIPANTS)
        return SWOFTY_ASSET_INVALID;
    if (!g_live.load(std::memory_order_acquire))
        return SWOFTY_ASSET_UNAVAILABLE;
    SwoftyTelemetryFeed f;
    if (!feed_read(&master, &f))
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = f.participants[index];
    return SWOFTY_ASSET_OK;
}

int tel_shared_memory(const char **name)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> g(map_lock);
    if (!view) {
        map_handle = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0,
                                        sizeof(SwoftyTelemetryFeed), L"Local\\SwoftyTelemetry");
        if (map_handle)
            view = static_cast<SwoftyTelemetryFeed *>(
                MapViewOfFile(map_handle, FILE_MAP_WRITE, 0, 0, sizeof(SwoftyTelemetryFeed)));
        if (!view) {
            swofty::log("[telemetry] shared_memory: CreateFileMapping/MapViewOfFile for %s failed (err %lu)", FEED_NAME,
                        GetLastError());
            if (map_handle)
                CloseHandle(map_handle), map_handle = nullptr;
            return SWOFTY_ASSET_UNAVAILABLE;
        }
        feed_init(view);
        {
            std::lock_guard<std::mutex> w(write_lock);
            feed_copy(view, &master);
        }
        swofty::log("[telemetry] shared_memory: %s mapped, %u bytes", FEED_NAME, unsigned(sizeof(SwoftyTelemetryFeed)));
    }
    feed_on.store(true, std::memory_order_release);
    if (name)
        *name = FEED_NAME;
    return SWOFTY_ASSET_OK;
}

void udp_first(const udp_tel::Spec *sp)
{
    if (sp->id > 15)
        return;
    uint64_t bit = uint64_t(1) << sp->id;
    if (first_seen[sp->format - 2024].fetch_or(bit) & bit)
        return;
    DWORD err = GetLastError();
    swofty::log("[telemetry] first UDP packet %d decoded: format %d, %zu bytes", sp->id, sp->format, sp->size);
    SetLastError(err);
}

const SwoftyTelemetry table = {tel_available, tel_session, tel_car, tel_participant, tel_shared_memory};

void hook(safetyhook::MidHook &slot, const runtime::Sig &sig, safetyhook::MidHookFn fn)
{
    uintptr_t at = runtime::find(sig, "telemetry");
    if (!at)
        return;
    auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), fn);
    if (!h) {
        swofty::log("[telemetry] mid hook on %s at base+0x%llx failed: %s", sig.what,
                    (unsigned long long)(at - swofty::api->module_base()), runtime::hook_error(h.error()).c_str());
        return;
    }
    slot = std::move(*h);
    swofty::log("[telemetry] %s hooked at base+0x%llx", sig.what, (unsigned long long)(at - swofty::api->module_base()));
}

void setup()
{
    if (!runtime::enabled("telemetry", nullptr))
        return;
    feed_init(&master);
    QueryPerformanceFrequency(&qpc_freq);
    QueryPerformanceCounter(&qpc_last);
    hook(h_flow, S_FLOW, on_flow);
    hook(h_onsess, S_ONSESS, on_sess);
    hook(h_results, S_RESULTS, on_results);
    g_on = true;
    swofty::log("[telemetry] on; frame tick %s, session witnesses %s/%s; car, participant and session fields fill from "
                "the game's UDP packets while its UDP telemetry output is on",
                h_flow ? "hooked" : "unresolved", h_onsess ? "on" : "off", h_results ? "on" : "off");
}

}

namespace runtime {

bool telemetry_wants_udp()
{
    return g_on;
}

void telemetry_udp(const uint8_t *buf, size_t len)
{
    if (!g_on || !g_live.load(std::memory_order_acquire))
        return;
    if (!udp_tel::identify(buf, len))
        return;
    std::vector<udp_tel::Event> events;
    const udp_tel::Spec *sp;
    {
        std::lock_guard<std::mutex> g(write_lock);
        feed_begin(&master);
        sp = udp_tel::decode(buf, len, master, &events);
        feed_end(&master);
        publish();
        if (pending.size() < 256)
            pending.insert(pending.end(), events.begin(), events.end());
    }
    if (sp)
        udp_first(sp);
}

const SwoftyTelemetry *telemetry_table()
{
    return &table;
}

void telemetry_setup()
{
    setup();
}

}
