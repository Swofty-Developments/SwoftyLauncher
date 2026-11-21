#include "core/boxes/picker/picker.hpp"
#include "core/guards/guard.hpp"

#include "core/runtime/runtime.hpp"

#include "core/apply/content/appliers.hpp"
#include "core/boxes/box_grid.hpp"
#include "core/boxes/box_options.hpp"
#include "core/hooks/race_fuel_rules.hpp"

namespace era_picker {

namespace {

constinit Cap<LOG_FIRST> put_back_failed_lines;

}

RuleTarget host_target(void *mode)
{
    RuleTarget t;
    t.host = logic::HOST;
    SetupModes modes = setup_modes(mode);
    if (!modes)
        return t;
    for (auto **p = modes.begin; p != modes.end; ++p) {
        const char *name = name_of(*p, SETUP_MODE_NAME);
        if (name && !strcmp(name, logic::HOST))
            t.modes.push_back(*p);
    }
    return t;
}

void apply_drs(const RuleTarget &t, const char *list)
{
    auto box = swofty_core::apply::content::drs_for_box(list, t.host);
    if (box.label.empty()) {
        box_rules.drs.put_back();
        std::string own;
        for (uint8_t *m : t.modes) {
            auto *holder = at<uint8_t *>(m, logic::MODE_DRS_GROUP);
            const char *text = holder && mem::readable(holder + logic::HOLDER_TEXT, 8) ? logic::holder_text(holder) : nullptr;
            if (own.empty() && text && mem::readable(text, 1))
                own = std::string(text, strnlen(text, logic::DRS_LABEL_MAX));
        }
        box_rules.drs_log.say(t.prefix + box.line + (own.empty() ? std::string() : " (" + own + ")"));
        return;
    }
    logic::DrsHolder *holder = box_rules.drs.holder(box.label);
    for (uint8_t *m : t.modes)
        box_rules.drs.swap(m, holder);
    box_rules.drs_log.say(t.prefix +
                          (holder && !t.modes.empty() ? box.line : box.line + "; not applied (no " + t.host + " mode found)"));
}

// Tyre warmers per box: the pressed box's rules.tyre_warmers goes into every target mode's has_tyre_warmers byte;
// a box without the rule puts the game's value back.
void apply_warmers(const RuleTarget &t, const char *list)
{
    auto box = swofty_core::apply::content::tyre_warmers_for_box(list, t.host);
    if (!box.warmers) {
        box_rules.warmers.put_back();
        box_rules.warmers_log.say(t.prefix + box.line);
        return;
    }
    for (uint8_t *m : t.modes)
        box_rules.warmers.swap(m, *box.warmers);
    box_rules.warmers_log.say(t.prefix +
                              (!t.modes.empty() ? box.line : box.line + "; not applied (no " + t.host + " mode found)"));
}

// Rev lights per box: the stat id of the engine most of the pressed box's vehicles name goes into every swofty_f1 mode's
// equal_performance_engine, the row the wheel LEDs and the HUD rev bar take for a car running equal performance; a box
// whose vehicles name no engine, or an engine the database lacks, puts the game's engine back. SWOFTY_REV_LIGHTS=0 turns
// it off.
void apply_rev_lights(void *mode, const char *list)
{
    const RuleTarget t = host_target(mode);
    char off[16];
    if (swofty_core::guards::env_off("SWOFTY_REV_LIGHTS", off)) {
        box_rules.rev_lights.put_back();
        box_rules.rev_lights_log.say(std::string("off (SWOFTY_REV_LIGHTS=") + off + "): " + std::string(logic::HOST) +
                                     " keeps its own equal-performance engine");
        return;
    }
    auto box = swofty_core::apply::content::rev_lights_for_box(list);
    if (box.engine.empty()) {
        box_rules.rev_lights.put_back();
        box_rules.rev_lights_log.say(box.line);
        return;
    }
    const SwoftyDb *db = runtime::db_table();
    SwoftyDbCursor row{};
    SwoftyDbValue stat{};
    if (!db || db->row("entrants.engines", box.engine.c_str(), &row) != SWOFTY_ASSET_OK ||
        db->cell(&row, "stat_id", &stat) != SWOFTY_ASSET_OK || stat.tag != SWOFTY_DB_INT || stat.i < 0 || stat.i > 0xff) {
        box_rules.rev_lights.put_back();
        box_rules.rev_lights_log.say(box.line + "; not applied (no entrants.engines row " + box.engine + " with a stat_id)");
        return;
    }
    size_t modes = t.modes.size(), swapped = 0;
    for (uint8_t *m : t.modes)
        if (box_rules.rev_lights.swap(m, uint8_t(stat.i)))
            swapped++;
    std::string line = box.line + " (stat id " + std::to_string(stat.i) + ")";
    if (!modes)
        line += "; not applied (no " + std::string(logic::HOST) + " mode found)";
    else if (!swapped)
        line += "; not needed (no " + std::string(logic::HOST) + " mode names an equal-performance engine)";
    box_rules.rev_lights_log.say(line);
}

// Fuel rules per box: the pressed box's refuelling, race and qualifying fuel and parc ferme rule go to the per-frame fuel
// plan (hooks/car_live.cpp); a stock box press or leaving the boxes hands over no rules.
void apply_fuel(const RuleTarget &t, const char *list)
{
    auto rules = swofty_core::apply::content::fuel_rules_for_box(list);
    rules.line = t.prefix + rules.line;
    swofty_core::hooks::race_fuel_rules(rules);
}

void clear_fuel() { swofty_core::hooks::race_fuel_rules({}); }

// Race time limit and weekend per box: rules.race_time_limit_s goes into every target mode's
// race_time_limit_in_seconds; rules.weekend_mode's weekend options are exchanged with the first target mode's (a box
// press only: a career target keeps its weekends). A box without a rule puts the game's value back.
void apply_weekend(const RuleTarget &t, void *mode, const char *list)
{
    auto box = swofty_core::apply::content::weekend_for_box(list, t.host);
    std::string line = box.line;
    if (!box.time_limit_s) {
        box_rules.time_limits.put_back();
    } else {
        for (uint8_t *m : t.modes)
            box_rules.time_limits.swap(m, *box.time_limit_s);
        if (t.modes.empty())
            line += "; time limit not applied (no " + t.host + " mode found)";
        else
            line += "; time limit written into " + std::to_string(t.modes.size()) + " " + t.host +
                    " mode(s) (race_time_limit_in_seconds now " +
                    std::to_string(at<uint32_t>(t.modes.front(), logic::MODE_RACE_TIME_LIMIT)) + " s)";
    }
    if (box.weekend_mode.empty() || !t.weekend) {
        box_rules.weekends.put_back();
        if (!box.weekend_mode.empty())
            line += "; weekend not applied (a career keeps its own weekends)";
    } else {
        uint8_t *host = t.modes.empty() ? nullptr : t.modes.front();
        size_t hosts = t.modes.size();
        std::string where;
        uint8_t *helper = find_helper_mode(mode, box.weekend_mode, where);
        if (!helper) {
            box_rules.weekends.put_back();
            line += "; weekend not applied (no setup mode " + box.weekend_mode + " in " + where + ")";
        } else if (!box_rules.weekends.swap(host, helper)) {
            line += "; weekend not applied (no " + t.host + " mode found)";
        } else {
            line += "; weekend applied (" + box.weekend_mode + " from " + where + ")";
            if (hosts > 1)
                line += "; weekend on the first of " + std::to_string(hosts) + " " + t.host + " modes";
        }
    }
    box_rules.weekend_log.say(t.prefix + line);
}

// Game options per box: each of the pressed box's option rules goes into its options store option; a rule the box
// does not carry gives the player's index back.
void apply_box_options(const char *list)
{
    namespace content = swofty_core::apply::content;
    auto box = content::box_options_for_box(list);
    std::string line = box.line, put;
    for (size_t i = 0; i < content::BOX_OPTION_COUNT; ++i) {
        if (box.settings[i])
            line += "; " + std::string(content::BOX_OPTION_RULES[i].rule) + ": " + runtime::box_option_apply(i, *box.settings[i]);
        else if (runtime::box_option_put_back(i))
            put += (put.empty() ? "" : ", ") + std::string(content::BOX_OPTION_RULES[i].rule);
    }
    if (!put.empty())
        line += "; the player's " + put + " setting put back";
    box_rules.box_options_log.say(line);
}

void put_back_box_options()
{
    namespace content = swofty_core::apply::content;
    std::string put;
    for (size_t i = 0; i < content::BOX_OPTION_COUNT; ++i)
        if (runtime::box_option_put_back(i))
            put += (put.empty() ? "" : ", ") + std::string(content::BOX_OPTION_RULES[i].rule);
    if (!put.empty())
        box_rules.box_options_log.say("the player's " + put + " setting put back");
}

// Grid rules per box: the pressed box's rules.grid goes to box_grid.cpp; a box without the rule clears it.
void apply_grid(const RuleTarget &t, const char *list)
{
    auto box = swofty_core::apply::content::grid_for_box(list);
    runtime::box_grid_set(box.rule, list);
    box_rules.grid_log.say(t.prefix + box.line);
}

// Qualifying tyres per box: the pressed box's rules.qualifying_tyres mask goes to the qualifying tyre hooks
// (quali_tyres_guard.cpp), which refuse those options in race sessions; a box without the rule stores 0.
void apply_quali_tyres(const RuleTarget &t, const char *list)
{
    auto box = swofty_core::apply::content::quali_tyres_for_box(list);
    runtime::quali_tyres_box(box.mask, box.mask ? t.prefix + box.line : std::string());
}

// Points per box: the race sessions of every target mode point at the preset the boot apply added for the pressed
// box's rules.points, found among the parsed presets by its points; a box without the rule puts the game's back.
void apply_points(const RuleTarget &t, const char *list)
{
    auto box = swofty_core::apply::content::points_for_box(list, t.host);
    if (!box.found) {
        box_rules.points.put_back();
        box_rules.points_log.say(t.prefix + box.line);
        return;
    }
    logic::Readable ok = [](const void *p, size_t n) { return mem::readable(p, n); };
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_Scenarios") : 0;
    const uint8_t *scenarios = nullptr, *begin = nullptr, *end = nullptr;
    logic::peek(ok, reinterpret_cast<const void *>(slot), 0, scenarios);
    logic::peek(ok, scenarios, logic::SCENARIOS_PRESETS_BEGIN, begin);
    logic::peek(ok, scenarios, logic::SCENARIOS_PRESETS_END, end);
    logic::PointsWant want{box.full, box.reduced, box.bonus, box.rule};
    const uint8_t *preset = logic::find_points_preset(ok, begin, end, want);
    if (!preset) {
        box_rules.points.put_back();
        box_rules.points_log.say(t.prefix + box.line + "; not applied (preset " + box.label +
                                 " not among the game's parsed presets)");
        return;
    }
    size_t sessions = 0;
    for (uint8_t *m : t.modes)
        for (uint64_t *s : logic::race_sessions(ok, m)) {
            box_rules.points.swap(s, reinterpret_cast<uint64_t>(preset));
            sessions++;
        }
    box_rules.points_log.say(t.prefix + (sessions ? box.line + " on " + std::to_string(sessions) + " race session(s)"
                                                  : box.line + "; not applied (no race session in a " + t.host + " mode)"));
}

// Mandatory tyre change per box: the pressed box's rules.mandatory_tyre_change goes into every target mode's
// tyre regulation requirement flags; a box without the rule puts the game's flags back.
void apply_tyre_change(const RuleTarget &t, const char *list)
{
    auto box = swofty_core::apply::content::tyre_change_for_box(list, t.host);
    if (!box.required) {
        box_rules.tyre_change.put_back();
        box_rules.tyre_change_log.say(t.prefix + box.line);
        return;
    }
    for (uint8_t *m : t.modes)
        box_rules.tyre_change.swap(m, *box.required);
    box_rules.tyre_change_log.say(t.prefix +
                                  (!t.modes.empty() ? box.line : box.line + "; not applied (no " + t.host + " mode found)"));
}

void put_back_drs()
{
    forget_career_rules();
    runtime::box_grid_set(std::nullopt, std::string());
    runtime::quali_tyres_box(0, std::string());
    const std::string &h = box_rules.host;
    if (size_t n = box_rules.drs.put_back())
        box_rules.drs_log.say(h + "'s DRS group put back on " + std::to_string(n) + " mode(s)");
    if (size_t n = box_rules.warmers.put_back())
        box_rules.warmers_log.say(h + "'s tyre warmer setting put back on " + std::to_string(n) + " mode(s)");
    if (size_t n = box_rules.rev_lights.put_back())
        box_rules.rev_lights_log.say(h + "'s equal-performance engine put back on " + std::to_string(n) + " mode(s)");
    if (size_t n = box_rules.tyre_change.put_back())
        box_rules.tyre_change_log.say(h + "'s tyre regulations put back on " + std::to_string(n) + " mode(s)");
    if (size_t n = box_rules.time_limits.put_back())
        box_rules.weekend_log.say(h + "'s race time limit put back on " + std::to_string(n) + " mode(s)");
    if (box_rules.weekends.put_back())
        box_rules.weekend_log.say(h + "'s own weekend put back");
    if (size_t n = box_rules.points.put_back())
        box_rules.points_log.say(h + "'s points put back on " + std::to_string(n) + " race session(s)");
    put_back_box_options();
}

void put_back_failed(const Narrowed &n)
{
    if (!put_back_failed_lines.note())
        return;
    std::vector<std::string> names;
    for (void *list : logic::left_out(n)) {
        std::string name = list_cars(list).name;
        names.push_back(name.empty() ? "?" : name);
    }
    int64_t now = -1;
    if (n.setup)
        mem::read(reinterpret_cast<uintptr_t>(n.setup) + LIST_COUNT, now);
    swofty::log("[era] %s", logic::put_back_failed_line(names, now, n.kept, n.time_trial).c_str());
}

}
