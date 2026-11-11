#include "ghost_swlap_logic.hpp"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iterator>
#include <random>

namespace gt = runtime::ghost_trace;
namespace lf = runtime::lap_file;

static int failures = 0;
#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

static gt::Trace sample_trace(std::mt19937 &rng, size_t n, float lap)
{
    gt::Trace t;
    gt::default_summary(t.summary);
    float sec[3] = {lap * 0.3f, lap * 0.3f, lap * 0.4f};
    gt::set_times(t, lap, sec, true);
    std::uniform_int_distribution<int> byte(0, 255), mask(0, 0x7ff);
    std::uniform_real_distribution<float> pos(-900, 900);
    uint32_t ms = 0;
    for (size_t i = 0; i < n; i++) {
        gt::Record r;
        r.ms = ms;
        ms += 10 + uint32_t(byte(rng));
        r.mask = uint32_t(mask(rng)) | gt::CH_POSITION;
        for (float &p : r.pos)
            p = pos(rng);
        for (int16_t &q : r.quat)
            q = int16_t(byte(rng) * 100 - 12000);
        r.steer = int8_t(byte(rng));
        r.gear = uint8_t(byte(rng) & 15);
        r.throttle = uint8_t(byte(rng));
        r.brake = uint8_t(byte(rng));
        r.rpm = uint8_t(byte(rng));
        r.wing = uint8_t(byte(rng) & 1);
        r.speed = uint8_t(byte(rng));
        r.teleport = byte(rng) & 1 ? gt::TELEPORT_JUMP : gt::TELEPORT_NONE;
        r.progress = uint16_t(byte(rng) * 257);
        t.records.push_back(r);
    }
    return t;
}

static bool same(const gt::Record &a, const gt::Record &b)
{
    if (a.ms != b.ms || a.mask != b.mask)
        return false;
    uint32_t m = a.mask;
    if ((m & gt::CH_POSITION) && std::memcmp(a.pos, b.pos, sizeof a.pos))
        return false;
    if ((m & gt::CH_ORIENTATION) && std::memcmp(a.quat, b.quat, sizeof a.quat))
        return false;
    if ((m & gt::CH_STEERING) && a.steer != b.steer)
        return false;
    if ((m & gt::CH_GEAR) && a.gear != b.gear)
        return false;
    if ((m & gt::CH_THROTTLE) && a.throttle != b.throttle)
        return false;
    if ((m & gt::CH_BRAKE) && a.brake != b.brake)
        return false;
    if ((m & gt::CH_RPM) && a.rpm != b.rpm)
        return false;
    if ((m & gt::CH_REAR_WING) && a.wing != b.wing)
        return false;
    if ((m & gt::CH_SPEED) && a.speed != b.speed)
        return false;
    if ((m & gt::CH_TELEPORT) && a.teleport != b.teleport)
        return false;
    if ((m & gt::CH_PROGRESS) && a.progress != b.progress)
        return false;
    return true;
}

static void layout_tests()
{
    CHECK(gt::record_bytes(0) == 4);
    CHECK(gt::record_bytes(gt::CH_ALL) == 4 + 33);
    CHECK(gt::record_bytes(gt::CH_POSITION) == 16);
    CHECK(gt::record_bytes(gt::CH_ORIENTATION | gt::CH_PROGRESS) == 4 + 8 + 2);
    CHECK(gt::HEAD == 0x88 && gt::MAX_DATA + gt::FIXED == 0xc800);
    gt::Record r;
    r.ms = 1234;
    r.mask = gt::CH_POSITION | gt::CH_GEAR;
    r.pos[0] = 1.5f;
    r.gear = 4;
    auto b = gt::encode_record(r);
    CHECK(b.size() == gt::record_bytes(r.mask));
    uint32_t h = gt::get<uint32_t>(b.data());
    CHECK((h & 0x3ffff) == 1234 && (h >> 18) == 9);
    gt::Trace t;
    gt::default_summary(t.summary);
    CHECK(gt::get<uint32_t>(t.summary) == 0x16000003 && gt::get<uint32_t>(t.summary + 4) == 0x7ff);
    CHECK(gt::get<uint32_t>(t.summary + 0x18) == 0xffffffffu);
}

