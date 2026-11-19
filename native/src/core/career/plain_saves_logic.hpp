#pragma once
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <system_error>
#include <vector>

#include "core/career/save_splice_logic.hpp"
#include "core/runtime/durable_file.hpp"
#include "core/runtime/json.hpp"

// Pure logic of the plain save store (plain_saves.cpp): the launcher's own copy of every root container of the
// profile save, as the game's bag writer emits it (decoded, not compressed, not encrypted). The layout is the
// contract in docs/reference/saves.md: CURRENT, generations/<n>/manifest.json and containers/<fnv64>.bag, then
// inject.json and injected.json. No game calls, so the unit test runs on the host.

namespace career::plain_saves {

namespace fs = std::filesystem;
namespace sp = career::save_splice;

constexpr int SCHEMA = 1;
constexpr uint32_t BAG_TYPE = sp::ENTRY_BAG_TYPE;
constexpr const char *SETTINGS_KEY = "plain_saves";
constexpr const char *CURRENT = "CURRENT";
constexpr const char *GENERATIONS = "generations";
constexpr const char *MANIFEST = "manifest.json";
constexpr const char *CONTAINERS = "containers";
constexpr const char *INJECT = "inject.json";
constexpr const char *INJECTED = "injected.json";
constexpr size_t MAX_SCAN = 4096;

inline std::string hex16(uint64_t v)
{
    char b[24];
    snprintf(b, sizeof b, "%016llx", (unsigned long long)v);
    return b;
}

inline std::string generation_name(uint64_t n)
{
    char b[32];
    snprintf(b, sizeof b, "%012llu", (unsigned long long)n);
    return b;
}

// Every game mode is stored as a pair of root containers, gm_<mode> and <mode>_SaveSlotGameModeComponent. The mode's name,
// or empty for a container that is not half of a pair.
inline std::string pair_of(const std::string &name)
{
    constexpr std::string_view slot = "_SaveSlotGameModeComponent";
    if (name.size() > slot.size() && name.compare(name.size() - slot.size(), slot.size(), slot) == 0)
        return name.substr(0, name.size() - slot.size());
    if (name.size() > 3 && name.compare(0, 3, "gm_") == 0)
        return name.substr(3);
    return {};
}

inline std::string container_file(const std::string &name) { return hex16(sp::fnv1a64(name.c_str())) + ".bag"; }

// ---- SHA-256 ----

class Sha256 {
public:
    Sha256() { reset(); }

    void update(const void *data, size_t size)
    {
        auto p = static_cast<const uint8_t *>(data);
        total_ += size;
        while (size) {
            size_t take = std::min(size, sizeof buf_ - fill_);
            memcpy(buf_ + fill_, p, take);
            fill_ += take, p += take, size -= take;
            if (fill_ == sizeof buf_)
                block(buf_), fill_ = 0;
        }
    }

    std::string finish()
    {
        uint64_t bits = total_ * 8;
        uint8_t pad = 0x80;
        update(&pad, 1);
        uint8_t zero = 0;
        while (fill_ != 56)
            update(&zero, 1);
        uint8_t len[8];
        for (int i = 0; i < 8; i++)
            len[i] = uint8_t(bits >> (56 - 8 * i));
        update(len, 8);
        char out[65];
        for (int i = 0; i < 8; i++)
            snprintf(out + i * 8, 9, "%08x", h_[i]);
        reset();
        return out;
    }

    static std::string of(const void *data, size_t size)
    {
        Sha256 s;
        s.update(data, size);
        return s.finish();
    }

private:
    void reset()
    {
        static constexpr uint32_t init[8] = {0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                                             0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19};
        memcpy(h_, init, sizeof h_);
        fill_ = 0, total_ = 0;
    }

    static uint32_t rotr(uint32_t x, int n) { return x >> n | x << (32 - n); }

    void block(const uint8_t *p)
    {
        static constexpr uint32_t k[64] = {
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
        uint32_t w[64];
        for (int i = 0; i < 16; i++)
            w[i] = uint32_t(p[i * 4]) << 24 | uint32_t(p[i * 4 + 1]) << 16 | uint32_t(p[i * 4 + 2]) << 8 | p[i * 4 + 3];
        for (int i = 16; i < 64; i++) {
            uint32_t s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
            uint32_t s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16] + s0 + w[i - 7] + s1;
        }
        uint32_t a = h_[0], b = h_[1], c = h_[2], d = h_[3], e = h_[4], f = h_[5], g = h_[6], h = h_[7];
        for (int i = 0; i < 64; i++) {
            uint32_t t1 = h + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + ((e & f) ^ (~e & g)) + k[i] + w[i];
            uint32_t t2 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c));
            h = g, g = f, f = e, e = d + t1, d = c, c = b, b = a, a = t1 + t2;
        }
        h_[0] += a, h_[1] += b, h_[2] += c, h_[3] += d, h_[4] += e, h_[5] += f, h_[6] += g, h_[7] += h;
    }

    uint32_t h_[8];
    uint8_t buf_[64];
    size_t fill_ = 0;
    uint64_t total_ = 0;
};

