#pragma once

#include "presence_id_logic.hpp"
#include <swofty/json.hpp>

#include <cstdint>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

namespace runtime::lap_file {

constexpr uint8_t MAGIC[8] = {'S', 'W', 'L', 'A', 'P', 0, 1, 0};
constexpr uint32_t FORMAT = 1;

enum class Type : uint8_t { f32, f64, i8, u8, i16, u16, i32, u32 };

struct Field {
    const char *name;
    Type type;
    const char *unit;
};

inline size_t type_size(Type t)
{
    switch (t) {
    case Type::i8:
    case Type::u8:
        return 1;
    case Type::i16:
    case Type::u16:
        return 2;
    case Type::f64:
        return 8;
    default:
        return 4;
    }
}

inline const char *type_name(Type t)
{
    static const char *names[] = {"f32", "f64", "i8", "u8", "i16", "u16", "i32", "u32"};
    return names[size_t(t)];
}

inline std::optional<Type> type_of(const std::string &s)
{
    for (int i = 0; i < 8; i++)
        if (s == type_name(Type(i)))
            return Type(i);
    return std::nullopt;
}

template <class T> inline void store(uint8_t *at, T v)
{
    std::memcpy(at, &v, sizeof v);
}

template <class T> inline T load(const uint8_t *at)
{
    T v;
    std::memcpy(&v, at, sizeof v);
    return v;
}

inline void put(uint8_t *at, Type t, double v)
{
    switch (t) {
    case Type::f32: store<float>(at, float(v)); break;
    case Type::f64: store<double>(at, v); break;
    case Type::i8: store<int8_t>(at, int8_t(v)); break;
    case Type::u8: store<uint8_t>(at, uint8_t(v)); break;
    case Type::i16: store<int16_t>(at, int16_t(v)); break;
    case Type::u16: store<uint16_t>(at, uint16_t(v)); break;
    case Type::i32: store<int32_t>(at, int32_t(v)); break;
    case Type::u32: store<uint32_t>(at, uint32_t(v)); break;
    }
}

inline double get(const uint8_t *at, Type t)
{
    switch (t) {
    case Type::f32: return load<float>(at);
    case Type::f64: return load<double>(at);
    case Type::i8: return load<int8_t>(at);
    case Type::u8: return load<uint8_t>(at);
    case Type::i16: return load<int16_t>(at);
    case Type::u16: return load<uint16_t>(at);
    case Type::i32: return load<int32_t>(at);
    case Type::u32: return load<uint32_t>(at);
    }
    return 0;
}

struct Column {
    std::string name, unit;
    Type type;
    size_t offset;
};

struct Layout {
    std::vector<Column> columns;
    size_t stride = 0;

    void add(const std::string &name, Type t, const std::string &unit = {})
    {
        columns.push_back({name, unit, t, stride});
        stride += type_size(t);
    }

    int index(const std::string &name) const
    {
        for (size_t i = 0; i < columns.size(); i++)
            if (columns[i].name == name)
                return int(i);
        return -1;
    }

    void set(uint8_t *sample, size_t column, double v) const { put(sample + columns[column].offset, columns[column].type, v); }
    double read(const uint8_t *sample, size_t column) const
    {
        return get(sample + columns[column].offset, columns[column].type);
    }