static void round_trips()
{
    std::mt19937 rng(7);
    for (int round = 0; round < 40; round++) {
        gt::Trace t = sample_trace(rng, 1 + size_t(rng() % 1200), 90.0f);
        std::vector<uint8_t> bytes;
        std::string why;
        bool ok = gt::encode(t, bytes, &why);
        size_t data = 0;
        bool fit = gt::fits(t, &data) == gt::Fit::ok;
        CHECK(ok == fit);
        if (!ok)
            continue;
        CHECK(bytes.size() == gt::HEAD + data);
        CHECK(gt::get<uint64_t>(bytes.data()) == gt::FIXED + data);
        gt::Trace back;
        size_t used = 0;
        CHECK(gt::decode(bytes.data(), bytes.size(), back, &used) == gt::Status::ok);
        CHECK(used == bytes.size());
        CHECK(back.records.size() == t.records.size());
        CHECK(std::memcmp(back.summary, t.summary, gt::SUMMARY) == 0);
        for (size_t i = 0; i < back.records.size() && i < t.records.size(); i++)
            CHECK(same(back.records[i], t.records[i]));
        std::vector<uint8_t> again;
        CHECK(gt::encode(back, again) && again == bytes);
    }
}

static void reader_rules()
{
    std::mt19937 rng(3);
    gt::Trace t = sample_trace(rng, 200, 60.0f);
    std::vector<uint8_t> b;
    CHECK(gt::encode(t, b));
    gt::Trace out;
    CHECK(gt::decode(b.data(), 7, out) == gt::Status::short_input);
    auto z = b;
    std::memset(z.data(), 0, 8);
    CHECK(gt::decode(z.data(), z.size(), out) == gt::Status::bad_size);
    auto big = b;
    uint64_t n = b.size();
    std::memcpy(big.data(), &n, 8);
    CHECK(gt::decode(big.data(), big.size(), out) == gt::Status::bad_size);
    auto small = b;
    n = 0x7f;
    std::memcpy(small.data(), &n, 8);
    CHECK(gt::decode(small.data(), small.size(), out) == gt::Status::bad_size);
    auto cut = b;
    cut.resize(b.size() - 3);
    n = cut.size() - 8;
    std::memcpy(cut.data(), &n, 8);
    CHECK(gt::decode(cut.data(), cut.size(), out) == gt::Status::bad_record);
    std::vector<uint8_t> padded = b;
    padded.insert(padded.end(), 50, 0xab);
    CHECK(gt::decode(padded.data(), padded.size(), out) == gt::Status::ok);
    CHECK(out.records.size() == t.records.size());

    gt::Trace empty;
    gt::default_summary(empty.summary);
    CHECK(gt::fits(empty) == gt::Fit::empty && !gt::encode(empty, b));
    gt::Trace back = sample_trace(rng, 5, 10.0f);
    std::swap(back.records[1].ms, back.records[3].ms);
    back.records[1].ms += 5000;
    CHECK(gt::fits(back) == gt::Fit::time_order);
    gt::Trace wide = sample_trace(rng, 5, 10.0f);
    wide.records[4].ms = gt::MAX_MS + 1;
    CHECK(gt::fits(wide) == gt::Fit::time_range);
    gt::Trace mask = sample_trace(rng, 5, 10.0f);
    mask.records[2].mask |= 0x800;
    CHECK(gt::fits(mask) == gt::Fit::bad_mask);

    gt::Trace huge;
    gt::default_summary(huge.summary);
    for (size_t i = 0; i < 1400; i++) {
        gt::Record r;
        r.ms = uint32_t(i);
        r.mask = gt::CH_ALL;
        huge.records.push_back(r);
    }
    CHECK(gt::fits(huge) == gt::Fit::too_big);
    gt::Trace many;
    gt::default_summary(many.summary);
    for (size_t i = 0; i < gt::MAX_RECORDS; i++) {
        gt::Record r;
        r.ms = uint32_t(i);
        r.mask = 0;
        many.records.push_back(r);
    }
    CHECK(gt::fits(many) == gt::Fit::too_many_records);

    gt::Trace ok = sample_trace(rng, 150, 60.0f);
    CHECK(gt::accepted_when_bound(ok));
    gt::Trace nolap = ok;
    float zero = 0;
    std::memcpy(nolap.summary + gt::SUMMARY_LAP, &zero, 4);
    CHECK(!gt::accepted_when_bound(nolap));
    gt::Trace nosec = ok;
    std::memcpy(nosec.summary + gt::SUMMARY_SECTOR + 4, &zero, 4);
    CHECK(!gt::accepted_when_bound(nosec));
    gt::Trace fewer = sample_trace(rng, 100, 60.0f);
    CHECK(!gt::accepted_when_bound(fewer));
}

