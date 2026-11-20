#include "core/frontend/view_state.hpp"

#include "core/runtime/build.hpp"
#include "core/runtime/hook_errors.hpp"
#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <mutex>
#include <string>

#include "core/career/career_state.hpp"
#include "core/frontend/pause_edge.hpp"
#include "core/frontend/viewport_camera_logic.hpp"
#include "core/frontend/view_flow_logic.hpp"
#include "core/frontend/view_camera_catalog.hpp"

namespace frontend::view {

namespace {

using frontend::view_flow::G_CUTSCENE;
using frontend::view_flow::G_DRIVING;
using frontend::view_flow::G_PAUSE;
using frontend::view_flow::G_REPLAY;
using frontend::view_flow::G_SESSION;
using frontend::view_flow::G_SPECTATE;
using frontend::view_flow::GROUP_MASK_ALL;
using frontend::view_flow::TRACKED;
using frontend::view_flow::TRACKED_N;

const runtime::Sig S_PHOTO_ON{
    "site_Game_PhotoModeActiveSet", "C5 F8 29 85 80 03 00 00 C6 85 94 03 00 00 01", 0x2bdba77, runtime::Sig::addr, 0, 8};
const runtime::Sig S_PHOTO_OFF{
    "site_Game_PhotoModeActiveClear", "C6 86 25 04 00 00 00 C6 86 94 03 00 00 00", 0x2bdc327, runtime::Sig::addr, 0, 7};
const runtime::Sig S_CAMERA{"site_Game_UseCameraSettingCall", "48 89 F1 B2 0F E8 ?? ?? ?? ?? 90 48 83 C4 70", 0x2424bbd};
const runtime::Sig S_PAUSE{"Game_App_SetPauseReason", "41 57 41 56 41 54 56 57 53 48 83 EC 28 89 D3 48 89 CE 84 D2", 0x80e110};

// Game_Camera_SetForViewport writes the selected type from AL and the viewport index is in EDI.
const runtime::Sig S_VIEWPORT_CAMERA{"site_Game_ViewportCameraTypeStore",
                                     "42 88 44 ED 20 48 8B 4E 30 4D 8D 7E 18 4C 89 FA 41 89 F8", 0x45394e2};
constexpr runtime::build_proof::Proof VIEWPORT_CAMERA_LAYOUT{
    "the primary viewport camera type", runtime::build_proof::bit(runtime::build_proof::Build::steam_1_0_145)};
constexpr runtime::build_proof::Gate VIEWPORT_CAMERA_GATE{"active viewport camera", &VIEWPORT_CAMERA_LAYOUT};

const char *const CAMERA_NAMES[8] = {"unknown", "nose_cam",      "nose_cam_alt", "cockpit_cam",
                                     "tv_pod",  "tv_pod_offset", "near_chase",   "far_chase"};

constexpr uint64_t pack(uint32_t kind, uint32_t flags, bool valid, uint32_t serial)
{
    return uint64_t(kind & 0xff) | uint64_t(flags & 0xff) << 8 | uint64_t(valid ? 1 : 0) << 16 | uint64_t(serial) << 32;
}

frontend::view::State unpack(uint64_t p)
{
    frontend::view::State s;
    s.kind = uint32_t(p & 0xff);
    s.flags = uint32_t(p >> 8 & 0xff);
    s.valid = (p >> 16 & 1) != 0;
    s.serial = uint32_t(p >> 32);
    return s;
}

class ViewState {
public:
    frontend::view::State current() const { return unpack(packed.load(std::memory_order_acquire)); }
    uint32_t pause_reasons() const { return pause_mask.load(std::memory_order_relaxed); }
    void flow(uint32_t hash, uint32_t to);
    void setup();
    void photo_on();
    void photo_off();
    void camera(safetyhook::Context &ctx);
    void viewport_camera(safetyhook::Context &ctx);
    void load_trackside_cameras(std::string_view track);
    void pause(safetyhook::Context &ctx);

private:
    uint32_t groups_active();
    void recompute();
    void recompute_locked();
    Kind classify_fallback_camera(const safetyhook::Context &ctx) const;

