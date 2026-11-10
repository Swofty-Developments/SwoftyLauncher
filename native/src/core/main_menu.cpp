#include "runtime.hpp"

#include "main_menu.hpp"
#include "main_menu_logic.hpp"

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>

namespace runtime {
int overlay_image(const std::string &id, const std::string &path, uint32_t *width, uint32_t *height);
}

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

constexpr size_t OFF_F1WORLD = 11, OFF_CHALLENGE = 32, OFF_THEATRE = 98, OFF_END_JMP = 103;
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
    Add_t f1world = nullptr, challenge = nullptr, theatre = nullptr;
} block;

std::atomic<bool> resolved{false}, patched{false}, home_open{false};
std::once_flag once;
void *f1w_populate = nullptr, *f1w_action = nullptr;
std::atomic<bool> tt_warned{false};
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
}

Action *card_action(HomeItem *item, Action *out)
{
    Action *a = reinterpret_cast<GetAction_t>(f1w_action)(item, out);
    swofty::log("[main-menu] %s card pressed: route type %d (the F1 World route)", id_of(item), a ? a->type : -1);
    return a;
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

void read_state()
{
    uint64_t now = GetTickCount64();
    std::lock_guard<std::mutex> g(state_lock);
    if (state_read_at && now - state_read_at < 1000)
        return;
    state_read_at = now;
    auto next = logic::parse_state(runtime::read_file(runtime::under_swofty("home_menu.json"), size_t(1) << 20));
    if (next.loaded != state.loaded || next.signed_in != state.signed_in || next.video_url != state.video_url)
        swofty::log("[main-menu] state: %s, %s, video %s", next.loaded ? logic::version_line(next).c_str() : "no file",
                    logic::discord_line(next).c_str(), logic::has_video(next) ? "yes" : "none");
    state = next;
}

bool pad_connected_and_y(bool *connected)
{
    using Get_t = DWORD(WINAPI *)(DWORD, void *);
    static Get_t get = [] {
        HMODULE m = LoadLibraryA("xinput1_4.dll");
        if (!m)
            m = LoadLibraryA("xinput1_3.dll");
        return m ? reinterpret_cast<Get_t>(GetProcAddress(m, "XInputGetState")) : nullptr;
    }();
    uint8_t buf[16] = {};
    *connected = get && get(0, buf) == 0;
    uint16_t buttons;
    std::memcpy(&buttons, buf + 4, 2);
    return *connected && (buttons & 0x8000);
}

bool bind_pressed(bool *pad)
{
    static bool was = false;
    bool connected = false;
    bool y = pad_connected_and_y(&connected);
    *pad = connected;
    bool key = (GetAsyncKeyState(logic::WATCH_KEY_VK) & 0x8000) != 0 && GetForegroundWindow() != nullptr;
    bool now = y || key;
    bool edge = now && !was;
    was = now;
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
    return p;
}

}

bool enabled()
{
    return resolve();
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
    swofty::log("[main-menu] tiles added after Career: Grand Prix, Time Trial%s, Theatre (last index %u)",
                flags && (flags[8] & 1) ? ", challenge" : "", unsigned(index));
    return index;
}

void screen(const char *name)
{
    if (!name)
        return;
    if (std::strncmp(name, "fe_", 3) == 0)
        home_open = !std::strcmp(name, "fe_home");
}

void command(void *, uint32_t command_hash, uint32_t focused_hash)
{
    if (!patched.load() || command_hash != logic::djb2("select_pressed") ||
        focused_hash != logic::djb2(logic::ID_TIME_TRIAL))
        return;
    read_state();
    logic::State s;
    {
        std::lock_guard<std::mutex> g(state_lock);
        s = state;
    }
    if (!logic::time_trial_warning_due(s, true, tt_warned.load()))
        return;
    tt_warned = true;
    int r = runtime::popup_show(logic::TT_TITLE, logic::TT_BODY, "swofty_core (main menu)");
    swofty::log("[main-menu] Time Trial while not signed in to Discord: warning %s", r == 0 ? "queued" : "refused");
}

void paint(hud::Frame &f, uint64_t now)
{
    (void)now;
    if (!patched.load() || !home_open.load())
        return;
    read_state();
    logic::State s;
    {
        std::lock_guard<std::mutex> g(state_lock);
        s = state;
    }
    if (!s.loaded)
        return;
    hud::reset_state(f);
    float W = f.width, H = f.height, u = H / 1080.0f;
    float pad = 28 * u, bar = 64 * u, cw = 460 * u;
    hud::TextStyle line;
    line.px = 26 * u;
    line.rgba = 0xe9ecf3ff;
    float x = W - pad - cw, y = pad;
    hud::rect_rounded(f, x - 14 * u, y - 10 * u, cw + 28 * u, bar + 14 * u, 8 * u, 0x0b0d12f2);
    float tx = x;
    if (logic::has_avatar(s)) {
        uint32_t w = 0, h = 0;
        int slot = runtime::overlay_image("core.main_menu\navatar", host_path(s.avatar), &w, &h);
        if (slot >= 0) {
            hud::image(f, slot, x, y, bar, bar, 0xffffffff);
            tx = x + bar + 14 * u;
        }
    }
    hud::text(f, tx, y + 4 * u, line, logic::version_line(s));
    line.rgba = s.signed_in ? 0x8fd694ff : 0xa3a8b7ff;
    line.px = 22 * u;
    hud::text(f, tx, y + 4 * u + 30 * u, line, logic::discord_line(s));

    if (!logic::has_video(s))
        return;
    float vw = 400 * u, vh = vw * 9.0f / 16.0f;
    float vx = W - pad - vw, vy = H - 190 * u - vh - 34 * u;
    uint32_t w = 0, h = 0;
    int slot = runtime::overlay_image("core.main_menu\nvideo", host_path(s.video_thumbnail), &w, &h);
    if (slot < 0)
        return;
    hud::image(f, slot, vx, vy, vw, vh, 0xffffffff);
    hud::TextStyle cap;
    cap.px = 22 * u;
    cap.rgba = 0xe9ecf3ff;
    bool on_pad = false;
    bool pressed = bind_pressed(&on_pad);
    hud::text(f, vx, vy + vh + 8 * u, cap, logic::watch_line(logic::button_name(on_pad)));
    if (pressed)
        open_video(s);
}

}
