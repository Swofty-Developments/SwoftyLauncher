#include "core/runtime/json.hpp"
#include "core/runtime/scan_cache.hpp"
#include "../src/loader/pattern_scan.h"

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iterator>
#include <map>
#include <set>
#include <random>
#include <string>
#include <vector>

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static int naive(const uint8_t *lo, size_t n, const std::vector<int> &pat, size_t *first)
{
    int hits = 0;
    for (size_t i = 0; i + pat.size() <= n; i++)
        if (pattern_hit(lo + i, pat.data(), int(pat.size())) && hits++ == 0)
            *first = i;
    return hits;
}

static int fast(const uint8_t *lo, size_t n, const std::vector<int> &pat, size_t *first)
{
    int a1, a2;
    if (!pick_anchors(pat.data(), int(pat.size()), &a1, &a2))
        return -1;
    uintptr_t f = 0;
    int hits = scan_range(lo, lo + n, pat.data(), int(pat.size()), a1, a2, &f);
    if (hits)
        *first = size_t(f - uintptr_t(lo));
    return hits;
}

static void kernel_tests()
{
    std::mt19937 rng(1234);
    int mismatches = 0, cases = 0, planted = 0;
    for (int round = 0; round < 400; round++) {
        size_t n = 1 + rng() % 600;
        std::vector<uint8_t> buf(n);
        for (auto &b : buf)
            b = uint8_t(rng() % 7 == 0 ? 0x48 : rng() % 6);
        size_t len = 1 + rng() % 24;
        std::vector<int> pat(len);
        for (auto &p : pat)
            p = rng() % 4 == 0 ? -1 : int(rng() % 6);
        if (std::all_of(pat.begin(), pat.end(), [](int v) { return v < 0; }))
            pat[rng() % len] = int(rng() % 6);
        for (int k = int(rng() % 3); k-- && len <= n;) {
            size_t at = rng() % (n - len + 1);
            for (size_t i = 0; i < len; i++)
                if (pat[i] >= 0)
                    buf[at + i] = uint8_t(pat[i]);
            planted++;
        }
        size_t f1 = SIZE_MAX, f2 = SIZE_MAX;
        int h1 = naive(buf.data(), n, pat, &f1), h2 = fast(buf.data(), n, pat, &f2);
        cases++;
        mismatches += h1 != h2 || (h1 && f1 != f2);
    }
    expect(mismatches == 0, "kernel: SSE2 scan agrees with a byte-by-byte scan on " + std::to_string(cases) +
                                " random buffers (" + std::to_string(planted) + " planted hits, wildcards, overlaps)");

    std::vector<uint8_t> buf(100, 0x90);
    const uint8_t sig[] = {0x41, 0x57, 0xA2, 0x56, 0xAD};
    memcpy(buf.data(), sig, 5);
    memcpy(buf.data() + 95, sig, 5);
    std::vector<int> pat = {0x41, 0x57, -1, 0x56, 0xAD};
    size_t first = 0;
    expect(fast(buf.data(), buf.size(), pat, &first) == 2 && first == 0,
           "kernel: hits at the very first and very last positions both count, the first is reported");
    expect(fast(buf.data() + 1, 99, pat, &first) == 1 && first == 94, "kernel: an unaligned start still finds the last hit");
    expect(fast(buf.data(), 4, pat, &first) == 0, "kernel: a range shorter than the pattern has no hit");
    std::vector<int> one = {-1, 0x56, -1};
    int a1, a2;
    expect(pick_anchors(one.data(), 3, &a1, &a2) && a1 == 1 && a2 == 1, "anchors: one fixed byte anchors both filters");
    std::vector<int> none = {-1, -1};
    expect(!pick_anchors(none.data(), 2, &a1, &a2), "anchors: an all-wildcard pattern is invalid");
    std::vector<int> mixed = {0x48, 0x8B, 0xAD, 0x00, 0xA2};
    expect(pick_anchors(mixed.data(), 5, &a1, &a2) && a1 == 2 && a2 == 4,
           "anchors: the two rarest fixed bytes of x64 code are chosen (AD, A2 over 48, 8B, 00)");
}

struct Scanned {
    int hits = 0;
    uintptr_t first = 0;
    bool operator==(const Scanned &o) const { return hits == o.hits && (!hits || first == o.first); }
};