    safetyhook::MidHook h_photo_on, h_photo_off, h_camera_setting, h_pause, h_viewport_camera;
    std::atomic<uint64_t> packed{0};
    std::atomic<uint64_t> active{0};
    std::atomic<int> camera_value{-1};
    std::atomic<bool> photo{false};
    std::atomic<bool> flow_seen{false};
    std::atomic<uint32_t> pause_mask{0};
    std::mutex change_lock;
    ViewportCamera assigned_camera;
    std::vector<CameraUid> trackside_camera_uids;
    bool trust_camera = false;
    std::atomic<int> pause_logged{0};
};

// Not constinit: safetyhook's hooks hold a std::recursive_mutex, whose constructor is not constexpr.
ViewState view_state;

uint32_t ViewState::groups_active()
{
    uint64_t bits = active.load(std::memory_order_relaxed);
    uint32_t out = 0;
    for (size_t i = 0; i < TRACKED_N; i++)
        if (bits >> i & 1)
            out |= TRACKED[i].group;
    return out & GROUP_MASK_ALL;
}

uint32_t camera_kind(int value)
{
    switch (value) {
    case 3:
        return frontend::view::COCKPIT;
    case 4:
        return frontend::view::TCAM;
    case 5:
        return frontend::view::TCAM_OFFSET;
    case 6:
        return frontend::view::CHASE_NEAR;
    case 7:
        return frontend::view::CHASE_FAR;
    case 1:
    case 2:
        return frontend::view::OTHER;
    default:
        return frontend::view::UNKNOWN;
    }
}

std::string flag_text(uint32_t flags)
{
    static const struct {
        uint32_t bit;
        const char *name;
    } NAMES[] = {{frontend::view::PAUSED, "paused"},     {frontend::view::REPLAY, "replay"},
                 {frontend::view::PHOTO, "photo"},       {frontend::view::MENU, "menu"},
                 {frontend::view::CUTSCENE, "cutscene"}, {frontend::view::SPECTATING, "spectating"}};
    std::string out;
    for (auto &n : NAMES)
        if (flags & n.bit)
            out += (out.empty() ? "" : "|") + std::string(n.name);
    return out.empty() ? "none" : out;
}

const char *kind_text(uint32_t kind)
{
    static constexpr const char *NAMES[] = {"unknown",   "cockpit", "tcam",      "tcam_offset", "chase_near",
                                            "chase_far", "other",   "trackside", "reverse_pod"};
    return kind < std::size(NAMES) ? NAMES[kind] : "unknown";
}

void ViewState::recompute()
{
    std::lock_guard lock(change_lock);
    recompute_locked();
}

void ViewState::recompute_locked()
{
    uint32_t g = groups_active();
    bool in_session = g & G_SESSION;
    bool driving = g & G_DRIVING;
    bool paused = (g & G_PAUSE) && in_session;
    bool replay = g & G_REPLAY;
    assigned_camera.update_phase(in_session, replay);
    bool cutscene = g & G_CUTSCENE;
    bool spectating = g & G_SPECTATE;
    bool is_photo = photo.load(std::memory_order_relaxed);

    uint32_t flags = 0;
    if (paused)
        flags |= frontend::view::PAUSED;
    if (replay)
        flags |= frontend::view::REPLAY;
    if (is_photo)
        flags |= frontend::view::PHOTO;
    if (cutscene)
        flags |= frontend::view::CUTSCENE;
    if (spectating)
        flags |= frontend::view::SPECTATING;
    bool player_view = in_session && (driving || paused);
    if (!in_session || !(player_view || replay || cutscene || spectating || is_photo))
        flags |= frontend::view::MENU;

    uint32_t kind = frontend::view::UNKNOWN;
    bool valid = flow_seen.load(std::memory_order_relaxed);
    if (flags & frontend::view::MENU) {
        // MENU comes from no recognised session state, which is also what an unrecognised one looks like.
        kind = frontend::view::UNKNOWN;
        valid = false;
    } else if (is_photo || cutscene || spectating) {
        kind = frontend::view::OTHER;
    } else if (replay) {
        kind = assigned_camera.kind();
    } else {
        if (h_viewport_camera)
            kind = assigned_camera.kind();
        else if (trust_camera)
            kind = camera_kind(camera_value.load(std::memory_order_relaxed));
        if (kind == frontend::view::UNKNOWN)
            valid = false;
    }

    frontend::view::State old = unpack(packed.load(std::memory_order_relaxed));
    if (old.kind == kind && old.flags == flags && old.valid == valid && old.serial)
        return;
    uint32_t serial = old.serial + 1;
    packed.store(pack(kind, flags, valid, serial), std::memory_order_release);
    swofty::log("[view] kind=%s flags=%s valid=%d serial=%u", kind_text(kind), flag_text(flags).c_str(), valid ? 1 : 0, serial);
}

void ViewState::photo_on()
{
    photo.store(true, std::memory_order_relaxed);
    recompute();
}

void ViewState::photo_off()
{
    photo.store(false, std::memory_order_relaxed);
    recompute();
}

void ViewState::camera(safetyhook::Context &ctx)
{
    int value = int(uint32_t(ctx.r8));
    bool secondary = (ctx.r9 & 0xff) != 0;
    if (secondary)
        return;
    if (value < 0 || value > 7)
        value = 0;
    int was = camera_value.exchange(value, std::memory_order_relaxed);
    if (was != value)
        swofty::log("[view] camera setting %s (%d)%s", CAMERA_NAMES[value], value,
                    trust_camera ? "" : "; static only, not used for kind");
    recompute();
}

Kind ViewState::classify_fallback_camera(const safetyhook::Context &ctx) const
{
    // The same verified viewport record holds the selected object and definition before the type write.
    const uintptr_t viewport = ctx.rbp + ctx.r13 * 8;
    uintptr_t camera = 0;
    if (!swofty::mem::read(viewport, camera) || !camera || camera != ctx.r14)
        return OTHER;

    // The game's UID accessor copies the 16 bytes at camera +0x70 on this build.
    uintptr_t uid_vtable = 0, getter = 0;
    CameraUid uid{};
    const bool circuit_camera = !trackside_camera_uids.empty() && swofty::mem::read(camera + 0x18, uid_vtable) && uid_vtable &&
                                swofty::mem::read(uid_vtable + 0x20, getter) &&
                                getter == swofty::api->module_base() + 0x44cc570 && swofty::mem::read(camera + 0x70, uid) &&
                                std::binary_search(trackside_camera_uids.begin(), trackside_camera_uids.end(), uid);

    if (circuit_camera)
        return TRACKSIDE;

    uintptr_t definition = 0, name = 0;
    std::string label;
    if (swofty::mem::read(viewport + 0x18, definition) && definition && swofty::mem::read(definition + 0x28, name) &&
        swofty::mem::c_string(name, label, sizeof("podoff_reverse")) && is_reverse_pod(label))
        return REVERSE_POD;
    return OTHER;
}

void ViewState::load_trackside_cameras(std::string_view track)
{
    if (!runtime::holds(VIEWPORT_CAMERA_GATE))
        return;
    auto uids = circuit_camera_uids(runtime::game_root(), track);
    const auto count = uids.size();
    std::lock_guard lock(change_lock);
    trackside_camera_uids = std::move(uids);
    swofty::log("[view] loaded %zu trackside camera UIDs for %.*s", count, int(track.size()), track.data());
}

void ViewState::viewport_camera(safetyhook::Context &ctx)
{
    const auto viewport = uint32_t(ctx.rdi);
    if (viewport != 0)
        return;
    std::lock_guard lock(change_lock);
    if (!(groups_active() & G_SESSION))
        return;
    const auto type = uint8_t(ctx.rax);
    const Kind fallback = type == 8 ? classify_fallback_camera(ctx) : OTHER;
    if (assigned_camera.observe(viewport, type, fallback))
        recompute_locked();
}

void ViewState::pause(safetyhook::Context &ctx)
{
    bool set = (ctx.rdx & 0xff) != 0;
    int bit = int(uint32_t(ctx.r8));
    if (bit < 0 || bit >= 32)
        return;
    uint32_t before = pause_mask.load(std::memory_order_relaxed);
    uint32_t mask = set ? (before | 1u << bit) : (before & ~(1u << bit));
    pause_mask.store(mask, std::memory_order_relaxed);
    if (pause_logged.fetch_add(1, std::memory_order_relaxed) < 64)
        swofty::log("[view] pause reason bit %d %s, mask %08x", bit, set ? "set" : "cleared", mask);
    switch (frontend::view::pause_edge(before, mask)) {
    case frontend::view::PauseEdge::paused:
        runtime::events_fire(SWOFTY_EVENT_PAUSE, mask, bit, nullptr);
        break;
    case frontend::view::PauseEdge::resumed:
        runtime::events_fire(SWOFTY_EVENT_RESUME, 0, bit, nullptr);
        break;
    case frontend::view::PauseEdge::none:
        break;
    }
}

safetyhook::MidHook install(const runtime::Sig &sig, safetyhook::MidHookFn fn)
{
    uintptr_t at = runtime::find(sig, "view state", "view");
    if (!at)
        return {};
    auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), fn);
    if (!h) {
        swofty::log("[view] %s at base+0x%llx: the hook failed: %s", sig.what,
                    (unsigned long long)(at - swofty::api->module_base()), runtime::hook_error(h.error()).c_str());
        return {};
    }
    return std::move(*h);
}

