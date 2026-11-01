// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Progressive Career: gamedb labels (names.hpp).

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include <swofty/db.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "names.hpp"
#include "names_logic.hpp"
#include "seats.hpp"

namespace pg::names {

namespace {

constexpr size_t kLabelMax = 128;
constexpr int64_t kMaxRows = 8192;
constexpr int64_t kRetryMs = 5000;

using names_logic::Tables;

names_logic::Cache g_cache;

int64_t now_ms()
{
    return std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch())
        .count();
}

bool text(const std::string &path, std::string &out)
{
    out.clear();
    const char *v = nullptr;
    if (swofty::db::string_at(path, &v) != swofty::db::Result::ok || !v)
        return false;
    size_t n = strnlen(v, kLabelMax);
    if (!n || n >= kLabelMax || !swofty::mem::printable_utf8(v, n))
        return false;
    out.assign(v, n);
    return true;
}

bool integer(const std::string &path, int64_t &out)
{
    return swofty::db::int_at(path, &out) == swofty::db::Result::ok;
}

template <class F> void each_row(const std::string &table, const std::string &stem, F f)
{
    int64_t count = 0;
    if (!integer(table + "._" + stem + "_instancecount_", count) || count <= 0 || count > kMaxRows)
        return;
    for (int64_t i = 0; i < count; ++i)
        f(table + "." + stem + "_" + std::to_string(i));
}

void add(Tables &tables, Kind k, const std::string &row)
{
    int64_t id = -1;
    std::string name;
    if (!integer(row + ".stat_id", id) || id < 0 || id >= 0x3fff || !text(row + "._metadata_", name))
        return;
    tables.kinds[static_cast<int>(k)].add(static_cast<uint32_t>(id), name);
}

uint64_t fingerprint()
{
    uint64_t h = swofty::api && swofty::api->module_base ? static_cast<uint64_t>(swofty::api->module_base()) : 0;
    for (const char *key : {"entrants.drivers._driver_instancecount_", "entrants.teams._team_instancecount_",
                            "entrants.vehicles._vehicle_instancecount_",
                            "entrants.driver_customisations._folder_instancecount_"}) {
        int64_t n = -1;
        integer(key, n);
        h = h * 1099511628211ull + static_cast<uint64_t>(n + 1);
    }
    return h;
}

Tables build()
{
    Tables t;
    each_row("entrants.drivers", "driver", [&](const std::string &r) { add(t, Kind::driver, r); });
    each_row("entrants.teams", "team", [&](const std::string &r) { add(t, Kind::team, r); });
    each_row("entrants.vehicles", "vehicle", [&](const std::string &r) { add(t, Kind::vehicle, r); });
    each_row("entrants.driver_customisations", "folder", [&](const std::string &folder) {
        each_row(folder, "preloaded_driver_customisation_properties", [&](const std::string &r) { add(t, Kind::look, r); });
    });
    return t;
}

void run(uint64_t id)
{
    int64_t began = now_ms();
    Tables t = build();
    bool ok = names_logic::usable(t);
    swofty::log("[progressive] gamedb labels %s in %lld ms: %zu drivers, %zu teams, %zu vehicles, %zu looks",
                ok ? "loaded" : "NOT loaded (retrying at the next Team HQ)", static_cast<long long>(now_ms() - began),
                t.kinds[0].by_stat.size(), t.kinds[1].by_stat.size(), t.kinds[2].by_stat.size(),
                t.kinds[3].by_stat.size());
    g_cache.finish(id, std::move(t), now_ms());
}

bool start()
{
    if (!swofty::db::available())
        return false;
    uint64_t id = fingerprint();
    if (!g_cache.claim(id, now_ms(), kRetryMs))
        return false;
    std::thread(run, id).detach();
    return true;
}

const names_logic::Table &table(const names_logic::Cache::Ptr &p, Kind k)
{
    static const names_logic::Table none;
    return p ? p->kinds[static_cast<int>(k)] : none;
}

} // namespace

const char *kind_name(Kind k)
{
    switch (k) {
    case Kind::driver:
        return "driver";
    case Kind::team:
        return "team";
    case Kind::vehicle:
        return "vehicle";
    case Kind::look:
        return "look";
    }
    return "?";
}

bool loaded()
{
    return g_cache.ready() != nullptr;
}

bool load()
{
    if (g_cache.ready())
        return true;
    start();
    return false;
}

bool load_wait()
{
    if (load())
        return true;
    g_cache.wait();
    return g_cache.ready() != nullptr;
}

void refresh()
{
    start();
}

std::string label(Kind k, uint32_t s)
{
    auto &m = table(g_cache.ready(), k).by_stat;
    auto it = m.find(s);
    return it == m.end() ? std::string() : it->second;
}

bool stat(Kind k, const std::string &l, uint32_t &out)
{
    auto &m = table(g_cache.ready(), k).by_label;
    auto it = m.find(l);
    if (it == m.end())
        return false;
    out = it->second;
    return true;
}

bool known(Kind k, uint32_t s)
{
    return table(g_cache.ready(), k).by_stat.count(s) != 0;
}

size_t count(Kind k)
{
    return table(g_cache.ready(), k).by_stat.size();
}

std::string key_text(Kind k, uint32_t key)
{
    if (none(key))
        return "none";
    uint32_t s = stat_of(key);
    std::string l = label(k, s);
    char b[48];
    std::snprintf(b, sizeof b, "(%u%s)", s, (key & 0xffffu) ? "+low" : "");
    return (l.empty() ? std::string("?") : l) + b;
}

} // namespace pg::names
