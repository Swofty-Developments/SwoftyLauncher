// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the 1990 R&D look: a 1990 colour and a period icon per R&D department, only while a 1990 career is
// loaded. Its own DLL, so a binding it can't resolve drops only the look.
//
// Colours. The game takes the department colours from compiled tables indexed by the department byte (ENUM A:
// 0 aero, 1 chassis, 2 durability, 3 engine, 4 = the Tyres department of rnd-tyres-thermal). While a 1990 career is
// loaded, k_Career_RnDDeptColourTable (department tabs, purchase panel) and k_Career_RnDCardColourTable (upgrade
// cards, the performance pie) hold the 1990 colours; career.unloaded, or another career's career.loaded, puts the
// values back. The tables the code only reads through stores (R&D lists and their light twin, Team HQ R&D popups,
// the In Progress list) get the 1990 colour at the store, and the R&D > Performance tiles at the pie's wheel-icon
// format, where the slot colour [rsp+0x64] is not stored yet.
//
// Icons. Every department atlas path is "<package>.<atlas>.<frame>", the package being the career's R&D one
// (#overrides.career.rnd_om_xm). erp/rnd_look.json adds a uidb package of its own, #overrides.career.f1990_rnd,
// whose rnd_dept_icons_small, rnd_wheel_icon and rnd_department_strips have new texture arrays with the 1990 icons
// and colours. While a 1990 career is loaded, r8 (the package) at the department atlas sites becomes that package,
// and the department tabs' icon path, built before Career_Construct_RnDDeptCard, is rewritten the same way. Part
// images, masks and category icons keep the career's package.
//
// data/rnd-look.json holds the package name and the colours. Option rnd_look_1990 turns it all off.

#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>

#include <atomic>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <string>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#include "career_1990.hpp"
#include "json_strings.hpp"
#include "rnd_look.hpp"

using namespace swofty::game;

SWOFTY_SETTING(bool, rnd_look_1990, true);

