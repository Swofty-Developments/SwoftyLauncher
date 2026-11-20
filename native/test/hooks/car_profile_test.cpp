// Host unit test for car.profile (car_profile.hpp) on fake cars: the F1 1990 race engine and AI quali DLLs as one
// record, scope (session types, ai_only and player_only, car indices, identity-needing selectors), priority order,
// extra live field paths, re-evaluation dropping the old layers, the record parsing, and within against a Grand Prix
// and a Time Trial box's facts with the check that names a within no box can hold.
// native/test/run_tests.sh car_live builds and runs it.
#include "core/hooks/car_profile.hpp"

#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>

namespace cl = swofty_core::hooks::car_live;
namespace cp = swofty_core::hooks::car_profile;

static int fails;
static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Tag {};
using Ops = cl::Ops<Tag>;
using Applier = cp::Applier<Ops>;

static constexpr int CARS = 3;
alignas(16) static uint8_t car_mem[CARS][0x600], aero[CARS][0x1a10], engine[CARS][0x8c0];
static uintptr_t table[32];
static uint32_t count = CARS;

template <class T> static T &at(uint8_t *base, size_t off) { return *reinterpret_cast<T *>(base + off); }

static void world()
{
    memset(car_mem, 0, sizeof car_mem);
    memset(aero, 0, sizeof aero);
    memset(engine, 0, sizeof engine);
    for (int i = 0; i < CARS; i++) {
        uint8_t *c = car_mem[i];
        table[i] = reinterpret_cast<uintptr_t>(c);
        c[cl::CAR_VEHICLE_ID] = uint8_t(i);
        at<uint32_t>(c, cl::CAR_IS_AI) = i == 0 ? 0 : 1;
        at<void *>(c, cl::F1_AERO) = aero[i];
        at<void *>(aero[i], cl::SYSTEM_CAR) = c;
        at<void *>(c, cl::ENGINE) = engine[i];
        at<void *>(engine[i], cl::SYSTEM_CAR) = c;
        at<float>(aero[i], 0x19a4) = 1.0f;
        at<float>(aero[i], 0x19a0) = 1.0f;
    }
    cl::Env e;
    e.read = [](uintptr_t a, void *out, size_t n) {
        memcpy(out, reinterpret_cast<const void *>(a), n);
        return true;
    };
    e.write = [](uintptr_t a, const void *in, size_t n) {
        memcpy(reinterpret_cast<void *>(a), in, n);
        return true;
    };
    e.flow_thread = [] { return true; };
    e.enabled = [] { return true; };
    e.table = reinterpret_cast<uintptr_t>(table);
    e.count = reinterpret_cast<uintptr_t>(&count);
    Ops::bind(std::move(e));
}

static cp::Entry entry(const char *id, const char *json)
{
    cp::Entry e;
    e.id = id;
    std::string why;
    swofty::json::Error err;
    auto v = swofty::json::parse(json, &err);
    bool ok = err.ok() && cp::parse(v, e.profile, &why);
    expect(ok, std::string("parse ") + id + (ok ? "" : ": " + why));
    return e;
}

static const char *RACE_ENGINE =
    R"({"when":["practice","race"],"power_scale":0.9725,"limiter_offset_rpm":-300,"ai_grip_scale":0.87})";