static std::vector<Scanned> one_by_one(const std::vector<std::pair<size_t, size_t>> &runs, const uint8_t *base,
                                       const std::vector<std::vector<int>> &pats)
{
    std::vector<Scanned> out(pats.size());
    for (size_t j = 0; j < pats.size(); j++) {
        int a1, a2;
        pick_anchors(pats[j].data(), int(pats[j].size()), &a1, &a2);
        for (auto [lo, hi] : runs) {
            uintptr_t f = 0;
            int h = scan_range(base + lo, base + hi, pats[j].data(), int(pats[j].size()), a1, a2, &f);
            if (h && !out[j].hits)
                out[j].first = f;
            out[j].hits += h;
        }
    }
    return out;
}

static std::vector<ScanPattern> scan_patterns(const std::vector<std::vector<int>> &pats)
{
    std::vector<ScanPattern> sp(pats.size());
    for (size_t j = 0; j < pats.size(); j++) {
        sp[j].pat = pats[j].data(), sp[j].n = int(pats[j].size());
        pick_anchors(sp[j].pat, sp[j].n, &sp[j].a1, &sp[j].a2);
    }
    return sp;
}

static std::vector<Scanned> all_at_once(const std::vector<std::pair<size_t, size_t>> &runs, const uint8_t *base,
                                        const std::vector<std::vector<int>> &pats, size_t pieces, int *solos = nullptr)
{
    std::vector<ScanPattern> sp = scan_patterns(pats);
    MultiScan ms;
    if (!multi_prepare(&ms, sp.data(), int(sp.size())))
        return {};
    if (solos)
        *solos = ms.solos;
    std::vector<int> hits(pats.size());
    std::vector<uintptr_t> first(pats.size());
    for (auto [lo, hi] : runs) {
        const uint8_t *l = base + lo, *h = base + hi;
        size_t len = hi - lo, step = std::max<size_t>(1, len / pieces);
        std::vector<std::vector<int>> ph;
        std::vector<std::vector<uintptr_t>> pf;
        for (size_t at = 0; at < len; at += step) {
            ph.emplace_back(pats.size());
            pf.emplace_back(pats.size());
            multi_scan_keys(&ms, l, h, l + at, l + std::min(len, at + step), ph.back().data(), pf.back().data());
        }
        for (size_t k = 0; k < ph.size(); k++)
            for (size_t j = 0; j < pats.size(); j++) {
                if (ph[k][j] && !hits[j])
                    first[j] = pf[k][j];
                hits[j] += ph[k][j];
            }
        multi_scan_solos(&ms, l, h, hits.data(), first.data());
    }
    multi_free(&ms);
    std::vector<Scanned> out(pats.size());
    for (size_t j = 0; j < pats.size(); j++)
        out[j] = {hits[j], first[j]};
    return out;
}

