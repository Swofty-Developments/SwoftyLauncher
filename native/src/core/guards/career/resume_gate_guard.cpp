#include "core/guards/guard.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/build_proof.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>
#include <windows.h>
#include <mutex>
#include <string>
#include <vector>

#include "core/career/career_saves_logic.hpp"
#include "core/career/resume_gate_logic.hpp"
#include "core/career/content_state.hpp"
#include "core/career/resume_list.hpp"
#include "core/career/resume_press_logic.hpp"

// Resume Career gate (re/db/globals.tsv vt_Game_CareerContinueDriverViewModel and vt_Game_CareerContinueMyTeamViewModel,
// slot 10 = Game_CareerContinue_HandleCommand, 0x141a1db90 on the KB build).
//
// The launcher writes resume_needs into settings.json: for each career its save slot key (the slot guid as 16 hex digits,
// else the slot id; the key the core's career index keeps as `slot`) and the add-ons the career needs, each with
// whether this profile has it on and at the version the career was made with. A press on a resume card
// (slot_pressed, data = the card index 0..2, cards at vm+0x328 + i*0x540 with has_save at +0 and the slot id at +8) names
// the card's save slot; the slot's key is found from the save slots the core listed at the last profile load or save.
// A career whose add-ons are all on loads as the game does. One that needs an add-on this profile lacks does not load on
// the press: the core raises the swofty_confirm message (the launcher's UI patch adds it, options load_anyway and back)
// naming the add-ons. The message's response callback is {fn, ctx} passed to Ui_Message_Construct (r9), as the stock
// purchase confirm does (0x1419e3d02..0x1419e3ccc on the KB build, the same instructions at 0x1419e54e2 on Steam 1.0.145);
// fn(ctx, void **answer) runs when an option is chosen and *answer points at a string object whose text is at +9, the
// chosen option's response (the stock handler 0x1419e3ff0 compares it with "yes"). load_anyway runs the game's own
// slot_pressed for that card; back does nothing. Where the confirm cannot be raised (no message in the UI build, a build
// the callback was not proven on) the first press raises the OK message and a second press of the same card loads it.
// Leaving the screen, opening it again and Back reset the pending press.
//
// The card titles stay the game's own. A career the launcher gave no needs for, a card whose slot is not listed and a
// settings.json with no resume_needs are never gated.
//
// The cards' layout was checked on the KB build, Steam 1.0.145 and EA 1.0.145 (the test agent reads the same fields).
// SWOFTY_RESUME_GATE=0 turns the gate off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (resume gate)";
constexpr const char *DRIVER_VT = "vt_Game_CareerContinueDriverViewModel";
constexpr const char *MY_TEAM_VT = "vt_Game_CareerContinueMyTeamViewModel";
constexpr const char *MAKE_STR = "Ne_Make_StrFromCStr";
constexpr uintptr_t CARDS = 0x328, CARD_SIZE = 0x540, CARD_COUNT = 3;

namespace bp = runtime::build_proof;
namespace guards = swofty_core::guards;
namespace rg = career::resume_gate;
namespace cs = career::saves;
namespace mem = swofty::mem;

constexpr uint32_t PROVEN = bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145);
constexpr bp::Proof CARD_LAYOUT{"the resume screens' cards (+0x328, 0x540 each; has_save +0, slot id +8)", PROVEN};
constexpr bp::Gate CARD_GATE{"the resume gate's card reads", &CARD_LAYOUT};
constexpr bp::Proof CONFIRM_CALLBACK{"the message response callback ({fn, ctx} at Ui_Message_Construct's r9; fn(ctx, answer**), "
                                     "answer text at +9)",
                                     PROVEN};
constexpr bp::Gate CONFIRM_GATE{"the resume gate's confirm message", &CONFIRM_CALLBACK};

using Handle = uint64_t(void *vm, void *cmd, void *value);
using MakeStr = uint32_t(void **out, const char *text, uint32_t flags, uintptr_t len);
Handle *orig_driver = nullptr, *orig_my_team = nullptr;
uintptr_t make_str = 0;
bool confirm_on = false, list_on = false;