    swofty::json::Value json() const
    {
        swofty::json::Value out = swofty::json::Value::array();
        for (auto &c : columns) {
            swofty::json::Value f = swofty::json::Value::object({{"name", c.name}, {"type", type_name(c.type)}});
            if (!c.unit.empty())
                f["unit"] = c.unit;
            out.push_back(std::move(f));
        }
        return out;
    }
};

struct Header {
    std::string core, game_build, nonce;
    std::string content, track, weather, game_board, ghost_id;
    std::string team, vehicle, driver;
    std::string content_name, track_name, track_addon, track_kind, performance;
    std::optional<int64_t> team_stat, vehicle_stat, livery_stat;
    int64_t time_ms = 0;
    std::vector<int64_t> sectors_ms;
    std::optional<bool> valid;
    std::string source;
    std::optional<int64_t> lap_index, driver_assists;
    std::string started_utc;
    std::optional<double> session_time_s;
    std::optional<int> rate_hz;
    swofty::json::Value assists, setup;
    std::optional<runtime::presence_id::Id> presence;
};

inline swofty::json::Value presence_json(const runtime::presence_id::Id &id)
{
    using swofty::json::Value;
    auto number = [](uint64_t v) { return v > uint64_t(INT64_MAX) ? Value(std::to_string(v)) : Value(int64_t(v)); };
    Value values = Value::array();
    values.push_back(number(id.value[0]));
    for (int i = 1; i < 3; i++)
        values.push_back(id.value[i] ? number(id.value[i]) : Value(nullptr));
    Value flags = Value::array();
    flags.push_back(Value(int64_t(id.flag39)));
    flags.push_back(Value(int64_t(id.flag38)));
    std::vector<uint8_t> stream = runtime::presence_id::serialise(id);
    Value v = Value::object({{"type", int64_t(id.type)}});
    int platform = runtime::presence_id::platform_of(id.type);
    v["platform"] = platform ? Value(int64_t(platform)) : Value(nullptr);
    v["values"] = std::move(values);
    v["flags"] = std::move(flags);
    v["stream"] = runtime::presence_id::base64(stream);
    return v;
}

inline swofty::json::Value header_json(const Header &h, const Layout &layout)
{
    using swofty::json::Value;
    Value v = Value::object({{"format", int64_t(FORMAT)}});
    auto text = [](Value &o, const char *key, const std::string &s) {
        if (!s.empty())
            o[key] = s;
    };
    text(v, "core", h.core);
    text(v, "game_build", h.game_build);
    Value board = Value::object();
    text(board, "content", h.content);
    text(board, "track", h.track);
    text(board, "weather", h.weather);
    text(board, "game_board", h.game_board);
    text(board, "content_name", h.content_name);
    text(board, "track_name", h.track_name);
    text(board, "track_addon", h.track_addon);
    text(board, "track_kind", h.track_kind);
    v["board"] = std::move(board);
    Value car = Value::object();
    text(car, "team", h.team);
    text(car, "vehicle", h.vehicle);
    text(car, "driver", h.driver);
    if (h.team_stat)
        car["team_stat"] = *h.team_stat;
    if (h.vehicle_stat)
        car["vehicle_stat"] = *h.vehicle_stat;
    if (h.livery_stat)
        car["livery_stat"] = *h.livery_stat;
    v["car"] = std::move(car);
    Value lap = Value::object({{"time_ms", h.time_ms}});
    if (!h.sectors_ms.empty()) {
        Value s = Value::array();
        for (auto ms : h.sectors_ms)
            s.push_back(Value(ms));
        lap["sectors_ms"] = std::move(s);
    }
    if (h.valid)
        lap["valid"] = *h.valid;
    text(lap, "source", h.source);
    if (h.lap_index)
        lap["lap_index"] = *h.lap_index;
    text(lap, "started_utc", h.started_utc);
    if (h.session_time_s)
        lap["session_time_s"] = *h.session_time_s;
    v["lap"] = std::move(lap);
    if (h.assists.type() == swofty::json::Type::object)
        v["assists"] = h.assists;
    if (h.setup.type() == swofty::json::Type::object)
        v["setup"] = h.setup;
    if (h.driver_assists)
        v["driver_assists"] = *h.driver_assists;
    text(v, "performance", h.performance);
    if (h.rate_hz)
        v["rate_hz"] = int64_t(*h.rate_hz);
    v["fields"] = layout.json();
    text(v, "nonce", h.nonce);
    text(v, "ghost_id", h.ghost_id);
    if (h.presence)
        v["presence"] = presence_json(*h.presence);
    return v;
}

struct Section {
    char tag[4];
    std::vector<uint8_t> bytes;
};

inline Section section(const char (&tag)[5], std::vector<uint8_t> bytes)
{
    Section s{{tag[0], tag[1], tag[2], tag[3]}, std::move(bytes)};
    return s;
}

inline bool reserved(const char tag[4])
{
    return !std::memcmp(tag, "SMPL", 4) || !std::memcmp(tag, "GHST", 4) || !std::memcmp(tag, "END\0", 4);
}

inline void append32(std::vector<uint8_t> &out, uint32_t v)
{
    uint8_t b[4];
    store<uint32_t>(b, v);
    out.insert(out.end(), b, b + 4);
}

inline void append_section(std::vector<uint8_t> &out, const char tag[4], const uint8_t *data, size_t len)
{
    out.insert(out.end(), tag, tag + 4);
    append32(out, uint32_t(len));
    if (len)
        out.insert(out.end(), data, data + len);
}

// samples: count * layout.stride bytes. ghost: the game's own blob, written verbatim when not empty. extra: further
// sections in order, such as the integrity attestation; a tag that is the format's own is dropped.
inline std::vector<uint8_t> build(const Header &h, const Layout &layout, const uint8_t *samples, uint32_t count,
                                  const std::vector<uint8_t> &ghost, const std::vector<Section> &extra)
{
    std::string text = swofty::json::dump(header_json(h, layout));
    std::vector<uint8_t> out(MAGIC, MAGIC + sizeof MAGIC);
    append32(out, uint32_t(text.size()));
    out.insert(out.end(), text.begin(), text.end());
    std::vector<uint8_t> smpl;
    append32(smpl, count);
    append32(smpl, uint32_t(layout.stride));
    if (count && samples)
        smpl.insert(smpl.end(), samples, samples + size_t(count) * layout.stride);
    append_section(out, "SMPL", smpl.data(), smpl.size());
    if (!ghost.empty())
        append_section(out, "GHST", ghost.data(), ghost.size());
    for (auto &s : extra)
        if (!reserved(s.tag))
            append_section(out, s.tag, s.bytes.data(), s.bytes.size());
    append_section(out, "END\0", nullptr, 0);
    return out;
}

struct Parsed {
    swofty::json::Value header;
    Layout layout;
    uint32_t count = 0;
    std::vector<uint8_t> samples, ghost;
    std::vector<Section> others;
    std::vector<std::string> tags;
    bool ended = false;