static void multi_tests()
{
    std::mt19937 rng(4321);
    int cases = 0, mismatched = 0, patterns = 0, solo = 0, multi_hit = 0;
    for (int round = 0; round < 300; round++) {
        size_t n = 64 + rng() % 4000;
        std::vector<uint8_t> buf(n);
        for (auto &b : buf)
            b = uint8_t(rng() % 5 == 0 ? 0x48 : rng() % 9);
        std::vector<std::vector<int>> pats(1 + rng() % 60);
        for (auto &pat : pats) {
            size_t len = 1 + rng() % 20;
            if (rng() % 2 && len <= n) {
                size_t at = rng() % (n - len + 1);
                for (size_t i = 0; i < len; i++)
                    pat.push_back(rng() % 5 == 0 ? -1 : buf[at + i]);
            } else {
                for (size_t i = 0; i < len; i++)
                    pat.push_back(rng() % 4 == 0 ? -1 : int(rng() % 9));
            }
            if (std::all_of(pat.begin(), pat.end(), [](int v) { return v < 0; }))
                pat[rng() % len] = int(rng() % 9);
        }
        if (rng() % 3 == 0)
            pats.push_back(pats[rng() % pats.size()]);
        std::vector<std::pair<size_t, size_t>> runs;
        for (size_t at = 0; at < n;) {
            size_t len = 1 + rng() % (n / 2 + 1), end = std::min(n, at + len);
            if (rng() % 4)
                runs.push_back({at, end});
            at = end;
        }
        int solos = 0;
        auto a = one_by_one(runs, buf.data(), pats);
        auto b = all_at_once(runs, buf.data(), pats, 1 + rng() % 7, &solos);
        cases++;
        patterns += int(pats.size()), solo += solos;
        bool same = a.size() == b.size();
        for (size_t j = 0; same && j < a.size(); j++)
            same = a[j] == b[j];
        for (auto &x : a)
            multi_hit += x.hits > 1;
        mismatched += !same;
    }
    expect(mismatched == 0, "multi: one pass agrees with a scan per pattern on hits and first hit over " + std::to_string(cases) +
                                " random images in readable runs (" + std::to_string(patterns) + " patterns, " +
                                std::to_string(solo) + " without four fixed bytes, " + std::to_string(multi_hit) +
                                " with several hits, ranges split into pieces)");

    std::vector<uint8_t> buf(64, 0x90);
    const uint8_t sig[] = {0x41, 0x57, 0xA2, 0x56, 0xAD, 0x11};
    memcpy(buf.data(), sig, 6);
    memcpy(buf.data() + 58, sig, 6);
    std::vector<std::vector<int>> pats = {{0x41, 0x57, 0xA2, 0x56, 0xAD, 0x11}, {-1, 0x57, 0xA2, 0x56, 0xAD}, {0xAD, 0x11}};
    auto r = all_at_once({{0, 64}}, buf.data(), pats, 3);
    expect(r.size() == 3 && r[0].hits == 2 && r[0].first == uintptr_t(buf.data()) && r[1].hits == 2 &&
               r[1].first == uintptr_t(buf.data()) && r[2].hits == 2 && r[2].first == uintptr_t(buf.data() + 4),
           "multi: hits at the very first and very last positions count, the first is reported, short patterns scan alone");
    r = all_at_once({{0, 32}, {32, 63}}, buf.data(), pats, 1);
    expect(r[0].hits == 1 && r[2].hits == 1, "multi: a hit that runs past its readable run is not counted");
    int tok[256];
    expect(parse_pattern("48 8B ?? 05", tok, 256) == 4 && tok[2] == -1 && parse_pattern("48 GZ", tok, 256) == -1 &&
               parse_pattern("? ?", tok, 256) == 2 && parse_pattern("??", tok, 256) == 1,
           "multi: the loader's pattern parser is shared");
}

static void cache_tests()
{
    using scancache::Cache;
    expect(scancache::tokens("48 8B ?? 05").size() == 4 && scancache::tokens("48 8B ?? 05")[2] == -1, "cache: pattern tokens");
    expect(scancache::tokens("48 GZ").empty() && scancache::tokens("?? ??").empty() && scancache::tokens("").empty() &&
               scancache::tokens("123").empty(),
           "cache: bad tokens, all wildcards or an empty pattern are refused");
    uint8_t bytes[] = {0x48, 0x8B, 0x01, 0x05};
    expect(scancache::matches(bytes, scancache::tokens("48 8B ?? 05")) &&
               !scancache::matches(bytes, scancache::tokens("48 8B ?? 06")),
           "cache: the pattern is checked byte for byte, wildcards skipped");

    Cache c;
    c.parse("", 0x1122334455667788ull);
    expect(c.dirty && c.rva.empty(), "cache: an empty file starts an empty cache that will be written");
    c.dirty = false;
    c.found(".xtls", "48 8B 05", 1, 0x1234);
    c.found(".xtls", "41 57 41 56", 1, 0x41974e0);
    c.found(".data1", "74 D2 0F", 1, 0xfa4e016);
    c.found(".xtls", "E8 ?? ?? ?? ??", 3, 0);
    uint64_t v = 0;
    expect(c.dirty && c.rva.size() == 3 && c.find(".xtls", "41 57 41 56", v) && v == 0x41974e0 &&
               !c.find(".xtls", "E8 ?? ?? ?? ??", v),
           "cache: only patterns with exactly one hit are kept");
    std::string text = c.serialize();
    Cache d;
    expect(d.parse(text, 0x1122334455667788ull) && !d.dirty && d.rva == c.rva, "cache: serialize and parse round-trip");
    expect(text == d.serialize(), "cache: the file is written in a stable order");
    Cache e;
    expect(!e.parse(text, 0x1122334455667789ull) && e.rva.empty() && e.dirty,
           "cache: a file written for another exe key is ignored and will be rewritten");
    Cache f;
    expect(!f.parse("swofty-scan-cache\t0\t1122334455667788\n1234\t.xtls\t48 8B 05\n", 0x1122334455667788ull) && f.rva.empty(),
           "cache: a file of another schema is ignored");
    Cache g;
    std::string bad = text + "zz\t.xtls\t48 8B\nnotab\n12\t\t48\n13\t.xtls\t?? ??\n";
    expect(g.parse(bad, 0x1122334455667788ull) && g.rva.size() == 3 && g.dirty,
           "cache: malformed rows are dropped and the file is marked for rewrite");
    d.dirty = false;
    d.found(".xtls", "48 8B 05", 1, 0x1234);
    expect(!d.dirty, "cache: a scan that confirms the cached address does not rewrite the file");
    d.found(".xtls", "48 8B 05", 1, 0x2000);
    expect(d.dirty && d.find(".xtls", "48 8B 05", v) && v == 0x2000, "cache: a scan that finds the pattern elsewhere moves it");
    d.found(".xtls", "48 8B 05", 0, 0);
    expect(!d.find(".xtls", "48 8B 05", v), "cache: a scan with no single hit drops the entry");
    uint8_t hdr[] = {1, 2, 3};
    expect(runtime::fnv64(hdr, 3) != runtime::fnv64(hdr, 2), "cache: the exe key covers every header byte");
}

