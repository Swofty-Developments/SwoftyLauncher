#include "runtime.hpp"

#include "main_menu.hpp"
#include "main_menu_logic.hpp"
#include <safetyhook.hpp>
#include "virtual_pad.hpp"
#include "json.hpp"

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <atomic>
#include <map>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <mutex>
#include <utility>
#include <string>

namespace runtime {
std::string hook_error(const safetyhook::InlineHook::Error &e);
int overlay_image(const std::string &id, const std::string &path, uint32_t *width, uint32_t *height);
}

bool ui_vm_set_path(const std::string &path, int type, const void *value);

// The main menu in rows. FeTile_Build_List's home case adds the stock tiles in order: Braking Point, Career,
// F1 The Movie, F1 World, the challenge tile, a divider, Local Multiplayer, League Racing, Player Hub, Theatre.
// site_FeHome_AfterCareerCall (swofty_core.cpp) runs right after the Career tile and calls add_tiles, which adds
// Grand Prix and Time Trial as two copies of the F1 World item (their own id, title and description over the
// stock populate), the challenge tile when the game builds it and Theatre; swofty_core then adds Addon
// Settings last. arm_patch overwrites the first instruction of the F1 World block (lea r15d,[r12+1]) with a jmp to
// the block's final jmp, so F1 World, the divider, Local Multiplayer, League Racing, Player Hub and the stock
// Theatre are never added. The block is matched byte for byte first and the adders are the call targets read
// from it; when anything does not match nothing is patched, no tile is added and the stock menu stays.
//
// The status strip, the sign-in state and the latest video card are painted by the overlay frame
// (paint) from swofty/home_menu.json while the home screen is open. Images (the Discord avatar and the video
// thumbnail) are decoded by the overlay's image loader and drawn as overlay images.
//
// SWOFTY_MAIN_MENU=0 turns the whole feature off. SWOFTY_MAIN_MENU_NAV=single|rows picks the navigation
// layout; only single (one spawner, tile order) exists so far, and rows is logged as not built.

