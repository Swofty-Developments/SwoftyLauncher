// Host-native unit test for the core's DRED crash data (native/src/core/dred_logic.hpp): when DRED is switched
// on, which register values mean a removed GPU, the breadcrumb window and the wording of the report lines.
// Built and run by native/test/run_dred_test.sh.

#include "dred_logic.hpp"

#include <cstdio>
#include <string>

namespace dr = runtime::dred;

static int g_fail;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            g_fail++;                                                                                                  \
        }                                                                                                              \
    } while (0)

int main()
{
    CHECK(!dr::decide(nullptr, false).on);
    CHECK(!dr::decide("", false).on);
    CHECK(dr::decide(nullptr, true).on);
    CHECK(dr::decide("", true).on);
    CHECK(dr::decide("1", false).on);
    CHECK(dr::decide("1", true).on);
    CHECK(!dr::decide("0", true).on);
    CHECK(!dr::decide("0", false).on);
    CHECK(!dr::decide("10", false).on);
    CHECK(std::string(dr::decide("0", true).why) == "SWOFTY_DRED=0");

    CHECK(dr::is_removal_hresult(0x887a0001));
    CHECK(dr::is_removal_hresult(0x887a0005));
    CHECK(dr::is_removal_hresult(0xffffffff887a0007ull));
    CHECK(!dr::is_removal_hresult(0x887a0002));
    CHECK(!dr::is_removal_hresult(0x1000887a0005ull));
    CHECK(!dr::is_removal_hresult(0));

    CHECK(std::string(dr::op_name(3)) == "DRAWINSTANCED");
    CHECK(std::string(dr::op_name(15)) == "RESOURCEBARRIER");
    CHECK(std::string(dr::op_name(0x2c)) == "RESOLVEENCODEROUTPUTMETADATA");
    CHECK(std::string(dr::op_name(0x2d)) == "UNKNOWN_OP");
    CHECK(std::string(dr::allocation_type_name(0x22)) == "RESOURCE");
    CHECK(std::string(dr::allocation_type_name(0x99)) == "OTHER");

    auto w = dr::op_window(10, 4);
    CHECK(w.from == 0 && w.to == 10);
    w = dr::op_window(500, 300);
    CHECK(w.to - w.from == dr::kMaxOps && w.from <= 300 && w.to > 300);
    w = dr::op_window(500, 500);
    CHECK(w.to == 500 && w.to - w.from <= dr::kMaxOps);
    w = dr::op_window(500, 0);
    CHECK(w.from == 0 && w.to == dr::kMaxOps);
    w = dr::op_window(5, 99);
    CHECK(w.from == 0 && w.to == 5);

    CHECK(dr::node_incomplete(10, 9));
    CHECK(!dr::node_incomplete(10, 10));

    std::string l = dr::op_line(3, 4, 3);
    CHECK(l.find("DRAWINSTANCED") != std::string::npos && l.find("done") != std::string::npos);
    CHECK(l.find("last completed") != std::string::npos);
    l = dr::op_line(4, 4, 6);
    CHECK(l.find("DISPATCH") != std::string::npos && l.find("todo") != std::string::npos);
    CHECK(l.find("first not completed") != std::string::npos);
    CHECK(dr::op_line(1, 4, 6).find("<-") == std::string::npos);

    std::string h = dr::node_header(2, "gfx list", "", 10, 4);
    CHECK(h.find("gfx list") != std::string::npos && h.find("(unnamed)") != std::string::npos);
    CHECK(h.find("4 of 10") != std::string::npos && h.find("NOT FINISHED") != std::string::npos);
    CHECK(dr::node_header(0, nullptr, nullptr, 3, 3).find("NOT FINISHED") == std::string::npos);

    CHECK(dr::allocation_line("heap_a", 0x22, 0).find("RESOURCE") != std::string::npos);
    CHECK(dr::allocation_line("heap_a", 0x22, 0).find(" at ") == std::string::npos);
    CHECK(dr::allocation_line("", 0x19, 0x1234).find("(unnamed)") != std::string::npos);
    CHECK(dr::allocation_line("", 0x19, 0x1234).find("at 0x1234") != std::string::npos);
    CHECK(dr::page_fault_line(0xdeadbeef000ull).find("0xdeadbeef000") != std::string::npos);

    if (!g_fail)
        std::printf("dred test passed\n");
    return g_fail ? 1 : 0;
}
