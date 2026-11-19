// Host unit test for the fast-fail crash report logic (fast_fail_logic.hpp). native/test/run_tests.sh fast_fail builds and runs it.
#include "core/diagnostics/crash/fast_fail_logic.hpp"
#include "core/diagnostics/crash/prologue_cache_logic.hpp"
#include "core/runtime/scan_cache.hpp"

#include <cstdio>
#include <string>
#include <vector>

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Exe {
    std::vector<uint8_t> bytes = std::vector<uint8_t>(4096, 0xCC);
    int scans = 0;

    uintptr_t base() const { return reinterpret_cast<uintptr_t>(bytes.data()); }

    void put(size_t at, const char *pattern)
    {
        auto p = fast_fail::parse_pattern(pattern);
        for (size_t i = 0; i < p.bytes.size(); i++)
            bytes[at + i] = p.mask[i] ? p.bytes[i] : uint8_t(0x5A);
    }

    uintptr_t scan(const fast_fail::Pattern &p)
    {
        scans++;
        uintptr_t found = 0;
        int hits = 0;
        for (size_t i = 0; i + p.bytes.size() <= bytes.size(); i++)
            if (fast_fail::matches(bytes.data() + i, p))
                found = base() + i, hits++;
        return hits == 1 ? found : 0;
    }
};

static crash::prologue::Found find_cached(Exe &exe, scancache::Cache &cache, const char *pattern)
{
    auto p = fast_fail::parse_pattern(pattern);
    return crash::prologue::find(
        exe.base(), [&](uint64_t &rva) { return cache.find(crash::prologue::CACHE_SECTION, pattern, rva); },
        [&](uintptr_t a) {
            return a >= exe.base() && a - exe.base() + p.bytes.size() <= exe.bytes.size() &&
                   fast_fail::matches(reinterpret_cast<const uint8_t *>(a), p);
        },
        [&] { return exe.scan(p); },
        [&](int hits, uint64_t rva) { cache.found(crash::prologue::CACHE_SECTION, pattern, hits, rva); });
}

static void prologue_cache()
{
    const char *pattern = "48 83 EC 28 B9 17 00 00 00 FF 15 ?? ?? ?? ?? 85 C0 74 07 B9";
    Exe exe;
    exe.put(0x400, pattern);
    scancache::Cache cache;
    cache.parse("", 0x1234);

    auto first = find_cached(exe, cache, pattern);
    expect(first.at == exe.base() + 0x400 && !first.cached && exe.scans == 1, "the first boot of an exe scans for the prologue");
    uint64_t kept = 0;
    expect(cache.find(crash::prologue::CACHE_SECTION, pattern, kept) && kept == 0x400 && cache.dirty,
           "and keeps its address in the scan cache");

    scancache::Cache reread;
    expect(reread.parse(cache.serialize(), 0x1234) && reread.find(crash::prologue::CACHE_SECTION, pattern, kept) && kept == 0x400,
           "the address survives the scan cache file");
    auto again = find_cached(exe, reread, pattern);
    expect(again.at == exe.base() + 0x400 && again.cached && exe.scans == 1,
           "the next boot takes the cached address without a scan");

    exe.bytes.assign(exe.bytes.size(), 0xCC);
    exe.put(0x900, pattern);
    auto moved = find_cached(exe, reread, pattern);
    expect(moved.at == exe.base() + 0x900 && !moved.cached && exe.scans == 2,
           "a stale cached address whose bytes no longer match falls back to the scan");
    expect(reread.find(crash::prologue::CACHE_SECTION, pattern, kept) && kept == 0x900, "and the scan's address replaces it");

    exe.put(0x200, pattern);
    auto twice = find_cached(exe, reread, pattern);
    expect(twice.at == exe.base() + 0x900 && twice.cached,
           "a cached address is used as long as its bytes match, as the scan cache does for bindings");

    exe.bytes.assign(exe.bytes.size(), 0xCC);
    exe.put(0x200, pattern);
    exe.put(0x600, pattern);
    auto none = find_cached(exe, reread, pattern);
    expect(none.at == 0 && !none.cached && exe.scans == 3 && !reread.find(crash::prologue::CACHE_SECTION, pattern, kept),
           "a stale address where the scan finds several matches gives no address and leaves no entry");

    scancache::Cache edge;
    edge.parse("", 1);
    edge.rva[scancache::Cache::id(crash::prologue::CACHE_SECTION, pattern)] = 0xFFFFFFF0;
    exe.bytes.assign(exe.bytes.size(), 0xCC);
    exe.put(0x10, pattern);
    auto outside = find_cached(exe, edge, pattern);
    expect(outside.at == exe.base() + 0x10 && !outside.cached,
           "a cached address outside the exe is not read and falls back to the scan");
}

