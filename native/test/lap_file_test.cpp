// Host unit test for the lap file format (native/src/core/lap_file_logic.hpp): the field table and sample packing,
// a written file read back, truncation at every length, unknown sections, the game's ghost blob and appended
// sections. Built and run by native/test/run_lap_file_test.sh.

#include "lap_file_logic.hpp"

#include <cstdio>

namespace lf = runtime::lap_file;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static lf::Layout layout()
{
    lf::Layout l;
    l.add("t", lf::Type::f32, "s");
    l.add("px", lf::Type::f32);
    l.add("gear", lf::Type::i8);
    l.add("flags", lf::Type::u16);
    l.add("dist", lf::Type::f64, "m");
    return l;
}

static lf::Header header()
{
    lf::Header h;
    h.core = "1.8.38";
    h.content = "swofty.swofty-2022/f1_2022";
    h.track = "bahrain";
    h.weather = "dry";
    h.game_board = "LBV060012D";
    h.vehicle = "mercedes";
    h.time_ms = 91234;
    h.sectors_ms = {29001, 31002, 31231};
    h.source = "clock";
    h.lap_index = 3;
    h.rate_hz = 60;
    return h;
}

static std::vector<uint8_t> samples(const lf::Layout &l, uint32_t n)
{
    std::vector<uint8_t> out(size_t(n) * l.stride);
    for (uint32_t i = 0; i < n; i++) {
        uint8_t *s = out.data() + size_t(i) * l.stride;
        l.set(s, 0, i / 60.0);
        l.set(s, 1, -100.5 + i);
        l.set(s, 2, int(i % 9) - 1);
        l.set(s, 3, 60000 + i);
        l.set(s, 4, i * 3.25);
    }
    return out;
}

