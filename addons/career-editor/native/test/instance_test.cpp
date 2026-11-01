#include <cstdio>
#include <cstdlib>
#include <map>
#include <string>

#include "../src/instance.hpp"

static int failed = 0;

struct MemFiles {
    std::map<std::string, std::string> f;
    bool read(const std::string &p, std::string &o) const
    {
        auto it = f.find(p);
        if (it == f.end())
            return false;
        o = it->second;
        return true;
    }
    bool write(const std::string &p, const std::string &d)
    {
        f[p] = d;
        return true;
    }
    void remove(const std::string &p) { f.erase(p); }
};

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

    {
        MemFiles fs;
        const std::string legacy = "careers/dc-000b0000.season.json";
        fs.f[legacy] = "OLD";
        std::string text;
        CHECK(ce::adopt_legacy(fs, "gm_driver_career:new1", "careers/new1.season.json", legacy, text) ==
              ce::Adoption::provisional);
        CHECK(fs.f.count(legacy) == 1 && fs.f.size() == 1);

        CHECK(ce::adopt_legacy(fs, "gm_driver_career:00000000000000aa", "careers/aa.season.json", legacy, text) ==
              ce::Adoption::adopted);
        CHECK(text == "OLD");
        CHECK(fs.f["careers/aa.season.json"] == "OLD");
        CHECK(fs.f[legacy + ".adopted"] == "OLD");
        CHECK(fs.f.count(legacy) == 0);

        CHECK(ce::adopt_legacy(fs, "gm_driver_career:00000000000000aa", "careers/aa.season.json", legacy, text) ==
              ce::Adoption::none);

        std::string t2;
        CHECK(ce::adopt_legacy(fs, "gm_driver_career:00000000000000bb", "careers/bb.season.json", legacy, t2) ==
              ce::Adoption::no_legacy);
        CHECK(fs.f.count("careers/bb.season.json") == 0);
    }

    if (failed) {
        std::printf("%d failure(s)\n", failed);
        return 1;
    }
    std::printf("instance_test: ok\n");
    return 0;
}
