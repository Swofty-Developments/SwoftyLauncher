#include "core/apply/content/gvdb.hpp"

#include <chrono>
#include <cstdio>
#include <fstream>
#include <random>
#include <sstream>
#include <string>
#include <unordered_map>
#include <vector>

namespace gvdb = swofty_core::apply::content::gvdb;

static int failures = 0;

static void check(bool ok, const std::string &what)
{
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what.c_str());
    failures += !ok;
}

struct Rec {
    uint64_t key;
    int tag;
    std::string text;
};

static std::string build(const std::vector<Rec> &recs)
{
    std::string out(4, '\0');
    uint32_t count = uint32_t(recs.size());
    std::memcpy(out.data(), &count, 4);
    std::size_t stream = 0;
    for (auto &r : recs) {
        char head[13];
        uint32_t lo = uint32_t(r.key), hi = uint32_t(r.key >> 32), len = uint32_t(r.text.size());
        std::memcpy(head, &lo, 4);
        std::memcpy(head + 4, &hi, 4);
        head[8] = char(r.tag);
        std::memcpy(head + 9, &len, 4);
        out.append(head, 13);
        for (uint32_t k = 0; k < len; k++)
            out.push_back(char(
                (unsigned(uint8_t(r.text[k])) + unsigned(gvdb::MASK[(gvdb::MASK_START + stream + k) % gvdb::MASK_LEN])) & 0xff));
        out.push_back('\0');
        stream += len;
    }
    return out;
}

static bool same_as_read(const std::string &bytes, const gvdb::File &f, const std::vector<uint64_t> &absent, std::string &why)
{
    std::unordered_map<uint64_t, gvdb::Record> want;
    if (!gvdb::read(reinterpret_cast<const unsigned char *>(bytes.data()), bytes.size(), want)) {
        why = "the reference read refused the file";
        return false;
    }
    if (f.size() != want.size()) {
        why = "size " + std::to_string(f.size()) + ", the reference holds " + std::to_string(want.size());
        return false;
    }
    for (auto &[key, rec] : want) {
        auto got = f.get(key);
        if (!got || got->tag != rec.tag || got->text != rec.text || !f.has(key)) {
            char b[64];
            std::snprintf(b, sizeof b, "key %016llx", (unsigned long long)key);
            why = std::string(b) + (got ? " reads another record" : " is missing");
            return false;
        }
    }
    for (uint64_t key : absent)
        if (!want.count(key) && (f.get(key) || f.has(key))) {
            char b[64];
            std::snprintf(b, sizeof b, "absent key %016llx found", (unsigned long long)key);
            why = b;
            return false;
        }
    return true;
}

static void synthetic(std::size_t n, uint64_t seed)
{
    std::mt19937_64 rng(seed);
    std::vector<Rec> recs;
    for (std::size_t i = 0; i < n; i++) {
        uint64_t key = rng();
        if (i % 7 == 3)
            key &= 0xffffffff00000000ull;
        if (i % 11 == 5 && !recs.empty())
            key = recs[rng() % recs.size()].key;
        std::string text(rng() % 40 + (i % 13 == 0 ? 300 : 0), '\0');
        for (auto &c : text)
            c = char(rng());
        recs.push_back({key, int(rng() % 6), text});
    }
    if (n > 2) {
        recs.push_back({0, 1, "zero"});
        recs.push_back({~0ull, 2, ""});
    }
    std::vector<uint64_t> absent;
    for (auto &r : recs) {
        absent.push_back(r.key + 1);
        absent.push_back(r.key - 1);
        absent.push_back(r.key ^ (1ull << 63));
    }
    for (int i = 0; i < 1000; i++)
        absent.push_back(rng());
    absent.push_back(0);
    absent.push_back(~0ull);
    std::string bytes = build(recs);
    gvdb::File f;
    bool loaded = f.load(bytes);
    std::string why;
    check(loaded && same_as_read(bytes, f, absent, why), "a file of " + std::to_string(recs.size()) +
                                                             " record(s) (repeats kept last) reads the same as gvdb::read" +
                                                             (why.empty() ? "" : ": " + why));
}

static void refused()
{
    std::string bytes = build({{1, 0, "one"}, {2, 0, "two"}});
    gvdb::File f;
    check(!f.load(bytes.substr(0, bytes.size() - 3)) && !f.get(1) && !f.has(2) && f.empty(),
          "a cut file is refused and holds nothing");
    gvdb::File g;
    check(g.load(build({})) && g.empty() && !g.get(0) && !g.has(~0ull), "an empty file loads and finds nothing");
    check(g.load(bytes) && g.get(2) && g.get(2)->text == "two" && g.load(build({{3, 1, "three"}})) && !g.get(2) &&
              g.get(3)->text == "three",
          "a second load replaces the first");
    gvdb::File h;
    check(h.load(bytes) && !h.load("ab") && !h.get(1) && !h.has(2) && h.size() == 0,
          "a file too short for its count, loaded over a good one, holds nothing");
}

static void stock(const char *path)
{
    std::ifstream in(path, std::ios::binary);
    std::stringstream s;
    s << in.rdbuf();
    std::string bytes = s.str();
    gvdb::File f;
    auto t0 = std::chrono::steady_clock::now();
    bool loaded = f.load(bytes);
    double load_ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
    std::unordered_map<uint64_t, gvdb::Record> want;
    gvdb::read(reinterpret_cast<const unsigned char *>(bytes.data()), bytes.size(), want);
    std::vector<uint64_t> keys, absent;
    keys.reserve(want.size());
    for (auto &[k, r] : want)
        keys.push_back(k);
    std::mt19937_64 rng(7);
    for (int i = 0; i < 200000; i++)
        absent.push_back(rng());
    std::string why;
    check(loaded && f.size() > 100000 && same_as_read(bytes, f, absent, why),
          "the stock database: " + std::to_string(f.size()) + " key(s) and 200000 random ones read the same as gvdb::read" +
              (why.empty() ? "" : ": " + why));
    t0 = std::chrono::steady_clock::now();
    std::size_t bytes_read = 0;
    for (uint64_t k : keys)
        bytes_read += f.get(k)->text.size();
    double get_ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
    std::printf("      load %.1f ms; %zu gets in %.1f ms (%.3f us each, %zu text bytes)\n", load_ms, keys.size(), get_ms,
                1000 * get_ms / double(keys.size()), bytes_read);
}

int main(int argc, char **argv)
{
    for (std::size_t n : {std::size_t(0), std::size_t(1), std::size_t(2), std::size_t(3), std::size_t(257), std::size_t(5000),
                          std::size_t(200000)})
        synthetic(n, 1000 + n);
    refused();
    if (argc > 1)
        stock(argv[1]);
    std::printf("%s\n", failures ? "gvdb file test FAILED" : "gvdb file test passed");
    return failures ? 1 : 0;
}
