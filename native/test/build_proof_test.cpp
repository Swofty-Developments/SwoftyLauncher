// Host test of the per-build proof gates (core/runtime/build_proof.hpp): the KB build, Steam 1.0.145, EA App 1.0.145 and an
// unknown build.

#include "core/runtime/build_proof.hpp"

#include <cstdio>
#include <cstring>
#include <set>
#include <string>

namespace bp = runtime::build_proof;

static int failed;

static void expect(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    failed += !ok;
}

static constexpr bp::Exe KB{1789118616u, 0x6a00000}, KB_SMALL{1789118616u, 0x6000000}, EA{1789128892u, 0x155dc000},
    STEAM_145{1794312636u, 0x18434000}, STEAM_145_RESIZED{1794312636u, 0x18435000}, EA_145{1794315435u, 0x1615e000},
    UNKNOWN{1799999999u, 0x18500000};

static constexpr bp::Proof KB_AND_145{"the test layout at +0x10", bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};
static constexpr const char *BINDINGS[] = {"Game_Test_First", "Game_Test_Second"};
static constexpr bp::Gate PORTED{"the ported feature", &KB_AND_145, BINDINGS};
static constexpr bp::Gate KB_ONLY{"the kb feature", &bp::proofs::KB_LAYOUT};
static constexpr bp::Gate NO_PROOF{"the unproven feature", nullptr};

static_assert(bp::identify(KB) == bp::Build::kb && bp::identify(STEAM_145) == bp::Build::steam_1_0_145);
static_assert(bp::identify(UNKNOWN) == bp::Build::unknown);
static_assert(bool(bp::check(PORTED, bp::Build::steam_1_0_145, [](const char *) { return true; })));
static_assert(!bp::check(PORTED, bp::Build::unknown, [](const char *) { return true; }));

int main()
{
    expect(bp::identify(KB) == bp::Build::kb, "the KB build is known by its PE timestamp and a large image");
    expect(bp::identify(KB_SMALL) == bp::Build::unknown,
           "the KB timestamp with an image no larger than 0x6000000 is not the KB build");
    expect(bp::identify(EA) == bp::Build::ea, "the EA App link is known by its timestamp and exact image size");
    expect(bp::identify(STEAM_145) == bp::Build::steam_1_0_145, "Steam 1.0.145 is known by its timestamp and exact image size");
    expect(bp::identify(STEAM_145_RESIZED) == bp::Build::unknown, "the 1.0.145 timestamp with another image size is unknown");
    expect(bp::identify(EA_145) == bp::Build::ea_1_0_145, "EA App 1.0.145 is known by its timestamp and exact image size");
    expect(bp::identify(UNKNOWN) == bp::Build::unknown && bp::identify({0, 0}) == bp::Build::unknown,
           "an exe the table does not list is unknown");

    std::set<std::string> resolved = {"Game_Test_First", "Game_Test_Second"};
    auto resolves = [&](const char *id) { return resolved.count(id) != 0; };

    expect(bool(bp::check(PORTED, bp::Build::kb, resolves)), "a gate holds on the KB build when its proof names it");
    expect(bool(bp::check(PORTED, bp::Build::steam_1_0_145, resolves)), "a gate holds on 1.0.145 when its proof names it");
    bp::Verdict ea = bp::check(PORTED, bp::Build::ea, resolves);
    expect(!ea && ea.refusal == bp::Refusal::build, "a known build the proof does not name refuses");
    bp::Verdict unknown = bp::check(PORTED, bp::Build::unknown, resolves);
    expect(!unknown && unknown.refusal == bp::Refusal::build, "an unknown build refuses");

    expect(bool(bp::check(KB_ONLY, bp::Build::kb, resolves)), "a KB layout gate holds on the KB build");
    expect(!bp::check(KB_ONLY, bp::Build::steam_1_0_145, resolves), "a KB layout gate refuses on 1.0.145");
    expect(!bp::check(KB_ONLY, bp::Build::unknown, resolves), "a KB layout gate refuses on an unknown build");
    expect(!bp::check(NO_PROOF, bp::Build::kb, resolves), "a gate without a proof refuses everywhere");

    expect(bp::proven_on(bp::proofs::VERIFIED_LAYOUT, bp::Build::kb) &&
               bp::proven_on(bp::proofs::VERIFIED_LAYOUT, bp::Build::ea) &&
               bp::proven_on(bp::proofs::VERIFIED_LAYOUT, bp::Build::steam_1_0_145) &&
               !bp::proven_on(bp::proofs::VERIFIED_LAYOUT, bp::Build::ea_1_0_145) &&
               !bp::proven_on(bp::proofs::VERIFIED_LAYOUT, bp::Build::unknown),
           "the verified layout holds on the KB, EA App KB and Steam 1.0.145 builds and no other");

    resolved.erase("Game_Test_Second");
    bp::Verdict missing = bp::check(PORTED, bp::Build::steam_1_0_145, resolves);
    expect(!missing && missing.refusal == bp::Refusal::binding && !strcmp(missing.binding, "Game_Test_Second"),
           "a proven build refuses when a binding does not resolve, naming it");

    std::string line = bp::refusal_line(PORTED, bp::Build::unknown, UNKNOWN, unknown);
    expect(line == "the ported feature off on an unknown build (PE timestamp 1799999999, image size 0x18500000): the test layout "
                   "at +0x10 is proven on the Steam KB build, Steam 1.0.145 (26566308) only",
           "an unknown build's line names the exe and the builds the proof holds on");
    line = bp::refusal_line(KB_ONLY, bp::Build::steam_1_0_145, STEAM_145, bp::check(KB_ONLY, bp::Build::steam_1_0_145, resolves));
    expect(line == "the kb feature off on Steam 1.0.145 (26566308): the KB build's layout is proven on the Steam KB build only",
           "a known build's line names it");
    line = bp::refusal_line(PORTED, bp::Build::steam_1_0_145, STEAM_145, missing);
    expect(line == "the ported feature off on Steam 1.0.145 (26566308): Game_Test_Second does not resolve",
           "a missing binding's line names the binding");

    printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
