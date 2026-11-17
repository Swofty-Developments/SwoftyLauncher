#include "core/runtime/hashing.hpp"

#include <cstdio>
#include <string>

namespace {

int failures = 0;

void check(bool ok, const char *what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what);
        failures++;
    }
}

std::string blob()
{
    std::string out(1000, '\0');
    for (size_t i = 0; i < out.size(); i++)
        out[i] = char((i * 37 + 11) & 255);
    return out;
}

void fnv_vectors()
{
    check(runtime::fnv64("", 0) == 0xcbf29ce484222325ull, "fnv64 of nothing is the basis");
    check(runtime::fnv64("a", 1) == 0xaf63dc4c8601ec8cull, "fnv64(\"a\") is the FNV-1a test vector");
    check(runtime::fnv64(std::string("foobar")) == 0x85944171f73967e8ull, "fnv64(\"foobar\") is the FNV-1a test vector");
    check(runtime::fnv64("bar", 3, runtime::fnv64("foo", 3)) == runtime::fnv64("foobar", 6), "fnv64 continues from a seed");
    check(runtime::fnv32("a") == 0xe40c292cu, "fnv32(\"a\") is the FNV-1a test vector");
    check(runtime::fnv32("foobar") == 0xbf9cf968u, "fnv32(\"foobar\") is the FNV-1a test vector");
}

void cache_keys_unchanged()
{
    using runtime::FNV64_BASIS;
    check(runtime::fnv64_field(FNV64_BASIS, "logos-v3") == 0x6a5aa92c2a8f21ccull, "fnv64_field keeps the logo cache keys");
    check(runtime::fnv64_field(runtime::fnv64_field(FNV64_BASIS, "ab"), "c") == 0xe76e0c123918b0e5ull &&
              runtime::fnv64_field(runtime::fnv64_field(FNV64_BASIS, "a"), "bc") == 0xae51c0103ba33067ull,
          "fnv64_field keeps fields apart");
    std::string b = blob();
    check(runtime::mix64("", 0) == 0x9341ca267ea24cf5ull, "mix64 of nothing keeps the slice names");
    check(runtime::mix64("abc", 3) == 0xc70825a6b851a9fcull, "mix64 of a short tail keeps the slice names");
    check(runtime::mix64(b.data(), b.size()) == 0x25613c4a0c2a8741ull, "mix64 of a long blob keeps the slice names");
}

}

int main()
{
    fnv_vectors();
    cache_keys_unchanged();
    if (failures) {
        std::printf("%d failure(s)\n", failures);
        return 1;
    }
    std::printf("hashing: ok\n");
    return 0;
}