std::mutex lock;
rg::Table table;
bool table_read = false;
rg::State state;
int logged = 0;

struct Pending {
    void *vm = nullptr;
    Handle *orig = nullptr;
    uint8_t card = 0;
    std::string key, prompt;
    bool open = false;
} pending;

const rg::Table &needs_locked()
{
    if (!table_read) {
        table_read = true;
        table = rg::parse(runtime::settings_json_text());
        swofty::log("[career] resume gate: %s", table.present ? "the launcher gave the careers' add-on needs"
                                                              : "the launcher gave no career needs; no career is gated");
    }
    return table;
}

bool card_slot(void *vm, void *value, uint8_t &card, uint32_t &slot)
{
    auto base = reinterpret_cast<uintptr_t>(vm);
    if (!value || !mem::read(reinterpret_cast<uintptr_t>(value) + 8, card) || card >= CARD_COUNT)
        return false;
    uintptr_t entry = base + CARDS + card * CARD_SIZE;
    uint8_t has_save = 0;
    return mem::read(entry, has_save) && has_save && mem::read(entry + 8, slot);
}

uint32_t command_hash(void *cmd)
{
    uintptr_t str = 0;
    uint32_t hash = 0;
    if (!cmd || !mem::read(reinterpret_cast<uintptr_t>(cmd), str) || !str || !mem::read(str, hash))
        return 0;
    return hash;
}

void press_card(Handle *orig, void *vm, uint8_t card)
{
    struct alignas(8) Value {
        uint32_t type, pad;
        uint64_t data, zero;
    } v{4, 0, uint64_t(card), 0};
    void *cmd = nullptr;
    reinterpret_cast<MakeStr *>(make_str)(&cmd, "slot_pressed", 0, 0);
    orig(vm, &cmd, &v);
    reinterpret_cast<MakeStr *>(make_str)(&cmd, nullptr, 0, 0);
}

void answered(void *, void **answer)
{
    std::string text;
    uintptr_t str = 0;
    if (answer && mem::read(reinterpret_cast<uintptr_t>(answer), str) && str)
        mem::c_string(str + 9, text, 32);
    Pending p;
    {
        std::lock_guard<std::mutex> g(lock);
        p = pending;
        pending.open = false;
    }
    swofty::log("[career] resume gate: the confirm was answered \"%s\"", text.c_str());
    if (p.open && p.vm && rg::loads(text))
        press_card(p.orig, p.vm, p.card);
}

void confirm_failed(void *)
{
    std::string prompt;
    {
        std::lock_guard<std::mutex> g(lock);
        if (!pending.open)
            return;
        pending.open = false;
        state.armed = pending.key;
        prompt = pending.prompt;
    }
    swofty::log("[career] resume gate: the confirm message did not show; the press-twice prompt is used");
    runtime::popup_show("Add-ons do not match", prompt, "swofty_core");
}

void set_labels()
{
    static bool done = false;
    if (done)
        return;
    done = true;
    const SwoftyLang *l = runtime::lang_table();
    l->override_set(nullptr, "lng_swofty_load_anyway", "Load Anyway");
    l->override_set(nullptr, "lng_swofty_back", "Back");
}

constexpr uintptr_t PRESS_SLOT_VT = 24 * 8, CARD_BUSY = 0x1380, CARD_SELECTED = 0x1369;
constexpr uint8_t PRESS_SLOT_HEAD[] = {0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57,
                                       0x55, 0x53, 0x48, 0x81, 0xEC, 0x58, 0x01, 0x00, 0x00};
constexpr uint8_t PRESS_SLOT_IMUL[] = {0x4C, 0x69, 0xF0, 0x40, 0x05, 0x00, 0x00};
constexpr uintptr_t PRESS_SLOT_IMUL_AT = 0x2b;
constexpr bp::Proof PRESS_SLOT_PROOF{"Game_CareerContinue_PressSlot at vtable slot 24 (selects the card's slot id at entry+8, "
                                     "sets the game mode, starts the transition out)",
                                     PROVEN};
