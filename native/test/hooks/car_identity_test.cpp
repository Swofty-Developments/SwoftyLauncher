// Host unit test for the refs a car answers to in car.profile's cars selectors (car_identity.hpp) and the session
// entrant reader (live/entrant_ids.hpp) on fake memory: a SWOFTY 2014 Grand Prix car by its box's team key, its
// team.identity id and its row, the stock Mercedes by constructor/mercedes, a dressed career slot by the identity it
// wears, a customer team that is not the works team, drivers and vehicles, the names no record or row has, and the
// registry walk and entrant scan that find a car's team, driver and vehicle by its vehicle index.
// native/test/run_tests.sh car_live builds and runs it.
#include "core/hooks/car_identity.hpp"
#include "core/live/entrant_ids.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace ci = swofty_core::hooks::car_identity;
namespace ei = swofty_core::live::entrant_ids;
namespace L = swofty_core::live::layout;
using swofty::json::Value;

static int fails;
static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static bool has(const std::vector<std::string> &v, const std::string &x) { return std::find(v.begin(), v.end(), x) != v.end(); }

struct Registry {
    std::map<std::string, std::map<std::string, Value>> records;
    int gets = 0;

    void put(const char *registry, const std::string &id, const char *json) { records[registry][id] = swofty::json::parse(json); }

    ci::Source source()
    {
        ci::Source s;
        s.list = [this](const char *registry) {
            std::vector<std::string> out;
            for (const auto &[id, v] : records[registry])
                out.push_back(id);
            return out;
        };
        s.get = [this](const char *registry, const std::string &id) -> std::optional<Value> {
            gets++;
            auto r = records.find(registry);
            if (r == records.end() || !r->second.count(id))
                return std::nullopt;
            return r->second.at(id);
        };
        s.row = [](const char *what, const std::string &label) {
            static const std::map<std::string, std::vector<std::string>> rows = {
                {"team", {"mercedes", "williams", "sw14_mercedes", "sw14_williams"}},
                {"vehicle", {"sw14_mercedes", "sw14_williams", "mercedes_2025"}},
                {"driver", {"lewis_hamilton", "felipe_massa"}},
                {"package", {"f1cg_f1_2014_vehicle_package"}}};
            auto it = rows.find(what);
            return it != rows.end() && has(it->second, label);
        };
        return s;
    }
};

static const char *BOX = "swofty.swofty-seasons-2014/sw_2014";

static Registry swofty_2014()
{
    Registry r;
    r.put("team.identity", "game/mercedes", R"({"stat_id":0,"provides":["constructor/mercedes"]})");
    r.put("team.identity", "game/williams", R"({"stat_id":9,"provides":["constructor/williams"]})");
    r.put("team.identity", "swofty.swofty-seasons-2014/sw14_mercedes", R"({"donor":"game/mercedes"})");
    r.put("team.identity", "swofty.swofty-seasons-2014/sw14_williams", R"({"donor":"game/williams"})");
    r.put("team.identity", "other.pack/mercedes", R"({"donor":"game/mercedes","constructor":"constructor/mercedes"})");
    r.put("team.constructor", "constructor/mercedes", R"({"name":"Mercedes","team":"game/mercedes"})");
    r.put("team.constructor", "constructor/williams", R"({"name":"Williams","team":"game/williams"})");
    r.put("vehicle", "swofty.swofty-seasons-2014/mercedes",
          R"({"label":"sw14_mercedes","donor":"game/mercedes_2025","package":"game/f1cg_f1_2014_vehicle_package"})");
    r.put("vehicle", "swofty.swofty-seasons-2014/williams", R"({"label":"sw14_williams","donor":"game/mercedes_2025"})");
    r.put("driver.identity", "swofty.swofty-seasons-2014/sw14_rosberg", R"({"label":"sw14_rosberg","person":"person/rosberg"})");
    r.put("content.package", BOX, R"({"teams":[
        {"key":"mercedes","team":"swofty.swofty-seasons-2014/sw14_mercedes","vehicle":"swofty.swofty-seasons-2014/mercedes"},
        {"key":"williams","team":"swofty.swofty-seasons-2014/sw14_williams","vehicle":"swofty.swofty-seasons-2014/williams"}]})");
    r.put("content.package", "classic.pack/cs_2019", R"({"teams":[{"key":"racing_point","vehicle":"classic.pack/rp19"}]})");
    return r;
}

