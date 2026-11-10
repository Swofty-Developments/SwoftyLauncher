#include "lap_assists_logic.hpp"
#include "lap_file_logic.hpp"

#include <cstdio>
#include <fstream>
#include <iterator>
#include <vector>

namespace la = runtime::lap_assists;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    std::ifstream f("fixtures/real-suzuka-2016.trace", std::ios::binary);
    std::vector<uint8_t> d((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
    expect(d.size() == 19261, "the real trace is the fixture");
    la::Raw r;
    expect(la::from_summary(d.data() + 8, r), "the real summary holds assist levels");
    expect(r.brk == 0 && r.abs == 1 && r.trc == 2 && r.drl == 2 && r.gbx == 3 && r.ers == 0 && r.aux == 0, "the proof lap's bytes");
    expect(la::packed(r) == 0x5d, "packed 0x5d, the low byte of the u16 the game keeps at +0x27e (0x035d)");
    expect(d[8 + 0x1e] == 0x5d, "the summary's own packed byte agrees");

    std::vector<uint8_t> unset(la::SUMMARY_SIZE, 0xff);
    expect(!la::from_summary(unset.data(), r), "an unwritten summary is not read");

    la::Raw off{true, 0, 0, 0, 0, 1, 1, 0}, on{true, 1, 1, 2, 1, 3, 0, 1};
    expect(la::packed(off) == 0, "everything off packs to 0");
    expect(la::packed(on) == 0x7f, "everything on packs to 0x7f");
    la::Raw m = la::merge(off, on);
    expect(la::packed(m) == 0x7f && m.trc == 2, "a mid-lap change keeps the more assisted value");
    expect(la::packed(la::merge(on, off)) == 0x7f, "the merge is symmetric");
    expect(la::packed(la::merge(off, off)) == 0, "unchanged stays off");

    auto j = la::to_json(on, 1);
    expect(j["trc"].as_int() == 2 && j["input_device"].as_string() == "wheel", "json carries the bytes and the device");
    expect(std::string(la::device_name(0)) == "controller" && std::string(la::device_name(2)) == "keyboard" &&
               std::string(la::device_name(3)).empty(),
           "device names");
    expect(la::to_json(on, 3)["input_device"].type() == swofty::json::Type::null,
           "an unclassified device is omitted");
    expect(la::diag_text(on, 1, 0).find("packed 0x7f") != std::string::npos, "diag line");

    namespace lf = runtime::lap_file;
    lf::Header h;
    h.driver_assists = 93;
    h.assists = la::to_json(on, 1);
    lf::Layout l;
    l.add("t", lf::Type::f32);
    auto hj = lf::header_json(h, l);
    expect(hj["driver_assists"].as_int() == 93 && hj["assists"]["abs"].as_int() == 1, "header carries both");
    lf::Header none;
    expect(lf::header_json(none, l)["driver_assists"].type() == swofty::json::Type::null, "no packed value when unread");
    if (failed)
        return 1;
    std::printf("all passed\n");
}
