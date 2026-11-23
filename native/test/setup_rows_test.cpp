// Host-native unit test for core/apply/content/setup_rows.hpp.

#include "core/apply/content/setup_rows.hpp"

#include <cmath>
#include <cstdio>

namespace c = swofty_core::apply::content;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    std::vector<c::SetupRowRecord> rows;
    for (int i = 0; i < 4; i++) {
        c::SetupRowRecord r;
        r.id = "mix_" + std::to_string(i);
        r.slider = 37 + i;
        r.group = 4;
        r.name = "Mix";
        r.in_scope = true;
        rows.push_back(r);
    }
    expect(c::check_setup_row(rows[0], rows).ok, "a row on slider 37 in tab 4 is fine");
    auto bad = rows[0];
    bad.slider = 42;
    expect(!c::check_setup_row(bad, rows).ok, "slider 42 does not exist");
    bad = rows[0];
    bad.group = 8;
    expect(!c::check_setup_row(bad, rows).ok, "there are 8 tabs");
    bad = rows[0];
    bad.name.clear();
    expect(!c::check_setup_row(bad, rows).ok, "a row needs a name");
    auto twin = rows[1];
    twin.id = "other";
    twin.slider = 37;
    expect(!c::check_setup_row(twin, rows).ok, "two rows in scope cannot share a slider");
    twin.in_scope = false;
    expect(c::check_setup_row(twin, rows).ok, "a row out of scope does not clash");

    c::GroupIndexes stock;
    stock.idx[0] = 0x12;
    stock.idx[1] = 0x11;
    stock.count = 2;
    auto merged = c::merge_group(stock, 4, rows);
    expect(merged.count == 6 && merged.idx[2] == 37 && merged.idx[5] == 40, "the four sliders are appended to the brakes group");
    expect(c::merge_group(stock, 3, rows).count == 2, "another tab keeps its indexes");
    rows[0].in_scope = false;
    expect(c::merge_group(stock, 4, rows).count == 5, "a row out of scope adds nothing");
    expect(c::row_of_slider(rows, 37) == nullptr && c::row_of_slider(rows, 38) != nullptr, "the entry of a slider finds its row");

    c::GroupIndexes full;
    full.count = c::SETUP_GROUP_ENTRIES_MAX;
    expect(c::merge_group(full, 4, rows).count == c::SETUP_GROUP_ENTRIES_MAX, "a full group takes no more");
    expect(std::fabs(c::display_value(0.84f, 1.0f, 8, 16) - 0.92f) < 1e-4f, "step 8 of 16 on 0.84..1.00 shows 0.92");
    expect(c::display_value(1, 2, 99, 10) == 2 && c::display_value(1, 2, -3, 10) == 1, "a step outside the range clamps");
    std::printf(failed ? "%d failed\n" : "all passed\n", failed);
    return failed ? 1 : 0;
}