int main()
{
    using fast_fail::Source;
    expect(std::string(fast_fail::code_name(2)) == "FAST_FAIL_STACK_COOKIE_CHECK_FAILURE", "code 2 is the stack cookie check");
    expect(std::string(fast_fail::code_name(5)) == "FAST_FAIL_INVALID_ARG", "code 5 is an invalid argument");
    expect(std::string(fast_fail::code_name(7)) == "FAST_FAIL_FATAL_APP_EXIT", "code 7 is a fatal app exit");
    expect(std::string(fast_fail::code_name(999)) == "FAST_FAIL_UNKNOWN", "an unlisted code is unknown");

    auto gs = fast_fail::outcome(Source::GsFailure, true, 0);
    expect(gs.has_code && gs.code == 2 && gs.exit_status == 0xC0000409, "a failed stack cookie ends with 0xc0000409 and code 2");
    auto gs_old = fast_fail::outcome(Source::GsFailure, false, 0);
    expect(gs_old.code == 2 && gs_old.exit_status == 0xC0000409, "without fast-fail the cookie path still raises 0xc0000409");
    auto inv = fast_fail::outcome(Source::InvalidParameter, true, 0);
    expect(inv.has_code && inv.code == 5 && inv.exit_status == 0xC0000409, "an invalid parameter fast-fails with code 5");
    auto inv_old = fast_fail::outcome(Source::InvalidParameter, false, 0);
    expect(!inv_old.has_code && inv_old.exit_status == 0xC0000417, "without fast-fail an invalid parameter ends with 0xc0000417");
    auto ab = fast_fail::outcome(Source::Abort, true, 0);
    expect(ab.has_code && ab.code == 7 && ab.exit_status == 0xC0000409, "abort fast-fails with code 7");
    auto rf = fast_fail::outcome(Source::RaiseFailFast, true, 0);
    expect(!rf.has_code && rf.exit_status == 0xC0000602, "RaiseFailFastException without a record ends with 0xc0000602");
    auto rf2 = fast_fail::outcome(Source::RaiseFailFast, true, 0xC0000005);
    expect(rf2.exit_status == 0xC0000005, "RaiseFailFastException ends with its record's code");

    expect(fast_fail::is_fast_fail_status(0xC0000409) && fast_fail::is_fast_fail_status(0xC0000602) &&
               fast_fail::is_fast_fail_status(0xC0000417),
           "the three fast-fail exit statuses are recognised");
    expect(!fast_fail::is_fast_fail_status(0xC0000005) && !fast_fail::is_fast_fail_status(0), "an access violation is not one");

    expect(fast_fail::reason(Source::GsFailure, gs, "base+0x1234") ==
               "fast-fail FAST_FAIL_STACK_COOKIE_CHECK_FAILURE (2) from __report_gsfailure (a stack cookie check failed); the "
               "process would end with exit code 0xc0000409; at base+0x1234",
           "the gs failure reason names the function, the code and the exit code");
    expect(fast_fail::reason(Source::RaiseFailFast, rf, "x") ==
               "fast-fail from RaiseFailFastException; the process would end with exit code 0xc0000602; at x",
           "a reason without a code omits it");
    expect(fast_fail::exception_line(0xC0000409, true, 7) == "fast-fail code 7: FAST_FAIL_FATAL_APP_EXIT",
           "the exception section names parameter 0");
    expect(fast_fail::exception_line(0xC0000005, true, 7).empty(), "no fast-fail line for other exceptions");

    auto p = fast_fail::parse_pattern("48 83 EC 28 E8 ?? ?? ?? ?? 48 85 C0");
    expect(p.bytes.size() == 12 && p.mask[5] == 0 && p.mask[4] == 0xFF, "a pattern parses its wildcards");
    expect(fast_fail::first_fixed(p) == 0 && fast_fail::first_fixed(fast_fail::parse_pattern("?? 48")) == 1,
           "the first fixed byte is found");
    const uint8_t code[] = {0x90, 0x48, 0x83, 0xEC, 0x28, 0xE8, 0x11, 0x22, 0x33, 0x44, 0x48, 0x85, 0xC0};
    expect(fast_fail::matches(code + 1, p), "a pattern matches across its wildcards");
    expect(!fast_fail::matches(code, p), "a pattern does not match at another offset");
    prologue_cache();
    return fails ? 1 : 0;
}