// ---- the settings key ----

struct Settings {
    bool present = false, ok = false;
    std::string why, root;
};

// settings.json: {"plain_saves":{"root":"<windows path>","schema":1}}. Without the key the core writes and injects
// nothing; a key it cannot use is reported and also leaves the store alone.
inline Settings parse_settings(std::string_view text)
{
    Settings s;
    json::Value doc;
    if (text.empty() || !json::parse(std::string(text), doc) || doc.type != json::Value::Obj)
        return s;
    const json::Value &b = doc[SETTINGS_KEY];
    if (b.type == json::Value::Null)
        return s;
    s.present = true;
    if (b.type != json::Value::Obj)
        return s.why = "settings.json's plain_saves is not an object", s;
    if (b["schema"].num(0) != SCHEMA)
        return s.why = "settings.json's plain_saves has schema " + std::to_string(b["schema"].num(0)) +
                       ", this core writes schema " + std::to_string(SCHEMA),
               s;
    if (b["root"].type != json::Value::Str || b["root"].s.empty() || b["root"].s.size() > 400)
        return s.why = "settings.json's plain_saves names no root folder", s;
    for (unsigned char c : b["root"].s)
        if (c < 0x20)
            return s.why = "settings.json's plain_saves root has a control character", s;
    const std::string &r = b["root"].s;
    size_t at = 0;
    while (at <= r.size()) {
        size_t end = r.find_first_of("/\\", at);
        if (end == std::string::npos)
            end = r.size();
        if (r.substr(at, end - at) == "..")
            return s.why = "settings.json's plain_saves root has a .. part", s;
        at = end + 1;
    }
    s.root = r;
    s.ok = true;
    return s;
}

// ---- manifests ----

struct Slot {
    uint64_t slot = 0, guid = 0;
    std::string name;
    int64_t season = -1, round = -1;
    int team = 0, driver = 0;
    uint64_t last_played = 0;
    int flags = 0;
};

struct Body {
    std::string name;
    std::shared_ptr<const std::vector<uint8_t>> bytes;
    bool registered = false;
    std::string source;
    std::string as_of;
    std::vector<Slot> slots;
};

struct Header {
    std::string event, core_version, exe_sha256, profile, session;
    int64_t written_at = 0;
    std::vector<std::string> loaded_addons;
};

inline std::string esc(const std::string &s)
{
    std::string o;
    for (unsigned char c : s) {
        if (c == '"' || c == '\\')
            o += '\\', o += char(c);
        else if (c < 0x20) {
            char b[8];
            snprintf(b, sizeof b, "\\u%04x", c);
            o += b;
        } else
            o += char(c);
    }
    return o;
}

inline std::string manifest_json(uint64_t generation, const Header &h, const std::vector<Body> &bodies)
{
    std::string t = "{\n  \"schema\": " + std::to_string(SCHEMA) + ",\n  \"generation\": " + std::to_string(generation) +
                    ",\n  \"event\": \"" + esc(h.event) + "\",\n  \"written_at\": " + std::to_string(h.written_at) +
                    ",\n  \"core_version\": \"" + esc(h.core_version) + "\",\n  \"exe_sha256\": \"" + esc(h.exe_sha256) +
                    "\",\n  \"profile\": \"" + esc(h.profile) +
                    "\",\n  \"session\": " + (h.session.empty() ? std::string("null") : "\"" + esc(h.session) + "\"") +
                    ",\n  \"loaded_addons\": [";
    for (size_t i = 0; i < h.loaded_addons.size(); i++)
        t += (i ? ", " : "") + std::string("\"") + esc(h.loaded_addons[i]) + "\"";
    t += "],\n  \"containers\": [";
    for (size_t i = 0; i < bodies.size(); i++) {
        const Body &b = bodies[i];
        const auto &bytes = *b.bytes;
        t += std::string(i ? "," : "") + "\n    {\"name\": \"" + esc(b.name) + "\", \"fnv64\": \"" +
             hex16(sp::fnv1a64(b.name.c_str())) + "\", \"type_id\": " + std::to_string(BAG_TYPE) +
             ", \"bytes\": " + std::to_string(bytes.size()) + ", \"sha256\": \"" + Sha256::of(bytes.data(), bytes.size()) +
             "\", \"pair\": " + (pair_of(b.name).empty() ? std::string("null") : "\"" + esc(pair_of(b.name)) + "\"") +
             ", \"registered\": " + (b.registered ? "true" : "false") + ", \"source\": \"" + esc(b.source) + "\"" +
             (b.as_of.empty() ? "" : ", \"as_of\": \"" + esc(b.as_of) + "\"") + ", \"slots\": [";
        for (size_t k = 0; k < b.slots.size(); k++) {
            const Slot &s = b.slots[k];
            char guid[32];
            snprintf(guid, sizeof guid, "0x%016llx", (unsigned long long)s.guid);
            t += std::string(k ? ", " : "") + "{\"slot\": " + std::to_string(s.slot) + ", \"guid\": \"" + guid +
                 "\", \"name\": \"" + esc(s.name) +
                 "\", \"season\": " + (s.season < 0 ? std::string("null") : std::to_string(s.season)) +
                 ", \"round\": " + (s.round < 0 ? std::string("null") : std::to_string(s.round)) +
                 ", \"team\": " + std::to_string(s.team) + ", \"driver\": " + std::to_string(s.driver) +
                 ", \"last_played\": " + std::to_string(s.last_played) + ", \"flags\": " + std::to_string(s.flags) + "}";
        }
        t += "]}";
    }
    t += "\n  ]\n}\n";
    return t;
}

