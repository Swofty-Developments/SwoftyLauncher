#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <span>
#include <string>

// The game builds the core knows by their PE header, and the proofs that tie code to them. A proof names the builds on
// which a layout (struct offsets, byte patterns, data addresses) was checked against the KB build (re/tools/sigport.py
// --compare, sigverify.py --disp). A gate names the proof its code needs and the bindings it calls: it holds on an exe
// only when that exe is one of the proof's builds and every binding resolves there. An exe the table does not know
// holds no proof, so code gated this way stays off on a game update until its layout is checked again.
namespace runtime::build_proof {

enum class Build : uint8_t { kb, ea, steam_1_0_145, ea_1_0_145, unknown };

struct Exe {
    uint32_t pe_timestamp, size_of_image;
};

struct Known {
    Build build;
    const char *name;
    uint32_t pe_timestamp, size_of_image;
    // The KB build's entry takes any image larger than size_of_image: its packed and unpacked images differ in size.
    bool larger;
};

inline constexpr Known KNOWN[] = {
    {Build::kb, "the Steam KB build", 1789118616u, 0x6000000, true},
    {Build::ea, "the EA App KB build", 1789128892u, 0x155dc000, false},
    {Build::steam_1_0_145, "Steam 1.0.145 (26566308)", 1794312636u, 0x18434000, false},
    {Build::ea_1_0_145, "EA App 1.0.145", 1794315435u, 0x1615e000, false},
};

constexpr const Known *known(Build b)
{
    for (const Known &k : KNOWN)
        if (k.build == b)
            return &k;
    return nullptr;
}

constexpr Build identify(Exe e)
{
    for (const Known &k : KNOWN)
        if (e.pe_timestamp == k.pe_timestamp &&
            (k.larger ? e.size_of_image > k.size_of_image : e.size_of_image == k.size_of_image))
            return k.build;
    return Build::unknown;
}

constexpr uint32_t bit(Build b) { return 1u << uint32_t(b); }

struct Proof {
    // What was checked, as the refusal line names it: "the StateIntro layout (+0xcd0, +0xef3, +0xef4)".
    const char *what;
    uint32_t builds;
};

constexpr bool proven_on(const Proof &p, Build b) { return b != Build::unknown && (p.builds & bit(b)); }

// The proofs more than one feature shares. A feature's own proof sits beside its code.
namespace proofs {

// Offsets read from the KB build's headers and never checked on another build.
inline constexpr Proof KB_LAYOUT{"the KB build's layout", bit(Build::kb)};
// The struct layouts behind the signatures' functions (re/tools/sigverify.py) on every build the core knows.
inline constexpr Proof VERIFIED_LAYOUT{"the signature functions' struct layouts",
                                       bit(Build::kb) | bit(Build::ea) | bit(Build::steam_1_0_145)};

}

struct Gate {
    const char *feature;
    const Proof *proof;
    std::span<const char *const> bindings = {};
};

enum class Refusal : uint8_t { none, build, binding };

struct Verdict {
    Refusal refusal = Refusal::none;
    // The first binding that did not resolve.
    const char *binding = nullptr;
    explicit constexpr operator bool() const { return refusal == Refusal::none; }
};

template <class Resolves> constexpr Verdict check(const Gate &g, Build b, Resolves &&resolves)
{
    if (!g.proof || !proven_on(*g.proof, b))
        return {Refusal::build, nullptr};
    for (const char *id : g.bindings)
        if (!resolves(id))
            return {Refusal::binding, id};
    return {};
}

inline std::string exe_name(Build b, Exe e)
{
    if (const Known *k = known(b))
        return k->name;
    char buf[96];
    snprintf(buf, sizeof buf, "an unknown build (PE timestamp %u, image size 0x%x)", e.pe_timestamp, e.size_of_image);
    return buf;
}

inline std::string proven_builds(const Proof &p)
{
    std::string out;
    for (const Known &k : KNOWN)
        if (p.builds & bit(k.build))
            out += (out.empty() ? "" : ", ") + std::string(k.name);
    return out.empty() ? "no build" : out;
}

// "<feature> off on <exe>: <what> is proven on <builds> only" or "...: <binding> does not resolve".
inline std::string refusal_line(const Gate &g, Build b, Exe e, const Verdict &v)
{
    std::string head = std::string(g.feature) + " off on " + exe_name(b, e) + ": ";
    if (v.refusal == Refusal::binding)
        return head + v.binding + " does not resolve";
    if (!g.proof)
        return head + "it names no proof";
    return head + g.proof->what + " is proven on " + proven_builds(*g.proof) + " only";
}

}
