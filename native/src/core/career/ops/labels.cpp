#include "core/career/ops/tables.hpp"

#include "core/runtime/runtime.hpp"

#include "core/registry/table.hpp"
#include "core/career/ops/entrants.hpp"

namespace career::ops::live {

bool db_int(const SwoftyDb *db, const std::string &path, int64_t &out)
{
    return db->int_at(path.c_str(), &out) == SWOFTY_ASSET_OK;
}

bool db_text(const SwoftyDb *db, const std::string &path, std::string &out)
{
    const char *v = nullptr;
    if (db->string_at(path.c_str(), &v) != SWOFTY_ASSET_OK || !v)
        return false;
    size_t n = strnlen(v, 128);
    if (!n || n >= 128 || !mem::printable_utf8(v, n))
        return false;
    out.assign(v, n);
    return true;
}

template <class F> void each_row(const SwoftyDb *db, const std::string &table, const std::string &stem, F f)
{
    int64_t count = 0;
    if (!db_int(db, table + "._" + stem + "_instancecount_", count) || count <= 0 || count > 8192)
        return;
    for (int64_t i = 0; i < count; ++i) {
        std::string row = table + "." + stem + "_" + std::to_string(i), name;
        int64_t id = -1;
        if (db_int(db, row + ".stat_id", id) && id >= 0 && id < 0x3fff && db_text(db, row + "._metadata_", name))
            f(static_cast<uint32_t>(id), name);
    }
}

bool labels()
{
    std::lock_guard<std::mutex> l(tables.lock);
    if (tables.labels.loaded)
        return true;
    const SwoftyDb *db = runtime::db_table();
    if (!db || !db->available || !db->available())
        return false;
    Labels t;
    each_row(db, "entrants.drivers", "driver", [&](uint32_t s, const std::string &n) {
        t.driver_by_label.emplace(n, s);
        t.driver_by_stat.emplace(s, n);
    });
    for (int folder = 0; folder < 4; ++folder)
        each_row(db, "entrants.driver_customisations.folder_" + std::to_string(folder),
                 "preloaded_driver_customisation_properties",
                 [&](uint32_t s, const std::string &n) { t.look_by_label.emplace(n, s); });
    each_row(db, "entrants.teams", "team", [&](uint32_t s, const std::string &n) { t.team_by_stat.emplace(s, n); });
    int64_t vehicles = 0;
    if (db_int(db, "entrants.vehicles._vehicle_instancecount_", vehicles) && vehicles > 0 && vehicles <= 8192)
        for (int64_t i = 0; i < vehicles; ++i) {
            std::string row = "entrants.vehicles.vehicle_" + std::to_string(i), name, package;
            int64_t id = -1;
            if (!db_int(db, row + ".stat_id", id) || id < 0 || id >= 0x3fff || !db_text(db, row + "._metadata_", name))
                continue;
            t.vehicle_by_stat.emplace(static_cast<uint32_t>(id), name);
            if (db_text(db, row + ".vehicle_package", package))
                t.package_of_vehicle.emplace(name, package);
        }
    t.loaded = !t.driver_by_label.empty();
    if (t.loaded)
        tables.labels = std::move(t);
    return t.loaded || tables.labels.loaded;
}

std::string driver_name(uint32_t stat)
{
    auto it = tables.labels.driver_by_stat.find(stat);
    return (it == tables.labels.driver_by_stat.end() ? std::string("?") : it->second) + "(" + std::to_string(stat) + ")";
}

std::string team_name(uint32_t stat)
{
    if (stat == stat_of(kNoKey))
        return "no team";
    auto it = tables.labels.team_by_stat.find(stat);
    return (it == tables.labels.team_by_stat.end() ? std::string("?") : it->second) + "(" + std::to_string(stat) + ")";
}

static std::string label_in(const std::unordered_map<uint32_t, std::string> &m, uint32_t stat)
{
    auto it = m.find(stat);
    return it == m.end() ? std::string() : it->second;
}

std::string team_label(uint32_t stat)
{
    std::lock_guard<std::mutex> l(tables.lock);
    return label_in(tables.labels.team_by_stat, stat);
}

std::string driver_label(uint32_t stat)
{
    std::lock_guard<std::mutex> l(tables.lock);
    return label_in(tables.labels.driver_by_stat, stat);
}

std::string vehicle_label(uint32_t stat)
{
    std::lock_guard<std::mutex> l(tables.lock);
    return label_in(tables.labels.vehicle_by_stat, stat);
}

std::string vehicle_package_label(const std::string &vehicle)
{
    std::lock_guard<std::mutex> l(tables.lock);
    auto it = tables.labels.package_of_vehicle.find(vehicle);
    return it == tables.labels.package_of_vehicle.end() ? std::string() : it->second;
}

bool has_label(const char *what, const std::string &label)
{
    std::lock_guard<std::mutex> l(tables.lock);
    const Labels &t = tables.labels;
    if (!t.loaded)
        return true;
    auto in = [&](const std::unordered_map<uint32_t, std::string> &m) {
        for (const auto &[stat, name] : m)
            if (name == label)
                return true;
        return false;
    };
    std::string w = what ? what : "";
    if (w == "team")
        return in(t.team_by_stat);
    if (w == "vehicle")
        return in(t.vehicle_by_stat);
    if (w == "driver")
        return t.driver_by_label.count(label) > 0;
    if (w == "package") {
        for (const auto &[vehicle, package] : t.package_of_vehicle)
            if (package == label)
                return true;
        return t.package_of_vehicle.empty();
    }
    return true;
}

std::string record_label_of(const char *registry, const std::string &id)
{
    auto rec = swofty_core::registry::store().get(registry, id);
    if (rec.status == swofty_core::registry::Status::ok && rec.value["label"].is_string() &&
        !rec.value["label"].as_string().empty())
        return std::string(rec.value["label"].as_string());
    size_t slash = id.rfind('/');
    return slash == std::string::npos ? id : id.substr(slash + 1);
}

std::string person_driver_label(const std::string &ref)
{
    auto &st = swofty_core::registry::store();
    std::string found;
    for (auto &id : st.list("driver.identity", "")) {
        if (id.rfind("game/", 0) == 0)
            continue;
        auto rec = st.get("driver.identity", id);
        if (rec.status == swofty_core::registry::Status::ok && rec.value["person"].is_string() &&
            rec.value["person"].as_string() == ref)
            found = record_label_of("driver.identity", id);
    }
    return found;
}

int resolve_driver(const std::string &ref)
{
    size_t slash = ref.find('/');
    if (ref.empty() || slash == std::string::npos || !labels())
        return -1;
    std::string ns = ref.substr(0, slash), label = ref.substr(slash + 1);
    if (ns == "person") {
        std::string own = person_driver_label(ref);
        if (!own.empty())
            label = own;
    } else if (ns != "game") {
        label = record_label_of("driver.identity", ref);
    }
    std::lock_guard<std::mutex> l(tables.lock);
    auto it = tables.labels.driver_by_label.find(label);
    return it == tables.labels.driver_by_label.end() ? -1 : static_cast<int>(it->second);
}

int resolve_look(const std::string &ref)
{
    size_t slash = ref.find('/');
    if (ref.empty() || slash == std::string::npos || !labels())
        return -1;
    std::string label = ref.substr(0, slash) == "game" ? ref.substr(slash + 1) : record_label_of("driver.look", ref);
    std::lock_guard<std::mutex> l(tables.lock);
    auto it = tables.labels.look_by_label.find(label);
    return it == tables.labels.look_by_label.end() ? -1 : static_cast<int>(it->second);
}

int resolve_team(const std::string &ref)
{
    size_t slash = ref.find('/');
    if (ref.empty() || slash == std::string::npos || !labels())
        return -1;
    std::string label = ref.substr(0, slash) == "game" ? ref.substr(slash + 1) : record_label_of("team.identity", ref);
    std::lock_guard<std::mutex> l(tables.lock);
    for (auto &[stat, name] : tables.labels.team_by_stat)
        if (name == label)
            return static_cast<int>(stat);
    return -1;
}

}