static void refs()
{
    Registry reg = swofty_2014();
    ci::Source src = reg.source();
    ci::Index ix = ci::build(src);
    const std::vector<std::string> chain = {BOX};

    ci::Seen merc;
    merc.team = "sw14_mercedes";
    merc.vehicle = "sw14_mercedes";
    merc.package = "f1cg_f1_2014_vehicle_package";
    merc.driver = "lewis_hamilton";
    ci::Refs r = ci::refs_of(merc, chain, ix, src);
    expect(has(r.team, "mercedes") && has(r.team, "swofty.swofty-seasons-2014/sw14_mercedes") &&
               has(r.team, "game/sw14_mercedes"),
           "a SWOFTY 2014 Mercedes answers to the box's team key, its team.identity id and its row");
    expect(!has(r.team, "constructor/mercedes"), "a pack team that declares no constructor is not constructor/mercedes");
    expect(has(r.vehicle, "swofty.swofty-seasons-2014/mercedes") && has(r.vehicle, "game/sw14_mercedes") &&
               has(r.vehicle_package, "game/f1cg_f1_2014_vehicle_package") && has(r.driver, "game/lewis_hamilton"),
           "its vehicle, vehicle package and driver refs");
    swofty::cars::CarSelect works;
    works.team = {"mercedes"};
    expect(ci::selects(works, r), "cars.team [\"mercedes\"] picks it");

    ci::Seen will = merc;
    will.team = "sw14_williams";
    will.vehicle = "sw14_williams";
    ci::Refs w = ci::refs_of(will, chain, ix, src);
    expect(!ci::selects(works, w) && has(w.team, "williams"),
           "the Williams with a Mercedes engine donor is not the Mercedes works team");

    ci::Refs away = ci::refs_of(merc, {}, ix, src);
    expect(!has(away.team, "mercedes") && has(away.team, "swofty.swofty-seasons-2014/sw14_mercedes"),
           "a box's team key holds only while that box is active");

    ci::Seen stock;
    stock.team = "mercedes";
    ci::Refs s = ci::refs_of(stock, {}, ix, src);
    expect(has(s.team, "game/mercedes") && has(s.team, "constructor/mercedes") && !has(s.team, "constructor/williams"),
           "the stock Mercedes row is game/mercedes and constructor/mercedes");
    expect(has(s.team, "other.pack/mercedes") && !has(r.team, "other.pack/mercedes"),
           "an identity whose key is the row's label is that row");
    int gets = reg.gets;
    ci::refs_of(stock, {}, ix, src);
    expect(reg.gets == gets, "a stock row's own identity is read once");

    ci::Seen dressed;
    dressed.team = "mercedes";
    dressed.worn = "swofty.swofty-seasons-2014/sw14_mercedes";
    ci::Refs d = ci::refs_of(dressed, chain, ix, src);
    expect(has(d.team, "swofty.swofty-seasons-2014/sw14_mercedes") && has(d.team, "mercedes") && !has(d.team, "game/mercedes"),
           "a dressed career slot answers to the identity it wears, not the stock row under it");

    ci::Seen rosberg;
    rosberg.driver = "sw14_rosberg";
    ci::Refs rr = ci::refs_of(rosberg, chain, ix, src);
    expect(has(rr.driver, "swofty.swofty-seasons-2014/sw14_rosberg") && has(rr.driver, "person/rosberg") &&
               has(rr.driver, "game/sw14_rosberg") && rr.team.empty(),
           "a driver answers to its identity, its person and its row");

    ci::Seen none;
    expect(!ci::selects(works, ci::refs_of(none, chain, ix, src)), "a car with no team row matches no team");

    expect(ix.boxes["classic.pack/cs_2019"].size() == 1 && ix.boxes["classic.pack/cs_2019"][0].row == "rp19",
           "a box team without a team record is its vehicle's row");

    swofty::cars::CarSelect names;
    names.team = {"mercedes",
                  "mercedez",
                  "game/mercedes",
                  "game/merc",
                  "constructor/mercedes",
                  "constructor/merc",
                  "swofty.swofty-seasons-2014/sw14_mercedes",
                  "swofty.swofty-seasons-2014/sw14_merc"};
    names.vehicle = {"swofty.swofty-seasons-2014/mercedes", "game/nope"};
    names.driver = {"game/lewis_hamilton", "person/rosberg", "person/nobody"};
    auto lines = ci::unknown(names, ix, src);
    auto named = [&](const std::string &x) {
        for (const auto &l : lines)
            if (l.find("\"" + x + "\"") != std::string::npos)
                return true;
        return false;
    };
    expect(lines.size() == 6 && named("mercedez") && named("game/merc") && named("constructor/merc") &&
               named("swofty.swofty-seasons-2014/sw14_merc") && named("game/nope") && named("person/nobody"),
           "every name no box, record or row has is reported, and only those");
}

struct FakeMem {
    bool read(uintptr_t a, void *dst, size_t n) const
    {
        if (!a)
            return false;
        std::memcpy(dst, reinterpret_cast<const void *>(a), n);
        return true;
    }
    template <class T> bool get(uintptr_t a, T &v) const { return read(a, &v, sizeof v); }
};

template <class T> static void put(uint8_t *base, size_t off, T v) { std::memcpy(base + off, &v, sizeof v); }

