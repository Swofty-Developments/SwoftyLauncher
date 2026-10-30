// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <map>
#include <string>

#include "profile.hpp"

static int failures;
static bool g_available;
static std::map<std::string, std::string> g_text;
static std::map<std::string, int64_t> g_int;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static void log(const char *, ...) {}
static int available()
{
    return g_available;
}
static int get(const char *key, SwoftyDbValue *out)
{
    auto it = g_text.find(key);
    if (it == g_text.end())
        return SWOFTY_ASSET_UNAVAILABLE;
    out->tag = SWOFTY_DB_STRING;
    out->s = it->second.c_str();
    return SWOFTY_ASSET_OK;
}
static int int_at(const char *key, int64_t *out)
{
    auto it = g_int.find(key);
    if (it == g_int.end())
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = it->second;
    return SWOFTY_ASSET_OK;
}

int main()
{
    SwoftyDb db{};
    db.available = available;
    db.get = get;
    db.int_at = int_at;
    SwoftyApi api{};
    api.version = SWOFTY_API_VERSION;
    api.log = log;
    api.db = &db;
    swofty::api = &api;

    check("database not loaded yet", f1990::detect_1990_content("t") == -1);
    g_available = true;
    check("no ModeChampionship count", f1990::detect_1990_content("t") == -1);
    g_int["game_modes._ModeChampionship_instancecount_"] = 3;
    g_text["game_modes.ModeChampionship_1.save_data.name"] = "driver_career_26";
    check("stock database", f1990::detect_1990_content("t") == 0);
    g_text["game_modes.ModeChampionship_2.save_data.name"] = "driver_career_1990";
    check("1990 save container", f1990::detect_1990_content("t") == 1);
    g_text.erase("game_modes.ModeChampionship_2.save_data.name");
    g_text["game_modes.ModeChampionship_0.event_list"] = "F1_career_1990";
    check("1990 event list", f1990::detect_1990_content("t") == 1);
    g_text["game_modes.ModeChampionship_0.event_list"] = "F1_career_1990x";
    check("similar names do not match", f1990::detect_1990_content("t") == 0);
    g_int["game_modes._ModeChampionship_instancecount_"] = 100000;
    check("absurd row count is unreadable", f1990::detect_1990_content("t") == -1);
    if (failures)
        std::printf("%d failure(s)\n", failures);
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