constexpr bp::Gate PRESS_SLOT_GATE{"the resume probe's direct slot load", &PRESS_SLOT_PROOF};

std::atomic<bool> probe_done{false};
bool probe_armed = false;
uint64_t probe_slot = 0, probe_guid = 0;

bool press_slot_shaped(void *vm, uintptr_t &fn, const char *who, bool check_busy = true)
{
    auto base = reinterpret_cast<uintptr_t>(vm);
    uintptr_t vt = 0;
    uint8_t busy = 0;
    if (!mem::read(base, vt) || !mem::in_image(vt) || !mem::read(vt + PRESS_SLOT_VT, fn) || !mem::in_image(fn)) {
        swofty::log("[career] resume screen: %s: the view model's vtable slot 24 does not read; nothing was done", who);
        return false;
    }
    uint8_t head[sizeof PRESS_SLOT_HEAD] = {}, imul[sizeof PRESS_SLOT_IMUL] = {};
    bool shape = true;
    for (size_t i = 0; i < sizeof head && shape; i++)
        shape = mem::read(fn + i, head[i]) && head[i] == PRESS_SLOT_HEAD[i];
    for (size_t i = 0; i < sizeof imul && shape; i++)
        shape = mem::read(fn + PRESS_SLOT_IMUL_AT + i, imul[i]) && imul[i] == PRESS_SLOT_IMUL[i];
    if (!shape) {
        swofty::log("[career] resume screen: %s: vtable slot 24 (base+0x%llx) is not PressSlot's code on this build; nothing "
                    "was done",
                    who, (unsigned long long)(fn - reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr))));
        return false;
    }
    if (check_busy && (!mem::read(base + CARD_BUSY, busy) || busy)) {
        swofty::log("[career] resume screen: %s: the screen is busy (vm+0x1380 = %u); nothing was done", who, busy);
        return false;
    }
    return true;
}

namespace rl = career::resume_list;

constexpr uintptr_t VM_WIZARD = 0x1370, WIZARD_GROUP = 0x128, WIZARD_SUFFIX = 0x140, WIZARD_SUFFIX_CAP = 0x148,
                    WIZARD_SUFFIX_CHARS = 0x150, CARD_PENDING = 0x1368, CARD_MODE = 0x53c, MODE_STAT_ID = 0x2a8,
                    MODE_SLOT = 33 * 8;
constexpr uint8_t MODE_HEAD[] = {0x56, 0x57, 0x48, 0x83, 0xEC, 0x68, 0x80, 0xFA, 0x01, 0x74, 0x2E,
                                 0x0F, 0xB6, 0xC2, 0x85, 0xC0, 0x0F, 0x85, 0x88, 0x00, 0x00, 0x00};
const runtime::Sig SIG_SET_DATA_GROUP{"game.career_wizard_set_data_group",
                                      "56 57 48 83 EC 28 48 89 D7 48 89 CE 48 85 D2 74 0D 48 89 F9 E8 ?? ?? ?? ?? 48 FF C0 EB 05 "
                                      "B8 01 00 00 00 48 8B 8E 20 01 00 00",
                                      0x387d100};
using SetDataGroup = void(void *wizard, const char *group);
using Reserve = void(void *str, uint64_t n);
using ModeOf = void *(void *vm, uint8_t kind);
uintptr_t set_data_group = 0, reserve_str = 0;
bool set_data_group_tried = false;

void *wizard_of(void *vm)
{
    uintptr_t w = 0;
    return mem::read(reinterpret_cast<uintptr_t>(vm) + VM_WIZARD, w) && w && !(w & 7) && mem::readable(w, 0x200)
               ? reinterpret_cast<void *>(w)
               : nullptr;
}

std::string wizard_group(void *wizard)
{
    uintptr_t chars = 0;
    std::string text;
    if (wizard && mem::read(reinterpret_cast<uintptr_t>(wizard) + WIZARD_GROUP, chars) && chars)
        mem::c_string(chars, text, 96);
    return text;
}

