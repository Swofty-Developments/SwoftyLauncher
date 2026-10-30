// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - R&D card art for the 1990 upgrades and the 1990 department names. Its own DLL.
//
// Card art: the three upgrade card binders build a card's frame from its upgrade_name (plus
// _in_progress or _complete for the state) and format pkg.rnd_part_image.<frame> or
// pkg.rnd_part_mask.<upgrade_name> right after the six site_Career_RnDCardArt{Image,Mask}N sites
// ([rsp+0x20] = the frame, r8 = pkg). The 1990 upgrades are named f1990_*, which no list has, so at
// those sites the frame of an f1990_ upgrade becomes its stock donor's frame for the same state
// (data/rnd-art.json "frames"), named the way the list in pkg names it (rnd_om_xm or plain, as the
// rnd-tyres addon does). Only 1990 R&D trees carry f1990_ names, so this needs no career check.
// pkg is the career's R&D UI package (upgrades.<path>.general.ui_data_0.texture_array_dictionary_path,
// read by the upgrades loader 0x143540790); a build without upgrades.1990.general leaves it empty, so for
// an f1990_ card with no R&D package r8 becomes #overrides.career.rnd_om_xm. Each card is logged once per
// site.
//
// Department names: data/rnd-strings.json ("strings" per language) is set through the core's
// runtime string overrides (API v8) while a 1990 career is loaded and cleared when it unloads.
// Built against an SDK below v8 that part is compiled out.

#include <atomic>
#include <cstring>
#include <mutex>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#if SWOFTY_API_VERSION >= 8
#include <swofty/lang.hpp>
#endif

#include "career_1990.hpp"
#include "json_strings.hpp"
#include "rnd_art.hpp"