static void rows_of(const int16_t q[4], float rows[3][3])
{
    float x = q[0] / 32767.0f, y = q[1] / 32767.0f, z = q[2] / 32767.0f, w = q[3] / 32767.0f;
    float r[3][3] = {{1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w)},
                     {2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)},
                     {2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y)}};
    std::memcpy(rows, r, sizeof r);
}

static std::vector<gt::Sample> lap_samples(float seconds, float hz, bool quats)
{
    std::vector<gt::Sample> s;
    size_t n = size_t(seconds * hz);
    for (size_t i = 0; i <= n; i++) {
        gt::Sample x;
        x.t = float(i) / hz;
        float a = x.t / seconds * 6.2831855f;
        x.pos[0] = 300 * std::sin(a);
        x.pos[1] = 2 + 0.1f * std::sin(5 * a);
        x.pos[2] = 300 * std::cos(a) - 300;
        float speed = 2 * 3.14159f * 300 / seconds;
        x.vel[0] = speed * std::cos(a);
        x.vel[2] = -speed * std::sin(a);
        x.dist = speed * x.t;
        x.throttle = 0.5f + 0.5f * std::sin(3 * a);
        x.brake = x.throttle < 0.2f ? 1.0f : 0.0f;
        x.steer = 0.3f * std::sin(2 * a);
        x.gear = 1 + int(4 * x.throttle);
        x.rpm = 6000 + 4000 * x.throttle;
        x.drs = x.t > seconds * 0.4f && x.t < seconds * 0.6f ? 1.0f : 0.0f;
        if (quats) {
            float yaw = std::atan2(x.vel[0], x.vel[2]);
            x.quat[0] = 0;
            x.quat[1] = std::sin(yaw / 2);
            x.quat[2] = 0;
            x.quat[3] = std::cos(yaw / 2);
            x.has_quat = true;
        }
        s.push_back(x);
    }
    return s;
}

static void converter()
{
    for (float seconds : {20.0f, 60.0f, 89.348f, 120.0f, 200.0f, 262.0f}) {
        for (bool quats : {true, false}) {
            auto s = lap_samples(seconds, 60, quats);
            gt::Plan plan;
            plan.lap_s = seconds;
            plan.sectors_s[0] = seconds * 0.3f;
            plan.sectors_s[1] = seconds * 0.3f;
            plan.sectors_s[2] = seconds * 0.4f;
            std::string why;
            auto t = gt::from_samples(s, plan, &why);
            CHECK(t.has_value());
            if (!t)
                continue;
            CHECK(gt::fits(*t) == gt::Fit::ok);
            CHECK(t->records.front().ms == 0 && (t->records.front().mask & gt::CH_ALL) == (gt::CH_ALL));
            CHECK(t->records.back().ms == uint32_t(std::lround(seconds * 1000.0f)));
            CHECK(gt::lap_seconds(*t) == seconds);
            if (seconds >= 20)
                CHECK(gt::accepted_when_bound(*t));
            std::vector<uint8_t> bytes;
            CHECK(gt::encode(*t, bytes));
            CHECK(bytes.size() <= gt::HEAD + gt::MAX_DATA);
            gt::Trace back;
            CHECK(gt::decode(bytes.data(), bytes.size(), back) == gt::Status::ok);
            CHECK(back.records.size() == t->records.size());
            for (auto &r : t->records) {
                float rows[3][3];
                rows_of(r.quat, rows);
                float len = rows[2][0] * rows[2][0] + rows[2][1] * rows[2][1] + rows[2][2] * rows[2][2];
                CHECK(std::fabs(len - 1.0f) < 0.01f);
                CHECK(r.quat[3] >= 0);
            }
            float rate = float(t->records.size()) / seconds;
            std::printf("  %.1f s %s: %zu records, %.1f Hz, %zu bytes\n", seconds, quats ? "oriented" : "heading", t->records.size(), rate,
                        bytes.size());
        }
    }
    auto s = lap_samples(90, 60, true);
    gt::Plan plan;
    plan.lap_s = 90;
    plan.sectors_s[0] = plan.sectors_s[1] = plan.sectors_s[2] = 30;
    auto t = gt::from_samples(s, plan);
    CHECK(t.has_value());
    float worst = 0;
    for (auto &r : t->records) {
        float at = float(r.ms) / 1000.0f, a = at / 90.0f * 6.2831855f;
        float ex = 300 * std::sin(a), ez = 300 * std::cos(a) - 300;
        worst = std::fmax(worst, std::hypot(r.pos[0] - ex, r.pos[2] - ez));
    }
    CHECK(worst < 0.5f);
    float rows[3][3];
    for (auto &r : t->records) {
        if (r.ms < 20000 || r.ms > 20100)
            continue;
        rows_of(r.quat, rows);
        float a = float(r.ms) / 1000.0f / 90.0f * 6.2831855f;
        CHECK(std::fabs(rows[2][0] - std::cos(a)) < 0.01f && std::fabs(rows[2][2] + std::sin(a)) < 0.01f);
        CHECK(std::fabs(rows[1][1] - 1.0f) < 0.01f);
    }
    gt::Plan tiny;
    tiny.lap_s = 0.5f;
    CHECK(!gt::from_samples(s, tiny).has_value());
    CHECK(!gt::from_samples({}, plan).has_value());
    gt::Plan toolong;
    toolong.lap_s = 300;
    CHECK(!gt::from_samples(s, toolong).has_value());
    auto back = s;
    std::swap(back[10], back[20]);
    CHECK(!gt::from_samples(back, plan).has_value());
}