static void entrants()
{
    alignas(16) static uint8_t manager[0x100], registry[0x100], slots[4 * 16 + 4], gms[0x100], mode[0x300], game[0x300],
        rows[3 * L::badge::entrant_stride];
    alignas(16) static uint8_t key_other[16], key_mode[16];
    static uintptr_t manager_site;
    manager_site = reinterpret_cast<uintptr_t>(manager);
    uintptr_t site = reinterpret_cast<uintptr_t>(&manager_site);
    put(manager, L::registry::from_manager, reinterpret_cast<uintptr_t>(registry));
    put(registry, L::registry::slots, reinterpret_cast<uintptr_t>(slots));
    put(registry, L::registry::slots + 8, reinterpret_cast<uintptr_t>(slots + 4 * 16));
    put(slots, 0, reinterpret_cast<uintptr_t>(key_other));
    put(slots, 8, reinterpret_cast<uintptr_t>(game));
    put(slots, 2 * 16, reinterpret_cast<uintptr_t>(key_mode));
    put(slots, 2 * 16 + 8, reinterpret_cast<uintptr_t>(gms));
    slots[4 * 16 + 0] = 1;
    slots[4 * 16 + 2] = 1;
    put(gms, L::badge::mode, reinterpret_cast<uintptr_t>(mode));
    put(mode, L::badge::game, reinterpret_cast<uintptr_t>(game));
    put(game, L::badge::entrants, reinterpret_cast<uintptr_t>(rows));
    put(game, L::badge::entrants + 8, reinterpret_cast<uintptr_t>(rows + sizeof rows));
    auto row = [&](int i, uint8_t vid, uint32_t team, uint32_t driver, uint32_t vehicle) {
        uint8_t *e = rows + i * L::badge::entrant_stride;
        std::memset(e, 0, L::badge::entrant_stride);
        e[L::badge::entrant_vid] = vid;
        put(e, L::badge::entrant_team, team);
        put(e, L::driver::entrant_ref, driver);
        put(e, offsetof(swofty::game::EntrantRecord, vehicle_ref), vehicle);
    };
    row(0, 0xff, 0x3fff0000u, 0x3fff0000u, 0x3fff0000u);
    row(1, 4, (12u << 16) | 0u, (300u << 16) | 0u, 77u << 16);
    row(2, 0, (5u << 16) | ei::MY_TEAM, (301u << 16) | 1u, 78u << 16);

    FakeMem m;
    ei::SystemSlot cache;
    uintptr_t sys = ei::system_by_key(m, site, reinterpret_cast<uintptr_t>(key_mode), cache);
    expect(sys == reinterpret_cast<uintptr_t>(gms) && cache.slot == 2, "the registry walk finds the game mode system by its key");
    expect(ei::system_by_key(m, site, reinterpret_cast<uintptr_t>(key_mode), cache) == sys, "and again from its cached slot");
    slots[4 * 16 + 2] = 0;
    expect(ei::system_by_key(m, site, reinterpret_cast<uintptr_t>(key_mode), cache) == 0, "an unused slot is not taken");
    slots[4 * 16 + 2] = 1;

    ei::Ids ids;
    expect(ei::entrant(m, sys, 4, true, ids) == ei::Read::ok && ids.team_kind == ei::TEAM_TABLE && ids.team == 12 &&
               ids.driver == 300 && ids.driver_domain == 0 && ids.vehicle == 77 && ids.driver_read,
           "the entrant with the car's vehicle index gives its team, driver and vehicle stat ids");
    expect(ei::entrant(m, sys, 0, false, ids) == ei::Read::ok && ids.team_kind == ei::MY_TEAM && ids.team == 5 &&
               !ids.driver_read && ids.driver == ei::NONE,
           "a My Team entrant is its own table kind, and the driver is left unread when asked");
    expect(ei::entrant(m, sys, 9, true, ids) == ei::Read::no_entrant, "a vehicle index no entrant has (the safety car) is none");
    alignas(16) static uint8_t many[40 * L::badge::entrant_stride];
    for (int i = 0; i < 40; i++) {
        uint8_t *e = many + i * L::badge::entrant_stride;
        e[L::badge::entrant_vid] = uint8_t(i);
        put(e, L::badge::entrant_team, uint32_t(100 + i) << 16);
    }
    put(game, L::badge::entrants, reinterpret_cast<uintptr_t>(many));
    put(game, L::badge::entrants + 8, reinterpret_cast<uintptr_t>(many + sizeof many));
    expect(ei::entrant(m, sys, 35, true, ids) == ei::Read::ok && ids.team == 135 &&
               ei::entrant(m, sys, 31, true, ids) == ei::Read::ok && ids.team == 131 &&
               ei::entrant(m, sys, 32, true, ids) == ei::Read::ok && ids.team == 132,
           "a list longer than one read finds the entrants on each side of the read boundary");
    put(game, L::badge::entrants, reinterpret_cast<uintptr_t>(rows));
    put(game, L::badge::entrants + 8, reinterpret_cast<uintptr_t>(rows));
    expect(ei::entrant(m, sys, 4, true, ids) == ei::Read::not_ready, "an empty entrant list is not ready");
    put(game, L::badge::entrants + 8, reinterpret_cast<uintptr_t>(rows + 1));
    expect(ei::entrant(m, sys, 4, true, ids) == ei::Read::not_ready, "a list that is not whole entrants is refused");
}

int main()
{
    refs();
    entrants();
    if (fails) {
        printf("car_identity_test: %d failure(s)\n", fails);
        return 1;
    }
    printf("car_identity_test: ok\n");
    return 0;
}
