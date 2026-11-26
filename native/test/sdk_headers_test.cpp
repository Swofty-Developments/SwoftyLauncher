// Compiles every standard library 1.6 header and instantiates its templates. Built for x86_64-windows-gnu
// (compile and link) and for the host (run): with no core, every call reports unavailable and nothing
// crashes; records round-trip through their JSON form; schemas carry the registry keywords.
#include <bit>
#include <cassert>
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <string>
#include <type_traits>

#include <swofty/d3d12.hpp>
#include <swofty/ease.hpp>
#include <swofty/events.hpp>
#include <swofty/fake_hud.hpp>
#include <swofty/game_assets.hpp>
#include <swofty/game_mem.hpp>
#include <swofty/hud_format.hpp>
#include <swofty/hud_frame.hpp>
#include <swofty/hud_leaves.hpp>
#include <swofty/hud_slot.hpp>
#include <swofty/log_csv.hpp>
#include <swofty/mem.hpp>
#include <swofty/overlay_path.hpp>
#include <swofty/settings_struct.hpp>
#include <swofty/timing.hpp>
#include <swofty/hooks.hpp>
#include <swofty/registry.hpp>
#include <swofty/services.hpp>
#include <swofty/systems/ai.hpp>
#include <swofty/systems/career.hpp>
#include <swofty/systems/cars.hpp>
#include <swofty/systems/content.hpp>
#include <swofty/systems/drivers.hpp>
#include <swofty/systems/liveries.hpp>
#include <swofty/systems/power.hpp>
#include <swofty/systems/pitlane.hpp>
#include <swofty/systems/race_control.hpp>
#include <swofty/systems/sessions.hpp>
#include <swofty/systems/sounds.hpp>
#include <swofty/systems/laps.hpp>
#include <swofty/systems/spectator.hpp>
#include <swofty/systems/teams.hpp>
#include <swofty/systems/tyres.hpp>
#include <swofty/systems/wearables.hpp>
#include <swofty/systems/tracks.hpp>
#include <swofty/systems/tyre_damage.hpp>
#include <swofty/systems/ui.hpp>
#include <swofty/systems/weather.hpp>

#include "sdk_payloads.h"

#if defined(_WIN32)
#include <swofty/assets.hpp>
#include <swofty/career_content.hpp>
#include <swofty/career_wizard.hpp>
#include <swofty/characters.hpp>
#include <swofty/db.hpp>
#include <swofty/hub.hpp>
#include <swofty/hud.hpp>
#include <swofty/view.hpp>
#include <swofty/lang.hpp>
#include <swofty/popup.hpp>
#include <swofty/storage.hpp>
#include <swofty/telemetry.hpp>
#endif