bool write_suffix(void *wizard, const std::string &suffix)
{
    auto w = reinterpret_cast<uintptr_t>(wizard);
    uint64_t cap = 0, chars = 0;
    if (suffix.empty() || !mem::read(w + WIZARD_SUFFIX_CAP, cap))
        return false;
    if (cap < suffix.size() + 1) {
        if (!reserve_str)
            return false;
        reinterpret_cast<Reserve *>(reserve_str)(reinterpret_cast<void *>(w + WIZARD_SUFFIX), suffix.size() + 1);
        if (!mem::read(w + WIZARD_SUFFIX_CAP, cap))
            return false;
    }
    if (cap < suffix.size() + 1 || !mem::read(w + WIZARD_SUFFIX_CHARS, chars) || !chars)
        return false;
    auto *out = reinterpret_cast<volatile char *>(chars);
    for (size_t i = 0; i < suffix.size(); i++)
        out[i] = suffix[i];
    out[suffix.size()] = 0;
    return true;
}

void match_wizard(void *vm, const rl::Target &t, rl::Facts &f)
{
    f.want_group = t.group;
    void *wizard = wizard_of(vm);
    f.wizard_group = wizard_group(wizard);
    if (!wizard || !rl::group_needs_write(t.group, f.wizard_group))
        return;
    if (!set_data_group_tried) {
        set_data_group_tried = true;
        set_data_group = runtime::find(SIG_SET_DATA_GROUP, "resume screen", "career");
        reserve_str = swofty::api->resolve("Ne_String_Reserve");
    }
    if (!set_data_group)
        return;
    const std::string was = f.wizard_group;
    reinterpret_cast<SetDataGroup *>(set_data_group)(wizard, t.group.c_str());
    bool suffixed = write_suffix(wizard, t.suffix);
    f.wizard_group = wizard_group(wizard);
    swofty::log("[career] resume screen: row %zu: the wizard data group was %s, set to %s (suffix %s)", t.row,
                rl::group_text(was).c_str(), rl::group_text(f.wizard_group).c_str(),
                t.suffix.empty() ? "kept"
                : suffixed       ? t.suffix.c_str()
                                 : "not written");
}

std::string press_summary(void *vm, void *wizard)
{
    char b[512];
    auto base = reinterpret_cast<uintptr_t>(vm);
    uint8_t card[16] = {}, w[8] = {}, pending = 0, busy = 0, selected = 0, kind = 0;
    for (size_t i = 0; i < sizeof card; i++)
        mem::read(base + CARDS + i, card[i]);
    auto wz = reinterpret_cast<uintptr_t>(wizard);
    const uintptr_t offs[] = {0x181, 0x182, 0x1e0, 0x1e1, 0x1e2, 0x1e3, 0x1e4, 0x1e5};
    for (size_t i = 0; i < 8 && wizard; i++)
        mem::read(wz + offs[i], w[i]);
    mem::read(base + CARD_PENDING, pending);
    mem::read(base + CARD_BUSY, busy);
    mem::read(base + 0x1369, selected);
    mem::read(base + CARDS + CARD_MODE, kind);
    uint32_t content = 0;
    if (uintptr_t g = swofty::api->resolve("g_FeTile_SelectedCareerContent"))
        mem::read(g, content);
    snprintf(
        b, sizeof b,
        "card0 bytes %02x %02x %02x %02x %02x %02x %02x %02x kind %u, wizard +181/182 %02x %02x +1e0..1e5 %02x %02x %02x %02x "
        "%02x %02x, vm pending %u busy %u selected %u, selected content %08x, group %s",
        card[0], card[1], card[2], card[3], card[4], card[5], card[6], card[7], kind, w[0], w[1], w[2], w[3], w[4], w[5], w[6],
        w[7], pending, busy, selected, content, wizard_group(wizard).c_str());
    return b;
}