    const uint8_t *sample(uint32_t i) const { return samples.data() + size_t(i) * layout.stride; }
};

enum class Status { ok, bad_magic, truncated, bad_header, bad_fields, bad_samples };

inline Status parse(const uint8_t *data, size_t len, Parsed &out)
{
    out = Parsed{};
    if (len < 12 || std::memcmp(data, MAGIC, sizeof MAGIC))
        return Status::bad_magic;
    uint32_t hlen = load<uint32_t>(data + 8);
    if (size_t(hlen) > len - 12)
        return Status::truncated;
    swofty::json::Error err;
    out.header = swofty::json::parse(std::string_view(reinterpret_cast<const char *>(data + 12), hlen), &err);
    if (!err.ok() || out.header.type() != swofty::json::Type::object)
        return Status::bad_header;
    const swofty::json::Array *fields = out.header["fields"].if_array();
    if (!fields)
        return Status::bad_fields;
    for (auto &f : *fields) {
        auto t = type_of(std::string(f["type"].as_string()));
        std::string name(f["name"].as_string());
        if (!t || name.empty())
            return Status::bad_fields;
        out.layout.add(name, *t, std::string(f["unit"].as_string()));
    }
    size_t at = 12 + hlen;
    bool have_samples = false;
    while (at < len) {
        if (len - at < 8)
            return Status::truncated;
        const char *tag = reinterpret_cast<const char *>(data + at);
        uint32_t n = load<uint32_t>(data + at + 4);
        at += 8;
        if (size_t(n) > len - at)
            return Status::truncated;
        const uint8_t *body = data + at;
        at += n;
        out.tags.emplace_back(tag, tag[3] ? 4 : 3);
        if (!std::memcmp(tag, "END\0", 4)) {
            out.ended = true;
            break;
        }
        if (!std::memcmp(tag, "SMPL", 4)) {
            if (n < 8)
                return Status::bad_samples;
            uint32_t count = load<uint32_t>(body), stride = load<uint32_t>(body + 4);
            if (stride != out.layout.stride || size_t(count) * stride != size_t(n) - 8)
                return Status::bad_samples;
            out.count = count;
            out.samples.assign(body + 8, body + n);
            have_samples = true;
        } else if (!std::memcmp(tag, "GHST", 4))
            out.ghost.assign(body, body + n);
        else
            out.others.push_back({{tag[0], tag[1], tag[2], tag[3]}, std::vector<uint8_t>(body, body + n)});
    }
    if (!have_samples)
        return Status::bad_samples;
    return out.ended ? Status::ok : Status::truncated;
}

}