#define SWOFTY_CPP_SIZE(C, ns, T, n)                                                                                             \
    static_assert(std::is_standard_layout_v<swofty::ns::events::T> && std::is_trivially_copyable_v<swofty::ns::events::T>,       \
                  #T " is standard layout and trivially copyable");                                                              \
    static_assert(sizeof(swofty::ns::events::T) == n && sizeof(C) == n, #T " size matches " #C);                                 \
    static_assert(alignof(swofty::ns::events::T) == alignof(C), #T " alignment matches " #C);
SWOFTY_EV_PAYLOADS(SWOFTY_CPP_SIZE)
#define SWOFTY_CPP_FIELD(C, ns, T, f, off)                                                                                       \
    static_assert(offsetof(swofty::ns::events::T, f) == off && offsetof(C, f) == off, #T "." #f " offset matches " #C);
SWOFTY_EV_FIELDS(SWOFTY_CPP_FIELD)
static_assert(std::is_standard_layout_v<swofty::cars::live::Car> && std::is_trivially_copyable_v<swofty::cars::live::Car> &&
              sizeof(swofty::cars::live::Car) == sizeof(SwoftyEvCar) && alignof(swofty::cars::live::Car) == alignof(SwoftyEvCar));
static_assert(std::is_same_v<std::underlying_type_t<swofty::cars::Department>, int32_t>);
static_assert(std::is_same_v<std::underlying_type_t<swofty::teams::LogoContext>, int32_t>);
static_assert(std::is_same_v<std::underlying_type_t<swofty::drivers::Stat>, int32_t>);
static_assert(std::is_same_v<std::underlying_type_t<swofty::race_control::Flag>, uint8_t> &&
              std::is_same_v<std::underlying_type_t<swofty::race_control::RedFlag>, uint8_t> &&
              std::is_same_v<std::underlying_type_t<swofty::race_control::Status>, uint8_t> &&
              std::is_same_v<std::underlying_type_t<swofty::race_control::Deployment>, uint8_t>);

using namespace swofty;

static int failures = 0;
#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #c);                                                             \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

struct ShopApi {
    static constexpr const char *name = "test.shop";
    static constexpr uint32_t version = 1;
    uint32_t size;
    int (*count)(void);
};

struct ShopPoint {
    static constexpr const char *point = "test.shop.open";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    int day;
};

namespace test {
struct Thing {
    static constexpr Version version{1, 2};
    std::string name;
    int n = 0;
    json::Value extra;
};
SWOFTY_RECORD(Thing, name, n)
inline Registry<Thing> things{"test.thing"};
}

SWOFTY_REGISTER
{
    auto v10 = sounds::engine_sounds.add("v10_screamer", {
                                                             .bundle = file("sounds/v10_screamer.bdl"),
                                                             .settings_from = sounds::engine_sounds["game/ferrari_2025"],
                                                         });
    cars::vehicles.layer(cars::vehicles["swofty.classic-seasons-2017/ferrari"], [=](cars::Vehicle &v) { v.engine_sound = v10; });
}

static void unavailable_without_core()
{
    CHECK(!detail::registry_table());
    CHECK(!d3d12::available());
    CHECK(d3d12::on_srv_created([](void *, const d3d12::SrvCreated *) {}) == d3d12::Result::unavailable);
    CHECK(d3d12::on_srv_created(nullptr) == d3d12::Result::invalid);
    CHECK(d3d12::remove_srv_callback(1) == d3d12::Result::unavailable);
    auto v10 = sounds::engine_sounds.add("v10", {.bundle = file("a.bdl")});
    CHECK(v10.empty());
    auto ferrari =
        cars::vehicles.variant("ferrari", cars::vehicles["game/ferrari_2025"], [](cars::Vehicle &v) { v.label = "x"; });
    CHECK(ferrari.empty());
    auto h = content::packages.layer(content::packages["swofty.classic-seasons-2017/f1_2017"],
                                     [](content::Package &p) { p.teams.at("ferrari").engine_sound = {}; });
    h.in(events::mode(events::Mode::time_trial)).in(events::content("swofty.classic-seasons-2017/f1_2017"));
    CHECK(h.result() == Result::unavailable);
    auto sel = cars::handling.layer(tagged<cars::Handling>(cars::handling.tag("c/f1_cars")), [](cars::Handling &x) {
        if (option<std::string>("wing_damage") == "heavy")
            x.wing_damage.damage_scale *= 2;
        x.upgrade("radUpgradeEngine").set("FuelTank", "FuelBsfcMin", '*', 0.85f);
    });
    CHECK(sel.result() == Result::unavailable);
    cars::vehicles.layer(
        under<cars::Vehicle>("game/").except({cars::vehicles["game/ferrari_2025"]}), [](cars::Vehicle &) {}, Scope::profile);
    cars::vehicles.layer(where<cars::Vehicle>([](const cars::Vehicle &v) { return v.performance_class == cars::Class::f1_26; }),
                         [](cars::Vehicle &) {});
    content::packages.layer(tagged(&content::Package::calendar, Tag<career::Calendar>("career.calendar", "c/x")),
                            [](content::Package &) {});
    CHECK(cars::handling.protect(cars::handling["mod.a/ferrari"], {"chassis", "f1_aero"}) == Result::unavailable);
    CHECK(!career::seasons.get(career::seasons["swofty.progressive/2017"]));
    CHECK(career::seasons.status(career::seasons["swofty.progressive/2017"]) == Status::no_registry);
    CHECK(career::seasons.all("swofty.progressive/").empty());
    CHECK(career::seasons.origin(career::seasons["swofty.progressive/2017"], "teams").empty());
    CHECK(!career::seasons.watch([](Ref<career::Season>) {}));
    CHECK(career::ratings.check([](const career::DriverRatings &r, Errors &e) {
        if (r.pace > 100)
            e.add("pace", "0-100");
    }) == Result::unavailable);
    CHECK(career::seasons.provide([](std::string_view) -> std::optional<career::Season> { return std::nullopt; }) ==
          Result::unavailable);
    CHECK(career::seasons.invalidate() == Result::unavailable);
    CHECK(cars::vehicles.tag_add(cars::vehicles.tag("c/f1_cars"), cars::vehicles["mod.a/rb"]) == Result::unavailable);
    CHECK(!cars::vehicles.tag("c/f1_cars").contains(cars::vehicles["game/ferrari_2025"]));
    CHECK(cars::vehicles.tag("c/f1_cars").members().empty());
    CHECK(!cars::vehicles["game/ferrari_2025"].resolved());
    CHECK(option<int>("laps") == 0 && option<std::string>("x").empty() && !option<bool>("y"));
    CHECK(!cars::vehicles.stat_id(cars::vehicles["game/ferrari_2025"]));
    CHECK(last_error().empty());
    Registry<json::Value> untyped{"tails.whatif.presets"};
    CHECK(!untyped.get(untyped["tails.whatif-history/real_2016"]));
    CHECK((adapter<test::Thing, test::Thing>(
               test::things, test::things, [](test::Thing t) { return t; }, [](test::Thing t) { return t; }) ==
           Result::unavailable));
    CHECK(
        (migrate<test::Thing, test::Thing>(test::things, test::things, [](test::Thing t) { return t; }) == Result::unavailable));

    using events::any, events::content, events::Context, events::Mode, events::mode, events::not_, events::on, events::owner,
        events::track, events::year;
    CHECK(on<career::events::SeasonStart>()
              .in(content("swofty.progressive/f1_career_progressive"))
              .when(year(2016, 2019))
              .after([](const career::events::SeasonStart &, const Context &ctx) {
                  auto s = ctx.record<career::Season>();
                  (void)s;
                  (void)ctx.career();
                  (void)ctx.participants();
              }) == swofty::events::Result::unavailable);
    CHECK(on<career::events::AddActivity>()
              .in(any({content("a/x"), not_(track("game/monaco"))}))
              .replace([](career::events::AddActivity &, const Context &) { return false; }) ==
          swofty::events::Result::unavailable);
    CHECK(on<cars::events::Built>().before([](cars::events::Built &e, const Context &) {
        (void)e.car.get(cars::live::Field::steering_lock);
    }) == swofty::events::Result::unavailable);
    CHECK(on("swofty.progressive.seat_change").after([](std::span<const std::byte>, const Context &) {}) ==
          swofty::events::Result::unavailable);
    CHECK(events::define<ShopPoint>(owner("test")) == swofty::events::Result::unavailable);
    ShopPoint p{sizeof p, 1, 3};
    CHECK(events::fire(p));
    CHECK(!Context().career() && Context().participants().empty() && !Context().in(mode(Mode::grand_prix)));

    CHECK(hooks::before<career::events::AddActivity>([](career::events::AddActivity &) {}) == hooks::Result::unavailable);
    CHECK(hooks::before<career::events::AddActivity>(hooks::Priority{10}, [](career::events::AddActivity &) {}) ==
          hooks::Result::unavailable);
    CHECK(hooks::replace<ShopPoint>([](ShopPoint &) { return true; }) == hooks::Result::unavailable);
    CHECK(hooks::after<ShopPoint>([](const ShopPoint &, const hooks::Outcome &) {}) == hooks::Result::unavailable);
    CHECK(hooks::define<ShopPoint>() == hooks::Result::unavailable);
    CHECK(hooks::fire(p) && !hooks::live<ShopPoint>());

    static const ShopApi shop{sizeof(ShopApi), [] { return 3; }};
    CHECK(services::publish(shop) == services::Result::unavailable);
    CHECK(services::when<ShopApi>([](const ShopApi &) {}) == services::Result::unavailable);
    CHECK(!services::available());

    career::ApplyReport report;
    CHECK(career::apply_lineup(career::Season{}, &report) == career::Result::unavailable);
    CHECK(career::apply_teams(career::Season{}, &report) == career::Result::unavailable);
    CHECK(teams::set_logo(teams::identities["game/williams"], teams::logos["swofty.progressive/williams_2017"]) ==
          Result::unavailable);
    CHECK(teams::set_name(teams::identities["game/williams"], {.title = Text::literal("Williams")}) == Result::unavailable);
    CHECK(teams::set_colours(teams::identities["game/williams"], {.primary = Colour("#00A0DE")}) == Result::unavailable);
    CHECK(drivers::set_look(drivers::identities["game/lewis_hamilton"], drivers::looks["swofty.classic-seasons-2017/hamilton"]) ==
          Result::unavailable);
    auto toro = teams::identities.add("toro_rosso", {
                                                        .donor = teams::identities["game/tororosso"],
                                                        .constructor = teams::constructors["constructor/toro_rosso"],
                                                        .provides = {teams::constructors["constructor/toro_rosso"]},
                                                        .names = {.title = Text::key("lng_cs_team_17_toro_rosso"), .code = "STR"},
                                                        .livery_name = "cs17_toro_rosso",
                                                    });
    CHECK(toro.empty());
    teams::pit_times.add("toro_rosso", {.team = toro, .from = teams::pit_times["game/tororosso"]});
    teams::logos.layer(teams::logos["game/williams"],
                       [](teams::Logo &l) { l.images[teams::LogoContext::hud] = file("logos/w.png"); });
    auto look =
        drivers::looks.add("salo_look", {.helmet = wearables::helmets["game/helmet_tier01_livery03"], .pose = "arms_crossed"});
    drivers::identities.variant("salo", drivers::identities["game/jarno_opmeer"], [=](drivers::Identity &i) {
        i.person = drivers::people["person/mika_salo"];
        i.look = look;
    });
    drivers::looks.layer(
        drivers::looks["game/lewis_hamilton"], [](drivers::Look &l) { l.helmet = wearables::helmets["x/y"]; }, Scope::career);
    wearables::season_sets.add("mercedes_2016",
                               {.team = teams::identities["swofty.classic-seasons-2016/mercedes"], .season = 2016});
    CHECK(wearables::helmets.all("giggles.helmet-exchange/").empty());
    CHECK(!drivers::heads.get(drivers::heads["mod.senna-face/senna"]));
    CHECK(career::set_seat(7, 1, Ref<drivers::Identity>("game/pierre_gasly")) == career::Result::unavailable);
    CHECK(cars::reset_upgrades(0) == Result::unavailable);
    CHECK(!cars::live::available() && cars::live::cars().empty());
    CHECK(!cars::live::field("aero.lift_scale") && !cars::live::Car(0).get("aero.lift_scale") &&
          cars::live::Car(0).set("aero.lift_scale", 0.5f) == Result::invalid);
    {
        auto layer = cars::live::Layer::scale(cars::live::Cars::all(), "engine.power_scale", 0.9725f, 5);
        CHECK(!layer.ok() && layer.result() == Result::unavailable);
    }
    cars::Profile profile;
    profile.power_scale = 0.9725f;
    profile.limiter_offset_rpm = -300;
    profile.ai_grip_scale = 0.87f;
    profile.when = {events::SessionType::practice, events::SessionType::race};
    cars::Profile round;
    CHECK(from_json(to_json(profile), round) && round.power_scale == 0.9725f && round.limiter_offset_rpm == -300 &&
          round.ai_grip_scale == 0.87f && round.when.size() == 2 && !round.fuel_modes);
    CHECK(std::string(cars::profiles.name()) == "car.profile" && !cars::profiles.schema().empty());
    CHECK(std::string(cars::setup_layouts.name()) == "setup.layout" &&
          cars::setup_layouts.schema().find("\"x-version\":\"1.0\"") != std::string::npos &&
          cars::setup_layouts.schema().find("\"sections\"") != std::string::npos);
    cars::SetupLayout layout_round;
    cars::SetupLayoutRow lean_row;
    lean_row.slider = "brake_cooling_front";
    lean_row.name = "Lean";
    lean_row.unit = "%";
    lean_row.min = 83.3f;
    lean_row.max = 100.f;
    lean_row.steps = 20;
    layout_round.sections.push_back({json::Value(int64_t(0)), "Fuel", {lean_row}});
    cars::SetupLayout layout_back;
    CHECK(from_json(to_json(layout_round), layout_back) && layout_back.sections.size() == 1 &&
          layout_back.sections[0].rows[0].steps == 20 && layout_back.sections[0].rows[0].min == 83.3f);
    cars::Profile team;
    team.cars.team = {"mercedes"};
    team.cars.driver = {"game/lewis_hamilton"};
    team.scope = cars::ProfileScope::global;
    cars::Profile team_round;
    CHECK(from_json(to_json(team), team_round) && team_round.cars.team.size() == 1 && team_round.cars.driver.size() == 1 &&
          team_round.scope == cars::ProfileScope::global && cars::Profile{}.scope == cars::ProfileScope::automatic);
    CHECK(cars::profiles.schema().find("\"x-version\":\"1.1\"") != std::string::npos &&
          cars::profiles.schema().find("\"automatic\"") != std::string::npos &&
          cars::profiles.schema().find("\"driver\"") != std::string::npos);
    CHECK(detail::run_register(nullptr) == 1);
}

static std::string added_node;
static float added_at;

static int32_t add_part(void *user, const char *node, float detach_at, bool debris)
{
    added_node = node;
    added_at = detach_at;
    return debris && user ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

static std::string chosen_sound;

static int32_t set_sound(void *user, const char *id)
{
    chosen_sound = id;
    static_cast<sounds::events::BundleResolve *>(user)->engine_sound_id = chosen_sound.c_str();
    return SWOFTY_ASSET_OK;
}

static std::string logo_key;

static int32_t set_logo_key(void *user, const char *key)
{
    logo_key = key;
    static_cast<teams::events::LogoResolve *>(user)->key_str = logo_key.c_str();
    return SWOFTY_ASSET_OK;
}

static std::string added_item;

static int32_t add_item(void *, const char *registry, const char *id)
{
    added_item = std::string(registry) + " " + id;
    return SWOFTY_ASSET_OK;
}

static void payload_accessors()
{
    int wing = 0;
    cars::live::Car car(4, &wing);
    SwoftyEvCar c = std::bit_cast<SwoftyEvCar>(car);
    CHECK(c.index == 4 && c.handle == &wing && car.handle() == &wing);

    cars::events::VtfRead r{sizeof r, 1, car, "F1WingDamage", "DamageScale", nullptr, 1.5f};
    CHECK(r.section() == "F1WingDamage" && r.field() == "DamageScale" && r.upgrade().empty() && r.car.index() == 4);
    SwoftyEvVtfRead rc;
    std::memcpy(&rc, &r, sizeof rc);
    CHECK(std::strcmp(rc.section_str, "F1WingDamage") == 0 && rc.car.index == 4 && rc.value == 1.5f);

    cars::events::UpgradesFit f{};
    CHECK(f.vehicle().empty());
    f.vehicle_id = "game/haas_2026";
    f.department = cars::Department::aero;
    CHECK(f.vehicle() == cars::vehicles["game/haas_2026"] && f.vehicle().id() == "game/haas_2026");

    cars::events::PartsRegister p{};
    CHECK(p.add({.node = "mirror_l"}) == Result::unavailable);
    p.add_fn = &add_part;
    p.user = &wing;
    CHECK(p.add({.node = "mirror_l", .detach_at = 0.6f}) == Result::ok && added_node == "mirror_l" && added_at == 0.6f);
    CHECK(p.add({.node = "mirror_r", .debris = false}) == Result::invalid);

    sounds::events::BundleResolve b{sizeof b, 1, 4, "game/williams_2026", "williams", "game/williams_2026", nullptr, nullptr};
    CHECK(b.vehicle().id() == "game/williams_2026" && b.switch_name() == "williams");
    CHECK(b.set_engine_sound(sounds::engine_sounds["mod.esp/v10_screamer"]) == Result::unavailable);
    b.set_engine_sound_fn = &set_sound;
    b.user = &b;
    CHECK(b.set_engine_sound(sounds::engine_sounds["mod.esp/v10_screamer"]) == Result::ok &&
          b.engine_sound().id() == "mod.esp/v10_screamer");

    teams::events::LogoResolve lr{
        sizeof lr, 1, teams::LogoContext::driver_select, "game/williams", "williams", "williams_logo", nullptr, nullptr};
    CHECK(lr.team().id() == "game/williams" && lr.livery_name() == "williams" && lr.key() == "williams_logo");
    CHECK(lr.set_key("progressive_williams_2017") == Result::unavailable);
    lr.set_key_fn = &set_logo_key;
    lr.user = &lr;
    CHECK(lr.set_key("progressive_williams_2017") == Result::ok && lr.key() == "progressive_williams_2017");
    SwoftyEvLogoResolve lc;
    std::memcpy(&lc, &lr, sizeof lc);
    CHECK(lc.context == 0 && std::strcmp(lc.team_id, "game/williams") == 0);

    teams::events::NameResolve nr{sizeof nr, 1, "game/mercedes", "Mercedes", true, nullptr, nullptr};
    CHECK(nr.team().id() == "game/mercedes" && nr.name() == Text::literal("Mercedes"));
    CHECK(nr.set_name(Text::key("lng_x")) == Result::unavailable);

    drivers::events::RatingsRead rr{sizeof rr, 1, "game/lando_norris", drivers::Stat::pace, 90.0f};
    CHECK(rr.driver().id() == "game/lando_norris" && rr.stat == drivers::Stat::pace);
    SwoftyEvRatingsRead rrc = std::bit_cast<SwoftyEvRatingsRead>(rr);
    CHECK(rrc.stat == 3 && rrc.value == 90.0f);
    drivers::events::NumberAssign na{sizeof na, 1, nullptr, 27};
    CHECK(na.driver().empty() && na.number == 27);
    drivers::events::HeadResolve hr{sizeof hr, 1, "game/daniil_kvyat", "game/kvyat", nullptr, nullptr};
    CHECK(hr.driver().id() == "game/daniil_kvyat" && hr.head().id() == "game/kvyat");
    CHECK(hr.set_head(drivers::heads["mod.kvyat-face/kvyat"]) == Result::unavailable);

    wearables::events::CatalogueBuild cb{sizeof cb, 1, nullptr, nullptr};
    CHECK(cb.add(wearables::helmets["giggles.helmet-exchange/hamilton_2017"]) == Result::unavailable);
    cb.add_fn = &add_item;
    cb.user = &cb;
    CHECK(cb.add(wearables::helmets["giggles.helmet-exchange/hamilton_2017"]) == Result::ok &&
          added_item == "helmet giggles.helmet-exchange/hamilton_2017");
    CHECK(cb.add(wearables::gloves["mod.kit/g"]) == Result::ok && added_item == "gloves mod.kit/g");

    career::events::SeasonStart s{sizeof s, 1, 2017, 1};
    SwoftyEvSeasonStart sc = std::bit_cast<SwoftyEvSeasonStart>(s);
    CHECK(sc.year == 2017 && sc.season_index == 1);
}

static void round_trip()
{
    career::Season s{
        .data_group = "f1_career_progressive",
        .year = 2017,
        .box = content::packages["swofty.classic-seasons-2017/f1_2017"].or_else(content::packages["game/f1_2025"]),
        .calendar = career::calendars["swofty.progressive/2017"],
        .teams = {{.slot = 1,
                   .team = Ref<teams::Identity>("game/mercedes"),
                   .seats = {Ref<drivers::Identity>("game/lewis_hamilton"), Ref<drivers::Identity>("game/valtteri_bottas")}}},
        .changes = {{.round = 15,
                     .slot = 7,
                     .seat = 1,
                     .out = Ref<drivers::Identity>("game/daniil_kvyat"),
                     .in = Ref<drivers::Identity>("game/pierre_gasly")}},
        .extra = json::parse("{\"reserve\":\"game/x\"}"),
    };
    s.teams.at(2).team = Ref<teams::Identity>("game/ferrari");
    CHECK(s.teams.size() == 2 && s.teams.find(2) && !s.teams.find(3));
    CHECK(s.changes.find(15, 7, 1) && s.changes.erase(15, 7, 1) == true && s.changes.empty());
    std::string text = to_json(s);
    CHECK(text.find("\"box\":\"swofty.classic-seasons-2017/f1_2017|game/f1_2025\"") != std::string::npos);
    CHECK(text.find("\"reserve\":\"game/x\"") != std::string::npos);
    career::Season back;
    CHECK(from_json(text, back));
    CHECK(back.year == 2017 && back.teams.size() == 2 && back.teams.find(1)->seats[1].id() == "game/valtteri_bottas");
    CHECK(to_json(back) == text);

    cars::Handling h{.cls = cars::Class::f1_26};
    h.upgrade("radUpgradeEngine").set("Engine", "WearICERate", '*', 0.8f);
    h.engine.revs_power_spline = Curve::flat(0, 1200, 550000).scale_y(1.05f);
    std::string ht = to_json(h);
    CHECK(ht.find("\"class\":\"f1_26\"") != std::string::npos && ht.find("\"op\":\"*\"") != std::string::npos);
    cars::Handling hb;
    CHECK(from_json(ht, hb) && hb.cls == cars::Class::f1_26 && hb.rad_upgrades.size() == 1);

    content::Package pkg{.title = Text::literal("F1 2017 (V10)"), .entrant_list = "esp_f1_2017_v10", .season = 2017};
    pkg.teams.at("ferrari").engine_sound = Ref<sounds::EngineSound>("mod.esp/v10_screamer");
    std::string pt = to_json(pkg);
    CHECK(pt.find("{\"literal\":\"F1 2017 (V10)\"}") != std::string::npos &&
          pt.find("\"stat_id\":\"auto\"") != std::string::npos);
    content::Package pb;
    CHECK(from_json(pt, pb) && pb.title.is_literal() && pb.teams.find(std::string("ferrari")));

    sounds::EngineSound es{.bundle = stock("audio/2025/bundles/redbull_2025.bdl")};
    CHECK(to_json(es).find("{\"stock\":\"audio/2025/bundles/redbull_2025.bdl\"}") != std::string::npos);

    career::Calendar cal{.rounds = {{.round = 1,
                                     .race = "lng_gp_usa",
                                     .venue = Ref<tracks::Venue>("venue/phoenix"),
                                     .date = std::chrono::year{1990} / 3 / 11}},
                         .seasons = 1};
    CHECK(to_json(cal).find("\"date\":\"1990-03-11\"") != std::string::npos);
    career::Calendar cb;
    CHECK(from_json(to_json(cal), cb) && cb.rounds.find(1)->date == cal.rounds.find(1)->date);

    teams::Identity t{.donor = teams::identities["game/tororosso"],
                      .constructor = teams::constructors["constructor/toro_rosso"],
                      .names = {.title = Text::literal("Toro Rosso"), .code = "STR"},
                      .colours = {.menu = Colour(0xFF0A1E3Cu), .primary = Colour("#0A1E3C")},
                      .livery_name = "cs17_toro_rosso",
                      .stat_id = 300};
    std::string tt = to_json(t);
    CHECK(tt.find("\"constructor\":\"constructor/toro_rosso\"") != std::string::npos &&
          tt.find("\"wear\":null") != std::string::npos && tt.find("\"stat_id\":300") != std::string::npos &&
          tt.find("\"menu\":\"#0A1E3C\"") != std::string::npos);
    teams::Identity tb;
    CHECK(from_json(tt, tb) && tb.names.title.is_literal() && tb.colours.primary == Colour("#0A1E3C") && !tb.wear);
    CHECK(to_json(tb) == tt);

    teams::Logo logo;
    logo.images[teams::LogoContext::hud] = file("logos/williams_2017_hud.png");
    logo.images[teams::LogoContext::driver_select] = stock("ui/williams_driver_select.dds");
    std::string lt = to_json(logo);
    CHECK(lt.find("\"driver_select\":{\"stock\":\"ui/williams_driver_select.dds\"}") != std::string::npos &&
          lt.find("\"hud\":\"logos/williams_2017_hud.png\"") != std::string::npos);
    teams::Logo lb;
    CHECK(from_json(lt, lb) && lb.images.size() == 2 &&
          lb.images[teams::LogoContext::hud] == file("logos/williams_2017_hud.png"));

    drivers::Identity di{.person = drivers::people["person/mika_salo"],
                         .names = {Text::literal("Mika"), Text::literal("Salo"), Text::literal("Mika Salo"), "SAL"},
                         .number = 17,
                         .look = drivers::looks["swofty.driver-manager/mika_salo_look"],
                         .pools = {"f1_2026_driver_pool"}};
    drivers::Identity db;
    CHECK(from_json(to_json(di), db) && db.number == 17 && db.pools.size() == 1 && db.names.code == "SAL" &&
          db.look.id() == "swofty.driver-manager/mika_salo_look");
    drivers::Stats st{.driver = drivers::identities["swofty.driver-manager/mika_salo"], .pace = {78, 84}};
    CHECK(to_json(st).find("\"pace\":{\"start\":78,\"peak\":84}") != std::string::npos);
    drivers::Head hd{
        .archive = file("heads/senna/"), .rig_from = drivers::heads["game/male_ls222"], .body = drivers::BodyType::male};
    CHECK(to_json(hd).find("\"body\":\"male\"") != std::string::npos);

    wearables::Helmet hm{.archive = file("helmets/hamilton_2017_helmet.erp"),
                         .from = wearables::helmets["game/lewis_hamilton"],
                         .name = Text::literal("Hamilton 2017 (HX)"),
                         .driver = drivers::identities["game/lewis_hamilton"],
                         .colours =
                             std::array<Colour, 4>{Colour("#FFE600"), Colour("#00A19B"), Colour("#7A2A8B"), Colour("#FFFFFF")}};
    std::string hmt = to_json(hm);
    CHECK(hmt.find("\"colours\":[\"#FFE600\",\"#00A19B\",\"#7A2A8B\",\"#FFFFFF\"]") != std::string::npos);
    wearables::Helmet hmb;
    CHECK(from_json(hmt, hmb) && hmb.colours && (*hmb.colours)[3] == Colour("#FFFFFF"));
    wearables::Suit su{.archive = file("kit/mercedes_2016_suit.erp"), .from = wearables::suits["game/mercedes"]};
    CHECK(to_json(su).find("\"colours\":null") != std::string::npos);
    wearables::SeasonSet ss{.team = teams::identities["swofty.classic-seasons-2016/mercedes"], .season = 2016};
    wearables::SeasonSet ssb;
    CHECK(from_json(to_json(ss), ssb) && ssb.season == 2016 && ssb.suit.empty());

    career::Calendar cal_json{.seasons = 3, .transfer_windows = {10}};
    CHECK(to_json(cal_json).find("\"seasons\":3") != std::string::npos);
    CHECK(!from_json("{\"year\":", back));
}

static void schemas()
{
    std::string s = career::seasons.schema();
    CHECK(s.find("\"x-registry\":\"career.season\"") != std::string::npos);
    CHECK(s.find("\"x-version\":\"1.1\"") != std::string::npos);
    CHECK(s.find("\"x-applies\":\"read\"") != std::string::npos);
    CHECK(s.find("\"x-snapshot\":true") != std::string::npos && s.find("\"x-snapshot-budget\":65536") != std::string::npos);
    CHECK(s.find("\"x-key\":[\"slot\"]") != std::string::npos);
    CHECK(s.find("\"x-key\":[\"round\",\"slot\",\"seat\"]") != std::string::npos);
    CHECK(s.find("\"x-ref\":\"content.package\"") != std::string::npos);
    CHECK(s.find("\"x-ref\":\"driver.identity\"") != std::string::npos);
    std::string v = cars::vehicles.schema();
    CHECK(v.find("\"x-applies\":\"boot\"") != std::string::npos && v.find("\"enum\":[\"f1\",\"f1_26\"") != std::string::npos);
    CHECK(cars::handling.schema().find("\"class\":") != std::string::npos);
    std::string hs = cars::handling.schema();
    CHECK(hs.find("\"x-version\":\"1.1\"") != std::string::npos && v.find("\"x-version\":\"1.1\"") != std::string::npos);
    CHECK(hs.find("\"ratios\":{\"type\":\"array\"") != std::string::npos);
    CHECK(hs.find("\"ers\":{\"type\":[\"object\",\"null\"]") != std::string::npos);
    CHECK(hs.find("\"ffb_scale\"") != std::string::npos && hs.find("\"tyre_physics_group\"") != std::string::npos);
    CHECK(hs.find("\"slipstream\"") != std::string::npos && hs.find("\"hardware\"") != std::string::npos);
    CHECK(hs.find("\"mode\":{\"type\":\"string\",\"enum\":[\"off\",\"lag_spoof\",\"full\"]") != std::string::npos);
    CHECK(hs.find("\"op\":{\"type\":\"string\",\"enum\":[\"=\",\"+\",\"-\",\"*\"]}") != std::string::npos);
    CHECK(hs.find("\"chassis_com_z\":{\"type\":\"object\"") != std::string::npos);
    CHECK(v.find("\"drs_capable\":{\"type\":[\"boolean\",\"null\"]") != std::string::npos);
    CHECK(v.find("\"x-ref\":\"power.supplier\"") != std::string::npos && v.find("\"tyre_group\"") != std::string::npos);
    {
        cars::Handling h;
        h.suspension.front.rest_z = 1.7f;
        h.suspension.rear.rest_z = -1.7f;
        h.chassis.com_z = 0.171f;
        CHECK(std::abs(cars::front_weight(h) - 0.4497f) < 0.001f);
        h.ers.reset();
        CHECK(to_json(h).find("\"ers\":null") != std::string::npos);
        cars::Handling back;
        CHECK(from_json(to_json(h), back) && !back.ers.has_value());
        h.gearbox.ratios = {3.0f, 2.0f, 1.0f};
        CHECK(to_json(h).find("\"ratios\":[3,2,1]") != std::string::npos);
    }
    CHECK(tyres::physical.name() == std::string("tyre.physical") && power::suppliers.name() == std::string("power.supplier"));
    CHECK(content::packages.schema().find("\"x-key\":[\"key\"]") != std::string::npos);
    CHECK(test::things.schema().find("\"x-version\":\"1.2\"") != std::string::npos);
    CHECK(Registry<json::Value>("x.y").schema().empty());
    CHECK(sounds::engine_sounds.at_major() == "engine_sound@1");
    CHECK(teams::identities.schema().find("\"x-ref\":\"team.constructor\"") != std::string::npos);
    CHECK(teams::identities.schema().find("\"x-ref\":\"team.logo\"") != std::string::npos);
    CHECK(teams::logos.schema().find("\"x-applies\":\"read\"") != std::string::npos);
    CHECK(teams::pit_times.schema().find("\"x-ref\":\"team.pit_times\"") != std::string::npos);
    std::string garage = teams::garages.schema();
    CHECK(garage.find("\"x-registry\":\"team.garage\"") != std::string::npos &&
          garage.find("\"x-applies\":\"boot\"") != std::string::npos);
    CHECK(garage.find("\"x-ref\":\"content.package\"") != std::string::npos && teams::garages.at_major() == "team.garage@1");
    teams::Garage garage_rec;
    CHECK(swofty::to_json(garage_rec).find("\"root\":\"eaid://environment_package/core_assets/objects/textures/\"") !=
              std::string::npos &&
          garage_rec.srgb);
    std::string boards = tracks::sponsors.schema();
    CHECK(boards.find("\"x-registry\":\"track.sponsors\"") != std::string::npos &&
          boards.find("\"x-applies\":\"boot\"") != std::string::npos);
    CHECK(boards.find("\"x-ref\":\"content.package\"") != std::string::npos && tracks::sponsors.at_major() == "track.sponsors@1");
    tracks::Sponsors boards_rec;
    CHECK(swofty::to_json(boards_rec).find("\"root\":\"eaid://environment_package/core_assets/shared_textures/sponsors/\"") !=
              std::string::npos &&
          boards_rec.srgb);
    std::string swap = teams::vehicle_overrides.schema();
    CHECK(swap.find("\"x-registry\":\"team.vehicle_override\"") != std::string::npos &&
          swap.find("\"x-applies\":\"session\"") != std::string::npos);
    CHECK(swap.find("\"x-ref\":\"vehicle\"") != std::string::npos &&
          swap.find("\"x-ref\":\"driver.identity\"") != std::string::npos &&
          swap.find("\"x-ref\":\"track\"") != std::string::npos &&
          teams::vehicle_overrides.at_major() == "team.vehicle_override@1");
    teams::VehicleOverride swap_rec;
    CHECK(swap_rec.when.year == 0 && swap_rec.drivers.empty() && swap_rec.when.session_types.empty());
    std::string box_ui = content::package_ui.schema();
    CHECK(box_ui.find("\"x-registry\":\"ui.per_package\"") != std::string::npos &&
          box_ui.find("\"x-applies\":\"content\"") != std::string::npos);
    CHECK(box_ui.find("\"x-ref\":\"content.package\"") != std::string::npos &&
          content::package_ui.at_major() == "ui.per_package@1");
    content::PackageUi ui_rec;
    CHECK(swofty::to_json(ui_rec).find("\"root\":\"eaid://ui_package/art/\"") != std::string::npos && ui_rec.srgb &&
          ui_rec.archives.empty());
    CHECK(std::string(ui::texture_slices.name()) == "ui.texture_slice" && std::string(ui::tiles.name()) == "ui.tile" &&
          std::string(ui::screens.name()) == "ui.screen" && std::string(ui::routes.name()) == "ui.route" &&
          std::string(ui::options.name()) == "ui.option" && std::string(ui::hud_widgets.name()) == "hud.widget");
    std::string ui_tile = ui::tiles.schema();
    CHECK(ui_tile.find("\"x-applies\":\"read\"") != std::string::npos &&
          ui_tile.find("\"x-ref\":\"ui.texture_slice\"") == std::string::npos &&
          ui_tile.find("\"x-ref\":\"ui.route\"") == std::string::npos && ui_tile.find("\"car_category\"") != std::string::npos &&
          ui::tiles.at_major() == "ui.tile@1");
    CHECK(ui::routes.schema().find("\"x-ref\":\"ui.screen\"") == std::string::npos &&
          ui::texture_slices.schema().find("\"x-applies\":\"boot\"") != std::string::npos &&
          ui::hud_widgets.schema().find("\"x-applies\":\"session\"") != std::string::npos);
    ui::Tile ui_tile_rec;
    ui_tile_rec.place = ui::Place::f1_world;
    ui_tile_rec.title = Text::key("lng_mod_tile");
    ui_tile_rec.image = ui::texture_slices["mod.settings/tile"];
    ui_tile_rec.route = ui::routes["mod.settings/open"];
    std::string ui_tile_json = swofty::to_json(ui_tile_rec);
    CHECK(ui_tile_json.find("\"place\":\"f1_world\"") != std::string::npos &&
          ui_tile_json.find("\"image\":\"mod.settings/tile\"") != std::string::npos &&
          ui_tile_json.find("\"route\":\"mod.settings/open\"") != std::string::npos);
    ui::Tile ui_tile_back;
    CHECK(swofty::from_json(ui_tile_json, ui_tile_back) && ui_tile_back.place == ui::Place::f1_world &&
          ui_tile_back.route.id() == "mod.settings/open");
    CHECK(ui::set_text("career.hub.title", "x") == Result::unavailable && ui::set_bool("a.b", true) == Result::unavailable &&
          ui::set_ref("a.b", "c.d") == Result::unavailable);
    {
        static std::string ui_path, ui_text;
        static int ui_flag = -1;
        SwoftyUi fake_ui{sizeof(SwoftyUi), SWOFTY_UI_VERSION,
                         [](const char *p, const char *t) -> int {
                             ui_path = p;
                             ui_text = t;
                             return SWOFTY_ASSET_OK;
                         },
                         [](const char *p, int v) -> int {
                             ui_path = p;
                             ui_flag = v;
                             return SWOFTY_ASSET_OK;
                         },
                         [](const char *, const char *) -> int { return SWOFTY_ASSET_UNAVAILABLE; }};
        ui::detail::table_override() = &fake_ui;
        CHECK(ui::set_text("career.hub.title", "Season 1990") == Result::ok && ui_path == "career.hub.title" &&
              ui_text == "Season 1990");
        CHECK(ui::set_bool("ui_menu_data.flag", true) == Result::ok && ui_path == "ui_menu_data.flag" && ui_flag == 1);
        CHECK(ui::set_ref("a.b", "c.d") == Result::unavailable);
        ui::detail::table_override() = nullptr;
    }
    {
        static std::vector<std::string> fake_ids{"a", "b"};
        static SwoftyTileList fake_tiles{sizeof(SwoftyTileList),
                                         SWOFTY_TILE_LIST_VERSION,
                                         nullptr,
                                         [](void *) -> int { return int(fake_ids.size()); },
                                         [](void *, int i, SwoftyTileView *out) -> int {
                                             if (i < 0 || size_t(i) >= fake_ids.size())
                                                 return SWOFTY_ASSET_INVALID;
                                             *out = {fake_ids[size_t(i)].c_str(), "T", "R", "", "home", 0};
                                             return SWOFTY_ASSET_OK;
                                         },
                                         [](void *, int i) -> int {
                                             if (i < 0 || size_t(i) >= fake_ids.size())
                                                 return SWOFTY_ASSET_INVALID;
                                             fake_ids.erase(fake_ids.begin() + i);
                                             return SWOFTY_ASSET_OK;
                                         },
                                         nullptr,
                                         [](void *, int at, const SwoftyTileSpec *spec) -> int {
                                             fake_ids.insert(fake_ids.begin() + at, spec->id);
                                             return SWOFTY_ASSET_OK;
                                         }};
        ui::events::HomeTiles home{sizeof(ui::events::HomeTiles), 2, nullptr, &fake_tiles};
        CHECK(home.tiles().count() == 2 && home.tiles().get(1)->id == "b" && home.tiles().get(1)->place == "home");
        CHECK(home.remove(0) == Result::ok && home.insert(1, {.id = "z", .title = "Z", .route = "r"}) == Result::ok &&
              home.tiles().count() == 2 && home.tiles().get(1)->id == "z");
        CHECK(home.tiles().move(0, 1) == Result::unavailable && home.remove(9) == Result::invalid);
        std::string seen;
        for (auto t : home.tiles())
            seen += std::string(t.id) + ";";
        CHECK(seen == "b;z;");
        ui::events::HomeTiles v1{16, 1, nullptr, &fake_tiles};
        CHECK(!v1.tiles() && v1.remove(0) == Result::unavailable);
    }
    ui::events::LangLookup lookup{sizeof lookup, 1, "lng_x", "en", "X", nullptr, nullptr};
    CHECK(lookup.key() == "lng_x" && lookup.language() == "en" && lookup.text() == "X" &&
          lookup.set_text("Y") == Result::unavailable);
    std::string kit = content::box_assets.schema();
    CHECK(kit.find("\"x-registry\":\"content.box_assets\"") != std::string::npos &&
          kit.find("\"x-applies\":\"boot\"") != std::string::npos && kit.find("\"marshals\"") != std::string::npos);
    CHECK(kit.find("\"x-ref\":\"content.package\"") != std::string::npos &&
          content::box_assets.at_major() == "content.box_assets@1");
    content::BoxAssets kit_rec;
    kit_rec.resources = {"eaid://character_package_shared/crowd/"};
    std::string kit_json = swofty::to_json(kit_rec);
    CHECK(kit_json.find("\"kind\":\"other\"") != std::string::npos && kit_json.find("\"archive\":null") != std::string::npos &&
          kit_json.find("\"resources\":[\"eaid://character_package_shared/crowd/\"]") != std::string::npos && kit_rec.srgb);
    std::string tree = career::rnd_trees.schema();
    CHECK(tree.find("\"x-applies\":\"boot\"") != std::string::npos && tree.find("\"x-key\":[\"label\"]") != std::string::npos);
    CHECK(tree.find("\"enum\":[\"aerodynamics\",\"chassis\",\"durability\",\"engine\"]") != std::string::npos);
    std::string market = career::markets.schema();
    CHECK(market.find("\"x-applies\":\"boot\"") != std::string::npos && market.find("\"x-key\":[\"team\"]") != std::string::npos);
    CHECK(market.find("\"x-ref\":\"team.identity\"") != std::string::npos &&
          market.find("\"x-ref\":\"driver.identity\"") != std::string::npos);
    career::Market m{.retirements = std::vector<career::Retirement>{{.driver = drivers::identities["game/fernando_alonso"],
                                                                     .retire = std::chrono::year{2027} / 12 / 14,
                                                                     .announce = std::chrono::year{2027} / 9 / 16}}};
    m.team_ratings.items().push_back({.team = teams::identities["game/mclaren_26"], .rating = 97});
    CHECK(to_json(m).find("\"retire\":\"2027-12-14\"") != std::string::npos);
    career::RndTree rt{.token = "mercedes"};
    rt.nodes.items().push_back(
        {.label = "sw_wing_1", .type = "Drag", .size = career::UpgradeSize::major, .cost = 650, .after = {"default"}});
    CHECK(to_json(rt).find("\"size\":\"major\"") != std::string::npos &&
          to_json(rt).find("\"data_group\":\"\"") != std::string::npos);
    CHECK(drivers::identities.schema().find("\"x-ref\":\"driver.look\"") != std::string::npos);
    CHECK(drivers::identities.schema().find("\"x-ref\":\"driver.person\"") != std::string::npos);
    CHECK(drivers::looks.schema().find("\"x-ref\":\"helmet\"") != std::string::npos);
    CHECK(drivers::stats.schema().find("\"x-ref\":\"driver.identity\"") != std::string::npos);
    CHECK(wearables::season_sets.schema().find("\"x-ref\":\"suit\"") != std::string::npos);
    CHECK(wearables::helmets.schema().find("\"x-applies\":\"boot\"") != std::string::npos);
    CHECK(wearables::season_sets.at_major() == "wearable.season_set@1" && drivers::heads.at_major() == "driver.head@1");
    std::string l = liveries::liveries.schema();
    CHECK(l.find("\"x-registry\":\"livery\"") != std::string::npos && l.find("\"x-applies\":\"boot\"") != std::string::npos);
    CHECK(l.find("\"x-ref\":\"vehicle\"") != std::string::npos && l.find("\"enum\":[\"owned\",\"game\"]") != std::string::npos);
    CHECK(liveries::liveries.at_major() == "livery@1");
    std::string d = power::drs_rules.schema();
    CHECK(d.find("\"x-registry\":\"rules.drs\"") != std::string::npos && d.find("\"x-applies\":\"boot\"") != std::string::npos);
    CHECK(d.find("\"race_start\":{\"type\":\"integer\"}") != std::string::npos && power::drs_rules.at_major() == "rules.drs@1");
    CHECK(content::packages.schema().find("\"x-ref\":\"rules.drs\"") != std::string::npos);
    power::DrsRule never{.race_start = 200, .red_flag_restart = 200, .safety_car_end = 200};
    std::string never_json = swofty::to_json(never);
    power::DrsRule back;
    CHECK(never_json.find("\"race_start\":200") != std::string::npos && swofty::from_json(never_json, back) &&
          back.safety_car_end == 200 && power::DrsRule{}.race_start == 1);
    std::string trk = tracks::tracks.schema();
    CHECK(trk.find("\"x-registry\":\"track\"") != std::string::npos && trk.find("\"x-applies\":\"boot\"") != std::string::npos);
    CHECK(trk.find("\"x-ref\":\"track.venue\"") != std::string::npos && trk.find("\"x-ref\":\"track\"") != std::string::npos);
    std::string ven = tracks::venues.schema();
    CHECK(ven.find("\"x-shared\":{\"provider_registry\":\"track\",\"provides\":\"/provides\",\"resolved\":\"/track\"}") !=
          std::string::npos);
    std::string td = tracks::data.schema();
    CHECK(td.find("\"x-applies\":\"session\"") != std::string::npos && td.find("\"x-ref\":\"tyre.class\"") != std::string::npos &&
          td.find("\"x-ref\":\"vehicle\"") != std::string::npos);
    std::string pp = tracks::per_package.schema();
    CHECK(pp.find("\"x-applies\":\"content\"") != std::string::npos &&
          pp.find("\"x-ref\":\"track.layer\"") != std::string::npos &&
          pp.find("\"x-ref\":\"content.package\"") != std::string::npos);
    CHECK(tracks::tracks.at_major() == "track@1" && tracks::per_package.at_major() == "track.per_package@1");
}

static void tracks_records()
{
    tracks::Track hock{
        .code = "hockenheim",
        .name = "Hockenheimring",
        .base = tracks::tracks["game/austria"],
        .provides = {tracks::venues["venue/hockenheimring"]},
        .names = {.title = Text::key("lng_hockenheim_title"), .gp = Text::key("lng_hockenheim_gp"), .acronym = "HOC"},
        .files = file("track/"),
        .length_metres = 4574,
        .corners = 17};
    std::string ht = to_json(hock);
    CHECK(ht.find("\"base\":\"game/austria\"") != std::string::npos && ht.find("\"slot\":null") != std::string::npos &&
          ht.find("\"provides\":[\"venue/hockenheimring\"]") != std::string::npos &&
          ht.find("\"time_trial\":false") != std::string::npos);
    tracks::Track hb;
    CHECK(from_json(ht, hb) && hb.code == "hockenheim" && hb.provides.size() == 1 && hb.stat_id.is_automatic() && !hb.time_trial);

    tracks::TrackData d{.track = tracks::tracks["mod.hockenheim/hockenheim"], .data_set = "f1_2026", .laps = 67};
    d.fuel.burn_per_lap = 1.62f;
    d.tyres = {.softer = "C4", .medium = "C3", .harder = "C2"};
    d.date = {.day = 21, .month = 7};
    std::string dt = to_json(d);
    CHECK(dt.find("\"data_set\":\"f1_2026\"") != std::string::npos && dt.find("\"softer\":\"C4\"") != std::string::npos &&
          dt.find("\"month\":7") != std::string::npos && dt.find("\"tyre_class\":null") != std::string::npos);
    tracks::TrackData db;
    CHECK(from_json(dt, db) && db.laps == 67 && db.tyres.harder == "C2" && db.date.day == 21);

    tracks::PackageTrack pt{.package = content::packages["swofty.classic-seasons-2016/f1_2016"],
                            .venue = tracks::venues["venue/hockenheimring"],
                            .laps = 67,
                            .fuel = tracks::Fuel{.burn_per_lap = 1.71f}};
    std::string pj = to_json(pt);
    CHECK(pj.find("\"venue\":\"venue/hockenheimring\"") != std::string::npos && pj.find("\"track\":null") != std::string::npos &&
          pj.find("\"offered\":true") != std::string::npos && pj.find("\"world\":null") != std::string::npos &&
          pj.find("\"pit\":null") != std::string::npos);
    tracks::PackageTrack pb;
    CHECK(from_json(pj, pb) && pb.offered && pb.laps == 67 && pb.fuel && !pb.pit && !pb.names &&
          pb.venue.id() == "venue/hockenheimring");

    tracks::TrackList list{
        .calendar = {tracks::tracks["game/bahrain"]},
        .more = {tracks::tracks["mod.bahrain24/bahrain24"]},
        .left_out = {{tracks::tracks["mod.miami24/igora_drive"], "its swofty_f1 world has 10 garages; the box has 11 teams"}}};
    std::string lj = to_json(list);
    CHECK(lj.find("\"left_out\":[{\"track\":\"mod.miami24/igora_drive\",\"reason\":") != std::string::npos);

    CHECK(tracks::tracks.add("hockenheim", hock).id().empty());
    CHECK(tracks::venues.get(tracks::venues["venue/sochi"]) == std::nullopt);
    CHECK(tracks::set_track_fuel(tracks::tracks["game/monaco"], {.burn_per_lap = 1.4f}) == tracks::Result::unavailable);
}

static void liveries_records()
{
    auto car = cars::vehicles["game/redbull_2025"];
    auto look = liveries::liveries.add("repaint", {.vehicle = car, .textures = file("livery/")});
    CHECK(look.empty());
    cars::vehicles.layer(car, [=](cars::Vehicle &v) { v.livery = look; });
    liveries::Livery special{.vehicle = cars::vehicles["game/toro_rosso_2026"],
                             .archive = file("archives/toro_rosso_alphatauri.erp"),
                             .special = true,
                             .picker_name = "lng_rm_livery_alphatauri",
                             .unlock = liveries::Unlock::owned};
    std::string j = to_json(special);
    CHECK(j.find("\"special\":true") != std::string::npos && j.find("\"unlock\":\"owned\"") != std::string::npos);
    liveries::Livery back;
    CHECK(from_json(j, back) && back.special && back.suffix == "auto" && back.picker_name == "lng_rm_livery_alphatauri");
}

static uint32_t g_mode, g_flags;
static int g_active = 1, g_follow = -1, g_camera = -1, g_on = -1;
static int fake_control(uint32_t car, uint32_t *mode, uint32_t *flags)
{
    if (car == 9)
        return SWOFTY_ASSET_LATE;
    *mode = g_mode;
    *flags = g_flags;
    return SWOFTY_ASSET_OK;
}
static int fake_set_control(uint32_t car, uint32_t mode)
{
    if (car == 7)
        return SWOFTY_REFUSED;
    g_mode = mode;
    return SWOFTY_ASSET_OK;
}
static int fake_disqualify(uint32_t car)
{
    return car == 6 ? SWOFTY_ASSET_LATE : car == 2 ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}
static int fake_active(int on) { return g_on = on, SWOFTY_ASSET_OK; }
static int fake_follow(uint32_t car) { return g_follow = int(car), SWOFTY_ASSET_OK; }
static int fake_camera(int32_t c) { return g_camera = c, c == 1 || c == 2 ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID; }
static int fake_current(int32_t *car, int32_t *camera)
{
    if (g_follow < 0)
        return SWOFTY_ASSET_LATE;
    *car = g_follow;
    *camera = g_camera;
    return SWOFTY_ASSET_OK;
}
static int fake_available() { return g_active; }

static int fake_owned(const char *name)
{
    return std::string(name) == "f1_2026"   ? SWOFTY_OWNERSHIP_OWNED
           : std::string(name) == "f1_2025" ? SWOFTY_OWNERSHIP_NOT_OWNED
                                            : SWOFTY_OWNERSHIP_UNKNOWN;
}

static void content_ownership()
{
    using swofty::content::Ownership;
    CHECK(swofty::content::owned("f1_2026") == Ownership::unknown);
    SwoftyContentOps ops{SWOFTY_CONTENT_OPS_VERSION, fake_owned};
    swofty::detail::set_content_ops_table(&ops);
    CHECK(swofty::content::owned("f1_2026") == Ownership::owned);
    CHECK(swofty::content::owned("f1_2025") == Ownership::not_owned);
    CHECK(swofty::content::owned("other") == Ownership::unknown);
    ops.version = 0;
    CHECK(swofty::content::owned("f1_2026") == Ownership::unknown);
    swofty::detail::set_content_ops_table(nullptr);
}

static void session_operations()
{
    cars::live::Car car(3);
    CHECK(sessions::set_control(car, sessions::Control::ai) == sessions::Result::unavailable);
    CHECK(!car.control());
    CHECK(spectator::set_active(true) == Result::unavailable && !spectator::current());
    SwoftySessionOps ops{SWOFTY_SESSION_OPS_VERSION,
                         fake_available,
                         fake_control,
                         fake_set_control,
                         fake_active,
                         fake_follow,
                         fake_camera,
                         fake_current,
                         fake_disqualify};
    CHECK(sessions::disqualify(car) == sessions::Result::unavailable);
    swofty::detail::set_session_ops_table(&ops);
    CHECK(sessions::disqualify(cars::live::Car(2)) == sessions::Result::ok);
    CHECK(sessions::disqualify(cars::live::Car(6)) == sessions::Result::late);
    CHECK(sessions::disqualify(cars::live::Car(9)) == sessions::Result::invalid);
    ops.version = 1;
    CHECK(sessions::disqualify(cars::live::Car(2)) == sessions::Result::unavailable);
    ops.version = SWOFTY_SESSION_OPS_VERSION;
    CHECK(sessions::set_control(car, sessions::Control::ai) == sessions::Result::ok && g_mode == 1);
    g_flags = 1;
    auto st = car.control();
    CHECK(st && st->mode == sessions::Control::ai && st->pit_lane());
    g_flags = 0x20;
    CHECK(car.control() && !car.control()->pit_lane());
    CHECK(!cars::live::Car(9).control());
    CHECK(sessions::set_control(cars::live::Car(7), sessions::Control::human) == sessions::Result::blocked);
    CHECK(spectator::set_active(true) == Result::ok && g_on == 1);
    CHECK(!spectator::current());
    CHECK(spectator::follow(cars::live::Car(5)) == Result::ok);
    CHECK(spectator::set_camera(spectator::Camera::broadcast) == Result::ok && g_camera == 2);
    auto view = spectator::current();
    CHECK(view && view->following.index() == 5 && view->camera == spectator::Camera::broadcast);
    CHECK(spectator::set_camera(static_cast<spectator::Camera>(9)) == Result::invalid);
    g_active = 0;
    CHECK(sessions::set_control(car, sessions::Control::human) == sessions::Result::unavailable && !car.control());
    swofty::detail::set_session_ops_table(nullptr);
}

static void hud_runtime_headers()
{
    swofty::hud::Leaves leaves{"view_model_data.hud_probe_"};
    auto flag = leaves.boolean("on");
    auto rows = leaves.rows<swofty::hud::BoolLeaf>("r", 2, 2, [](swofty::hud::Leaves &l, int) { return l.boolean("a"); });
    leaves.set(flag, true);
    leaves.set(rows[1], true);
    swofty::hud::Leaves::Atomic group(leaves);
    CHECK(leaves.flush() == 0);
    swofty::timing::Tower tower;
    swofty::timing::Session session;
    swofty::timing::Config config;
    tower.update(session, {}, -1, 0.0, config);
    CHECK(!tower.on() && tower.rows() == 0);
    swofty::diag::Csv csv("probe", {{"a", swofty::diag::Csv::i32}}, 5);
    CHECK(!csv.available() && !csv.row());
    swofty::hud::Frame frame;
    CHECK(!frame.begin({}, 0.01));
    CHECK(swofty::ease::level(1.f, 4) == 4 && swofty::hud::format::speed(10.f).kph == 36);
}

namespace fake_game_assets {
int available() { return 1; }
int read(const char *uri, uint32_t fragment, void *buf, size_t cap, size_t *len)
{
    std::string_view bytes = fragment == SWOFTY_GAME_ALL_FRAGMENTS ? "abcdef" : "abc";
    if (std::string_view(uri) != "eaid://ui_package/x.bin")
        return SWOFTY_ASSET_INVALID;
    *len = bytes.size();
    if (!buf || cap < bytes.size())
        return SWOFTY_TOO_SMALL;
    std::memcpy(buf, bytes.data(), bytes.size());
    return SWOFTY_ASSET_OK;
}
int texture(const char *, SwoftyGameTexture *out)
{
    *out = {70, 2, 2, 1, 3};
    return SWOFTY_ASSET_OK;
}
int texture_rgba(const char *, uint32_t layer, uint32_t, void *rgba, size_t cap, uint32_t *w, uint32_t *h)
{
    if (cap < 16)
        return SWOFTY_TOO_SMALL;
    std::memset(rgba, int(layer), 16);
    *w = *h = 2;
    return SWOFTY_ASSET_OK;
}
const char *const names[] = {"ferrari", "redbull"};
int catalogue_find(const char *, const char *name, char *uri, size_t cap, size_t *len, uint32_t *layer)
{
    std::string_view u = "eaid://ui_package/art/a.texarray.image";
    for (uint32_t i = 0; i < 2; i++)
        if (std::string_view(name) == names[i] && cap > u.size()) {
            std::memcpy(uri, u.data(), u.size());
            uri[u.size()] = 0;
            *len = u.size();
            *layer = i;
            return SWOFTY_ASSET_OK;
        }
    return SWOFTY_ASSET_INVALID;
}
int catalogue_name(const char *, uint32_t index, char *name, size_t cap, size_t *len)
{
    if (index >= 2)
        return SWOFTY_ASSET_LATE;
    *len = std::strlen(names[index]);
    if (cap <= *len)
        return SWOFTY_TOO_SMALL;
    std::memcpy(name, names[index], *len + 1);
    return SWOFTY_ASSET_OK;
}
const SwoftyGameAssets table = {1, available, read, texture, texture_rgba, catalogue_find, catalogue_name};
}

static void game_assets_wrappers()
{
    namespace ga = swofty::game_assets;
    ga::detail::set_table(nullptr);
#if !defined(_WIN32)
    CHECK(!ga::available());
#endif
    std::string out;
    ga::detail::set_table(&fake_game_assets::table);
    CHECK(ga::available());
    CHECK(ga::read("eaid://ui_package/x.bin", out) == ga::Result::ok && out == "abcdef");
    CHECK(ga::read("eaid://ui_package/x.bin", out, 0) == ga::Result::ok && out == "abc");
    CHECK(ga::read("eaid://ui_package/y.bin", out) == ga::Result::invalid && out == "abc");
    size_t n = 0;
    CHECK(ga::size("eaid://ui_package/x.bin", &n) == ga::Result::ok && n == 6);
    ga::Image im;
    CHECK(ga::rgba("eaid://ui_package/art/a.texarray.image", 2, im) == ga::Result::ok && im.width == 2 && im.rgba.size() == 16 &&
          im.rgba[5] == 2);
    auto slot = ga::find("overrides.team_logos.list", "redbull");
    CHECK(slot && slot->layer == 1 && slot->texture == "eaid://ui_package/art/a.texarray.image");
    CHECK(!ga::find("overrides.team_logos.list", "nobody"));
    std::vector<std::string> list;
    CHECK(ga::names("overrides.team_logos.list", list) == ga::Result::ok && list.size() == 2 && list[1] == "redbull");
    ga::detail::set_table(nullptr);
}

namespace game_mem_fake {
struct Call {
    std::string id;
    size_t size = 0, offset = 0, n = 0;
    uint32_t value = 0;
    bool expect = false;
    int restores = 0;
};
inline Call last;
inline uint64_t heap[4];
int write_data(const char *id, size_t size, size_t offset, const void *bytes, size_t n, const void *expect)
{
    last.id = id ? id : "";
    last.size = size, last.offset = offset, last.n = n, last.expect = expect != nullptr;
    std::memcpy(&last.value, bytes, n < sizeof last.value ? n : sizeof last.value);
    return SWOFTY_ASSET_OK;
}
int restore_data(const char *id)
{
    last.id = id ? id : "*";
    last.restores++;
    return SWOFTY_ASSET_OK;
}
void *alloc(void *, size_t size, size_t) { return size <= sizeof heap ? heap : nullptr; }
void free(void *, void *p) { last.value = p == heap ? 1u : 0u; }
inline constexpr swofty::data<"k_Test_ColourTable", uint32_t[6]> colours{};
inline constexpr swofty::data<"g_Test_Scalar", float> scalar{};
}

static void game_memory()
{
    namespace gm = swofty::game_mem;
    using namespace game_mem_fake;
    CHECK(!gm::available());
    CHECK(gm::write(colours, 4, 0xff00ff00u) == gm::Result::unavailable);
    CHECK(gm::alloc(16) == nullptr);
    static_assert(std::is_same_v<gm::detail::element_type<decltype(colours)>, uint32_t>);
    static_assert(gm::detail::element_count<decltype(colours)> == 6 && gm::detail::element_count<decltype(scalar)> == 1);
    SwoftyGameMem t{SWOFTY_GAME_MEM_VERSION, write_data, restore_data, alloc, free};
    gm::set_table(&t);
    CHECK(gm::available());
    CHECK(gm::write(colours, 5, 0xff112233u, 0xffffffffu) == gm::Result::ok);
    CHECK(last.id == "k_Test_ColourTable" && last.size == 24 && last.offset == 20 && last.n == 4 && last.value == 0xff112233u &&
          last.expect);
    CHECK(gm::write(colours, 6, 0u) == gm::Result::invalid);
    CHECK(gm::write(scalar, 0, 1.5f) == gm::Result::ok && last.size == 4 && !last.expect);
    CHECK(gm::restore(colours) == gm::Result::ok && last.id == "k_Test_ColourTable");
    CHECK(gm::restore_all() == gm::Result::ok && last.id == "*" && last.restores == 2);
    uint64_t *p = gm::alloc_array<uint64_t>(4);
    CHECK(p == heap && gm::alloc_array<uint64_t>(5) == nullptr && gm::alloc_array<uint64_t>(0) == nullptr);
    gm::free(p);
    CHECK(last.value == 1u);
    CHECK(std::strcmp(gm::name(gm::Result::invalid), "invalid") == 0);
    gm::set_table(nullptr);
}

int main()
{
    game_memory();
    hud_runtime_headers();
    game_assets_wrappers();
    unavailable_without_core();
    session_operations();
    content_ownership();
    liveries_records();
    payload_accessors();
    round_trip();
    schemas();
    tracks_records();
    if (failures) {
        std::printf("sdk_headers_test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("sdk_headers_test: ok\n");
    return 0;
}

#if defined(_WIN32)
SWOFTY_ADDON()
#endif