namespace {

constexpr char kArtFile[] = "data/rnd-art.json";

std::once_flag g_art_once;
std::unordered_map<std::string, std::string> g_frames;
std::atomic<uint32_t> g_missing_logged;
std::atomic<bool> g_package_logged;
std::mutex g_log_mu;
std::unordered_set<std::string> g_logged_cards;

bool read_leaves(const char *file, std::vector<f1990::StringLeaf> &out)
{
    std::string json;
    size_t error_at = 0;
    if (auto r = swofty::storage::read(file, json); r != swofty::storage::Result::ok) {
        swofty::log("[f1-1990] rnd: read %s: %s", file, swofty::storage::name(r));
        return false;
    }
    if (!f1990::parse_string_leaves(json, out, &error_at)) {
        swofty::log("[f1-1990] rnd: %s is not an object of strings (byte %zu)", file, error_at);
        return false;
    }
    return true;
}

void read_art()
{
    std::vector<f1990::StringLeaf> leaves;
    if (read_leaves(kArtFile, leaves))
        for (auto &l : leaves)
            if (l.path.size() == 2 && l.path[0] == "frames")
                g_frames.emplace(l.path[1], l.text);
    swofty::log("[f1-1990] rnd: %zu card art donor(s) from %s", g_frames.size(), kArtFile);
}

bool first_log(unsigned site, const char *name)
{
    std::lock_guard<std::mutex> lock(g_log_mu);
    return g_logged_cards.size() < 256 && g_logged_cards.insert(std::to_string(site) + ':' + name).second;
}

char *next_buf()
{
    static char bufs[8][128];
    static std::atomic<unsigned> slot{0};
    return bufs[slot.fetch_add(1) & 7];
}

void part_art(swofty::Context &c, unsigned site, bool mask)
{
    auto **arg = reinterpret_cast<const char **>(c.rsp + 0x20);
    const char *name = *arg;
    if (!name || !swofty::mem::readable(name, f1990::kUpgradePrefix.size()) ||
        std::strncmp(name, f1990::kUpgradePrefix.data(), f1990::kUpgradePrefix.size()))
        return;
    std::call_once(g_art_once, read_art);
    std::string_view base;
    std::string_view suffix = mask ? std::string_view() : f1990::state_suffix(name, base);
    if (mask)
        base = name;
    auto it = g_frames.find(std::string(base));
    if (it == g_frames.end()) {
        if (g_missing_logged++ < 16)
            swofty::log("[f1-1990] rnd: card art site %u: no donor for \"%s\"; the card asks for its own frame", site,
                        name);
        return;
    }
    std::string pkg;
    bool career = swofty::mem::c_string(c.r8, pkg, 128) && f1990::is_rnd_package(pkg);
    if (!g_package_logged.exchange(true))
        swofty::log("[f1-1990] rnd: card art: the career's R&D package is \"%s\"%s", pkg.c_str(),
                    career ? "" : "; not an R&D package (upgrades.1990.general missing from the build?), so 1990 "
                                  "cards use #overrides.career.rnd_om_xm");
    if (!career)
        c.r8 = reinterpret_cast<uintptr_t>(f1990::kRnDPackage);
    bool omxm = !career || f1990::package_is_omxm(pkg);
    std::string frame = f1990::donor_frame(it->second, suffix, omxm, mask);
    char *buf = next_buf();
    std::snprintf(buf, 128, "%s", frame.c_str());
    *arg = buf;
    if (first_log(site, name))
        swofty::log("[f1-1990] rnd: card art site %u %s \"%s\" -> \"%s\"", site, mask ? "mask" : "image", name, buf);
}

#if SWOFTY_API_VERSION >= 8
constexpr char kStringsFile[] = "data/rnd-strings.json";

struct Override {
    std::string language, key, text;
};

std::vector<Override> g_strings;
bool g_strings_read, g_strings_set;

void department_names(bool on)
{
    namespace lg = swofty::lang;
    if (on == g_strings_set || !lg::available())
        return;
    if (on && !g_strings_read) {
        g_strings_read = true;
        std::vector<f1990::StringLeaf> leaves;
        if (read_leaves(kStringsFile, leaves))
            for (auto &l : leaves)
                if (l.path.size() == 3 && l.path[0] == "strings")
                    g_strings.push_back({l.path[1], l.path[2], l.text});
    }
    size_t failed = 0;
    for (auto &s : g_strings)
        failed += (on ? lg::set(s.key, s.text, s.language) : lg::clear(s.key, s.language)) != lg::Result::ok;
    g_strings_set = on;
    swofty::log("[f1-1990] rnd: %s %zu department string(s) (%zu failed)", on ? "set" : "cleared", g_strings.size(),
                failed);
}
#else
void department_names(bool)
{
}
#endif

void on_career(const SwoftyEvent *ev, void *)
{
    bool yes = false;
    if (!std::strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED)) {
        void *career = swofty::career::entity();
        yes = career && f1990::is_1990_career(career);
    }
    if (yes)
        std::call_once(g_art_once, read_art);
    department_names(yes);
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    if (swofty::events::subscribe(event, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] rnd: subscribe %s failed; the department names stay stock", event);
}

}

SWOFTY_SITE(site_Career_RnDCardArtImage0, swofty::Context &c) { part_art(c, 0, false); }
SWOFTY_SITE(site_Career_RnDCardArtMask0, swofty::Context &c) { part_art(c, 1, true); }
SWOFTY_SITE(site_Career_RnDCardArtImage1, swofty::Context &c) { part_art(c, 2, false); }
SWOFTY_SITE(site_Career_RnDCardArtMask1, swofty::Context &c) { part_art(c, 3, true); }
SWOFTY_SITE(site_Career_RnDCardArtImage2, swofty::Context &c) { part_art(c, 4, false); }
SWOFTY_SITE(site_Career_RnDCardArtMask2, swofty::Context &c) { part_art(c, 5, true); }

SWOFTY_ON_LOAD
{
#if SWOFTY_API_VERSION < 8
    swofty::log("[f1-1990] rnd: built against SDK API v%d, without runtime string overrides; the department names stay "
                "stock",
                SWOFTY_API_VERSION);
#endif
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
}

SWOFTY_ADDON()