namespace {

constexpr char kLookFile[] = "data/rnd-look.json";

f1990::Look g_look;
bool g_look_read, g_look_ok;
std::atomic<bool> g_on{false};

struct Table {
    const char *what;
    uint32_t *(*at)();
    bool light;
    uint32_t saved[f1990::kLookDepartments];
    bool written;
};

Table g_tables[] = {
    {"k_Career_RnDDeptColourTable", [] { return &(*k_Career_RnDDeptColourTable)[0]; }, false, {}, false},
    {"k_Career_RnDCardColourTable", [] { return &(*k_Career_RnDCardColourTable)[0]; }, false, {}, false},
};

bool load_look()
{
    if (g_look_read)
        return g_look_ok;
    g_look_read = true;
    std::string json;
    std::vector<f1990::StringLeaf> leaves;
    size_t error_at = 0;
    if (auto r = swofty::storage::read(kLookFile, json); r != swofty::storage::Result::ok) {
        swofty::log("[f1-1990] rnd look: read %s: %s; the R&D look stays stock", kLookFile, swofty::storage::name(r));
        return false;
    }
    if (!f1990::parse_string_leaves(json, leaves, &error_at)) {
        swofty::log("[f1-1990] rnd look: %s is not an object of strings (byte %zu); the R&D look stays stock",
                    kLookFile, error_at);
        return false;
    }
    g_look_ok = f1990::read_look(leaves, g_look);
    if (!g_look_ok)
        swofty::log("[f1-1990] rnd look: %s lacks the package or a colour (colours 0x%x, lights 0x%x); the R&D look "
                    "stays stock",
                    kLookFile, g_look.colours, g_look.lights);
    else
        swofty::log("[f1-1990] rnd look: package %s, colours %08x %08x %08x %08x %08x", g_look.package.c_str(),
                    g_look.colour[0], g_look.colour[1], g_look.colour[2], g_look.colour[3], g_look.colour[4]);
    return g_look_ok;
}

bool write_words(uint32_t *at, const uint32_t *from)
{
    DWORD old = 0;
    if (!VirtualProtect(at, sizeof(uint32_t) * f1990::kLookDepartments, PAGE_EXECUTE_READWRITE, &old))
        return false;
    for (size_t i = 0; i < f1990::kLookDepartments; ++i)
        reinterpret_cast<volatile uint32_t *>(at)[i] = from[i];
    DWORD tmp = 0;
    VirtualProtect(at, sizeof(uint32_t) * f1990::kLookDepartments, old, &tmp);
    FlushInstructionCache(GetCurrentProcess(), at, sizeof(uint32_t) * f1990::kLookDepartments);
    return true;
}

void tables(bool on)
{
    for (auto &t : g_tables) {
        uint32_t *at = t.at();
        if (on && !t.written) {
            std::memcpy(t.saved, at, sizeof t.saved);
            t.written = write_words(at, t.light ? g_look.light : g_look.colour);
            swofty::log("[f1-1990] rnd look: %s %08x %08x %08x %08x %08x -> 1990 colours%s", t.what, t.saved[0],
                        t.saved[1], t.saved[2], t.saved[3], t.saved[4], t.written ? "" : " FAILED (VirtualProtect)");
        } else if (!on && t.written) {
            t.written = !write_words(at, t.saved);
            swofty::log("[f1-1990] rnd look: %s restored to %08x %08x %08x %08x %08x%s", t.what, t.saved[0],
                        t.saved[1], t.saved[2], t.saved[3], t.saved[4], t.written ? " FAILED (VirtualProtect)" : "");
        }
    }
}

void set_on(bool on)
{
    if (on == g_on.load())
        return;
    if (on) {
        tables(true);
        g_on.store(true);
    } else {
        g_on.store(false);
        tables(false);
    }
    swofty::log("[f1-1990] rnd look: %s", on ? "1990 career: 1990 department colours and icons on"
                                             : "no 1990 career: stock department colours and icons");
}

void on_career(const SwoftyEvent *ev, void *)
{
    bool yes = false;
    if (rnd_look_1990 && (!std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED) ||
                          !std::strcmp(ev->name, SWOFTY_EVENT_CAREER_HUB))) {
        void *career = swofty::career::entity();
        yes = career && f1990::is_1990_career(career) && load_look();
    }
    set_on(yes);
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] rnd look: subscribe %s failed", event);
}

enum Site : unsigned {
    kUpgradeListIcon,
    kListRowIcon,
    kUpgradeCardIcon,
    kPerksTitleIcon,
    kCardIcon0,
    kCardIcon1,
    kCardIcon2,
    kInProgressStrip,
    kPieWheelIcon,
    kDeptTab,
    kListColour,
    kListLight,
    kPopupColour,
    kInProgressColour,
    kPieColour,
    kSites
};

const char *const kSiteNames[kSites] = {
    "R&D upgrade list department icon", "R&D list row department icon", "upgrade card department icon",
    "perks title department icon", "upgrade card 0 department icon", "upgrade card 1 department icon",
    "upgrade card 2 department icon", "In Progress strip", "Performance tile wheel icon", "department tab icon",
    "R&D list row colour", "R&D list row light colour", "Team HQ R&D popup colour", "In Progress colour",
    "Performance tile colour",
};

std::atomic<unsigned> g_logged[kSites];

void note(Site s, const char *fmt, ...)
{
    if (g_logged[s].fetch_add(1) >= 4)
        return;
    char line[256];
    va_list ap;
    va_start(ap, fmt);
    std::vsnprintf(line, sizeof line, fmt, ap);
    va_end(ap);
    swofty::log("[f1-1990] rnd look: %s: %s", kSiteNames[s], line);
}

void swap_package(swofty::Context &c, Site s)
{
    if (!g_on.load(std::memory_order_relaxed))
        return;
    std::string pkg;
    if (!swofty::mem::c_string(c.r8, pkg, 128) || !f1990::is_career_package(pkg, g_look.package))
        return;
    c.r8 = reinterpret_cast<uintptr_t>(g_look.package.c_str());
    note(s, "package %s -> %s", pkg.c_str(), g_look.package.c_str());
}

