// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

#include "json_strings.hpp"

namespace f1990 {

inline constexpr size_t kLookDepartments = 5;
inline constexpr std::string_view kCareerPackagePrefix = "#overrides.career";
inline constexpr std::string_view kDeptIconAtlas = ".rnd_dept_icons_small.";

// data/rnd-look.json: the 1990 uidb package and, per department code (ENUM A: 0 aero, 1 chassis, 2 durability,
// 3 engine, 4 tyres), the colour and the light tint, as the game's ABGR words.
struct Look {
    std::string package;
    uint32_t colour[kLookDepartments] = {};
    uint32_t light[kLookDepartments] = {};
    unsigned colours = 0, lights = 0;

    bool complete() const
    {
        return !package.empty() && colours == (1u << kLookDepartments) - 1 && lights == (1u << kLookDepartments) - 1;
    }
};

inline bool parse_rgb(std::string_view text, uint32_t &abgr)
{
    if (text.size() != 7 || text[0] != '#')
        return false;
    uint32_t rgb = 0;
    for (char c : text.substr(1)) {
        rgb <<= 4;
        if (c >= '0' && c <= '9')
            rgb |= uint32_t(c - '0');
        else if (c >= 'a' && c <= 'f')
            rgb |= uint32_t(c - 'a' + 10);
        else if (c >= 'A' && c <= 'F')
            rgb |= uint32_t(c - 'A' + 10);
        else
            return false;
    }
    abgr = 0xff000000u | (rgb & 0xff) << 16 | (rgb & 0xff00) | (rgb >> 16 & 0xff);
    return true;
}

inline bool read_look(const std::vector<StringLeaf> &leaves, Look &out)
{
    out = Look{};
    for (auto &l : leaves) {
        if (l.path.size() == 1 && l.path[0] == "package") {
            if (std::string_view(l.text).substr(0, kCareerPackagePrefix.size()) == kCareerPackagePrefix)
                out.package = l.text;
            continue;
        }
        if (l.path.size() != 3 || l.path[0] != "departments" || l.path[1].size() != 1)
            continue;
        unsigned code = unsigned(l.path[1][0] - '0');
        uint32_t abgr = 0;
        if (code >= kLookDepartments || !parse_rgb(l.text, abgr))
            continue;
        if (l.path[2] == "colour") {
            out.colour[code] = abgr;
            out.colours |= 1u << code;
        } else if (l.path[2] == "light") {
            out.light[code] = abgr;
            out.lights |= 1u << code;
        }
    }
    return out.complete();
}

// A department colour for the department byte a colour store or a pie slot carries; anything past Tyres keeps
// the game's value.
inline bool look_colour(const Look &look, uint64_t dept, bool light, uint32_t &out)
{
    if (dept >= kLookDepartments)
        return false;
    out = light ? look.light[dept] : look.colour[dept];
    return true;
}

// The R&D atlas paths are <package>.<atlas>.<frame>; the package the game passes is a career one
// (#overrides.career.rnd_om_xm in the 2026 lists, #overrides.career in the older ones).
inline bool is_career_package(std::string_view pkg, std::string_view ours)
{
    if (pkg.substr(0, kCareerPackagePrefix.size()) != kCareerPackagePrefix || pkg == ours)
        return false;
    return pkg.size() == kCareerPackagePrefix.size() || pkg[kCareerPackagePrefix.size()] == '.';
}

// The department card's icon path <package>.rnd_dept_icons_small.<frame> with the 1990 package; empty when the
// path is not one.
inline std::string dept_icon_path(std::string_view path, std::string_view ours)
{
    size_t at = path.find(kDeptIconAtlas);
    if (at == std::string_view::npos || !is_career_package(path.substr(0, at), ours))
        return {};
    std::string out(ours);
    out += path.substr(at);
    return out;
}

// The pie's wheel-icon format also draws the shutdown and regulation slots, which keep their own colours.
inline bool pie_slot_is_department(std::string_view frame)
{
    return !frame.empty() && frame != "shutdown" && frame != "regulation";
}

}