namespace runtime::main_menu {

namespace {

using namespace swofty::game;
namespace logic = ::runtime::main_menu;

constexpr size_t OFF_F1WORLD = 11, OFF_CHALLENGE = 32, OFF_DIVIDER = 47, OFF_THEATRE = 98, OFF_END_JMP = 103;
constexpr size_t NESTR_CHARS = 9;

const runtime::Sig S_BLOCK{"FeHome_F1WorldBlock",
                           "45 8D 7C 24 01 4C 89 F1 44 89 FA E8 ?? ?? ?? ?? F6 46 08 01 74 12 41 80 C4 02 4C 89 F1 44 "
                           "89 E2 E8 ?? ?? ?? ?? 45 89 E7 41 8D 57 01 4C 89 F1 E8 ?? ?? ?? ?? 41 8D 57 02 4C 89 F1 E8 "
                           "?? ?? ?? ?? 41 8D 57 03 4C 89 F1 E8 ?? ?? ?? ?? 41 8D 57 04 4C 89 F1 E8 ?? ?? ?? ?? 41 80 "
                           "C7 05 4C 89 F1 44 89 FA E8 ?? ?? ?? ?? E9",
                           0x1742fee, runtime::Sig::addr, 0, 0, 0};

using Add_t = void (*)(TileList *, uint8_t);
using Populate_t = void (*)(HomeItem *, void *, uint32_t);
using GetAction_t = Action *(*)(HomeItem *, Action *);

struct Block {
    uint8_t *at = nullptr;
    Add_t f1world = nullptr, challenge = nullptr, divider = nullptr, theatre = nullptr;
} block;

std::atomic<bool> resolved{false}, patched{false}, home_open{false};
std::once_flag once;
void *f1w_populate = nullptr, *f1w_action = nullptr;
std::atomic<bool> tt_warned{false};
std::atomic<int> rows_flag{-1}, menu_flag{-1};
std::atomic<int> via{0};
std::atomic<uint64_t> swap_at{0}, home_opened_at{0};
std::mutex swap_lock;
std::string swap_screens;
bool swap_reported = true;
std::atomic<bool> state_loaded{false}, panel_disabled{false}, panel_prepared{false}, bind_pad{false}, overlay_drawn{false};

struct Prepared {
    logic::State s;
    int avatar_slot = -1, video_slot = -1;
    uint32_t video_w = 0, video_h = 0;
    std::string version, discord, watch;
};
std::mutex prepared_lock;
std::shared_ptr<const Prepared> prepared;
uint64_t prepared_at = 0;
std::string prepared_key;
std::atomic<int> strip_wanted{-1}, strip_set{-1};
void *theatre_populate = nullptr, *theatre_action = nullptr;
std::mutex state_lock;
logic::State state;
uint64_t state_read_at = 0;

const char *env(const char *name)
{
    const char *v = std::getenv(name);
    return v ? v : "";
}

Add_t target(const uint8_t *at, size_t off)
{
    int32_t rel;
    std::memcpy(&rel, at + off + 1, 4);
    return reinterpret_cast<Add_t>(const_cast<uint8_t *>(at) + off + 5 + rel);
}

bool resolve()
{
    std::call_once(once, [] {
        if (!std::strcmp(env("SWOFTY_MAIN_MENU"), "0")) {
            swofty::log("[main-menu] off (SWOFTY_MAIN_MENU=0); the stock main menu stays");
            return;
        }
        const char *nav = env("SWOFTY_MAIN_MENU_NAV");
        swofty::log("[main-menu] navigation layout: %s%s", *nav ? nav : "single",
                    !std::strcmp(nav, "rows") ? " (rows is not built yet; single is used)" : "");
        auto at = reinterpret_cast<uint8_t *>(runtime::find(S_BLOCK, "home", "core"));
        if (!at) {
            swofty::log("[main-menu] the home tile block did not resolve on this build; the stock main menu stays");
            return;
        }
        if (at[OFF_END_JMP] != 0xE9) {
            swofty::log("[main-menu] the home tile block has no closing jmp; the stock main menu stays");
            return;
        }
        block.at = at;
        block.f1world = target(at, OFF_F1WORLD);
        block.challenge = target(at, OFF_CHALLENGE);
        block.divider = target(at, OFF_DIVIDER);
        block.theatre = target(at, OFF_THEATRE);
        swofty::log("[main-menu] home tile block at %p; adders F1 World %p, challenge %p, Theatre %p", at,
                    reinterpret_cast<void *>(block.f1world), reinterpret_cast<void *>(block.challenge),
                    reinterpret_cast<void *>(block.theatre));
        resolved = true;
    });
    return resolved.load();
}

HomeItem *last_item(TileList *list)
{
    auto begin = reinterpret_cast<uintptr_t>(list->items_begin), end = reinterpret_cast<uintptr_t>(list->items_end);
    if (!begin || end <= begin || (end - begin) % sizeof(TileEntry))
        return nullptr;
    Tile *tile = list->items_end[-1].tile;
    return tile ? tile->item : nullptr;
}

void set_string(void *reg, void *parent, const char *name, const char *text)
{
    NeStr *s = nullptr;
    Ne_Make_StrFromCStr(&s, text, 0, 0);
    Vm_Set_String(reg, parent, name, &s, 1);
    Ne_Make_StrFromCStr(&s, nullptr, 0, 0);
}

const char *id_of(const HomeItem *item)
{
    return item && item->id ? reinterpret_cast<const char *>(item->id) + NESTR_CHARS : "";
}

bool rows_on()
{
    int f = rows_flag.load();
    if (f < 0) {
        f = runtime::read_file(runtime::under_swofty("ui\\home_rows.flag"), 16).empty() ? 0 : 1;
        rows_flag = f;
        swofty::log("[main-menu] the launcher's rows of cards %s", f ? "are in this build" : "are not in this build; the tiles stay in the stock list");
    }
    return f == 1;
}

bool menu_on()
{
    int f = menu_flag.load();
    if (f < 0) {
        f = runtime::read_file(runtime::under_swofty("ui\\home_menu.flag"), 16).empty() ? 0 : 1;
        menu_flag = f;
        swofty::log("[main-menu] the launcher's Showroom route %s", f ? "is in this build" : "is not in this build; no Showroom card");
    }
    return f == 1;
}

void style_item(HomeItem *item)
{
    const char *tpl = logic::template_for(id_of(item), rows_on());
    auto node = item ? static_cast<uint8_t *>(item->button_node.node) : nullptr;
    if (!*tpl)
        return;
    if (!node) {
        swofty::log("[main-menu] %s: no VM node yet, template %s not set", id_of(item), tpl);
        return;
    }
    set_string(*g_Vm_Registry, node + 8, "type", tpl);
    swofty::log("[main-menu] %s -> template %s", id_of(item), tpl);
}

std::map<void **, void *> typed_orig;
std::mutex typed_lock;

void typed_populate(HomeItem *item, void *node, uint32_t a7)
{
    void *orig = nullptr;
    {
        std::lock_guard<std::mutex> g(typed_lock);
        auto it = typed_orig.find(reinterpret_cast<void **>(item->vtable));
        orig = it == typed_orig.end() ? nullptr : it->second;
    }
    if (orig)
        reinterpret_cast<Populate_t>(orig)(item, node, a7);
    style_item(item);
}

void wrap_item(HomeItem *item)
{
    const char *id = id_of(item);
    if (!*logic::template_for(id, rows_on()))
        return;
    if (!std::strcmp(id, logic::ID_GRAND_PRIX) || !std::strcmp(id, logic::ID_TIME_TRIAL) ||
        !std::strcmp(id, logic::ID_SHOWROOM) || !std::strcmp(id, logic::ID_SETTINGS))
        return;
    auto vt = reinterpret_cast<void **>(item->vtable);
    {
        std::lock_guard<std::mutex> g(typed_lock);
        if (typed_orig.count(vt))
            return;
    }
    void *orig = vt[4];
    void **nv = swofty::subclass(item, {{4, reinterpret_cast<void *>(typed_populate)}});
    std::lock_guard<std::mutex> g(typed_lock);
    typed_orig[nv] = orig;
    swofty::log("[main-menu] %s: populate wrapped to set its template", id);
}

void style_list(TileList *list)
{
    for (auto e = list->items_begin; e && e < list->items_end; e++)
        if (e->tile && e->tile->item)
            wrap_item(e->tile->item);
}

void showroom_populate(HomeItem *item, void *node, uint32_t a7)
{
    reinterpret_cast<Populate_t>(theatre_populate)(item, node, a7);
    auto vm_node = static_cast<uint8_t *>(item->button_node.node);
    if (!vm_node)
        return;
    void *reg = *g_Vm_Registry, *parent = vm_node + 8;
    uint8_t no = 0;
    set_string(reg, parent, "title", "SHOWROOM");
    Vm_Set_Bool(reg, parent, "title_localised", &no, 1);
    set_string(reg, parent, "description", "Look over every car in the game");
    Vm_Set_Bool(reg, parent, "description_localised", &no, 1);
    style_item(item);
}

Action *showroom_action(HomeItem *item, Action *out)
{
    out->type = 1;
    out->str = nullptr;
    Ne_Make_StrFromCStr(&out->str, logic::SHOWROOM_ROUTE, 0, 0);
    via = 2;
    swofty::log("[main-menu] showroom card pressed (item %p): route %s", item, logic::SHOWROOM_ROUTE);
    return out;
}

void add_showroom(TileList *list, uint8_t index)
{
    block.theatre(list, index);
    HomeItem *item = last_item(list);
    if (!item)
        return;
    auto vt = reinterpret_cast<void **>(item->vtable);
    if (!theatre_populate) {
        theatre_populate = vt[4];
        theatre_action = vt[5];
    }
    if (vt[4] != theatre_populate) {
        swofty::log("[main-menu] showroom: the Theatre item's vtable changed; left as a Theatre tile");
        return;
    }
    swofty::subclass(item, {{4, reinterpret_cast<void *>(showroom_populate)}, {5, reinterpret_cast<void *>(showroom_action)}});
    Ne_Make_StrFromCStr(&item->id, logic::ID_SHOWROOM, 0, 0);
}

enum class Step { idle, wait_container, done };
struct Chain {
    Step step = Step::idle;
    bool time_trial = false;
    uint64_t started = 0, container_at = 0, release_at = 0;
    bool use_pad = false;
} chain;
std::mutex chain_lock;

const char *route_name()
{
    const char *r = env("SWOFTY_MAIN_MENU_ROUTE");
    return *r ? r : "home_to_activity";
}

const runtime::Sig S_CONTAINER{
    "Game_F1WorldActivityContainer_HandleCommand",
    "41 57 41 56 41 55 41 54 56 57 53 48 81 EC B0 00 00 00 48 8B 02 48 85 C0 0F 84 ?? ?? ?? ?? 4C 89 C7 49 89 D0 48 89 CE "
    "8B 00 3D 37 3B FC FC 0F 8E ?? ?? ?? ?? 3D BD FC E1 41",
    0x1a77d60};
safetyhook::InlineHook h_container;
std::atomic<void *> container_vm{nullptr}, container_third{nullptr};

uint64_t container_detour(void *container, NeStr **message, void *third)
{
    if (container_vm.load() != container) {
        container_vm = container;
        swofty::log("[main-menu] chain: activity container view model %p seen", container);
    }
    container_third = third;
    return h_container.call<uint64_t>(container, message, third);
}

const runtime::Sig S_FIRE{"Vm_Fire_Route", "48 8B 4E 28 48 89 F2 49 89 F8 45 31 C9 E8 ?? ?? ?? ?? E9 E4 00 00 00",
                          0x42c0e20, runtime::Sig::rel32, 13, 14, 5};
safetyhook::InlineHook h_fire;

std::atomic<void *> hub_router{nullptr};
std::atomic<uint64_t> hub_at{0}, hub_update_at{0}, hub_fired_at{0};
std::atomic<void *> hub_vm{nullptr};
std::mutex view_lock;
logic::HomeView home_view;

const runtime::Sig S_HUB_UPDATE{
    "Game_F1WorldScreenVM_Update",
    "56 57 55 53 48 83 EC 78 48 89 CE 80 B9 D1 16 00 00 00 74 0D 48 8B 86 30 07 00 00 80 78 2C 00 74 2E 48 8B 86 E0 15 00 "
    "00 48 85 C0 0F 84 ?? ?? ?? ?? 80 B8 A1 00 00 00 00 0F 84",
    0x1b1a2f0};
safetyhook::InlineHook h_hub;

uint64_t hub_detour(void *vm)
{
    hub_vm = vm;
    hub_update_at = GetTickCount64();
    return h_hub.call<uint64_t>(vm);
}

bool hub_recent()
{
    uint64_t at = hub_update_at.load();
    return at && GetTickCount64() - at < 300;
}

void hook_hub()
{
    if (h_hub)
        return;
    auto at = runtime::find(S_HUB_UPDATE, "home", "core");
    if (!at) {
        swofty::log("[main-menu] back: the F1 World hub update did not resolve; its view model cannot be captured");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(hub_detour));
    if (!h) {
        swofty::log("[main-menu] back: hooking the F1 World hub update failed: %s", runtime::hook_error(h.error()).c_str());
        return;
    }
    h_hub = std::move(*h);
    swofty::log("[main-menu] back: F1 World hub update hooked at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
}

void fire_detour(void *router, void *vm, NeStr **name, uint64_t a4)
{
    const char *text = name && *name ? reinterpret_cast<const char *>(*name) + NESTR_CHARS : "";
    if (home_open.load() && runtime::readable(static_cast<uint8_t *>(vm) + 0x250, 4) &&
        *reinterpret_cast<uint32_t *>(static_cast<uint8_t *>(vm) + 0x250) == logic::djb2("fe_home") && std::strncmp(text, "swofty_", 7) != 0) {
        std::lock_guard<std::mutex> g(view_lock);
        home_view.left();
        home_open = false;
        swofty::log("[main-menu] panel hidden: route %s fired from the home screen", text);
    }
    int mode = via.load();
    if (mode) {
        uint32_t gid = runtime::readable(static_cast<uint8_t *>(vm) + 0x250, 4) ? *reinterpret_cast<uint32_t *>(static_cast<uint8_t *>(vm) + 0x250) : 0;
        swofty::log("[main-menu] route fired while back-to-menu is armed (%s): %s (route group id of the firing view model 0x%08x; djb2 of f1_world_activity_container 0x%08x, of fe_f1_world 0x%08x)",
                    mode == 1 ? "card" : "showroom", text, gid, logic::djb2("f1_world_activity_container"), logic::djb2("fe_f1_world"));
        if (!hub_router.load())
            hub_router = router;
        if (uint64_t at = swap_at.load(); at && GetTickCount64() - at < 5000)
            swofty::log("[main-menu] route %s fired %llu ms after the back step (vm %p)", text, (unsigned long long)(GetTickCount64() - at), vm);
    }
    h_fire.call<void>(router, vm, name, a4);
}

void hook_fire()
{
    if (h_fire)
        return;
    auto at = runtime::find(S_FIRE, "home", "core");
    if (!at) {
        swofty::log("[main-menu] back: Vm_Fire_Route did not resolve; Back from the pickers and the Showroom stays stock");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(fire_detour));
    if (!h) {
        swofty::log("[main-menu] back: hooking Vm_Fire_Route failed: %s", runtime::hook_error(h.error()).c_str());
        return;
    }
    h_fire = std::move(*h);
    swofty::log("[main-menu] back: Vm_Fire_Route hooked at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
}

void hook_container()
{
    if (h_container)
        return;
    auto at = runtime::find(S_CONTAINER, "home", "core");
    if (!at) {
        swofty::log("[main-menu] chain: the activity container command handler did not resolve; the cards open the activity container only");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(container_detour));
    if (!h) {
        swofty::log("[main-menu] chain: hooking the activity container command handler failed: %s",
                    runtime::hook_error(h.error()).c_str());
        return;
    }
    h_container = std::move(*h);
    swofty::log("[main-menu] chain: activity container command handler hooked at base+0x%llx",
                (unsigned long long)(at - swofty::api->module_base()));
}

void card_populate(HomeItem *item, void *node, uint32_t a7)
{
    reinterpret_cast<Populate_t>(f1w_populate)(item, node, a7);
    auto vm_node = static_cast<uint8_t *>(item->button_node.node);
    if (!vm_node)
        return;
    bool gp = !std::strcmp(id_of(item), logic::ID_GRAND_PRIX);
    void *reg = *g_Vm_Registry, *parent = vm_node + 8;
    uint8_t no = 0;
    set_string(reg, parent, "title", gp ? "GRAND PRIX" : "TIME TRIAL");
    Vm_Set_Bool(reg, parent, "title_localised", &no, 1);
    set_string(reg, parent, "description",
               gp ? "Race a Grand Prix weekend" : "Set the fastest lap on any track");
    Vm_Set_Bool(reg, parent, "description_localised", &no, 1);
    style_item(item);
}

Action *card_action(HomeItem *item, Action *out)
{
    if (uint64_t at = home_opened_at.load(); at && GetTickCount64() - at < 700) {
        out->type = 0;
        out->str = nullptr;
        swofty::log("[main-menu] %s: press ignored %llu ms after fe_home opened (carried-over input)", id_of(item),
                    (unsigned long long)(GetTickCount64() - at));
        return out;
    }
    out->type = 1;
    out->str = nullptr;
    Ne_Make_StrFromCStr(&out->str, route_name(), 0, 0);
    via = 1;
    swofty::log("[main-menu] %s card pressed: route %s (the stock item returns its own id as the route, so the id is not used)",
                id_of(item), route_name());
    return out;
}

void add_card(TileList *list, uint8_t index, const char *id)
{
    block.f1world(list, index);
    HomeItem *item = last_item(list);
    if (!item) {
        swofty::log("[main-menu] %s: the adder left no item", id);
        return;
    }
    auto vt = reinterpret_cast<void **>(item->vtable);
    if (!f1w_populate) {
        f1w_populate = vt[4];
        f1w_action = vt[5];
    }
    if (vt[4] != f1w_populate || vt[5] != f1w_action) {
        swofty::log("[main-menu] %s: the item's vtable is not the F1 World one; left as a plain F1 World tile", id);
        return;
    }
    swofty::subclass(item, {{4, reinterpret_cast<void *>(card_populate)}, {5, reinterpret_cast<void *>(card_action)}});
    Ne_Make_StrFromCStr(&item->id, id, 0, 0);
}

void set_offline_text(long long online)
{
    static long long last = -1;
    std::string text = logic::offline_text(online);
    if (online == last)
        return;
    if (ui_vm_set_path("ui_menu_data.swofty_offline_text", 0, text.c_str())) {
        last = online;
        swofty::log("[main-menu] Launched with panel text set: %s", logic::online_line(online).empty() ? "no online line" : logic::online_line(online).c_str());
    } else {
        static int fails = 0;
        if (fails++ % 60 == 0)
            swofty::log("[main-menu] could not set ui_menu_data.swofty_offline_text yet (try %d)", fails);
    }
}

void read_state(bool force = false)
{
    uint64_t now = GetTickCount64();
    std::lock_guard<std::mutex> g(state_lock);
    if (!force && state_read_at && now - state_read_at < 1000)
        return;
    state_read_at = now;
    auto next = logic::parse_state(runtime::read_file(runtime::under_swofty("home_menu.json"), size_t(1) << 20));
    if (const char *override_path = std::getenv("SWOFTY_HOME_VIDEO_JSON"); override_path && *override_path) {
        json::Value v;
        if (json::parse(runtime::read_file(runtime::wide(override_path), size_t(1) << 20), v)) {
            next.loaded = true;
            next.video_title = v["title"].str();
            next.video_url = v["url"].str();
            next.video_thumbnail = v["thumbnail"].str();
        }
    }
    if (next.loaded != state.loaded || next.signed_in != state.signed_in || next.video_url != state.video_url)
        swofty::log("[main-menu] state: %s, %s, video %s", next.loaded ? logic::version_line(next).c_str() : "no file",
                    logic::discord_line(next).c_str(), logic::has_video(next) ? "yes" : "none");
    state = next;
    state_loaded = next.loaded;
    set_offline_text(next.online);
}

struct PadSample {
    bool connected = false;
    uint32_t packet = 0;
    uint16_t buttons = 0;
};

PadSample sample_pad()
{
    using Get_t = DWORD(WINAPI *)(DWORD, void *);
    static Get_t get = [] {
        HMODULE m = LoadLibraryA("xinput1_4.dll");
        if (!m)
            m = LoadLibraryA("xinput1_3.dll");
        return m ? reinterpret_cast<Get_t>(GetProcAddress(m, "XInputGetState")) : nullptr;
    }();
    uint8_t buf[16] = {};
    PadSample s;
    s.connected = get && get(0, buf) == 0;
    std::memcpy(&s.packet, buf, 4);
    std::memcpy(&s.buttons, buf + 4, 2);
    return s;
}

bool any_key_down()
{
    for (int vk = 8; vk < 255; vk++)
        if (GetAsyncKeyState(vk) & 0x8000)
            return true;
    return false;
}

bool bind_pressed(bool *pad)
{
    static bool was = false;
    static uint32_t last_packet = 0;
    static uint64_t pad_at = 0, key_at = 0;
    uint64_t now = GetTickCount64();
    PadSample p = sample_pad();
    if (p.connected && p.packet != last_packet)
        pad_at = now;
    last_packet = p.packet;
    bool key_active = GetForegroundWindow() != nullptr && any_key_down();
    if (key_active)
        key_at = now;
    static bool shown_pad = false;
    if (pad_at != key_at)
        shown_pad = p.connected && pad_at > key_at;
    *pad = shown_pad;
    bool y = p.connected && (p.buttons & logic::WATCH_PAD_MASK);
    bool key = (GetAsyncKeyState(logic::WATCH_KEY_VK) & 0x8000) != 0 && GetForegroundWindow() != nullptr;
    bool down = y || key;
    bool edge = down && !was;
    was = down;
    return edge;
}

void open_video(const logic::State &s)
{
    std::string body = logic::open_request_json(s.video_url);
    bool ok = runtime::write_file(runtime::under_swofty("open_url.json"), body);
    swofty::log("[main-menu] watch pressed: %s open_url.json for %s", ok ? "wrote" : "could not write", s.video_url.c_str());
}

std::string host_path(const std::string &p)
{
    if (!p.empty() && p[0] == '/') {
        std::string w = "Z:" + p;
        for (auto &c : w)
            if (c == '/')
                c = '\\';
        return w;
    }
    if (!p.empty() && p.find(':') == std::string::npos) {
        std::string rel = "swofty:" + p;
        for (auto &c : rel)
            if (c == '/')
                c = '\\';
        return rel;
    }
    return p;
}

void start_chain(bool time_trial)
{
    if (!std::strcmp(env("SWOFTY_MAIN_MENU_CHAIN"), "0")) {
        swofty::log("[main-menu] chain off (SWOFTY_MAIN_MENU_CHAIN=0); the card opens the activity container");
        return;
    }
    std::lock_guard<std::mutex> g(chain_lock);
    chain = {};
    chain.step = Step::wait_container;
    chain.time_trial = time_trial;
    chain.started = GetTickCount64();
    container_vm = nullptr;
    if (!time_trial && std::strcmp(env("SWOFTY_MAIN_MENU_GP"), "command") != 0) {
        runtime::pad::setup();
        chain.use_pad = runtime::pad::why().empty();
        if (!chain.use_pad)
            swofty::log("[main-menu] chain: the virtual pad is unavailable (%s); Grand Prix falls back to the grand_prix_new command",
                        runtime::pad::why().c_str());
    }
    swofty::log("[main-menu] chain: %s card pressed; route %s, then %s", time_trial ? "Time Trial" : "Grand Prix", route_name(),
                time_trial ? "the activity container's 'time_trial' command"
                           : (chain.use_pad ? "A on the Play tab's first tile through the virtual pad"
                                            : "the activity container's 'grand_prix_new' command"));
}

}

bool enabled()
{
    return resolve();
}

bool drawing()
{
    return logic::panel_due(patched.load(), home_open.load() && !runtime::in_session_now() && !hub_recent(), state_loaded.load(),
                            panel_disabled.load(), panel_prepared.load());
}

bool wants_overlay()
{
    if (!std::strcmp(env("SWOFTY_MAIN_MENU"), "0") || !std::strcmp(env("SWOFTY_MAIN_MENU_PANEL"), "0"))
        return false;
    return !runtime::read_file(runtime::under_swofty("home_menu.json"), size_t(1) << 20).empty();
}

bool arm_patch()
{
    if (!resolve())
        return false;
    if (patched.load())
        return true;
    uint8_t jmp[5] = {0xE9, uint8_t(OFF_END_JMP - 5), 0, 0, 0};
    DWORD old;
    if (!VirtualProtect(block.at, sizeof jmp, PAGE_EXECUTE_READWRITE, &old)) {
        swofty::log("[main-menu] could not make the home tile block writable; the stock main menu stays");
        resolved = false;
        return false;
    }
    std::memcpy(block.at, jmp, sizeof jmp);
    VirtualProtect(block.at, sizeof jmp, old, &old);
    FlushInstructionCache(GetCurrentProcess(), block.at, sizeof jmp);
    patched = true;
    hook_container();
    hook_fire();
    hook_hub();
    swofty::log("[main-menu] stock F1 World, divider, Local Multiplayer, League Racing, Player Hub and Theatre tiles skipped");
    return true;
}

uint8_t add_tiles(void *list_, uint8_t career_index, const uint8_t *flags)
{
    auto list = static_cast<TileList *>(list_);
    if (!arm_patch())
        return career_index;
    uint8_t index = career_index;
    add_card(list, ++index, logic::ID_GRAND_PRIX);
    add_card(list, ++index, logic::ID_TIME_TRIAL);
    if (flags && (flags[8] & 1))
        block.challenge(list, ++index);
    block.theatre(list, ++index);
    bool showroom = menu_on();
    if (showroom)
        add_showroom(list, ++index);
    block.divider(list, ++index);
    if (HomeItem *divider = last_item(list)) {
        Ne_Make_StrFromCStr(&divider->id, logic::ID_DIVIDER, 0, 0);
        swofty::log("[main-menu] divider tile added before Addon Settings (index %u)", unsigned(index));
    }
    style_list(list);
    swofty::log("[main-menu] tiles added after Career: Grand Prix, Time Trial%s, Theatre%s (last index %u)",
                flags && (flags[8] & 1) ? ", challenge" : "", showroom ? ", Showroom" : "", unsigned(index));
    return index;
}

void style_settings(void *item)
{
    if (rows_on())
        style_item(static_cast<HomeItem *>(item));
}

void prepare();
void poll_bind();

void tick()
{
    if (uint64_t at = hub_at.load(); at) {
        uint64_t now = GetTickCount64();
        if (!hub_router.load() && now - at > 1500) {
            hub_at = 0;
            swofty::log("[main-menu] back: no route router was captured before the hub opened; Back stays stock");
        } else if (hub_router.load() && now - at >= 350) {
            hub_at = 0;
            NeStr *route = nullptr;
            Ne_Make_StrFromCStr(&route, "back", 0, 0);
            {
                std::lock_guard<std::mutex> g(swap_lock);
                swap_screens.clear();
                swap_reported = false;
            }
            swap_at = now;
            hub_fired_at = now;
            static uint8_t scratch[0x400];
            *reinterpret_cast<uint32_t *>(scratch + 0x250) = logic::djb2("fe_f1_world");
            swofty::log("[main-menu] back: the hub opened %llu ms ago; firing route 'back' in group fe_f1_world (0x%08x), the route its own Back button fires",
                        (unsigned long long)(now - at), logic::djb2("fe_f1_world"));
            if (h_fire)
                h_fire.call<void>(hub_router.load(), scratch, &route, 0);
            Ne_Make_StrFromCStr(&route, nullptr, 0, 0);
        }
    }
    if (uint64_t at = hub_fired_at.load(); at && GetTickCount64() - at > 1500) {
        hub_fired_at = 0;
        uint64_t last = hub_update_at.load();
        swofty::log("[main-menu] back: 1.5 s after route back the hub update hook %s; fe_home panel %s",
                    last ? ("last fired " + std::to_string(GetTickCount64() - last) + " ms ago").c_str() : "never fired (it does not tick on this screen)",
                    drawing() ? "is due" : "is not due");
    }
    if (uint64_t at = swap_at.load(); at && GetTickCount64() - at > 1000) {
        std::lock_guard<std::mutex> g(swap_lock);
        if (!swap_reported) {
            swap_reported = true;
            swofty::log("[main-menu] back: 1 s after the route swap the screens opened since were: %s; fe_home %s",
                        swap_screens.empty() ? "(none)" : swap_screens.c_str(), home_open.load() ? "is the last front-end screen opened" : "is NOT the last front-end screen opened");
        }
    }
    if (home_open.load()) {
        read_state();
        if (patched.load() && !panel_disabled.load() && std::strcmp(env("SWOFTY_MAIN_MENU_PANEL"), "0") != 0) {
            prepare();
            poll_bind();
        }
    }
    int want = strip_wanted.load();
    if (want >= 0 && want != strip_set.load() && patched.load()) {
        int v = want;
        bool ok = ui_vm_set_path("ui_menu_data.swofty_strip_visible", 1, &v);
        static int tries = 0;
        if (ok) {
            strip_set = want;
            swofty::log("[main-menu] stock top-right strip %s (ui_menu_data.swofty_strip_visible = %d)", want ? "shown" : "hidden", want);
        } else if (tries++ % 120 == 0) {
            swofty::log("[main-menu] could not set ui_menu_data.swofty_strip_visible yet (try %d)", tries);
        }
    }
    std::lock_guard<std::mutex> g(chain_lock);
    if (chain.step == Step::idle || chain.step == Step::done)
        return;
    uint64_t now = GetTickCount64();
    if (now - chain.started > 20000) {
        swofty::log("[main-menu] chain: gave up at step %d after 20 s; the player continues by hand", int(chain.step));
        chain.step = Step::done;
        return;
    }
    if (chain.release_at && now >= chain.release_at) {
        runtime::pad::release();
        runtime::pad::claim(false);
        chain.release_at = 0;
        swofty::log("[main-menu] chain: virtual pad A released");
        chain.step = Step::done;
        return;
    }
    if (chain.step == Step::wait_container && chain.use_pad && !chain.release_at && chain.container_at && container_vm.load() &&
        now - chain.container_at > 2600) {
        runtime::pad::claim(true);
        runtime::pad::set(runtime::pad::A);
        chain.release_at = now + 160;
        swofty::log("[main-menu] chain: pressing A on the Play tab's first tile (the stock Grand Prix entry decides between saves and a new Grand Prix)");
        return;
    }
    if (chain.step == Step::wait_container && !chain.use_pad && !chain.release_at && chain.container_at && container_vm.load() &&
        now - chain.container_at > 900) {
        NeStr *name = nullptr;
        Ne_Make_StrFromCStr(&name, chain.time_trial ? "time_trial" : "grand_prix_new", 0, 0);
        swofty::log("[main-menu] chain: raising '%s' on the activity container %p",
                    chain.time_trial ? "time_trial" : "grand_prix_new", container_vm.load());
        uint64_t r = h_container ? h_container.call<uint64_t>(container_vm.load(), &name, container_third.load()) : 0;
        Ne_Make_StrFromCStr(&name, nullptr, 0, 0);
        swofty::log("[main-menu] chain: command returned 0x%llx; the %s season picker should be up",
                    (unsigned long long)r, chain.time_trial ? "Time Trial" : "Grand Prix");
        chain.step = Step::done;
    }
}

void screen(const char *name)
{
    if (!name)
        return;
    if (via.load() && !std::strcmp(name, "fe_f1_world")) {
        hub_at = GetTickCount64();
        swofty::log("[main-menu] back: the F1 World hub opened while back-to-menu is armed (router %p); route 'back' follows in 350 ms",
                    hub_router.load());
    }
    if (uint64_t at = swap_at.load(); at && GetTickCount64() - at < 5000)
    {
        swofty::log("[main-menu] back: screen %s opened %llu ms after the route swap", name,
                    (unsigned long long)(GetTickCount64() - at));
        std::lock_guard<std::mutex> g(swap_lock);
        swap_screens += (swap_screens.empty() ? "" : ", ") + std::string(name);
    }
    {
        std::lock_guard<std::mutex> g(chain_lock);
        uint64_t now = GetTickCount64();
        if (chain.step != Step::idle && chain.step != Step::done)
            swofty::log("[main-menu] chain: screen %s (step %d)", name, int(chain.step));
        if (!std::strcmp(name, "f1_world_activity_container") && chain.step == Step::wait_container) {
            chain.container_at = now;
            container_vm = nullptr;
        }
    }
    logic::Screen kind = logic::classify_screen(name);
    if (kind != logic::Screen::overlay) {
        {
            std::lock_guard<std::mutex> g(view_lock);
            home_view.screen(name);
            home_open = home_view.home;
        }
        bool home = !std::strcmp(name, "fe_home");
        if (home)
            home_opened_at = GetTickCount64();
        if (home) {
            hub_router = nullptr;
            hub_at = 0;
        }
        if (home && via.exchange(0))
            swofty::log("[main-menu] fe_home screen opened; back-to-menu disarmed (a screen-open event, not proof it is the visible screen)");
        strip_wanted = home ? 0 : 1;
        if (home) {
            read_state(true);
            swofty::log("[main-menu] home screen open: state read, stock top-right strip %s", rows_on() ? "hidden" : "hidden");
        }
    }
}

void command(void *, uint32_t command_hash, uint32_t focused_hash)
{
    if (!patched.load() || command_hash != logic::djb2("select_pressed"))
        return;
    bool tt = focused_hash == logic::djb2(logic::ID_TIME_TRIAL);
    bool gp = focused_hash == logic::djb2(logic::ID_GRAND_PRIX);
    if (!tt && !gp)
        return;
    if (uint64_t at = home_opened_at.load(); at && GetTickCount64() - at < 700)
        return;
    if (!tt)
        start_chain(false);
    if (!tt)
        return;
    read_state();
    logic::State s;
    {
        std::lock_guard<std::mutex> g(state_lock);
        s = state;
    }
    if (!logic::time_trial_warning_due(s, true, tt_warned.load())) {
        start_chain(true);
        return;
    }
    tt_warned = true;
    int r = runtime::popup_show(logic::TT_TITLE, logic::TT_BODY, "swofty_core (main menu)");
    swofty::log("[main-menu] Time Trial while not signed in to Discord: warning %s; the pad chain is skipped this once",
                r == 0 ? "queued" : "refused");
}

void prepare()
{
    uint64_t now = GetTickCount64();
    logic::State s;
    {
        std::lock_guard<std::mutex> g(state_lock);
        s = state;
    }
    if (!s.loaded)
        return;
    std::string key = s.version + "|" + s.avatar + "|" + s.video_thumbnail + "|" + (s.signed_in ? "1" : "0") + "|" + s.name;
    if (key == prepared_key && now - prepared_at < 60000)
        return;
    prepared_key = key;
    prepared_at = now;
    auto p = std::make_shared<Prepared>();
    p->s = s;
    uint32_t w = 0, h = 0;
    if (logic::has_avatar(s))
        p->avatar_slot = runtime::overlay_image("core.main_menu\navatar", host_path(s.avatar), &w, &h);
    if (logic::has_video(s)) {
        p->video_slot = runtime::overlay_image("core.main_menu\nvideo", host_path(s.video_thumbnail), &w, &h);
        p->video_w = w;
        p->video_h = h;
    }
    p->version = logic::version_line(s);
    p->discord = logic::discord_line(s);
    {
        std::lock_guard<std::mutex> g(prepared_lock);
        prepared = p;
    }
    panel_prepared = true;
    swofty::log("[main-menu] panel prepared on the flow thread: avatar slot %d, video slot %d", p->avatar_slot, p->video_slot);
}

void poll_bind()
{
    static uint64_t last = 0;
    uint64_t now = GetTickCount64();
    if (now - last < 100)
        return;
    last = now;
    std::shared_ptr<const Prepared> p;
    {
        std::lock_guard<std::mutex> g(prepared_lock);
        p = prepared;
    }
    if (!p || p->s.video_url.empty())
        return;
    bool on_pad = false;
    bool pressed = bind_pressed(&on_pad);
    bind_pad = on_pad;
    if (pressed)
        open_video(p->s);
}

void fade_panel(hud::Frame &f, float x, float y, float w, float h, float fade_left, float fade_right, float k)
{
    const int slices = 48;
    auto smooth = [](float t) {
        t = t < 0 ? 0 : t > 1 ? 1 : t;
        return t * t * (3 - 2 * t);
    };
    for (int i = 0; i < slices; i++) {
        float t0 = float(i) / slices, t1 = float(i + 1) / slices, tm = (t0 + t1) * 0.5f;
        float a = (fade_left > 0 && tm < fade_left ? smooth(tm / fade_left) : 1.0f) *
                  (fade_right > 0 && tm > 1 - fade_right ? smooth((1 - tm) / fade_right) : 1.0f);
        uint32_t fill = 0x0b0d1200u | uint32_t(a * 0xe4);
        uint32_t line = 0xffffff00u | uint32_t(a * 0x50);
        float sx = x + t0 * w, sw = (t1 - t0) * w + 0.6f;
        hud::rect(f, sx, y, sw, h, fill);
        hud::rect(f, sx, y, sw, 1.3f * k, line);
        hud::rect(f, sx, y + h - 1.3f * k, sw, 1.3f * k, line);
    }
}

void key_glyph(hud::Frame &f, float x, float y, float h, float k, const std::string &key, bool pad_style)
{
    hud::TextStyle st;
    st.px = h * 0.62f;
    st.rgba = 0xf3f5f9ffu;
    float w = 0, hh = 0;
    hud::measure(st, key, &w, &hh);
    float bw = pad_style ? std::max(h * 1.5f, w + 18 * k) : std::max(h, w + 14 * k);
    float radius = pad_style ? h * 0.5f : 6 * k;
    hud::rect_rounded(f, x, y, bw, h, radius, 0xffffff1cu);
    hud::rect_rounded_outline(f, x, y, bw, h, radius, 1.4f * k, 0xf3f5f9d8u);
    hud::text(f, x + (bw - w) * 0.5f, y + (h - st.px) * 0.5f, st, key);
}

void image_rounded(hud::Frame &f, int slot, float x, float y, float w, float h, float r, float u0, float v0, float u1, float v1)
{
    if (slot < 0 || w <= 0 || h <= 0 || hud::clipped_out(f))
        return;
    std::vector<hud::Pt> pts;
    hud::rounded_local(pts, x, y, w, h, r, f.xform.scale());
    if (pts.size() < 3 || !hud::reserve(f, pts.size() * 3, slot + 1))
        return;
    uint32_t col = hud::pack(0xffffffff, f.opacity);
    auto uv = [&](const hud::Pt &p, float *u, float *v) {
        *u = u0 + (p.x - x) / w * (u1 - u0);
        *v = v0 + (p.y - y) / h * (v1 - v0);
    };
    hud::Pt c{x + w * 0.5f, y + h * 0.5f};
    for (size_t i = 0; i < pts.size(); i++) {
        const hud::Pt &a = pts[i], &b = pts[(i + 1) % pts.size()];
        float cu, cv, au, av, bu, bv;
        uv(c, &cu, &cv);
        uv(a, &au, &av);
        uv(b, &bu, &bv);
        hud::vtx(f, c.x, c.y, col, cu, cv, hud::IMAGE);
        hud::vtx(f, a.x, a.y, col, au, av, hud::IMAGE);
        hud::vtx(f, b.x, b.y, col, bu, bv, hud::IMAGE);
    }
}

std::vector<std::string> wrap_title(const hud::TextStyle &style, const std::string &title, float max_w, size_t max_lines)
{
    std::vector<std::string> lines;
    std::string line;
    size_t i = 0;
    auto width = [&](const std::string &t) {
        float w = 0, h = 0;
        hud::measure(style, t, &w, &h);
        return w;
    };
    while (i < title.size() && lines.size() < max_lines) {
        size_t j = title.find(' ', i);
        std::string word = title.substr(i, j == std::string::npos ? std::string::npos : j - i);
        std::string trial = line.empty() ? word : line + " " + word;
        if (width(trial) <= max_w || line.empty()) {
            line = trial;
            i = j == std::string::npos ? title.size() : j + 1;
        } else {
            lines.push_back(line);
            line.clear();
        }
    }
    if (!line.empty() && lines.size() < max_lines)
        lines.push_back(line);
    bool cut = i < title.size();
    if (!lines.empty()) {
        std::string &last = lines.back();
        if (cut)
            last += "...";
        while (width(last) > max_w && last.size() > 4) {
            last.erase(last.size() - (cut ? 4 : 1), cut ? 4 : 1);
            if (cut)
                last += "...";
        }
    }
    return lines;
}

void paint(hud::Frame &f, uint64_t now)
{
    (void)now;
    static bool told_first = false, told_blocked = false, told_noprep = false;
    if (!std::exchange(told_first, true))
        swofty::log("[main-menu] panel: paint called on a %.0fx%.0f frame (patched %d, home open %d, state %d, prepared %d, disabled %d)",
                    double(f.width), double(f.height), int(patched.load()), int(home_open.load()), int(state_loaded.load()),
                    int(panel_prepared.load()), int(panel_disabled.load()));
    if (!drawing()) {
        if (home_open.load() && !std::exchange(told_blocked, true))
            swofty::log("[main-menu] panel: not due on an open home screen (patched %d, state %d, prepared %d, disabled %d)",
                        int(patched.load()), int(state_loaded.load()), int(panel_prepared.load()), int(panel_disabled.load()));
        return;
    }
    std::shared_ptr<const Prepared> p;
    {
        std::lock_guard<std::mutex> g(prepared_lock);
        p = prepared;
    }
    if (!p) {
        if (!std::exchange(told_noprep, true))
            swofty::log("[main-menu] panel: due but the prepared snapshot is empty");
        return;
    }
    size_t verts_before = f.verts.size();
    LARGE_INTEGER t0, t1, freq;
    QueryPerformanceCounter(&t0);
    static bool told_drawn = false;
    hud::reset_state(f);
    float W = f.width, H = f.height;
    if (!std::exchange(told_drawn, true))
        swofty::log("[main-menu] panel: first draw on a %.0fx%.0f frame; video card %s, avatar %s", double(W), double(H),
                    p->video_slot >= 0 ? "yes" : "no", p->avatar_slot >= 0 ? "yes" : "no");
    const float k = H / 1028.0f;
    auto anchor = logic::scene_to_px(logic::CARD_RIGHT_X, logic::CARD_BOTTOM_Y, W, H);
    const float right = anchor.x;
    {
        hud::TextStyle name_style;
        name_style.px = 26 * k;
        name_style.rgba = 0xf3f5f9ffu;
        std::string who = p->s.signed_in ? (p->s.name.empty() ? std::string("Discord") : p->s.name) : std::string("Not signed in");
        float tmp = 0, nw = 0;
        hud::measure(name_style, who, &nw, &tmp);
        nw = nw * 1.06f + 4 * k;
        float avatar = 44 * k, pad = 10 * k, gap = 12 * k;
        float block_h = name_style.px * 1.3f;
        float ph = avatar + 2 * pad;
        float pw = std::max(190 * k, pad + avatar + gap + 3 * k + 9 * k + nw + 20 * k);
        float px = right - pw, py = 44 * k;
        fade_panel(f, px - 110 * k, py, pw + 110 * k, ph, 0.45f, 0.04f, k);
        float ax = px + pad, ay = py + pad;
        if (p->avatar_slot >= 0) {
            image_rounded(f, p->avatar_slot, ax, ay, avatar, avatar, avatar * 0.5f, 0, 0, 1, 1);
        } else if (p->s.signed_in) {
            hud::rect_rounded(f, ax, ay, avatar, avatar, avatar * 0.5f, 0x5865f2ffu);
            hud::TextStyle initial;
            initial.px = 24 * k;
            initial.rgba = 0xffffffffu;
            std::string letter = p->s.name.empty() ? "D" : p->s.name.substr(0, 1);
            float lw = 0;
            hud::measure(initial, letter, &lw, &tmp);
            hud::text(f, ax + (avatar - lw) * 0.5f, ay + (avatar - initial.px) * 0.5f, initial, letter);
        } else {
            hud::rect_rounded(f, ax, ay, avatar, avatar, avatar * 0.5f, 0x3a3f4bffu);
            hud::rect_rounded(f, ax + avatar * 0.34f, ay + avatar * 0.18f, avatar * 0.32f, avatar * 0.32f, avatar * 0.16f, 0xb9bfcdffu);
            hud::rect_rounded(f, ax + avatar * 0.2f, ay + avatar * 0.56f, avatar * 0.6f, avatar * 0.26f, avatar * 0.13f, 0xb9bfcdffu);
        }
        float bx = ax + avatar + gap, by = py + (ph - block_h) * 0.5f;
        hud::rect_rounded(f, bx, by, 3 * k, block_h, 1.5f * k, 0xe10600ffu);
        hud::text(f, bx + 12 * k, by + (block_h - name_style.px) * 0.5f, name_style, who);
    }
    if (p->video_slot >= 0 && p->video_w && p->video_h) {
        hud::TextStyle cap_style;
        cap_style.px = 16 * k;
        cap_style.rgba = 0xa9afbfffu;
        cap_style.tracking = 16 * k * 0.12f;
        hud::TextStyle title_style;
        title_style.px = 24 * k;
        title_style.rgba = 0xf3f5f9ffu;
        hud::TextStyle watch_style;
        watch_style.px = 20 * k;
        watch_style.rgba = 0xd5d9e3ffu;
        const std::string caption = "LATEST VIDEO";
        const bool pad_dev = bind_pad.load();
        const std::string key = logic::button_name(pad_dev);
        float tmp_h = 0, cap_w = 0, watch_w = 0, key_w = 0;
        hud::TextStyle key_probe;
        key_probe.px = 26 * k * 0.62f;
        hud::measure(cap_style, caption, &cap_w, &tmp_h);
        hud::measure(watch_style, "Watch", &watch_w, &tmp_h);
        hud::measure(key_probe, key, &key_w, &tmp_h);
        const float row_h = 26 * k;
        const float glyph_w = pad_dev ? std::max(row_h * 1.5f, key_w + 18 * k) : std::max(row_h, key_w + 14 * k);
        auto plan0 = logic::plan_card(k, 0);
        auto title_lines = wrap_title(title_style, p->s.video_title, plan0.text_max, 2);
        float title_w = 0;
        for (auto &line : title_lines) {
            float w = 0;
            hud::measure(title_style, line, &w, &tmp_h);
            title_w = std::max(title_w, w);
        }
        float text_w = std::max({cap_w, title_w, glyph_w + 10 * k + watch_w}) * 1.06f + 4 * k;
        auto card = logic::plan_card(k, text_w);
        float cx = right - card.width, cy = anchor.y - card.height;
        fade_panel(f, cx - 110 * k, cy, card.width + 110 * k, card.height, 0.45f, 0.04f, k);
        float tx = cx + card.pad + 12 * k;
        float cap_h = cap_style.px * 1.25f, title_h = title_style.px * 1.2f * float(title_lines.size());
        float block_h = cap_h + 6 * k + title_h + 8 * k + row_h;
        float ty = cy + (card.height - block_h) * 0.5f;
        hud::rect_rounded(f, cx + card.pad, ty, 3 * k, block_h, 1.5f * k, 0xe10600ffu);
        hud::text(f, tx, ty, cap_style, caption);
        float yy = ty + cap_h + 6 * k;
        for (auto &line : title_lines) {
            hud::text(f, tx, yy, title_style, line);
            yy += title_style.px * 1.2f;
        }
        yy += 8 * k;
        key_glyph(f, tx, yy, row_h, k, key, pad_dev);
        hud::text(f, tx + glyph_w + 10 * k, yy + (row_h - watch_style.px) * 0.5f, watch_style, "Watch");
        float want = card.thumb_w / card.thumb_h, have = float(p->video_w) / float(p->video_h);
        float v0 = 0, v1 = 1, u0 = 0, u1 = 1;
        if (have < want) {
            float keep = have / want;
            v0 = (1 - keep) * 0.5f;
            v1 = 1 - v0;
        } else {
            float keep = want / have;
            u0 = (1 - keep) * 0.5f;
            u1 = 1 - u0;
        }
        image_rounded(f, p->video_slot, cx + card.width - card.pad - card.thumb_w, cy + card.pad, card.thumb_w, card.thumb_h,
                      8 * k, u0, v0, u1, v1);
    }
    overlay_drawn = true;
    static bool told_verts = false;
    if (!std::exchange(told_verts, true))
        swofty::log("[main-menu] panel: first draw added %zu vertices and %zu batch(es) (frame total %zu / %zu)",
                    f.verts.size() - verts_before, f.batches.size(), f.verts.size(), f.batches.size());
    QueryPerformanceCounter(&t1);
    QueryPerformanceFrequency(&freq);
    double ms = double(t1.QuadPart - t0.QuadPart) * 1000.0 / double(freq.QuadPart);
    if (logic::panel_too_slow(ms) && !panel_disabled.exchange(true))
        swofty::log("[main-menu] panel: one draw took %.2f ms (budget %.1f ms); the panel is disabled for this session", ms,
                    logic::PANEL_BUDGET_MS);
}

}
