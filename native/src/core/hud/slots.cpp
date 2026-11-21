#include "core/frontend/ui_vm.hpp"
#include "core/hud/slots_logic.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <windows.h>

#include <swofty/hud_slot.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>

namespace hud::slots {
namespace {

// lea rdx, [KEY]; mov rcx, rbx; call [rax+0x28] ... in the HUD editor's init: KEY names the component whose +0x838
// is the customisation data.
constexpr const char *OWNER_KEY = "48 8D 15 ?? ?? ?? ?? 48 89 D9 FF 50 28 48 85 C0 0F 84 ?? ?? ?? ?? 49 89 C6 48 8B 03 48 8D 15 "
                                  "?? ?? ?? ?? 48 89 D9 FF 50 28 48 89 46 70";
constexpr ULONGLONG RETRY_MS = 500;

struct Mem {
    bool readable(uintptr_t a, size_t n) const { return swofty::mem::readable(a, n); }
    uintptr_t translate(uintptr_t a) const { return a; }
};

std::atomic<uintptr_t> key_at{0}, data_at{0};
std::atomic<int> scanned{0};
std::atomic<ULONGLONG> next_try{0};
std::atomic<uint32_t> flow_tid{0};
std::once_flag armed;

uintptr_t owner_key()
{
    if (scanned.load(std::memory_order_acquire))
        return key_at.load(std::memory_order_acquire);
    uintptr_t at = 0;
    int hits = swofty::api ? swofty::api->scan(".xtls", OWNER_KEY, &at) : 0;
    uintptr_t k = 0;
    if (hits == 1 && at)
        k = at + 7 + *reinterpret_cast<const int32_t *>(at + 3);
    key_at.store(k, std::memory_order_release);
    scanned.store(1, std::memory_order_release);
    swofty::log("[hud-slots] customisation component key: %s (signature matched %d time(s))", k ? "found" : "missing", hits);
    return k;
}

uintptr_t data()
{
    uintptr_t d = data_at.load(std::memory_order_acquire);
    if (d || flow_tid.load(std::memory_order_relaxed) != GetCurrentThreadId())
        return d;
    ULONGLONG now = GetTickCount64();
    if (now < next_try.load(std::memory_order_relaxed))
        return 0;
    next_try.store(now + RETRY_MS, std::memory_order_relaxed);
    uintptr_t k = owner_key();
    void **sys_at = frontend::ui_vm_ui_system();
    if (!k || !sys_at || !swofty::mem::readable(sys_at, sizeof(void *)))
        return 0;
    void *sys = *sys_at;
    if (!sys || !swofty::mem::readable(sys, sizeof(void *)))
        return 0;
    using Get_t = void *(*)(void *);
    using Find_t = void *(*)(void *, const void *);
    auto **vt = *reinterpret_cast<void ***>(sys);
    if (!swofty::mem::readable(vt + 14, sizeof(void *)))
        return 0;
    void *reg = reinterpret_cast<Get_t>(vt[0x70 / 8])(sys);
    if (!reg || !swofty::mem::readable(reg, sizeof(void *)))
        return 0;
    auto **rvt = *reinterpret_cast<void ***>(reg);
    if (!swofty::mem::readable(rvt + 5, sizeof(void *)))
        return 0;
    void *owner = reinterpret_cast<Find_t>(rvt[0x28 / 8])(reg, reinterpret_cast<const void *>(k));
    if (!owner || !swofty::mem::readable(reinterpret_cast<uintptr_t>(owner) + DATA_AT_OWNER, sizeof(void *)))
        return 0;
    d = *reinterpret_cast<uintptr_t *>(reinterpret_cast<uintptr_t>(owner) + DATA_AT_OWNER);
    if (d) {
        data_at.store(d, std::memory_order_release);
        swofty::log("[hud-slots] HUD customisation data at %p", reinterpret_cast<void *>(d));
    }
    return d;
}

void frame(double, void *)
{
    flow_tid.store(GetCurrentThreadId(), std::memory_order_relaxed);
    data();
}

void arm()
{
    std::call_once(armed, [] {
        const SwoftyEvents *ev = ::runtime::events_table();
        uint32_t token = 0;
        if (!ev || !ev->on_frame) {
            swofty::log("[hud-slots] no flow tick: the placement stays the default box");
            return;
        }
        ::runtime::frame_probe_name("hud_slots");
        ev->on_frame(frame, nullptr, &token);
    });
}

uint32_t pick(const Reader<Mem> &r, uintptr_t d, uintptr_t ctrl, const std::string &k)
{
    uint32_t first_found = 0;
    for (const char *l : kLayouts) {
        uint32_t ek = djb2(std::string(l) + k);
        if (ctrl && r.layout_has(d, ctrl, ek))
            return ek;
        if (!first_found && r.entry(d, ek).found)
            first_found = ek;
    }
    return first_found;
}

}
}

extern "C" __declspec(dllexport) int SwoftyCoreHudSlotRead(const char *slot, const void *caller, SwoftyHudSlotState *out)
{
    namespace s = hud::slots;
    if (!out || out->size < sizeof(SwoftyHudSlotState))
        return 2;
    std::memset(reinterpret_cast<char *>(out) + sizeof(uint32_t), 0, sizeof(SwoftyHudSlotState) - sizeof(uint32_t));
    out->scale = 1;
    out->opacity = 1;
    out->enabled = 1;
    std::string dll;
    std::string addon = settings_addon(caller, dll);
    std::string k = slot && *slot && !addon.empty() ? s::key(addon, slot) : std::string();
    std::snprintf(out->key, sizeof out->key, "%s", k.c_str());
    s::arm();
    uintptr_t d = s::data();
    if (!d)
        return 1;
    s::Mem m;
    s::Reader<s::Mem> r{m};
    uintptr_t editor = r.editor(d);
    out->editor_open = editor != 0;
    if (k.empty())
        return 1;
    uintptr_t ctrl = r.any_controller(d);
    uint32_t ek = s::pick(r, d, ctrl, k);
    if (!ek)
        return 1;
    s::Entry e = r.entry(d, ek);
    if (!e.found)
        return 1;
    out->found = 1;
    out->element_key = ek;
    out->x = e.x;
    out->y = e.y;
    out->scale = e.scale;
    out->opacity = e.opacity;
    out->enabled = e.enabled;
    if (editor) {
        s::Box b = r.box(editor, ek);
        if (b.valid) {
            out->has_box = 1;
            out->box[0] = b.x0;
            out->box[1] = b.y0;
            out->box[2] = b.x1;
            out->box[3] = b.y1;
        }
    }
    return 0;
}

extern "C" __declspec(dllexport) uint32_t SwoftyCoreHudSlotVersion(void) { return 1; }