struct Image {
    std::vector<uint8_t> img;
    struct Sec {
        std::string name;
        uint32_t va, size;
    };
    std::vector<Sec> secs;
    bool load(const char *path)
    {
        std::ifstream f(path, std::ios::binary);
        std::vector<uint8_t> file((std::istreambuf_iterator<char>(f)), {});
        if (file.size() < 0x400)
            return false;
        uint32_t pe = *reinterpret_cast<uint32_t *>(&file[0x3c]);
        uint16_t nsec = *reinterpret_cast<uint16_t *>(&file[pe + 6]), opt = *reinterpret_cast<uint16_t *>(&file[pe + 20]);
        img.assign(*reinterpret_cast<uint32_t *>(&file[pe + 24 + 56]), 0);
        for (int i = 0; i < nsec; i++) {
            const uint8_t *h = &file[pe + 24 + opt + 40 * i];
            Sec s{std::string(reinterpret_cast<const char *>(h), strnlen(reinterpret_cast<const char *>(h), 8)),
                  *reinterpret_cast<const uint32_t *>(h + 12), *reinterpret_cast<const uint32_t *>(h + 8)};
            uint32_t raw = *reinterpret_cast<const uint32_t *>(h + 20), rawsize = *reinterpret_cast<const uint32_t *>(h + 16);
            if (raw && rawsize)
                memcpy(&img[s.va], &file[raw], std::min(rawsize, s.size ? s.size : rawsize));
            secs.push_back(s);
        }
        return true;
    }
    const Sec *section(const std::string &n) const
    {
        for (auto &s : secs)
            if (s.name == n)
                return &s;
        return nullptr;
    }
};

static double now() { return std::chrono::duration<double>(std::chrono::steady_clock::now().time_since_epoch()).count(); }

static int memchr_scan(const uint8_t *lo, const uint8_t *hi, const std::vector<int> &pat, uintptr_t *first)
{
    int n = int(pat.size()), anchor = 0, hits = 0;
    while (pat[anchor] < 0)
        anchor++;
    const uint8_t *last = hi - n + anchor;
    for (const uint8_t *p = lo + anchor; p <= last; p++) {
        p = static_cast<const uint8_t *>(memchr(p, pat[anchor], size_t(last - p + 1)));
        if (!p)
            break;
        if (pattern_hit(p - anchor, pat.data(), n) && hits++ == 0)
            *first = uintptr_t(p - anchor);
    }
    return hits;
}