bool mode_shaped(void *vm, uintptr_t &fn)
{
    uintptr_t vt = 0;
    if (!mem::read(reinterpret_cast<uintptr_t>(vm), vt) || !mem::in_image(vt) || !mem::read(vt + MODE_SLOT, fn) ||
        !mem::in_image(fn))
        return false;
    for (size_t i = 0; i < sizeof MODE_HEAD; i++) {
        uint8_t b = 0;
        if (!mem::read(fn + i, b) || b != MODE_HEAD[i])
            return false;
    }
    return true;
}

void *mode_entity(void *vm)
{
    uintptr_t fn = 0;
    uint8_t kind = 0;
    if (!mode_shaped(vm, fn) || !mem::read(reinterpret_cast<uintptr_t>(vm) + CARDS + CARD_MODE, kind))
        return nullptr;
    return reinterpret_cast<ModeOf *>(fn)(vm, kind);
}

void read_mode(void *vm, const rl::Target &t, rl::Facts &f)
{
    void *e = mode_entity(vm);
    f.game_mode = t.game_mode;
    f.mode_found = e != nullptr;
    f.mode_stat = -1;
    uint32_t stat = 0;
    if (e && mem::read(reinterpret_cast<uintptr_t>(e) + MODE_STAT_ID, stat))
        f.mode_stat = stat;
    f.want_stat = career::content::mode_stat_of(t.game_mode);
}

rl::Card read_card(void *vm, size_t i)
{
    rl::Card c;
    uintptr_t entry = reinterpret_cast<uintptr_t>(vm) + CARDS + i * CARD_SIZE;
    uint8_t has = 0;
    c.read = mem::read(entry, has) && mem::read(entry + 8, c.id);
    c.has_save = has != 0;
    return c;
}

void read_screen(void *vm, rl::Facts &f)
{
    auto base = reinterpret_cast<uintptr_t>(vm);
    mem::read(base + CARD_PENDING, f.pending);
    mem::read(base + CARD_BUSY, f.busy);
    for (size_t i = 0; i < rl::CARD_COUNT; i++)
        f.stock[i] = read_card(vm, i);
    f.card0 = f.stock[0];
}

uint64_t rewrite_card0(void *vm, const career::resume_list::Selection &sel)
{
    uint64_t slot_id = sel.id;
    runtime::snapshots_slot_selected(0, sel.id, sel.guid);
    auto base = reinterpret_cast<uintptr_t>(vm);
    uintptr_t card = base + CARDS;
    uint64_t was = 0;
    mem::read(card + 8, was);
    *reinterpret_cast<volatile uint64_t *>(card + 8) = slot_id;
    uint8_t has_save = 0;
    if (!mem::read(card, has_save) || !has_save)
        *reinterpret_cast<volatile uint8_t *>(card) = 1;
    *reinterpret_cast<volatile uint8_t *>(base + CARD_SELECTED) = 0;
    return was;
}

void probe(void *vm)
{
    if (!probe_armed || probe_done.exchange(true))
        return;
    uintptr_t fn = 0;
    if (!press_slot_shaped(vm, fn, "probe"))
        return;
    auto base = reinterpret_cast<uintptr_t>(vm);
    uint64_t was = rewrite_card0(vm, {probe_slot, probe_guid});
    swofty::log("[career] resume screen: probe: card 0 slot id %llu rewritten to %llu; calling PressSlot (base+0x%llx) on vm %p",
                (unsigned long long)was, (unsigned long long)probe_slot,
                (unsigned long long)(fn - reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr))), vm);
    using PressSlot = void(void *vm, uint8_t card);
    reinterpret_cast<PressSlot *>(fn)(vm, 0);
    uint8_t pending = 0;
    mem::read(base + 0x1368, pending);
    swofty::log("[career] resume screen: probe: PressSlot returned; pending action vm+0x1368 = %u (2 means the career loads "
                "when the screen finishes its transition out)",
                pending);
}

uint64_t entry(Handle *orig, bool my_team, void *vm, void *cmd, void *value);

struct ListAsk {
    void *vm = nullptr;
    Handle *orig = nullptr;
    bool my_team = false, open = false;
    size_t row = 0;
    rl::Target target;
} list_ask;