void recolour(uintptr_t &reg, uintptr_t dept, bool light, Site s)
{
    uint32_t to = 0;
    if (!g_on.load(std::memory_order_relaxed) || !f1990::look_colour(g_look, dept, light, to))
        return;
    uint32_t was = static_cast<uint32_t>(reg);
    reg = (reg & ~0xffffffffull) | to;
    note(s, "department %llu %08x -> %08x", static_cast<unsigned long long>(dept), was, to);
}

char *next_buf()
{
    static char bufs[8][192];
    static std::atomic<unsigned> slot{0};
    return bufs[slot.fetch_add(1) & 7];
}

}

SWOFTY_SITE(site_Career_RnDUpgradeListDeptIcon, swofty::Context &c) { swap_package(c, kUpgradeListIcon); }
SWOFTY_SITE(site_Career_RnDListRowDeptIcon, swofty::Context &c) { swap_package(c, kListRowIcon); }
SWOFTY_SITE(site_Career_UpgradeCardDeptIcon, swofty::Context &c) { swap_package(c, kUpgradeCardIcon); }
SWOFTY_SITE(site_Career_RnDPerksTitleDeptIcon, swofty::Context &c) { swap_package(c, kPerksTitleIcon); }
SWOFTY_SITE(site_Career_RnDCardDeptIcon0, swofty::Context &c) { swap_package(c, kCardIcon0); }
SWOFTY_SITE(site_Career_RnDCardDeptIcon1, swofty::Context &c) { swap_package(c, kCardIcon1); }
SWOFTY_SITE(site_Career_RnDCardDeptIcon2, swofty::Context &c) { swap_package(c, kCardIcon2); }
SWOFTY_SITE(site_Career_RnDInProgressStrip, swofty::Context &c) { swap_package(c, kInProgressStrip); }

SWOFTY_SITE(site_Career_RnDPieSliceIconArgs, swofty::Context &c)
{
    if (!g_on.load(std::memory_order_relaxed))
        return;
    std::string frame;
    uint32_t to = 0;
    const uint32_t code = static_cast<uint32_t>(c.r14);
    if (swofty::mem::c_string(*reinterpret_cast<const uintptr_t *>(c.rsp + 0x20), frame, 64) &&
        f1990::pie_slot_is_department(frame) && f1990::look_colour(g_look, code, false, to)) {
        auto *slot = reinterpret_cast<uint32_t *>(c.rsp + 0x64);
        note(kPieColour, "code %u (%s) %08x -> %08x", code, frame.c_str(), *slot, to);
        *slot = to;
    }
    swap_package(c, kPieWheelIcon);
}

SWOFTY_SITE(site_Career_RnDListRowColourStore, swofty::Context &c) { recolour(c.r8, c.rdx, false, kListColour); }
SWOFTY_SITE(site_Career_RnDListRowLightColourStore, swofty::Context &c) { recolour(c.rcx, c.rdx, true, kListLight); }
SWOFTY_SITE(site_Career_RnDPopupColourStore, swofty::Context &c) { recolour(c.rcx, c.rax, false, kPopupColour); }
SWOFTY_SITE(site_Career_RnDInProgressColourStore, swofty::Context &c) { recolour(c.rax, c.rcx, false, kInProgressColour); }

SWOFTY_WRAP(Career_Construct_RnDDeptCard, void *, void *card, void *ctx, void *builder, void *allocator,
            const char *name, const char *icon, uint8_t dept)
{
    std::string path;
    if (g_on.load(std::memory_order_relaxed) && swofty::mem::c_string(icon, path, 160)) {
        std::string ours = f1990::dept_icon_path(path, g_look.package);
        if (!ours.empty() && ours.size() < 192) {
            char *buf = next_buf();
            std::memcpy(buf, ours.c_str(), ours.size() + 1);
            note(kDeptTab, "department %u %s -> %s", dept, path.c_str(), buf);
            icon = buf;
        }
    }
    return orig(card, ctx, builder, allocator, name, icon, dept);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] rnd look: %s", rnd_look_1990 ? "on for 1990 careers" : "off (rnd_look_1990)");
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
    subscribe(SWOFTY_EVENT_CAREER_HUB);
}

SWOFTY_ADDON()