int main()
{
    lf::Layout l = layout();
    expect(l.stride == 4 + 4 + 1 + 2 + 8, "the stride is the sum of the field sizes");
    expect(l.columns[3].offset == 9 && l.columns[4].offset == 11, "fields are packed in order without padding");
    expect(l.index("gear") == 2 && l.index("nope") == -1, "a field is found by name");

    auto smp = samples(l, 5);
    std::vector<uint8_t> ghost = {1, 2, 3, 4, 5, 6, 7};
    auto file = lf::build(header(), l, smp.data(), 5, ghost, {lf::section("ATST", {9, 9})});

    expect(!std::memcmp(file.data(), "SWLAP\0\1\0", 8), "the file starts with the magic");
    lf::Parsed p;
    expect(lf::parse(file.data(), file.size(), p) == lf::Status::ok, "a written file parses");
    expect(p.count == 5 && p.layout.stride == l.stride, "the sample count and stride come back");
    expect(p.layout.columns.size() == 5 && p.layout.columns[4].type == lf::Type::f64 && p.layout.columns[0].unit == "s",
           "the field table comes back with types and units");
    expect(p.header["lap"]["time_ms"].as_int() == 91234 && p.header["lap"]["sectors_ms"][2].as_int() == 31231,
           "the lap time and sectors are in the header");
    expect(p.header["board"]["game_board"].as_string() == "LBV060012D" && p.header["format"].as_int() == 1,
           "the board and the format are in the header");
    expect(!p.header["lap"].contains("valid") && !p.header.contains("nonce") && !p.header["car"].contains("team"),
           "an unknown value is omitted, never written empty");
    bool same = true;
    for (uint32_t i = 0; i < 5; i++)
        for (size_t c = 0; c < 5; c++)
            same = same && p.layout.read(p.sample(i), c) == l.read(smp.data() + size_t(i) * l.stride, c);
    expect(same, "every sample value reads back as written");
    expect(p.layout.read(p.sample(2), 4) == 6.5 && p.layout.read(p.sample(3), 1) == -97.5, "values are the ones stored");
    expect(p.ghost == ghost, "the game's ghost blob is verbatim");
    expect(p.others.size() == 1 && !std::memcmp(p.others[0].tag, "ATST", 4) && p.others[0].bytes.size() == 2,
           "an appended section is kept");

    auto no_ghost = lf::build(header(), l, smp.data(), 5, {}, {});
    lf::Parsed q;
    expect(lf::parse(no_ghost.data(), no_ghost.size(), q) == lf::Status::ok && q.ghost.empty(), "the ghost is optional");
    expect(q.tags.size() == 2 && q.tags[0] == "SMPL" && q.tags[1] == "END", "the sections are SMPL and END");

    auto reserved = lf::build(header(), l, smp.data(), 5, {}, {lf::section("SMPL", {1}), lf::section("END\0", {1})});
    expect(reserved.size() == no_ghost.size(), "an extra section cannot reuse the format's own tags");

    auto empty = lf::build(header(), l, nullptr, 0, {}, {});
    expect(lf::parse(empty.data(), empty.size(), q) == lf::Status::ok && q.count == 0, "a file with no samples parses");

    int rejected = 0;
    for (size_t n = 0; n < file.size(); n++) {
        lf::Parsed t;
        if (lf::parse(file.data(), n, t) != lf::Status::ok)
            rejected++;
    }
    expect(rejected == int(file.size()), "every truncation is refused");

    std::vector<uint8_t> unknown(file.begin(), file.end() - 8);
    const char tag[4] = {'X', 'T', 'R', 'A'};
    lf::append_section(unknown, tag, reinterpret_cast<const uint8_t *>("hello"), 5);
    lf::append_section(unknown, "END\0", nullptr, 0);
    lf::Parsed u;
    expect(lf::parse(unknown.data(), unknown.size(), u) == lf::Status::ok && u.count == 5 && u.others.size() == 2 &&
               !std::memcmp(u.others[1].tag, "XTRA", 4),
           "an unknown section is skipped and the rest still reads");

    std::vector<uint8_t> bad = file;
    bad[0] = 'X';
    expect(lf::parse(bad.data(), bad.size(), q) == lf::Status::bad_magic, "a wrong magic is refused");
    bad = file;
    lf::store<uint32_t>(bad.data() + 8, 0xffffff);
    expect(lf::parse(bad.data(), bad.size(), q) == lf::Status::truncated, "a header longer than the file is refused");
    bad = file;
    bad[12] = '[';
    expect(lf::parse(bad.data(), bad.size(), q) == lf::Status::bad_header, "a header that is not an object is refused");

    bad = file;
    lf::store<uint32_t>(bad.data() + 12 + lf::load<uint32_t>(bad.data() + 8) + 8 + 4, uint32_t(l.stride + 1));
    expect(lf::parse(bad.data(), bad.size(), q) == lf::Status::bad_samples,
           "samples that do not fit the field table are refused");

    {
        lf::Header h;
        h.time_ms = 91234;
        h.game_board = "LBV060012D";
        h.content_name = "Formula 1 2022";
        h.track_name = "Bahrain";
        h.team_stat = 4;
        h.vehicle_stat = 17;
        h.livery_stat = 0;
        h.ghost_id = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4v";
        h.performance = "equal";
        h.session_time_s = 412.5;
        h.assists = swofty::json::Value::object({{"brk", int64_t(1)}, {"abs", int64_t(1)}, {"trc", int64_t(2)},
                                                 {"drl", int64_t(0)}, {"gbx", int64_t(3)}, {"ers", int64_t(1)},
                                                 {"input", int64_t(2)}});
        lf::Layout l = layout();
        auto v = lf::header_json(h, l);
        expect(v["board"]["game_board"].as_string() == "LBV060012D" && v["board"]["content_name"].as_string() == "Formula 1 2022" &&
                   v["board"]["track_name"].as_string() == "Bahrain",
               "the header carries the game board and the display names");
        expect(v["ghost_id"].as_string() == h.ghost_id, "the header carries the ghost id when it is known");
        expect(v["car"]["team_stat"].as_int(-1) == 4 && v["car"]["vehicle_stat"].as_int(-1) == 17 &&
                   v["car"]["livery_stat"].as_int(-1) == 0,
               "the header carries the participant stat ids, zero included");
        expect(v["performance"].as_string() == "equal" && v["assists"]["gbx"].as_int(-1) == 3 &&
                   v["assists"]["input"].as_int(-1) == 2 && v["lap"]["session_time_s"].as_double(0) == 412.5,
               "the header carries performance, assists and the session time");
        lf::Header custom;
        custom.track = "swofty.tracks:ring";
        custom.track_addon = "swofty.tracks";
        custom.track_name = "The Ring";
        custom.track_kind = "record";
        auto cv = lf::header_json(custom, l);
        expect(cv["board"]["track"].as_string() == "swofty.tracks:ring" && cv["board"]["track_addon"].as_string() == "swofty.tracks" &&
                   cv["board"]["track_name"].as_string() == "The Ring" && cv["board"]["track_kind"].as_string() == "record",
               "the header carries the custom track key, addon, name and kind");
        expect(v["board"]["track_addon"].type() == swofty::json::Type::null && v["board"]["track_kind"].type() == swofty::json::Type::null,
               "a stock track writes no track addon or kind");
        lf::Header bare;
        bare.time_ms = 1;
        auto b = lf::header_json(bare, l);
        expect(b["board"]["game_board"].type() == swofty::json::Type::null && b["ghost_id"].type() == swofty::json::Type::null && b["car"]["team_stat"].type() == swofty::json::Type::null &&
                   b["performance"].type() == swofty::json::Type::null && b["assists"].type() == swofty::json::Type::null &&
                   b["lap"]["session_time_s"].type() == swofty::json::Type::null,
               "a key that could not be read is omitted, never guessed");
    }

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
