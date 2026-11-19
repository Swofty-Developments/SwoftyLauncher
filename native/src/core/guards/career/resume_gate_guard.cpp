#include "core/guards/guard.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/build_proof.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <mutex>
#include <string>
#include <vector>

#include "core/career/resume_gate_logic.hpp"

// Resume Career gate (re/db/globals.tsv vt_Game_CareerContinueDriverViewModel and vt_Game_CareerContinueMyTeamViewModel,
// slot 10 = Game_CareerContinue_HandleCommand, 0x141a1db90 on the KB build).
//
// The launcher writes resume_needs into settings.json: for each career its save slot key (the slot guid as 16 hex digits,
// else the slot id; the key the core's career index keeps as `slot`) and the add-ons the career needs, each with
// whether this profile has it on and at the version the career was made with. A press on a resume card
// (slot_pressed, data = the card index 0..2, cards at vm+0x328 + i*0x540 with has_save at +0 and the slot id at +8) names
// the card's save slot; the slot's key is found from the save slots the core listed at the last profile load or save.
// A career whose add-ons are all on loads as the game does. One that needs an add-on this profile lacks does not load on
// the first press: the core shows a message that names the add-ons, and a second press of the same card loads it. Leaving
// the screen, opening it again and Back reset the pending press. A career the launcher gave no needs for, a card whose
// slot is not listed and a settings.json with no resume_needs are never gated.
//
// The cards' layout was checked on the KB build, Steam 1.0.145 and EA 1.0.145 (the test agent reads the same fields).
// SWOFTY_RESUME_GATE=0 turns the gate off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (resume gate)";
constexpr const char *DRIVER_VT = "vt_Game_CareerContinueDriverViewModel";
constexpr const char *MY_TEAM_VT = "vt_Game_CareerContinueMyTeamViewModel";
constexpr uintptr_t CARDS = 0x328, CARD_SIZE = 0x540, CARD_COUNT = 3;

namespace bp = runtime::build_proof;
namespace guards = swofty_core::guards;
namespace rg = career::resume_gate;
namespace mem = swofty::mem;

constexpr bp::Proof CARD_LAYOUT{"the resume screens' cards (+0x328, 0x540 each; has_save +0, slot id +8)",
                                bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea_1_0_145)};
constexpr bp::Gate CARD_GATE{"the resume gate's card reads", &CARD_LAYOUT};

using Handle = uint64_t(void *vm, void *cmd, void *value);
Handle *orig_driver = nullptr, *orig_my_team = nullptr;

std::mutex lock;
rg::Table table;
bool table_read = false;
rg::State state;
int logged = 0;

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

bool card_slot(void *vm, void *value, uint32_t &slot)
{
    uint8_t card = 0;
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

uint64_t entry(Handle *orig, bool my_team, void *vm, void *cmd, void *value)
{
    uint32_t hash = command_hash(cmd);
    if (hash == rg::CMD_TRANSITION_IN || hash == rg::CMD_BACK_PRESSED) {
        std::lock_guard<std::mutex> g(lock);
        rg::screen_changed(state);
    } else if (hash == rg::CMD_SLOT_PRESSED) {
        uint32_t slot = 0;
        if (card_slot(vm, value, slot)) {
            std::string key = runtime::career_saves_slot_key(slot, my_team);
            std::string title, body;
            {
                std::lock_guard<std::mutex> g(lock);
                const rg::Entry *e = rg::find(needs_locked(), key);
                bool block = rg::press(e, key, state) == rg::Verdict::Block;
                if (logged++ < 20)
                    swofty::log("[career] resume gate: slot %u key %s: %s%s", slot, key.empty() ? "(not listed)" : key.c_str(),
                                e ? rg::status_line(e).c_str() : "no needs given", block ? "; held for the prompt" : "");
                if (block) {
                    title = "Add-ons do not match";
                    body = rg::prompt_text(*e);
                }
            }
            if (!title.empty()) {
                runtime::popup_show(title, body, "swofty_core");
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
        out.push_back({MIXIN,
                       {{DRIVER_VT, SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 10), 100, (void *)&driver_entry,
                         (void **)&orig_driver},
                        {MY_TEAM_VT, SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 10), 100, (void *)&my_team_entry,
                         (void **)&orig_my_team}}});
    }

    void settle() override
    {
        if (offered_set(MIXIN))
            swofty::log("[career] resume gate %s; a career that needs add-ons this profile lacks asks before it loads",
                        guards::install_state(runtime::core_mixins_active(MIXIN), "armed"));
    }
};

ResumeGateGuard guard;

}
