#include "core/live/live_tyre_images.hpp"
#include <cassert>
#include <cstdio>
#include <map>
namespace ti = swofty_core::live::tyre_images;
static std::map<std::string, int64_t> ints;
static std::map<std::string, std::string> strings;
static bool ready = true;
static int available() { return ready; }
static int integer(const char *key, int64_t *out)
{
    auto it = ints.find(key);
    if (it == ints.end())
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = it->second;
    return SWOFTY_ASSET_OK;
}
static int string(const char *key, const char **out)
{
    auto it = strings.find(key);
    if (it == strings.end())
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = it->second.c_str();
    return SWOFTY_ASSET_OK;
}
static void mapping()
{
    SwoftyDb db{};
    db.available = available;
    db.int_at = integer;
    db.string_at = string;
    ints["tyres.visual_compounds._VisualCompound_instancecount_"] = 5;
    const std::string templates = "entrants.ui.asset_templates.tyre_compounds.textures";
    ints[templates + "._ui_texture_path_instancecount_"] = 1;
    strings[templates + ".ui_texture_path_0.name"] = "small";
    auto &path = strings[templates + ".ui_texture_path_0.path"];
    path = "#overrides.tyre_allocation.key_icons.[COMPOUND]";
    const uint8_t ids[] = {16, 17, 18, 7, 8};
    const char *names[] = {"Soft", "Medium", "Hard", "Intermediate", "Wet"};
    char out[160];
    for (unsigned i = 0; i < 5; ++i) {
        auto row = "tyres.visual_compounds.VisualCompound_" + std::to_string(i);
        ints[row + ".stat_id"] = ids[i];
        strings[row + ".texture_name"] = names[i];
    }
    for (unsigned i = 0; i < 5; ++i) {
        assert(ti::reference(&db, ids[i], out, sizeof out) == SWOFTY_ASSET_OK);
        assert(std::string(out) == "#overrides.tyre_allocation.key_icons." + std::string(names[i]));
    }
    // stat_id is not the row index, nor the audio/physical compound.
    ints["tyres.visual_compounds.VisualCompound_0.stat_id"] = 23;
    strings["tyres.visual_compounds.VisualCompound_0.texture_name"] = "Soft_f1_26";
    assert(ti::reference(&db, 23, out, sizeof out) == SWOFTY_ASSET_OK);
    assert(std::string(out).ends_with(".Soft_f1_26"));
    // Mod edits to both the name and template are read again, with no stale mapping.
    strings["tyres.visual_compounds.VisualCompound_0.texture_name"] = "CustomSoft";
    path = "#overrides.tyre_allocation.compound_icons.[COMPOUND]";
    assert(ti::reference(&db, 23, out, sizeof out) == SWOFTY_ASSET_OK);
    assert(std::string(out) == "#overrides.tyre_allocation.compound_icons.CustomSoft");
    assert(ti::reference(&db, 23, out, 2) == SWOFTY_ASSET_INVALID && !out[0]);
    assert(ti::reference(&db, 255, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    ints["tyres.visual_compounds.VisualCompound_1.stat_id"] = 23;
    assert(ti::reference(&db, 23, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    ints["tyres.visual_compounds.VisualCompound_1.stat_id"] = 17;
    for (const char *bad : {"[COMPOUND]", "#other.[COMPOUND]", "#overrides.tyre_allocation.key_icons.[COMPOUND][COMPOUND]"}) {
        path = bad;
        assert(ti::reference(&db, 23, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    }
    path = "#overrides.tyre_allocation.key_icons.[COMPOUND]";
    strings["tyres.visual_compounds.VisualCompound_0.texture_name"] = "../escape";
    assert(ti::reference(&db, 23, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    ready = false;
    assert(ti::reference(&db, 17, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    assert(ti::reference(nullptr, 17, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    assert(ti::reference(&db, 17, nullptr, 160) == SWOFTY_ASSET_INVALID);
    ready = true;
    ints["tyres.visual_compounds._VisualCompound_instancecount_"] = 257;
    assert(ti::reference(&db, 17, out, sizeof out) == SWOFTY_ASSET_UNAVAILABLE);
    std::puts("PASS: effective DB mapping, S/M/H/I/W, 2026, mod edits, ambiguity and bounds");
}

int main() { mapping(); }