void refuse(const rl::Facts &f, const std::string &why)
{
    if (!rl::busy_reason(f).empty()) {
        swofty::log("[career] resume screen: row %zu: not pressed: %s", f.row, why.c_str());
        return;
    }
    swofty::log("[career] resume screen: row %zu: not pressed: %s; the continue screen stays open and says to pick the career "
                "on it",
                f.row, why.c_str());
    runtime::popup_show(rl::REFUSE_TITLE, rl::failed_text(), "swofty_core");
}

void press_selected(void *vm, bool my_team, const rl::Target &t)
{
    uintptr_t fn = 0;
    if (!press_slot_shaped(vm, fn, "resume screen", false))
        return;
    rl::Facts f;
    f.row = t.row;
    f.target = t.sel.id;
    f.listed = true;
    read_screen(vm, f);
    match_wizard(vm, t, f);
    read_mode(vm, t, f);
    std::string why = rl::press_refusal(f);
    if (!why.empty()) {
        refuse(f, why);
        return;
    }
    using PressSlot = void(void *vm, uint8_t card);
    swofty::log(
        "[career] resume screen: row %zu: calling PressSlot (base+0x%llx) on the %s continue screen, wizard data group %s", t.row,
        (unsigned long long)(fn - reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr))), my_team ? "My Team" : "Driver Career",
        rl::group_text(f.wizard_group).c_str());
    reinterpret_cast<PressSlot *>(fn)(vm, 0);
    uint8_t pending = 0;
    mem::read(reinterpret_cast<uintptr_t>(vm) + CARD_PENDING, pending);
    swofty::log("[career] resume screen: row %zu: PressSlot returned; pending action vm+0x1368 = %u (2 means the career loads "
                "when the screen finishes its transition out)",
                t.row, pending);
    rl::press_noted(press_summary(vm, wizard_of(vm)));
    if (pending != 2)
        swofty::log("[career] resume screen: row %zu: the screen did not take the career (pending %u); it stays open", t.row,
                    pending);
}

void go_back(Handle *orig, void *vm)
{
    struct alignas(8) Value {
        uint32_t type, pad;
        uint64_t data, zero;
    } v{0, 0, 0, 0};
    void *cmd = nullptr;
    reinterpret_cast<MakeStr *>(make_str)(&cmd, "back_pressed", 0, 0);
    orig(vm, &cmd, &v);
    reinterpret_cast<MakeStr *>(make_str)(&cmd, nullptr, 0, 0);
}

void list_answered(void *, void **answer)
{
    std::string text;
    uintptr_t str = 0;
    if (answer && mem::read(reinterpret_cast<uintptr_t>(answer), str) && str)
        mem::c_string(str + 9, text, 32);
    ListAsk a;
    {
        std::lock_guard<std::mutex> g(lock);
        a = list_ask;
        list_ask.open = false;
    }
    swofty::log("[career] resume screen: row %zu: the confirm was answered \"%s\"", a.row, text.c_str());
    if (!a.open || !a.vm)
        return;
    if (rg::loads(text))
        press_selected(a.vm, a.my_team, a.target);
    else
        go_back(a.orig, a.vm);
}

void list_confirm_failed(void *)
{
    ListAsk a;
    {
        std::lock_guard<std::mutex> g(lock);
        a = list_ask;
        list_ask.open = false;
    }
    if (!a.open || !a.vm)
        return;
    swofty::log("[career] resume screen: row %zu: the confirm message did not show; Back is taken and nothing loads", a.row);
    go_back(a.orig, a.vm);
}

