#include "core/boxes/picker/picker.hpp"
#include "core/guards/guard.hpp"

#include "core/runtime/runtime.hpp"

#include "core/apply/content/appliers.hpp"
#include "core/apply/facts.hpp"
#include "core/apply/scheduler.hpp"
#include "core/boxes/box_options.hpp"

#include <cstdio>
#include <cstring>

// Career season rules (docs/std/systems/career.md#the-season-s-rules): a Driver Career or My Team session load puts the box
// rules of the career's season (apply/content/career_rules.hpp) into the career's current game setup mode
// (EntrantsManager +0x2b0), through the same swaps a box press makes in swofty_f1: the DRS group, tyre warmers,
// mandatory tyre change, race time limit and race points, and the fuel, grid and qualifying tyre rules the session
// hooks read. The weekend helper exchange, the game options and the rev lights stay as the career has them (the safety
// car option only by default, see below).
// A load of the same career, season, box and setup mode keeps what the last load put in, unless a put back ran since
// (put_back_drs); any other load puts the last rules back first (leave_front_end), and the career's unload puts them
// back (put_back_career_rules). The swaps are dropped with the setup modes when Sess_Apply_ContentAvailability rebuilds
// them (drop_setup_pointers).
// SWOFTY_CAREER_RULES=0 turns it off.
// SWOFTY_CAREER_SAFETY_CAR=1 also runs the season box's rules.safety_car: the load sets the options store's
// safety_car_frequency (box_options.cpp, the option a box press sets and the safety car logic's deployment points
// condition reads every update), and the put back on unload or on another load gives the player's index back.

namespace era_picker {

namespace {

struct Plan {
    bool career = false, off = false;
    int year = 0;
    uint8_t *mode = nullptr;
    std::string host, key;
    swofty_core::apply::content::CareerBox box;
};

struct CareerRules {
    Plan pending;
    std::string applied;
};

CareerRules &rules()
{
    static CareerRules r;
    return r;
}

bool safety_car_on()
{
    char v[16];
    return swofty_core::guards::env_off("SWOFTY_CAREER_SAFETY_CAR", v, [](const char *s) { return s && !strcmp(s, "1"); });
}

// The season box's rules.safety_car into the options store, when the plan runs it.
void apply_career_safety_car(const RuleTarget &t, const swofty_core::apply::content::CareerBox &box)
{
    namespace content = swofty_core::apply::content;
    bool runs = false;
    for (auto &r : box.runs)
        runs = runs || r == "safety_car";
    if (!runs)
        return;
    const int rule = content::box_option_index("safety_car");
    auto pick = content::box_options_for_box(box.entrant_list);
    if (rule < 0 || !pick.settings[size_t(rule)]) {
        box_rules.box_options_log.say(t.prefix + "safety_car: the box holds no setting the option takes");
        return;
    }
    box_rules.box_options_log.say(t.prefix + "safety_car " + *pick.settings[size_t(rule)] + ": " +
                                  runtime::box_option_apply(size_t(rule), *pick.settings[size_t(rule)]));
}

uint8_t *current_setup_mode(uintptr_t ctx, std::string &name)
{
    uintptr_t holder = 0, setup = 0, gsm = 0, gsm_holder = 0, gsm_name = 0;
    if (!mem::read(ctx + MODE_SETUP, holder) || !holder || !mem::read(holder + SETUP_MODES, setup) || !setup ||
        !mem::read(setup + SETUP_GAME_SETUP_MODE, gsm) || !gsm)
        return nullptr;
    if (mem::read(gsm + SETUP_MODE_NAME, gsm_holder) && gsm_holder && mem::read(gsm_holder + 0x10, gsm_name))
        mem::c_string(gsm_name, name, 64);
    return reinterpret_cast<uint8_t *>(gsm);
}

}

CareerSession career_session(uintptr_t ctx)
{
    Plan p;
    std::string mode = swofty_core::apply::facts::mode_of(mode_label(ctx));
    p.career = mode == "driver_career" || mode == "my_team";
    if (p.career) {
        char off[16];
        p.off = swofty_core::guards::env_off("SWOFTY_CAREER_RULES", off);
        if (p.off) {
            p.box.line = std::string("off (SWOFTY_CAREER_RULES=") + off + "): the career keeps the game's rules";
        } else {
            p.mode = current_setup_mode(ctx, p.host);
            if (p.host.empty())
                p.host = "the career's setup mode";
            swofty_core::registry::Context c = swofty_core::apply::scheduler().context();
            p.year = c.has_year ? c.year : 0;
            p.box = swofty_core::apply::content::career_box_now(c, safety_car_on());
            if (!p.box.package.empty() && !p.box.runs.empty() && p.mode) {
                char at[32];
                std::snprintf(at, sizeof at, "%p", static_cast<void *>(p.mode));
                p.key = p.box.line + "|" + p.host + "|" + at;
            }
        }
    }
    CareerSession s{p.career, p.career && !p.key.empty() && p.key == rules().applied};
    rules().pending = std::move(p);
    return s;
}

void apply_career_rules()
{
    Plan p = std::move(rules().pending);
    rules().pending = {};
    if (!p.career)
        return;
    if (!p.key.empty() && p.key == rules().applied)
        return;
    rules().applied.clear();
    if (p.key.empty()) {
        if (!p.off && !p.box.package.empty() && !p.mode)
            p.box.line += "; not applied (no game setup mode is current)";
        box_rules.career_log.say(p.box.line);
        return;
    }
    RuleTarget t;
    t.modes.push_back(p.mode);
    t.host = p.host;
    t.prefix = "career season " + std::to_string(p.year) + ": ";
    t.weekend = false;
    box_rules.host = t.host;
    box_rules.career_log.say(p.box.line + "; into " + p.host);
    const char *list = p.box.entrant_list.c_str();
    apply_drs(t, list);
    apply_warmers(t, list);
    apply_tyre_change(t, list);
    apply_fuel(t, list);
    apply_weekend(t, nullptr, list);
    apply_points(t, list);
    apply_grid(t, list);
    apply_quali_tyres(t, list);
    apply_career_safety_car(t, p.box);
    rules().applied = p.key;
}

void put_back_career_rules(const char *why)
{
    std::lock_guard<std::mutex> g(hold.lock);
    rules().pending = {};
    if (rules().applied.empty())
        return;
    put_back_drs();
    clear_fuel();
    box_rules.career_log.say(std::string(why) + ": the career season's rules put back");
}

void forget_career_rules() { rules().applied.clear(); }

}

namespace runtime {

void era_career_unloaded() { era_picker::put_back_career_rules("the career unloaded"); }

}
