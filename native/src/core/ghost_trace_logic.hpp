#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

// The game's ghost trace (re/db: game.trace_read_from_stream 0x1437ad610, the trace writer 0x1437ad550). One trace is
//
//   u64 N                  bytes that follow: 0x80 + the record bytes
//   0x6c bytes             summary, copied to the telemetry lap at +0x260 (lap time at +0x8, sectors at +0xc/+0x10/+0x14)
//   20 zero bytes
//   records, back to back
//
// A record is a u32 header, then the channels its mask names, in ascending mask order. Header: bits 0..17 the time in
// milliseconds since the lap start, bits 18..28 the channel mask. The game records a channel only when it changed; a
// reader looks a channel up in the nearest records that hold it. Channel layouts (FUN_1437ab7e0 builds the 11 channel
// objects; the byte layouts are the record/apply slots of their vtables):
//
//   0x001 Position     3 x f32 world position (chassis +0x200)
//   0x002 Orientation  4 x i16, (x, y, z, w) of the unit quaternion times 32767; the rotation matrix rows at chassis
//                      +0x190/+0x1a0/+0x1b0 are the body axes (right, up, forward)
//   0x004 Steering     i8, steering input times 127, the opposite sign to the recorder's steer column
//   0x008 Gear         u8, the gearbox index: the recorder's gear column plus one
//   0x010 Throttle     u8, 0..1 times 255
//   0x020 Brake        u8, 0..1 times 255
//   0x040 Rpm          u8, engine rad/s divided by 2100 times 255
//   0x080 RearWing     u8, a DRS state: 4 with the DRS open and 2 closed in the one real lap seen (1 once)
//   0x100 Speed        u8, forward speed in m/s
//   0x200 Teleport     u32, 0x40 after a teleport (no interpolation), 0x3f otherwise
//   0x400 Progress     u16, lap progress 0..1 times 65535
namespace runtime::ghost_trace {

constexpr uint32_t CH_POSITION = 0x001, CH_ORIENTATION = 0x002, CH_STEERING = 0x004, CH_GEAR = 0x008,
                   CH_THROTTLE = 0x010, CH_BRAKE = 0x020, CH_RPM = 0x040, CH_REAR_WING = 0x080, CH_SPEED = 0x100,
                   CH_TELEPORT = 0x200, CH_PROGRESS = 0x400, CH_ALL = 0x7ff;
constexpr int CHANNELS = 11;
constexpr uint8_t CHANNEL_SIZE[CHANNELS] = {12, 8, 1, 1, 1, 1, 1, 1, 1, 4, 2};

constexpr size_t FIXED = 0x80, SUMMARY = 0x6c, PAD = 20, HEAD = 8 + FIXED;
constexpr size_t MAX_DATA = 0xc780, MAX_RECORDS = 0x2000, MIN_RECORDS = 0x65, MIN_BYTES = 300;
constexpr uint32_t MAX_MS = 0x3ffff;
constexpr float RPM_SCALE = 2100.0f;
constexpr uint8_t WING_CLOSED = 2, WING_OPEN = 4;
constexpr uint32_t TELEPORT_NONE = 0x3f, TELEPORT_JUMP = 0x40;
constexpr size_t SUMMARY_MASK = 4, SUMMARY_LAP = 8, SUMMARY_SECTOR = 12, SUMMARY_FLAGS = 0x60;

struct Record {
    uint32_t ms = 0, mask = 0;
    float pos[3] = {};
    int16_t quat[4] = {0, 0, 0, 32767};
    int8_t steer = 0;
    uint8_t gear = 1, throttle = 0, brake = 0, rpm = 0, wing = 0, speed = 0;
    uint32_t teleport = TELEPORT_NONE;
    uint16_t progress = 0;
};

struct Trace {
    uint8_t summary[SUMMARY] = {};
    std::vector<Record> records;
};

inline size_t channel_bytes(uint32_t mask)
{
    size_t n = 0;
    for (int i = 0; i < CHANNELS; i++)
        if (mask & (1u << i))
            n += CHANNEL_SIZE[i];
    return n;
}

inline size_t record_bytes(uint32_t mask) { return 4 + channel_bytes(mask); }

template <class T> inline void put(std::vector<uint8_t> &out, T v)
{
    uint8_t b[sizeof v];
    std::memcpy(b, &v, sizeof v);
    out.insert(out.end(), b, b + sizeof v);
}

template <class T> inline T get(const uint8_t *at)
{
    T v;
    std::memcpy(&v, at, sizeof v);
    return v;
}

inline void default_summary(uint8_t s[SUMMARY])
{
    std::memset(s, 0, SUMMARY);
    uint32_t head = 0x16000003, mask = CH_ALL, none = 0xffffffff;
    std::memcpy(s, &head, 4);
    std::memcpy(s + SUMMARY_MASK, &mask, 4);
    std::memcpy(s + 0x18, &none, 4);
    s[0x1c] = s[0x1d] = 0xff;
}

inline void set_times(Trace &t, float lap_s, const float sectors_s[3], bool clean)
{
    std::memcpy(t.summary + SUMMARY_LAP, &lap_s, 4);
    for (int i = 0; i < 3; i++)
        std::memcpy(t.summary + SUMMARY_SECTOR + 4 * i, &sectors_s[i], 4);
    uint32_t flags = get<uint32_t>(t.summary + SUMMARY_FLAGS);
    flags = clean ? (flags & ~1u) : (flags | 1u);
    std::memcpy(t.summary + SUMMARY_FLAGS, &flags, 4);
}

inline float lap_seconds(const Trace &t) { return get<float>(t.summary + SUMMARY_LAP); }

inline std::vector<uint8_t> encode_record(const Record &r)
{
    std::vector<uint8_t> o;
    put<uint32_t>(o, (r.ms & MAX_MS) | (r.mask & CH_ALL) << 18);
    if (r.mask & CH_POSITION)
        for (float f : r.pos)
            put(o, f);
    if (r.mask & CH_ORIENTATION)
        for (int16_t q : r.quat)
            put(o, q);
    if (r.mask & CH_STEERING)
        put(o, r.steer);
    if (r.mask & CH_GEAR)
        put(o, r.gear);
    if (r.mask & CH_THROTTLE)
        put(o, r.throttle);
    if (r.mask & CH_BRAKE)
        put(o, r.brake);
    if (r.mask & CH_RPM)
        put(o, r.rpm);
    if (r.mask & CH_REAR_WING)
        put(o, r.wing);
    if (r.mask & CH_SPEED)
        put(o, r.speed);
    if (r.mask & CH_TELEPORT)
        put(o, r.teleport);
    if (r.mask & CH_PROGRESS)
        put(o, r.progress);
    return o;
}

enum class Fit { ok, empty, too_many_records, too_big, bad_mask, time_order, time_range };

inline const char *fit_name(Fit f)
{
    switch (f) {
    case Fit::ok: return "ok";
    case Fit::empty: return "no records";
    case Fit::too_many_records: return "more records than the game's index holds";
    case Fit::too_big: return "more record bytes than the game's trace buffer holds";
    case Fit::bad_mask: return "a record names a channel the game does not have";
    case Fit::time_order: return "record times go backwards";
    case Fit::time_range: return "a record time does not fit 18 bits";
    }
    return "?";
}

inline Fit fits(const Trace &t, size_t *data_bytes = nullptr)
{
    if (t.records.empty())
        return Fit::empty;
    if (t.records.size() >= MAX_RECORDS)
        return Fit::too_many_records;
    size_t bytes = 0;
    uint32_t last = 0;
    for (auto &r : t.records) {
        if (r.mask & ~CH_ALL)
            return Fit::bad_mask;
        if (r.ms > MAX_MS)
            return Fit::time_range;
        if (r.ms < last)
            return Fit::time_order;
        last = r.ms;
        bytes += record_bytes(r.mask);
        if (bytes > MAX_DATA)
            return Fit::too_big;
    }
    if (data_bytes)
        *data_bytes = bytes;
    return Fit::ok;
}

// What the game checks after reading a trace into a telemetry lap that is bound to a car (+0x130 non-null): the lap and
// sector words nonzero, at least 0x65 records, more record bytes than 100 times the average channel size (3).
inline bool accepted_when_bound(const Trace &t)
{
    size_t bytes = 0;
    if (fits(t, &bytes) != Fit::ok)
        return false;
    float lap = lap_seconds(t);
    if (!(lap > 0) || !std::isfinite(lap))
        return false;
    for (int i = 0; i < 3; i++) {
        float s = get<float>(t.summary + SUMMARY_SECTOR + 4 * i);
        if (!(s > 0) || !std::isfinite(s))
            return false;
    }
    return t.records.size() >= MIN_RECORDS && bytes > MIN_BYTES;
}

inline bool encode(const Trace &t, std::vector<uint8_t> &out, std::string *why = nullptr)
{
    size_t data = 0;
    Fit f = fits(t, &data);
    if (f != Fit::ok) {
        if (why)
            *why = fit_name(f);
        return false;
    }
    out.clear();
    out.reserve(HEAD + data);
    put<uint64_t>(out, FIXED + data);
    out.insert(out.end(), t.summary, t.summary + SUMMARY);
    out.insert(out.end(), PAD, 0);
    for (auto &r : t.records) {
        auto b = encode_record(r);
        out.insert(out.end(), b.begin(), b.end());
    }
    return true;
}

enum class Status { ok, short_input, bad_size, bad_record, too_many_records, too_big, trailing };

inline const char *status_name(Status s)
{
    switch (s) {
    case Status::ok: return "ok";
    case Status::short_input: return "shorter than the fixed part";
    case Status::bad_size: return "the size word is zero or does not fit the input";
    case Status::bad_record: return "a record runs past the end";
    case Status::too_many_records: return "too many records";
    case Status::too_big: return "too many record bytes";
    case Status::trailing: return "bytes after the last record";
    }
    return "?";
}

// The reader (0x1437ad610) with the checks it makes: size word nonzero and below the stream size, at least 0x6c bytes,
// every record whole, the index capacity and the buffer capacity. `used` is set to the bytes the trace takes.
inline Status decode(const uint8_t *in, size_t len, Trace &out, size_t *used = nullptr)
{
    out = Trace{};
    if (len < 8)
        return Status::short_input;
    uint64_t n = get<uint64_t>(in);
    if (n == 0 || n >= len || n < FIXED || n > len - 8)
        return Status::bad_size;
    std::memcpy(out.summary, in + 8, SUMMARY);
    size_t at = HEAD, end = 8 + size_t(n), bytes = 0;
    while (at < end) {
        if (end - at < 4)
            return Status::bad_record;
        uint32_t h = get<uint32_t>(in + at);
        Record r;
        r.ms = h & MAX_MS;
        r.mask = (h >> 18) & CH_ALL;
        size_t size = record_bytes(r.mask);
        if (end - at < size)
            return Status::bad_record;
        const uint8_t *p = in + at + 4;
        if (r.mask & CH_POSITION) {
            for (float &f : r.pos) {
                f = get<float>(p);
                p += 4;
            }
        }
        if (r.mask & CH_ORIENTATION) {
            for (int16_t &q : r.quat) {
                q = get<int16_t>(p);
                p += 2;
            }
        }
        if (r.mask & CH_STEERING)
            r.steer = int8_t(*p++);
        if (r.mask & CH_GEAR)
            r.gear = *p++;
        if (r.mask & CH_THROTTLE)
            r.throttle = *p++;
        if (r.mask & CH_BRAKE)
            r.brake = *p++;
        if (r.mask & CH_RPM)
            r.rpm = *p++;
        if (r.mask & CH_REAR_WING)
            r.wing = *p++;
        if (r.mask & CH_SPEED)
            r.speed = *p++;
        if (r.mask & CH_TELEPORT) {
            r.teleport = get<uint32_t>(p);
            p += 4;
        }
        if (r.mask & CH_PROGRESS)
            r.progress = get<uint16_t>(p);
        bytes += size;
        if (bytes > MAX_DATA)
            return Status::too_big;
        if (out.records.size() + 1 >= MAX_RECORDS)
            return Status::too_many_records;
        out.records.push_back(r);
        at += size;
    }
    if (used)
        *used = end;
    return Status::ok;
}

struct Sample {
    float t = 0, dist = 0;
    float pos[3] = {}, vel[3] = {};
    float throttle = 0, brake = 0, steer = 0, rpm = 0, drs = 0;
    int gear = 0;
    float quat[4] = {0, 0, 0, 1};
    bool has_quat = false;
};

struct Plan {
    float lap_s = 0, sectors_s[3] = {};
    bool clean = true;
    float max_hz = 60;
};

struct Quat {
    float x, y, z, w;
};

inline Quat normalised(Quat q)
{
    float n = std::sqrt(q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w);
    if (!(n > 1e-6f) || !std::isfinite(n))
        return {0, 0, 0, 1};
    return {q.x / n, q.y / n, q.z / n, q.w / n};
}

inline Quat nlerp(Quat a, Quat b, float u)
{
    float d = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    float s = d < 0 ? -1.0f : 1.0f;
    return normalised({a.x + u * (s * b.x - a.x), a.y + u * (s * b.y - a.y), a.z + u * (s * b.z - a.z),
                       a.w + u * (s * b.w - a.w)});
}

// The orientation of a body that points along `forward` and stays upright (y up): axes right, up, forward as rows, the
// quaternion of that matrix as the game's record slot computes it.
inline Quat upright_facing(const float forward[3], Quat fallback)
{
    float fx = forward[0], fz = forward[2], n = std::sqrt(fx * fx + fz * fz);
    if (!(n > 0.5f))
        return fallback;
    fx /= n;
    fz /= n;
    float yaw = std::atan2(fx, fz);
    return {0, std::sin(yaw / 2), 0, std::cos(yaw / 2)};
}

inline int16_t quantise(float v)
{
    v = std::fmax(-1.0f, std::fmin(1.0f, v));
    return int16_t(std::lround(v * 32767.0f));
}

inline uint8_t byte_of(float unit)
{
    return uint8_t(std::lround(std::fmax(0.0f, std::fmin(1.0f, unit)) * 255.0f));
}

inline Record sample_at(const std::vector<Sample> &s, size_t &cursor, float t, Quat &last_upright, float lap_dist)
{
    while (cursor + 2 < s.size() && s[cursor + 1].t <= t)
        cursor++;
    const Sample &a = s[cursor], &b = s[std::min(cursor + 1, s.size() - 1)];
    float span = b.t - a.t, u = span > 1e-6f ? std::fmax(0.0f, std::fmin(1.0f, (t - a.t) / span)) : 0.0f;
    auto mix = [&](float x, float y) { return x + (y - x) * u; };
    Record r;
    r.ms = uint32_t(std::lround(t * 1000.0f));
    r.mask = CH_POSITION | CH_ORIENTATION | CH_STEERING | CH_GEAR | CH_THROTTLE | CH_BRAKE | CH_RPM | CH_REAR_WING |
             CH_SPEED | CH_PROGRESS;
    for (int i = 0; i < 3; i++)
        r.pos[i] = mix(a.pos[i], b.pos[i]);
    Quat q;
    if (a.has_quat && b.has_quat)
        q = nlerp({a.quat[0], a.quat[1], a.quat[2], a.quat[3]}, {b.quat[0], b.quat[1], b.quat[2], b.quat[3]}, u);
    else {
        float v[3] = {mix(a.vel[0], b.vel[0]), mix(a.vel[1], b.vel[1]), mix(a.vel[2], b.vel[2])};
        q = upright_facing(v, last_upright);
    }
    last_upright = q;
    if (q.w < 0)
        q = {-q.x, -q.y, -q.z, -q.w};
    r.quat[0] = quantise(q.x);
    r.quat[1] = quantise(q.y);
    r.quat[2] = quantise(q.z);
    r.quat[3] = quantise(q.w);
    r.steer = int8_t(std::lround(std::fmax(-1.0f, std::fmin(1.0f, -mix(a.steer, b.steer))) * 127.0f));
    int g = (u < 0.5f ? a.gear : b.gear) + 1;
    r.gear = uint8_t(std::max(0, std::min(15, g)));
    r.throttle = byte_of(mix(a.throttle, b.throttle));
    r.brake = byte_of(mix(a.brake, b.brake));
    float rad = mix(a.rpm, b.rpm) * 6.2831855f / 60.0f;
    r.rpm = byte_of(rad / RPM_SCALE);
    r.wing = (u < 0.5f ? a.drs : b.drs) > 0.5f ? WING_OPEN : WING_CLOSED;
    float sp = std::sqrt(mix(a.vel[0], b.vel[0]) * mix(a.vel[0], b.vel[0]) + mix(a.vel[1], b.vel[1]) * mix(a.vel[1], b.vel[1]) +
                         mix(a.vel[2], b.vel[2]) * mix(a.vel[2], b.vel[2]));
    r.speed = uint8_t(std::fmin(255.0f, sp));
    float d = mix(a.dist, b.dist);
    r.progress = uint16_t(std::lround(std::fmax(0.0f, std::fmin(1.0f, lap_dist > 1 ? d / lap_dist : 0.0f)) * 65535.0f));
    return r;
}

// Drops a small channel from a record when it did not change since the last one the trace holds, as the game does. The
// first record keeps every channel; position and orientation stay in every record so an interpolating reader always
// finds a neighbour on both sides.
inline void thin(std::vector<Record> &records)
{
    if (records.empty())
        return;
    records[0].mask |= CH_TELEPORT;
    records[0].teleport = TELEPORT_NONE;
    Record keep = records[0];
    for (size_t i = 1; i < records.size(); i++) {
        Record &r = records[i];
        uint32_t m = CH_POSITION | CH_ORIENTATION | CH_PROGRESS;
        if (r.steer != keep.steer) m |= CH_STEERING, keep.steer = r.steer;
        if (r.gear != keep.gear) m |= CH_GEAR, keep.gear = r.gear;
        if (r.throttle != keep.throttle) m |= CH_THROTTLE, keep.throttle = r.throttle;
        if (r.brake != keep.brake) m |= CH_BRAKE, keep.brake = r.brake;
        if (r.rpm != keep.rpm) m |= CH_RPM, keep.rpm = r.rpm;
        if (r.wing != keep.wing) m |= CH_REAR_WING, keep.wing = r.wing;
        if (r.speed != keep.speed) m |= CH_SPEED, keep.speed = r.speed;
        r.mask = m;
    }
}

// Resamples recorded samples (60 Hz, world frame) to a trace the game accepts: the highest rate from 60 Hz down whose
// records fit the game's buffer. The coordinate frame is the recorder's own, which is the game's world frame (position
// at the car body state +0x1c0; the game's trace reads the chassis +0x200, to be confirmed in game).
inline std::optional<Trace> from_samples(const std::vector<Sample> &s, const Plan &plan, std::string *why = nullptr)
{
    auto fail = [&](const char *m) -> std::optional<Trace> {
        if (why)
            *why = m;
        return std::nullopt;
    };
    if (s.size() < 2)
        return fail("fewer than two samples");
    for (size_t i = 1; i < s.size(); i++)
        if (!(s[i].t >= s[i - 1].t))
            return fail("sample times go backwards");
    float lap = plan.lap_s > 0 ? plan.lap_s : s.back().t - s.front().t;
    if (!(lap > 1.0f) || lap * 1000.0f > float(MAX_MS))
        return fail("the lap time is outside what a trace holds (262 s)");
    float lap_dist = s.back().dist - s.front().dist;
    std::vector<Sample> rel = s;
    float t0 = s.front().t, d0 = s.front().dist;
    for (auto &x : rel) {
        x.t -= t0;
        x.dist -= d0;
    }
    static const float rates[] = {60, 40, 30, 24, 20, 15, 12, 10, 8, 6, 5, 4, 3, 2};
    for (float hz : rates) {
        if (hz > plan.max_hz)
            continue;
        Trace t;
        default_summary(t.summary);
        set_times(t, lap, plan.sectors_s, plan.clean);
        size_t count = size_t(std::floor(lap * hz)) + 1;
        size_t cursor = 0;
        Quat upright{0, 0, 0, 1};
        for (size_t i = 0; i < count; i++) {
            float at = std::fmin(float(i) / hz, lap);
            t.records.push_back(sample_at(rel, cursor, at, upright, lap_dist));
        }
        if (t.records.back().ms < uint32_t(std::lround(lap * 1000.0f))) {
            size_t c2 = cursor;
            t.records.push_back(sample_at(rel, c2, lap, upright, lap_dist));
        }
        thin(t.records);
        if (fits(t) == Fit::ok)
            return t;
    }
    return fail("no rate fits the trace buffer");
}

}