static void exe_tests(const char *exe, const char *bindings)
{
    Image im;
    std::ifstream bf(bindings, std::ios::binary);
    std::string text((std::istreambuf_iterator<char>(bf)), {});
    json::Value doc;
    if (!im.load(exe) || !json::parse(text, doc)) {
        expect(false, std::string("exe: load ") + exe + " and " + bindings);
        return;
    }
    struct Row {
        std::string id, section;
        std::vector<int> pat;
        long long match, rva, anchor, insn, len;
        std::string rkind;
    };
    std::vector<Row> rows;
    for (auto &j : doc["bindings"].a)
        rows.push_back({j["id"].str(), j["section"].str(), scancache::tokens(j["pattern"].str()), j["match"].num(-1),
                        j["rva"].num(-1), j["anchor"].num(), j["resolve"]["insn"].num(), j["resolve"]["len"].num(),
                        j["resolve"]["kind"].str("addr")});
    double t0 = now();
    int placed = 0, verified = 0, target_ok = 0;
    for (auto &r : rows) {
        auto *s = im.section(r.section);
        if (r.match < 0 || !s || r.pat.empty())
            continue;
        placed++;
        if (uint64_t(r.match) < s->va || uint64_t(r.match) + r.pat.size() > uint64_t(s->va) + s->size ||
            !scancache::matches(&im.img[r.match], r.pat))
            continue;
        verified++;
        long long at = r.match + r.anchor, target = at;
        if (r.rkind == "rel32")
            target = at + 4 + *reinterpret_cast<const int32_t *>(&im.img[at]);
        else if (r.rkind == "riprel")
            target = r.match + r.insn + r.len + *reinterpret_cast<const int32_t *>(&im.img[at]);
        target_ok += target == r.rva;
    }
    double kb = now() - t0;
    expect(placed == int(rows.size()) && verified == placed && target_ok == placed,
           "exe: every binding (" + std::to_string(verified) + "/" + std::to_string(rows.size()) +
               ") matches its pattern at its KB address and resolves to its KB rva");
    double ta = 0, tb = 0;
    int agree = 0, unique = 0;
    for (auto &r : rows) {
        auto *s = im.section(r.section);
        if (!s || r.pat.empty())
            continue;
        const uint8_t *lo = &im.img[s->va], *hi = lo + s->size;
        uintptr_t f1 = 0, f2 = 0;
        double t1 = now();
        int h1 = memchr_scan(lo, hi, r.pat, &f1);
        double t2 = now();
        int a1, a2;
        pick_anchors(r.pat.data(), int(r.pat.size()), &a1, &a2);
        int h2 = scan_range(lo, hi, r.pat.data(), int(r.pat.size()), a1, a2, &f2);
        double t3 = now();
        ta += t2 - t1, tb += t3 - t2;
        agree += h1 == h2 && (h1 != 1 || f1 == f2);
        unique += h2 == 1 && f2 - uintptr_t(im.img.data()) == uintptr_t(r.match);
    }
    expect(agree == int(rows.size()),
           "exe: the SSE2 scan and the memchr scan agree on all " + std::to_string(rows.size()) + " patterns");
    expect(unique == int(rows.size()), "exe: every pattern has exactly one hit, at its KB address");
    printf("      timing over %zu bindings: memchr scan %.0f ms, SSE2 scan %.0f ms, KB address check %.3f ms\n", rows.size(),
           1000 * ta, 1000 * tb, 1000 * kb);

    std::map<std::string, std::vector<std::vector<int>>> by_section;
    std::set<std::string> seen;
    for (auto &j : doc["bindings"].a) {
        std::string sec = j["section"].str(), text = j["pattern"].str();
        int tok[256], n = parse_pattern(text.c_str(), tok, 256);
        if (n > 0 && im.section(sec) && seen.insert(sec + '|' + text).second)
            by_section[sec].emplace_back(tok, tok + n);
    }
    int same = 0, total = 0;
    double t_one = 0, t_all = 0;
    for (auto &[sec, pats] : by_section) {
        auto *s = im.section(sec);
        std::vector<std::pair<size_t, size_t>> runs = {{s->va, size_t(s->va) + s->size}};
        double t1 = now();
        auto a = one_by_one(runs, im.img.data(), pats);
        double t2 = now();
        auto b = all_at_once(runs, im.img.data(), pats, 1);
        double t3 = now();
        t_one += t2 - t1, t_all += t3 - t2;
        total += int(pats.size());
        for (size_t j = 0; j < a.size() && j < b.size(); j++)
            same += a[j] == b[j];
    }
    expect(same == total && total > 0, "exe: one pass over each section agrees with a scan per pattern on all " +
                                           std::to_string(total) + " distinct patterns");
    printf("      timing over %d distinct patterns: one scan per pattern %.0f ms, one pass %.0f ms\n", total, 1000 * t_one,
           1000 * t_all);
}

int main(int argc, char **argv)
{
    kernel_tests();
    multi_tests();
    cache_tests();
    if (argc > 2)
        exe_tests(argv[1], argv[2]);
    printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    return fails != 0;
}
