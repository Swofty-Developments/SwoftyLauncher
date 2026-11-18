#include "ai_corner_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <cstdint>
#include <cstring>
#include <vector>

// AI ERS corner guard (re/db/globals.tsv site_Ai_Ers_CornerLineRead).
//
// Ai_Ers_DecidePushReasons checks whether the corner a car is heading for is one of the ERS boost entry's corner_id
// list. It takes the corner from the car's gap evaluation for the next corner (Ai_Overtake_EvaluateGap from the
// current corner end), whose first field is the next corner marker the route accepts between that corner's start
// and end lines. When the next marker is outside that bracket, the evaluation stores 0 there with its valid flag
// still set. A track whose AI line has corner markers that its corner start and end lines do not enclose, such as
// Brands Hatch Grand Prix (mod.brhgp 1.8.0), leaves that 0, and mov r8d,[r8+0x10] at F1_25.exe+0x649819
// (Ai_Ers_DecidePushReasons+0x1039) reads the marker's id without checking it: ACCESS_VIOLATION reading 0x10.
//
// At site_Ai_Ers_CornerLineRead a zero marker continues at site_Ai_Ers_NoCornerMatch, the path the game takes for an
// entry with no corner ids or no id that matches, so the car keeps its ERS decision without a corner bonus for that
// corner. A marker that is there runs the game's code unchanged. The first skip is logged.
//
// SWOFTY_AI_ERS_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (ai ers corner guard)";
constexpr const char *READ = "site_Ai_Ers_CornerLineRead";
constexpr const char *NONE = "site_Ai_Ers_NoCornerMatch";

uintptr_t no_match = 0;
bool offered = false, settled = false;
runtime::ai_corner::Tally tally;

void corner_read(SwoftyContext *c)
{
    if (!runtime::ai_corner::missing(c->r8))
        return;
    c->rip = no_match;
    if (tally.first())
        swofty::log("[ai] ers corner: car record +0x%llx has no corner marker between the next corner's start and end "
                    "lines on this track's AI line; its ERS decision skips that corner's bonus",
                    static_cast<unsigned long long>(c->rbx));
}

}

namespace runtime {

std::vector<CoreMixins> ai_ers_guard_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_AI_ERS_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[ai] ers corner guard off (SWOFTY_AI_ERS_GUARD=%s)", v);
        return out;
    }
    no_match = swofty::api->resolve(NONE);
    if (!no_match || !swofty::api->resolve(READ)) {
        no_match = 0;
        swofty::log("[ai] ers corner guard off: needs %s and %s, which this build does not resolve", READ, NONE);
        return out;
    }
    out.push_back({MIXIN, {{READ, SWOFTY_MIXIN_SITE, 100, (void *)&corner_read, nullptr}}});
    return out;
}

void ai_ers_guard_commit()
{
    if (!offered || settled || !no_match)
        return;
    settled = true;
    swofty::log("[ai] ers corner guard %s",
                core_mixins_active(MIXIN) ? "armed: a car with no corner marker for its next corner keeps its ERS "
                                            "decision without that corner's bonus"
                                          : "did not install (see the [core] lines)");
}

}