int main()
{
    world();
    auto race = entry("community.f1-1990/race_engine", RACE_ENGINE);
    expect(race.profile.power_scale == 0.9725f && race.profile.limiter_offset_rpm == -300.0f &&
               race.profile.ai_grip_scale == 0.87f && race.profile.when.size() == 2,
           "the F1 1990 race engine and AI grip are one record");

    cp::Memo memo;
    cp::Memo::Key key;
    key.store = 7;
    key.session = "race";
    expect(!memo.fresh(1, key), "a car never evaluated is not fresh");
    memo.note(1, key);
    expect(memo.fresh(1, key) && !memo.fresh(0, key), "an evaluated car is fresh for the same inputs only");
    cp::Memo::Key other = key;
    other.store = 8;
    expect(!memo.fresh(1, other), "a registry change makes the car evaluate again");
    other = key;
    other.session = "qualifying";
    expect(!memo.fresh(1, other), "a session change makes the car evaluate again");
    other = key;
    other.ai = true;
    expect(!memo.fresh(1, other), "a change of AI control makes the car evaluate again");
    memo.forget(1);
    expect(!memo.fresh(1, key), "a rebuilt car evaluates again");
    memo.note(1, key);
    memo.clear();
    expect(!memo.fresh(1, key), "clear forgets every car");

    float v = 0;
    auto in_session = [&](const char *type) {
        for (uint32_t i = 0; i < 3; i++)
            Applier::evaluate({race}, type, nullptr, i, i != 0);
    };
    in_session("race");
    uintptr_t eng = reinterpret_cast<uintptr_t>(engine[1]);
    expect(std::fabs(Ops::drive_torque(eng, 100.0f, 1000.0f, 1371.8f) - 97.25f) < 1e-3f &&
               std::fabs(Ops::ai_grip(1, 0, 2.0f) - 1.74f) < 1e-5f,
           "in a race: power x0.9725 and AI grip x0.87 in the dry");
    float cut = 1371.8f - 300.0f * cl::RPM_TO_RAD_S;
    expect(Ops::drive_torque(eng, 100.0f, cut, 1371.8f) == 0.0f, "and the torque is cut 300 rpm under the limiter");
    in_session("qualifying");
    expect(Ops::drive_torque(eng, 100.0f, 1000.0f, 1371.8f) == 100.0f && Ops::ai_grip(1, 0, 2.0f) == 2.0f &&
               Ops::op_get(0, nullptr, SWOFTY_CAR_ENGINE_POWER_SCALE, &v) == SWOFTY_ASSET_OK && v == 1.0f,
           "in qualifying the previous session's layers are dropped and the car is stock");
    in_session("practice");
    expect(std::fabs(Ops::drive_torque(eng, 100.0f, 1000.0f, 1371.8f) - 97.25f) < 1e-3f, "practice runs the race engine");

    world();
    auto ai = entry("a/ai_only", R"({"ai_only":true,"lift_scale":0.5})");
    auto player = entry("a/player_only", R"({"player_only":true,"drag_scale":0.9})");
    for (uint32_t i = 0; i < 3; i++)
        Applier::evaluate({ai, player}, "race", nullptr, i, i != 0);
    Ops::flush();
    expect(at<float>(aero[0], 0x19a4) == 1.0f && at<float>(aero[0], 0x19a0) == 0.9f && at<float>(aero[1], 0x19a4) == 0.5f &&
               at<float>(aero[1], 0x19a0) == 1.0f,
           "ai_only reaches the AI cars and player_only the human's, written to the aero system");
    auto indices = entry("a/indices", R"({"cars":{"indices":[2]},"power_scale":1.1})");
    for (uint32_t i = 0; i < 3; i++)
        Applier::evaluate({indices}, "race", nullptr, i, true);
    expect(Ops::op_get(2, nullptr, SWOFTY_CAR_ENGINE_POWER_SCALE, &v) == SWOFTY_ASSET_OK && std::fabs(v - 1.1f) < 1e-6f &&
               Ops::op_get(1, nullptr, SWOFTY_CAR_ENGINE_POWER_SCALE, &v) == SWOFTY_ASSET_OK && v == 1.0f,
           "cars.indices picks cars by index");
    Ops::flush();
    expect(at<float>(aero[1], 0x19a4) == 1.0f, "re-evaluating drops the earlier profiles' layers and restores the aero");

    auto team = entry("a/team", R"({"cars":{"team":["game/ferrari"]},"power_scale":1.1})");
    auto rep = Applier::evaluate({team}, "race", nullptr, 0, false);
    expect(rep.matched == 0 && rep.needs_identity.size() == 1, "a team or vehicle selector waits for the car identity (T121)");

    auto low = entry("a/low", R"({"priority":-1,"power_scale":0.5})");
    auto high = entry("a/high", R"({"priority":3,"extra":{},"engine.power_scale":{"set":2.0}})");
    expect(high.profile.extra.if_object() && high.profile.extra.if_object()->size() == 1,
           "a key the record does not name is a live field path");
    Applier::evaluate({high, low}, "race", nullptr, 0, false);
    expect(Ops::op_get(0, nullptr, SWOFTY_CAR_ENGINE_POWER_SCALE, &v) == SWOFTY_ASSET_OK && v == 2.0f,
           "profiles compose by priority: the higher set wins");
    auto path = entry("a/path", R"({"aero.drag_scale":0.7,"brakes.bias":{"add":0.01},"no.such.field":1})");
    auto r2 = Applier::evaluate({path}, "race", nullptr, 0, false);
    expect(r2.refused.size() == 1 && r2.layers == 2, "an unknown field path is refused and reported, the rest apply");
    Ops::flush();
    expect(at<float>(aero[0], 0x19a0) == 0.7f, "extra sets a live field");

    auto scoped = entry("a/scoped", R"({"power_scale":0.5})");
    int asked = 0;
    auto none = Applier::evaluate(
        {scoped}, "race",
        [&](const cp::Entry &) {
            ++asked;
            return false;
        },
        0, false);
    expect(asked == 1 && none.matched == 0, "a profile whose within selector does not match the session is skipped");
    const std::string gp = "swofty.swofty-seasons-2012/sw_2012", tt = "swofty.swofty-seasons-2012/sw_2012_tt";
    auto facts_of = [](const std::string &box, const char *mode, const char *type) {
        swofty_core::hooks::Facts f;
        f.mode = mode;
        f.content = box;
        f.content_registry = "content.package";
        f.content_owner = "swofty.swofty-seasons-2012";
        f.chain = {box};
        f.owners = {f.content_owner};
        f.session.emplace();
        f.session->type = type;
        f.session->setup_mode = "swofty_f1";
        return f;
    };
    auto time_trial = facts_of(tt, "time_trial", "time_trial");
    auto grand_prix = facts_of(gp, "grand_prix", "race");
    auto either = entry("a/either", (R"({"within":[{"content":[")" + gp + R"(",")" + tt + R"("]}],"lift_scale":0.9})").c_str());
    auto both =
        entry("a/both", (R"({"within":[{"content":")" + gp + R"("},{"content":")" + tt + R"("}],"lift_scale":0.9})").c_str());
    std::string why;
    expect(cp::within_matches(either.profile, time_trial, &why) && cp::within_matches(either.profile, grand_prix, &why),
           "one content entry naming the Grand Prix and Time Trial boxes matches both sessions");
    expect(!cp::within_matches(both.profile, time_trial, &why) && !cp::within_matches(both.profile, grand_prix, &why) &&
               why.empty(),
           "two content entries have to hold at once, so neither box matches");
    auto tt_only = entry("a/tt_only", (R"({"within":{"content":")" + tt + R"("},"lift_scale":0.9})").c_str());
    expect(cp::within_matches(tt_only.profile, time_trial, &why) && !cp::within_matches(tt_only.profile, grand_prix, &why),
           "a single within object scopes the Time Trial box alone");
    auto broken = entry("a/broken", R"({"within":[{"colour":"red"}],"lift_scale":0.9})");
    expect(!cp::within_matches(broken.profile, time_trial, &why) && why.find("colour") != std::string::npos,
           "a within that does not compile matches nowhere and says why");

    auto boxes = [&] { return std::vector<std::vector<std::string>>{{gp}, {tt}, {"game/f1_2025"}}; };
    std::string never = cp::unreachable(both.profile.within, boxes);
    expect(never.find(gp + " and " + tt) != std::string::npos &&
               never.find(R"({"content": [")" + gp + R"(", ")" + tt + R"("]})") != std::string::npos,
           "a within whose content entries no box holds at once is named, with the one-entry form");
    expect(cp::unreachable(either.profile.within, boxes).empty() && cp::unreachable(tt_only.profile.within, boxes).empty() &&
               cp::unreachable(swofty::json::Value(), boxes).empty(),
           "one content entry, a single object and no within are never named");
    auto variant = [&] { return std::vector<std::vector<std::string>>{{"a/f1_2017_v10", "a/f1_2017"}}; };
    auto nested = entry("a/nested", R"({"within":[{"content":"a/f1_2017"},{"content":"a/f1_2017_v10"}]})");
    expect(cp::unreachable(nested.profile.within, variant).empty(),
           "two content entries a variant and its base both hold are not named");
    auto exact = entry("a/exact", R"({"within":[{"content":"a/f1_2017"},{"content_exact":"a/f1_2017"}]})");
    expect(!cp::unreachable(exact.profile.within, variant).empty(),
           "content_exact holds only in the box itself, not in its variants");

    auto bad = swofty::json::parse(R"({"power_scale":"fast"})");
    swofty::cars::Profile p;
    expect(!cp::parse(bad, p, &why), "a record with a wrong type is refused");

    if (fails) {
        printf("car_profile_test: %d failure(s)\n", fails);
        return 1;
    }
    printf("car_profile_test: ok\n");
    return 0;
}
