#pragma once

// The database first-read probe (db_first_read.cpp). With SWOFTY_DB_FIRST_READ=1 it lists the call sites of
// Gdb_Lookup_Key 0x1445a22f0 that read the database first after Gdb_Load_GameDatabase returns, so the load-core lane
// knows where the core's database edits must finish (re/research/loading_pipeline.md §8.6). Log only: every call goes
// on to the game's function with the same argument and returns its result.
//
// Boot_Init_NeApplication 0x1443c0560 calls the boot steps in a fixed order on the boot thread. STEP_RETURNS holds each
// step's return site, the instruction after its call, as an offset from the function's start, and STEP_CALLS the KB
// address of the function it calls, numbered as §8 numbers them: step 10 is the database load, 15 the hardware
// settings, 17 the job queues, 19 the modules and 22 the window. While step N runs, the slot under the function's frame
// holds its return site, so a lookup on the boot thread finds its step by scanning its stack for one of them.

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>

namespace runtime::db_first_read {

inline constexpr std::array<uint16_t, 28> STEP_RETURNS{0xcb,  0xf5,  0x114, 0x132, 0x150, 0x16e, 0x18c, 0x1aa, 0x1e6, 0x1fe,
                                                       0x21a, 0x236, 0x24f, 0x26d, 0x27a, 0x296, 0x2b2, 0x2c7, 0x2e3, 0x2fb,
                                                       0x313, 0x332, 0x34a, 0x362, 0x377, 0x393, 0x3cc, 0x3f5};
inline constexpr std::array<uint64_t, 28> STEP_CALLS{0x1443c0c80, 0x1443c1ce0, 0x1443c2180, 0x1443c2320, 0x1443c24b0, 0x1443c2990,
                                                     0x1443c2e40, 0x1443c2ed0, 0x1443c36d0, 0x1443c45e0, 0x1443c4670, 0x1443c49b0,
                                                     0x1443c4f10, 0x1439e49c0, 0x1443c4fa0, 0x1443c8c50, 0x1443c8d70, 0x1443cae70,
                                                     0x1443cb030, 0x1443cb780, 0x1443cbf10, 0x1443cc250, 0x1443ce7b0, 0x1443d1620,
                                                     0x1443d1b90, 0x1443d1bc0, 0x143919b50, 0x1443d2280};

inline constexpr char ENV[] = "SWOFTY_DB_FIRST_READ";
// New call sites listed after the core's edits, and from other threads while they run.
inline constexpr unsigned AFTER = 24, DURING = 8;
inline constexpr size_t PATH_CHARS = 96, MAX_SCAN = 1u << 20;

constexpr bool env_on(const char *v) { return v && v[0] == '1' && v[1] == 0; }

constexpr int step_at(uint64_t boot_init, uint64_t word)
{
    if (word < boot_init + STEP_RETURNS.front() || word > boot_init + STEP_RETURNS.back())
        return 0;
    for (size_t s = 0; s < STEP_RETURNS.size(); s++)
        if (word == boot_init + STEP_RETURNS[s])
            return int(s + 1);
    return 0;
}

// The step of the first word, from the call outwards, that is a step's return site, or 0.
inline int step_in(uint64_t boot_init, const uint64_t *words, size_t n)
{
    if (!boot_init)
        return 0;
    for (size_t i = 0; i < n; i++)
        if (int s = step_at(boot_init, words[i]))
            return s;
    return 0;
}

enum class Phase : uint8_t { idle, edits, after, done };
enum class Take : uint8_t { skip, own, list };

// Before the load returned and once the listing is done, a lookup is skipped. While the core's edits run, one on the
// boot thread is the edits' own and one from another thread is listed; after them, each is listed.
constexpr Take take(Phase p, bool boot_thread)
{
    if (p == Phase::edits)
        return boot_thread ? Take::own : Take::list;
    return p == Phase::after ? Take::list : Take::skip;
}

template <unsigned N> struct Sites {
    std::array<uint64_t, N> seen{};
    unsigned n = 0;

    bool full() const { return n == N; }
    // True the first time `ret` comes while there is room.
    bool add(uint64_t ret)
    {
        for (unsigned i = 0; i < n; i++)
            if (seen[i] == ret)
                return false;
        if (full())
            return false;
        seen[n++] = ret;
        return true;
    }
};

constexpr double ms(int64_t from, int64_t to, int64_t freq) { return freq > 0 ? double(to - from) * 1000.0 / double(freq) : 0; }

// Printable characters only, cut at PATH_CHARS with "...".
inline std::string path_text(const char *p)
{
    if (!p)
        return "(null)";
    std::string out;
    size_t i = 0;
    for (; p[i] && i < PATH_CHARS; i++)
        out += p[i] >= 0x20 && p[i] < 0x7f && p[i] != '"' ? p[i] : '?';
    if (p[i])
        out += "...";
    return out;
}

struct Read {
    unsigned index = 0;
    uint64_t lookup = 0;
    std::string path, where;
    bool found = false;
    unsigned long thread = 0;
    bool boot_thread = false;
    double since_load_ms = 0, since_edits_ms = -1;
    int step = 0;
    unsigned scanned_kib = 0;
};

inline std::string step_text(const Read &r)
{
    char b[112];
    if (!r.boot_thread)
        return "no boot step (another thread)";
    if (!r.step) {
        snprintf(b, sizeof b, "outside the boot steps (no step return in the %u KiB of stack above the call)", r.scanned_kib);
        return b;
    }
    snprintf(b, sizeof b, "in boot step %d (KB 0x%llx)", r.step, static_cast<unsigned long long>(STEP_CALLS[size_t(r.step - 1)]));
    return b;
}

inline std::string line(const Read &r)
{
    char b[224];
    snprintf(b, sizeof b, "[assets] gamedb: first read #%u: Gdb_Lookup_Key(\"", r.index);
    std::string out = b + r.path + "\") " + (r.found ? "found" : "not found");
    snprintf(b, sizeof b, ", lookup %llu after the load, from ", static_cast<unsigned long long>(r.lookup));
    out += b + r.where;
    snprintf(b, sizeof b, ", on %s %lu, %.1f ms after the load returned", r.boot_thread ? "the boot thread" : "thread", r.thread,
             r.since_load_ms);
    out += b;
    if (r.since_edits_ms < 0)
        out += ", during the core's edits";
    else {
        snprintf(b, sizeof b, " and %.1f ms after the core's edits", r.since_edits_ms);
        out += b;
    }
    return out + ", " + step_text(r);
}

inline std::string armed_line(const std::string &build, uint64_t lookup_rva, uint64_t boot_init_rva)
{
    char b[512];
    snprintf(
        b, sizeof b,
        "[assets] gamedb: first read probe on (%s=1) on %s: Gdb_Lookup_Key hooked at base+0x%llx, Boot_Init_NeApplication at "
        "base+0x%llx; it lists the first %u call sites to read after the core's edits and the first %u from other "
        "threads during them",
        ENV, build.c_str(), static_cast<unsigned long long>(lookup_rva), static_cast<unsigned long long>(boot_init_rva), AFTER,
        DURING);
    return b;
}

inline std::string edits_line(double edits_ms, unsigned long thread, uint64_t own, uint64_t others, unsigned sites)
{
    char b[320];
    int n = snprintf(b, sizeof b,
                     "[assets] gamedb: first read probe: the core's edits ran %.1f ms on the boot thread %lu and made %llu "
                     "lookup(s) of their own; ",
                     edits_ms, thread, static_cast<unsigned long long>(own));
    if (!others)
        snprintf(b + n, sizeof b - size_t(n), "no other thread read the database during them");
    else
        snprintf(b + n, sizeof b - size_t(n),
                 "%llu lookup(s) from other threads came during them, from %u call site(s) listed above: a reader "
                 "concurrent with the edits",
                 static_cast<unsigned long long>(others), sites);
    return b;
}

inline std::string done_line(unsigned sites, uint64_t lookups)
{
    char b[192];
    snprintf(b, sizeof b,
             "[assets] gamedb: first read probe done: %u call site(s) listed from %llu lookup(s) after the core's edits; "
             "later lookups are not listed",
             sites, static_cast<unsigned long long>(lookups));
    return b;
}

inline std::string off_line(const std::string &why) { return "[assets] gamedb: first read probe off: " + why; }

}