static void real_lap()
{
    const char *path = std::getenv("SWOFTY_TEST_SWLAP");
    if (!path || !*path) {
        std::printf("  no SWOFTY_TEST_SWLAP: the real lap conversion is skipped\n");
        return;
    }
    std::ifstream f(path, std::ios::binary);
    std::vector<uint8_t> d((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
    lf::Parsed p;
    CHECK(lf::parse(d.data(), d.size(), p) == lf::Status::ok);
    std::string why;
    auto t = gt::trace_of_lap(p, &why);
    CHECK(t.has_value());
    if (!t) {
        std::printf("  real lap: %s\n", why.c_str());
        return;
    }
    std::vector<uint8_t> bytes;
    CHECK(gt::encode(*t, bytes));
    CHECK(gt::accepted_when_bound(*t));
    gt::Trace back;
    CHECK(gt::decode(bytes.data(), bytes.size(), back) == gt::Status::ok);
    std::printf("  real lap: %u samples to %zu records, %zu bytes, lap %.3f s\n", p.count, t->records.size(), bytes.size(),
                gt::lap_seconds(*t));
}

static void real_trace()
{
    std::ifstream f("fixtures/real-suzuka-2016.trace", std::ios::binary);
    std::vector<uint8_t> d((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
    CHECK(d.size() == 19261);
    gt::Trace t;
    size_t used = 0;
    CHECK(gt::decode(d.data(), d.size(), t, &used) == gt::Status::ok);
    CHECK(used == d.size() && t.records.size() == 2009);
    CHECK(gt::accepted_when_bound(t));
    CHECK(std::fabs(gt::lap_seconds(t) - 88.118f) < 0.001f);
    CHECK(t.records.front().ms == 0 && t.records.back().ms == 88117 && t.records.front().mask == gt::CH_ALL);
    CHECK(std::fabs(t.records.front().pos[0] - 627.40f) < 0.01f);
    std::vector<uint8_t> again;
    CHECK(gt::encode(t, again) && again == d);
    uint32_t seen = 0;
    for (auto &r : t.records) {
        seen |= r.mask;
        if (r.mask & gt::CH_ORIENTATION) {
            float n = 0;
            for (int16_t q : r.quat)
                n += (q / 32767.0f) * (q / 32767.0f);
            CHECK(std::fabs(n - 1.0f) < 0.001f);
        }
    }
    CHECK(seen == gt::CH_ALL);
}

int main()
{
    real_trace();
    layout_tests();
    round_trips();
    reader_rules();
    converter();
    real_lap();
    if (failures) {
        std::printf("%d failures\n", failures);
        return 1;
    }
    std::printf("ghost trace: all checks passed\n");
    return 0;
}