void load_target(Handle *orig, bool my_team, void *vm)
{
    rl::Target t;
    if (!rl::take_target(my_team, t))
        return;
    uintptr_t fn = 0;
    if (!make_str || !press_slot_shaped(vm, fn, "resume screen", false))
        return;
    rl::Facts f;
    f.row = t.row;
    f.target = t.sel.id;
    f.listed = true;
    read_screen(vm, f);
    std::string why = rl::opened_refusal(f);
    if (why.empty() && !rl::stock_has(f.stock, f.target))
        swofty::log("[career] resume screen: row %zu: slot id %llu is not one of the %zu cards the continue screen listed; the "
                    "core's save slot list has it",
                    t.row, (unsigned long long)t.sel.id, rl::CARD_COUNT);
    if (why.empty() && f.pending != 0)
        why = rl::busy_reason(f);
    if (why.empty()) {
        match_wizard(vm, t, f);
        read_mode(vm, t, f);
        why = rl::wizard_reason(f);
    }
    if (!why.empty()) {
        refuse(f, why);
        return;
    }
    uint64_t was = rewrite_card0(vm, t.sel);
    swofty::log("[career] resume screen: row %zu: card 0 slot id %llu rewritten to %llu on the %s continue screen", t.row,
                (unsigned long long)was, (unsigned long long)t.sel.id, my_team ? "My Team" : "Driver Career");
    if (!t.ask) {
        press_selected(vm, my_team, t);
        return;
    }
    {
        std::lock_guard<std::mutex> g(lock);
        list_ask = {vm, orig, my_team, true, t.row, t};
    }
    set_labels();
    if (!confirm_on || runtime::popup_confirm("Add-ons do not match", t.body, reinterpret_cast<void *>(&list_answered), nullptr,
                                              list_confirm_failed, "swofty_core") != SWOFTY_ASSET_OK) {
        swofty::log("[career] resume screen: row %zu: the Load Anyway / Back confirm could not be raised; Back is taken", t.row);
        {
            std::lock_guard<std::mutex> g(lock);
            list_ask.open = false;
        }
        go_back(orig, vm);
        return;
    }
    swofty::log("[career] resume screen: row %zu: asking Load Anyway / Back (%s)", t.row, t.body.c_str());
}

uint64_t entry(Handle *orig, bool my_team, void *vm, void *cmd, void *value)
{
    uint32_t hash = command_hash(cmd);
    if (hash == rg::CMD_TRANSITION_IN && ((probe_armed && !probe_done.load()) || career::resume_list::armed())) {
        uint64_t r = orig(vm, cmd, value);
        {
            std::lock_guard<std::mutex> g(lock);
            rg::screen_changed(state);
            pending.open = false;
        }
        probe(vm);
        load_target(orig, my_team, vm);
        return r;
    }
    if (hash == rg::CMD_TRANSITION_IN || hash == rg::CMD_BACK_PRESSED) {
        std::lock_guard<std::mutex> g(lock);
        rg::screen_changed(state);
        pending.open = false;
    } else if (hash == rg::CMD_SLOT_PRESSED) {
        uint32_t slot = 0;
        uint8_t card = 0;
        if (card_slot(vm, value, card, slot)) {
            std::string key = runtime::career_saves_slot_key(slot, my_team);
            std::string title = "Add-ons do not match", body, prompt;
            bool block = false, ask = false;
            {
                std::lock_guard<std::mutex> g(lock);
                const rg::Entry *e = rg::find(needs_locked(), key);
                block = !pending.open && rg::press(e, key, state) == rg::Verdict::Block;
                if (pending.open)
                    return 0;
                if (logged++ < 20)
                    swofty::log("[career] resume gate: slot %u key %s: %s%s", slot, key.empty() ? "(not listed)" : key.c_str(),
                                e ? rg::status_line(e).c_str() : "no needs given", block ? "; held for the prompt" : "");
                if (block) {
                    body = rg::dialog_text(*e);
                    prompt = rg::prompt_text(*e);
                    ask = confirm_on;
                    if (ask) {
                        state.armed.clear();
                        pending = {vm, orig, card, key, prompt, true};
                    }
                }
            }
            if (block) {
                if (ask) {
                    set_labels();
                    if (runtime::popup_confirm(title, body, reinterpret_cast<void *>(&answered), nullptr, confirm_failed,
                                               "swofty_core") != SWOFTY_ASSET_OK) {
                        std::lock_guard<std::mutex> g(lock);
                        pending.open = false;
                        state.armed = key;
                        ask = false;
                    }
                }
                if (!ask)
                    runtime::popup_show(title, prompt, "swofty_core");
                return 0;
            }
        }
    }
    return orig(vm, cmd, value);
}