// ---- the store on disk ----

inline std::optional<uint64_t> parse_decimal(const std::string &text)
{
    size_t a = 0, b = text.size();
    while (a < b && isspace((unsigned char)text[a]))
        a++;
    while (b > a && isspace((unsigned char)text[b - 1]))
        b--;
    if (a == b || b - a > 19)
        return std::nullopt;
    uint64_t n = 0;
    for (size_t i = a; i < b; i++) {
        if (text[i] < '0' || text[i] > '9')
            return std::nullopt;
        n = n * 10 + uint64_t(text[i] - '0');
    }
    return n;
}

inline std::string read_text(const fs::path &p, size_t limit = size_t(64) << 20)
{
    std::ifstream f(p, std::ios::binary);
    std::string t((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
    return t.size() > limit ? std::string() : t;
}

inline std::vector<uint8_t> read_bytes(const fs::path &p)
{
    std::ifstream f(p, std::ios::binary);
    return std::vector<uint8_t>((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
}

class Root {
public:
    explicit Root(fs::path root) : root_(std::move(root)) {}

    const fs::path &path() const { return root_; }
    fs::path generation_dir(uint64_t n) const { return root_ / GENERATIONS / generation_name(n); }

    // CURRENT as the decimal text it holds; nullopt when missing or not a number.
    std::optional<uint64_t> current() const { return parse_decimal(read_text(root_ / CURRENT, 64)); }

    // Generation numbers on disk, ascending; the folders that are not 12-digit numbers are not generations.
    std::vector<uint64_t> generations() const
    {
        std::vector<uint64_t> out;
        std::error_code ec;
        for (auto it = fs::directory_iterator(root_ / GENERATIONS, ec); !ec && it != fs::directory_iterator(); it.increment(ec)) {
            std::string n = it->path().filename().string();
            auto v = parse_decimal(n);
            if (v && n.size() == 12 && it->is_directory(ec))
                out.push_back(*v);
        }
        std::sort(out.begin(), out.end());
        return out;
    }

    // The number the next generation takes: above CURRENT and above every folder on disk, so a number is never reused,
    // not even when a writer died after its folder and before CURRENT.
    uint64_t next_number() const
    {
        uint64_t top = current().value_or(0);
        auto g = generations();
        if (!g.empty())
            top = std::max(top, g.back());
        return top + 1;
    }

    // Writes a generation: every container file, then the manifest, then CURRENT. Each file is tmp, flush, rename and a
    // directory flush. A failure anywhere leaves CURRENT where it was. Returns the generation number or 0 with why.
    uint64_t write_generation(const Header &h, const std::vector<Body> &bodies, std::string &why) const
    {
        uint64_t n = next_number();
        fs::path dir = generation_dir(n);
        std::error_code ec;
        fs::create_directories(dir / CONTAINERS, ec);
        if (ec)
            return why = "could not create " + dir.string() + ": " + ec.message(), 0;
        for (const Body &b : bodies) {
            if (!b.bytes)
                return why = "container " + b.name + " has no bytes", 0;
            if (!runtime::durable::write_file(dir / CONTAINERS / container_file(b.name), b.bytes->data(), b.bytes->size()))
                return why = "could not write container " + b.name, 0;
        }
        runtime::durable::sync_dir(dir / CONTAINERS);
        if (!runtime::durable::write_file(dir / MANIFEST, manifest_json(n, h, bodies)))
            return why = "could not write the manifest of generation " + std::to_string(n), 0;
        runtime::durable::sync_dir(dir);
        runtime::durable::sync_dir(root_ / GENERATIONS);
        if (!runtime::durable::write_file(root_ / CURRENT, std::to_string(n) + "\n"))
            return why = "could not move CURRENT to generation " + std::to_string(n), 0;
        return n;
    }

private:
    fs::path root_;
};

struct Entry {
    std::string name, fnv64, sha256, source;
    uint64_t bytes = 0;
    size_t careers = 0;
    bool registered = false;
};

struct Manifest {
    uint64_t generation = 0;
    std::string event, session;
    std::vector<Entry> containers;
};

inline std::optional<Manifest> parse_manifest(const std::string &text)
{
    json::Value doc;
    if (!json::parse(text, doc) || doc.type != json::Value::Obj || doc["schema"].num(0) != SCHEMA ||
        doc["containers"].type != json::Value::Arr)
        return std::nullopt;
    Manifest m;
    m.generation = uint64_t(doc["generation"].num(0));
    m.event = doc["event"].str();
    m.session = doc["session"].str();
    for (const json::Value &c : doc["containers"].a) {
        Entry e;
        e.name = c["name"].str();
        e.fnv64 = c["fnv64"].str();
        e.sha256 = c["sha256"].str();
        e.source = c["source"].str();
        e.bytes = uint64_t(c["bytes"].num(0));
        e.registered = c["registered"].type == json::Value::Bool && c["registered"].b;
        e.careers = c["slots"].type == json::Value::Arr ? c["slots"].a.size() : 0;
        if (!sp::name_ok(e.name) || e.fnv64 != hex16(sp::fnv1a64(e.name.c_str())))
            continue;
        m.containers.push_back(std::move(e));
    }
    return m;
}

// A generation a reader may use: at or below CURRENT, manifest readable.
inline std::optional<Manifest> read_generation(const Root &root, uint64_t n)
{
    auto cur = root.current();
    if (!cur || n > *cur)
        return std::nullopt;
    auto m = parse_manifest(read_text(root.generation_dir(n) / MANIFEST));
    if (!m || m->generation != n)
        return std::nullopt;
    return m;
}

// The container's bytes of a generation, checked against the manifest's size and SHA-256.
inline std::optional<std::vector<uint8_t>> read_container(const Root &root, uint64_t n, const Entry &e)
{
    auto bytes = read_bytes(root.generation_dir(n) / CONTAINERS / (e.fnv64 + ".bag"));
    if (bytes.empty() || bytes.size() != e.bytes || bytes.size() > sp::MAX_BODY ||
        Sha256::of(bytes.data(), bytes.size()) != e.sha256)
        return std::nullopt;
    return bytes;
}

// The newest generation that a session with the core wrote at a profile save (rule 2), walking back from CURRENT.
inline std::optional<Manifest> newest_core_save(const Root &root)
{
    auto cur = root.current();
    if (!cur)
        return std::nullopt;
    auto gens = root.generations();
    size_t seen = 0;
    for (auto it = gens.rbegin(); it != gens.rend() && seen < MAX_SCAN; ++it, ++seen) {
        if (*it > *cur)
            continue;
        auto m = read_generation(root, *it);
        if (m && m->event == "save" && !m->session.empty())
            return m;
    }
    return std::nullopt;
}

// ---- the put-back decision ----

struct Pick {
    Entry entry;
    uint64_t generation = 0;
};

struct Skip {
    std::string name, reason;
};

struct PutBack {
    std::vector<Pick> picks;
    std::vector<Skip> skipped;
};

// Rule 2: the containers of that save generation the tree the game just loaded lacks, matched by name. `tree` maps each
// container the tree holds to the SHA-256 of its bytes ("" when they are not known). A container the tree has is never
// replaced, and what the newest save generation does not hold (a deletion, or a container that never was) is not brought
// back from an older one. The halves of a mode's pair come back together or not at all: when one half is absent and the
// other is present, the absent one comes back only if the present one is byte for byte the one in that generation.
inline PutBack put_back(const Manifest &newest, const std::map<std::string, std::string> &tree)
{
    PutBack out;
    std::set<std::string> taken;
    for (const Entry &e : newest.containers) {
        if (tree.count(e.name) || !taken.insert(e.name).second)
            continue;
        std::string pair = pair_of(e.name);
        if (!pair.empty()) {
            const Entry *bad = nullptr;
            for (const Entry &o : newest.containers) {
                if (o.name == e.name || pair_of(o.name) != pair)
                    continue;
                auto it = tree.find(o.name);
                if (it != tree.end() && (it->second.empty() || it->second != o.sha256))
                    bad = &o;
            }
            if (bad) {
                out.skipped.push_back({e.name, "its pair half " + bad->name + " in the loaded save differs from generation " +
                                                   std::to_string(newest.generation) + "'s"});
                continue;
            }
        }
        out.picks.push_back({e, newest.generation});
    }
    return out;
}

inline std::string put_back_line(const std::string &name, size_t careers, uint64_t generation)
{
    return "[saves] put back " + name + ": " + std::to_string(careers) + " career(s) from generation " +
           std::to_string(generation);
}

// ---- inject.json ----

struct Request {
    std::string id, container;
    uint64_t generation = 0;
    bool replace = false;
    // The sha-256 of the container the request was made from, as the launcher's source generation holds it. A request with
    // one replaces only a loaded container that hashes the same.
    std::string base_sha256;
};

inline std::vector<Request> parse_requests(const std::string &text)
{
    std::vector<Request> out;
    json::Value doc;
    if (!json::parse(text, doc) || doc.type != json::Value::Obj || doc["schema"].num(0) != SCHEMA ||
        doc["requests"].type != json::Value::Arr)
        return out;
    for (const json::Value &r : doc["requests"].a) {
        Request q;
        q.id = r["id"].str();
        q.container = r["container"].str();
        q.generation = uint64_t(r["generation"].num(0));
        q.replace = r["replace"].type == json::Value::Bool && r["replace"].b;
        q.base_sha256 = r["base_sha256"].str();
        out.push_back(std::move(q));
    }
    return out;
}

// Whether the loaded container is the one a request was made from. A request without a base takes anything.
inline bool base_matches(const Request &q, const std::string &loaded_sha256)
{
    return q.base_sha256.empty() || (!loaded_sha256.empty() && loaded_sha256 == q.base_sha256);
}

inline std::string changed_since_detail(const Request &q)
{
    return "the loaded " + q.container + " changed since generation " + std::to_string(q.generation) + "; not replaced";
}

struct Result {
    std::string id;
    bool ok = false;
    std::string detail;
};

// What the tree holds under a name at the end of the load.
enum class Held { none, kept, registered };

enum class Action { none, add, replace };

struct Decision {
    Action action = Action::none;
    Result result;
};

// One request against the loaded tree. An absent container is added. A held container is replaced only with replace:true
// whether it is a kept one or a registered mode's (the core reads the bag over before the mode reads it).
inline Decision decide(const Request &q, Held held, bool source_ok)
{
    Decision d;
    d.result.id = q.id;
    if (!sp::name_ok(q.container)) {
        d.result.detail = "the container name is not valid";
        return d;
    }
    if (!source_ok) {
        d.result.detail = "generation " + std::to_string(q.generation) + " has no intact copy of " + q.container;
        return d;
    }
    if (held == Held::none) {
        d.action = Action::add;
        d.result.ok = true;
        d.result.detail = "added " + q.container + " from generation " + std::to_string(q.generation);
    } else if (!q.replace) {
        d.result.detail = "the loaded save already has " + q.container + " and the request is not a replace";
    } else if (held == Held::kept) {
        d.action = Action::replace;
        d.result.ok = true;
        d.result.detail = "replaced the kept " + q.container + " with generation " + std::to_string(q.generation);
    } else {
        d.action = Action::replace;
        d.result.ok = true;
        d.result.detail = "replaced the loaded " + q.container + " with generation " + std::to_string(q.generation);
    }
    return d;
}

inline std::string results_json(const std::vector<Result> &rs)
{
    std::string t = "{\"schema\":" + std::to_string(SCHEMA) + ",\"results\":[";
    for (size_t i = 0; i < rs.size(); i++)
        t += std::string(i ? "," : "") + "{\"id\":\"" + esc(rs[i].id) + "\",\"ok\":" + (rs[i].ok ? "true" : "false") +
             ",\"detail\":\"" + esc(rs[i].detail) + "\"}";
    t += "]}\n";
    return t;
}

// injected.json is written (durably) before inject.json goes, so a request is never lost without an answer.
inline bool answer(const Root &root, const std::vector<Result> &rs)
{
    if (!runtime::durable::write_file(root.path() / INJECTED, results_json(rs)))
        return false;
    std::error_code ec;
    fs::remove(root.path() / INJECT, ec);
    runtime::durable::sync_dir(root.path());
    return true;
}

}