void ViewState::flow(uint32_t hash, uint32_t to)
{
    std::lock_guard lock(change_lock);
    for (size_t i = 0; i < TRACKED_N; i++) {
        if (TRACKED[i].hash != hash)
            continue;
        uint64_t bit = uint64_t(1) << i;
        if (to == 1 || to == 2)
            active.fetch_or(bit, std::memory_order_relaxed);
        else if (to == 0 || to == 3)
            active.fetch_and(~bit, std::memory_order_relaxed);
        if (TRACKED[i].group == G_SESSION && (to == 0 || to == 3))
            trackside_camera_uids.clear();
        if (TRACKED[i].group == G_REPLAY && (to == 0 || to == 3) && photo.exchange(false))
            swofty::log("[view] photo mode cleared by the replay state leaving");
        break;
    }
    flow_seen.store(true, std::memory_order_relaxed);
    recompute_locked();
}

void ViewState::setup()
{
    char trust[8] = {};
    trust_camera = GetEnvironmentVariableA("SWOFTY_VIEW_TRUST_CAMERA", trust, sizeof trust) && trust[0] == '1';
    h_photo_on = install(S_PHOTO_ON, [](safetyhook::Context &) { view_state.photo_on(); });
    h_photo_off = install(S_PHOTO_OFF, [](safetyhook::Context &) { view_state.photo_off(); });
    h_camera_setting = install(S_CAMERA, [](safetyhook::Context &ctx) { view_state.camera(ctx); });
    h_pause = install(S_PAUSE, [](safetyhook::Context &ctx) { view_state.pause(ctx); });
    if (runtime::holds(VIEWPORT_CAMERA_GATE)) {
        auto hook = install(S_VIEWPORT_CAMERA, [](safetyhook::Context &ctx) { view_state.viewport_camera(ctx); });
        std::lock_guard lock(change_lock);
        h_viewport_camera = std::move(hook);
    }
    swofty::log("[view] setup: photo %s/%s, camera setting %s%s, pause reasons %s; flow states come from the career flow "
                "site",
                h_photo_on ? "armed" : "off", h_photo_off ? "armed" : "off", h_camera_setting ? "armed" : "off",
                trust_camera ? " (settings fallback enabled)" : " (diagnostic only)", h_pause ? "armed" : "off");
    swofty::log("[view] active viewport camera hook %s", h_viewport_camera ? "armed" : "off");
    recompute();
}

}

State current() { return view_state.current(); }

uint32_t pause_reasons() { return view_state.pause_reasons(); }

void flow(uint32_t hash, uint32_t to) { view_state.flow(hash, to); }

void setup() { view_state.setup(); }

void load_trackside_cameras(std::string_view track) { view_state.load_trackside_cameras(track); }

}

extern "C" __declspec(dllexport) uint32_t SwoftyCorePauseReasons(void) { return frontend::view::pause_reasons(); }