uint64_t driver_entry(void *vm, void *cmd, void *value) { return entry(orig_driver, false, vm, cmd, value); }
uint64_t my_team_entry(void *vm, void *cmd, void *value) { return entry(orig_my_team, true, vm, cmd, value); }

class ResumeGateGuard final : public guards::Guard {
public:
    ResumeGateGuard() : Guard(guards::CoreSetId::resume_gate) {}

private:
    void build(std::vector<runtime::CoreMixins> &out) override
    {
        char v[16] = {};
        if (guards::env_off("SWOFTY_RESUME_GATE", v)) {
            swofty::log("[career] resume gate off (SWOFTY_RESUME_GATE=%s)", v);
            return;
        }
        if (!swofty::api->resolve(DRIVER_VT) || !swofty::api->resolve(MY_TEAM_VT)) {
            swofty::log("[career] resume gate off: needs %s and %s, which this build does not resolve", DRIVER_VT, MY_TEAM_VT);
            return;
        }
        if (!runtime::holds(CARD_GATE)) {
            swofty::log("[career] resume gate off: the resume cards' layout is not proven on this build");
            return;
        }
        make_str = swofty::api->resolve(MAKE_STR);
        char on[16] = {}, slot[32] = {};
        bool probe_list_on = false;
        if (!(GetEnvironmentVariableA("SWOFTY_RESUME_LIST", on, sizeof on) && !strcmp(on, "0"))) {
            probe_list_on = true;
            if (!GetEnvironmentVariableA("SWOFTY_RESUME_LIST_PROBE", slot, sizeof slot) ||
                !(probe_slot = strtoull(slot, nullptr, 10)))
                swofty::log("[career] resume screen: the list screen is on and there is no SWOFTY_RESUME_LIST_PROBE slot id; "
                            "the list screen is used, not the probe");
            else if (!runtime::holds(PRESS_SLOT_GATE))
                swofty::log("[career] resume screen: the direct slot load is not proven on this build; not probed");
            else {
                char g[32] = {};
                if (GetEnvironmentVariableA("SWOFTY_RESUME_LIST_PROBE_GUID", g, sizeof g))
                    probe_guid = strtoull(g, nullptr, 16);
                probe_armed = true;
                swofty::log("[career] resume screen: probe armed: the first stock resume screen to open loads slot id %llu "
                            "through card 0",
                            (unsigned long long)probe_slot);
            }
        } else
            swofty::log("[career] resume screen: SWOFTY_RESUME_LIST=0: the list screen is off");
        list_on = probe_list_on && !probe_armed && make_str && runtime::holds(PRESS_SLOT_GATE);
        confirm_on = make_str && runtime::holds(CONFIRM_GATE);
        out.push_back(
            {MIXIN,
             {{DRIVER_VT, SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 10), 100, (void *)&driver_entry, (void **)&orig_driver},
              {MY_TEAM_VT, SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 10), 100, (void *)&my_team_entry,
               (void **)&orig_my_team}}});
    }

    void settle() override
    {
        if (list_on && offered_set(MIXIN) && runtime::core_mixins_active(MIXIN)) {
            career::resume_list::set_armed(true);
            swofty::log(
                "[career] resume screen: armed; Resume Career opens the list screen when the content select hooks are live");
        } else if (career::resume_list::wanted())
            swofty::log("[career] resume screen: not armed (the gate or the direct load is not available on this build)");
        if (offered_set(MIXIN))
            swofty::log("[career] resume gate %s; a career that needs add-ons this profile lacks asks before it loads (%s)",
                        guards::install_state(runtime::core_mixins_active(MIXIN), "armed"),
                        confirm_on ? "Load Anyway / Back" : "press twice");
    }
};

ResumeGateGuard guard;

}
