#include <cstdio>
#include <cstdlib>
#include <string>

#include "../src/instance.hpp"

static int failed = 0;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL line %d: %s\n", __LINE__, #c);                                                           \
            ++failed;                                                                                                  \
        }                                                                                                              \
    } while (0)

int main()
{
    const std::string max = "dc-000b0000";

    CHECK(ce::provisional_instance("f1_career_progressive:new3"));
    CHECK(ce::provisional_instance("new1"));
    CHECK(!ce::provisional_instance("f1_career_progressive:7731"));
    CHECK(!ce::provisional_instance("dc-000b0000"));

    {
        ce::Instance a;
        std::string first = a.resolve(true, max, "gm_driver_career:0123456789abcdef");
        CHECK(first == "gm_driver_career:0123456789abcdef");
        CHECK(a.resolve(true, max, "gm_driver_career:0123456789abcdef") == first);
        CHECK(a.resolve(false, "", "").empty());
        CHECK(a.resolve(true, max, "gm_driver_career:0123456789abcdef") == first);
        std::string second = a.resolve(true, max, "gm_driver_career:fedcba9876543210");
        CHECK(second != first);
        CHECK(a.renamed_to(first).empty());
    }

    {
        ce::Instance a;
        CHECK(a.resolve(true, max, "gm_driver_career:fedcba9876543210") == "gm_driver_career:fedcba9876543210");
        CHECK(a.renamed_to("gm_driver_career:0123456789abcdef").empty());
    }

    {
        ce::Instance a;
        CHECK(a.resolve(true, max, "gm_driver_career:new1") == "gm_driver_career:new1");
        CHECK(a.renamed_to("gm_driver_career:new1").empty());
        CHECK(a.resolve(true, max, "gm_driver_career:00000000000000aa") == "gm_driver_career:00000000000000aa");
        CHECK(a.renamed_to("gm_driver_career:new1") == "gm_driver_career:00000000000000aa");
        CHECK(a.renamed_to("gm_driver_career:new2").empty());
    }

    {
        ce::Instance a;
        a.resolve(true, max, "gm_driver_career:new1");
        a.resolve(false, "", "");
        a.resolve(true, max, "gm_driver_career:00000000000000bb");
        CHECK(a.renamed_to("gm_driver_career:new1").empty());
    }

    {
        ce::Instance a;
        a.resolve(true, max, "gm_driver_career:new1");
        a.resolve(true, "dc-000c0000", "gm_driver_career:00000000000000cc");
        CHECK(a.renamed_to("gm_driver_career:new1").empty());
    }

    {
        ce::Instance a;
        CHECK(a.resolve(true, max, "").empty());
        CHECK(a.resolve(true, "", "gm_driver_career:1").empty());
    }

    if (failed) {
        std::printf("%d failure(s)\n", failed);
        return 1;
    }
    std::printf("instance_test: ok\n");
    return 0;
}
