#include "core/runtime/hashing.hpp"
#include "core/apply/content/gvdb.hpp"
#include "core/apply/content/uidb.hpp"
#include "core/apply/tracks/keys.hpp"
#include "core/assets/dbread_logic.hpp"
#include "core/telemetry/udp_ids_logic.hpp"

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
    check(runtime::fnv32(std::string("foobar")) == 0xbf9cf968u && runtime::fnv32("foobar", 6) == 0xbf9cf968u &&
              runtime::fnv32("bar", 3, runtime::fnv32("foo", 3)) == 0xbf9cf968u,
          "fnv32 over a length or a string continues from a seed");
    check(runtime::fnv32(std::string("a\0\xff", 3)) == 0xa5f49661u && runtime::fnv32("\xff") == 0x7a0b824eu,
          "fnv32 reads unsigned bytes and goes past a NUL when given the length");
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

void folded_copies()
{
    namespace gvdb = swofty_core::apply::content::gvdb;
    namespace tracks = swofty_core::apply::tracks;
    namespace uidb = swofty_core::apply::content::uidb;
    const std::string nul_high("a\0\xff", 3);
    check(gvdb::fnv64("foobar") == 0x85944171f73967e8ull && gvdb::fnv64("\xff") == 0xaf64724c8602eb6eull &&
              gvdb::fnv64(nul_high) == 0xe5d22e190425b0e1ull,
          "gvdb::fnv64 is FNV-1a 64 over unsigned bytes, past a NUL");
    check(tracks::path_key("foobar") == 0x85944171f73967e8ull &&
              tracks::path_key("{" + tracks::hex16(gvdb::fnv64("foo")) + "}bar") == 0x85944171f73967e8ull &&
              tracks::at(tracks::key("foo"), "bar").h == 0x85944171f73967e8ull && tracks::FNV_BASIS == 0xcbf29ce484222325ull,
          "track keys continue FNV-1a 64 from a hashed prefix and a row's key");
    check(uidb::fnv1a32("foobar") == 0xbf9cf968u && uidb::fnv1a32("\xff") == 0x7a0b824eu &&
              uidb::fnv1a32(nul_high) == 0xa5f49661u,
          "uidb::fnv1a32 is FNV-1a 32 over unsigned bytes, past a NUL");
    check(runtime::udp_ids::hashed_track_id("mod.a/track") == 0xda1639e6u &&
              runtime::udp_ids::hashed_track_id("foobar") == 0xbf9cf968u,
          "a hashed track id is FNV-1a 32 with the top bit set");
    check(dbread::fnv64("foobar") == 0x85944171f73967e8ull && dbread::fnv64("foobar", 3) == dbread::fnv64(std::string("foo")) &&
              dbread::fnv_cont(dbread::fnv64("foo"), "bar") == 0x85944171f73967e8ull &&
              dbread::fnv64("entrants.teams.team_28.stat_id") == 0x83a0da0da7de0118ull &&
              dbread::fnv_index(dbread::fnv64("entrants.teams.team_"), 28) == dbread::fnv64("entrants.teams.team_28") &&
              dbread::fnv32("foobar") == 0xbf9cf968u && dbread::fnv32("melbourne") == 0x5f9b706cu,
          "the game database reader's keys and labels are FNV-1a 64 and 32");
}

}

int main()
{
    fnv_vectors();
    cache_keys_unchanged();
    folded_copies();
    if (failures) {
        std::printf("%d failure(s)\n", failures);
        return 1;
    }
    std::printf("hashing: ok\n");
    return 0;
}
