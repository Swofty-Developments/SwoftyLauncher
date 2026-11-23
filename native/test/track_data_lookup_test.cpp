// Host-native unit test for the track data lookup guard (native/src/core/guards/tracks/track_data_lookup_logic.hpp and
// its handler at site_Trk_TrackData_SetMatched in track_select.cpp). Built and run by native/test/run_tests.sh
// track_data_lookup.
//
// Game_TrackData_GetTrack below is the game's own code: the 0x127 bytes at Steam 26566308 0x142e63390 (the build of
// Send Logs 2026-11-22 aober420 1574181978926882986) and at the KB build d873a1c8 0x142e60e00, which differ only in
// the displacements of the two assert calls the queries here never reach. On an x86-64 Linux host the test maps them
// executable and calls them with the Windows calling convention on a TrackDataSystem built in memory: 44 track slots
// as on aober420's profile, two TrackDataSets in database row order. The site's first byte (+0xb1, movzx eax, byte
// [rdx+1]) becomes an int3 whose handler does what the core's site mixin does there: it hands the registers to the
// guard and then runs the relocated movzx. Without the guard the trap sees rdx 0, the crash of the bundle, notes it and
// runs the movzx on a placeholder record so the call returns; with it the lookup returns melbourne's track data of the
// matched set. A fault anywhere else prints its registers and where it is in the game's bytes.

#include "core/guards/tracks/track_data_lookup_logic.hpp"

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#if defined(__x86_64__) && defined(__linux__)
#define REAL_BYTES 1
#include <csignal>
#include <sys/mman.h>
#include <ucontext.h>
#include <unistd.h>
#endif

namespace tl = runtime::track_data_lookup;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static bool has(const std::string &s, const char *part) { return s.find(part) != std::string::npos; }

static bool host_read(uint64_t address, void *out, std::size_t n, void *)
{
    if (address < 0x10000)
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

static const uint8_t STEAM_26566308[] = {
    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xec, 0x20, 0x4c, 0x8b, 0x59, 0x68, 0x48, 0x8b,
    0x71, 0x70, 0x49, 0x39, 0xf3, 0x0f, 0x84, 0xec, 0x00, 0x00, 0x00, 0x45, 0x0f, 0xb6, 0x10, 0xbf, 0x01, 0x00, 0x00, 0x00,
    0x44, 0x89, 0xd1, 0xd3, 0xe7, 0x41, 0x0f, 0xb6, 0x58, 0x01, 0xbd, 0x01, 0x00, 0x00, 0x00, 0x45, 0x0f, 0xb6, 0x48, 0x02,
    0x41, 0xbe, 0x01, 0x00, 0x00, 0x00, 0x44, 0x89, 0xc9, 0x41, 0xd3, 0xe6, 0x45, 0x0f, 0xb6, 0x78, 0x03, 0x45, 0x0f, 0xb6,
    0x60, 0x05, 0x41, 0x0f, 0xb6, 0x48, 0x04, 0xd3, 0xe5, 0x31, 0xc0, 0xeb, 0x10, 0x0f, 0x1f, 0x00, 0x49, 0x83, 0xc3, 0x30,
    0x49, 0x39, 0xf3, 0x0f, 0x84, 0xa0, 0x00, 0x00, 0x00, 0x84, 0xdb, 0x74, 0x10, 0x41, 0x80, 0xfa, 0x20, 0x0f, 0x83, 0xa1,
    0x00, 0x00, 0x00, 0x41, 0x85, 0x7b, 0x20, 0x74, 0xdf, 0x45, 0x84, 0xff, 0x74, 0x10, 0x41, 0x80, 0xf9, 0x20, 0x0f, 0x83,
    0x8c, 0x00, 0x00, 0x00, 0x45, 0x85, 0x73, 0x24, 0x74, 0xca, 0x45, 0x84, 0xe4, 0x74, 0x11, 0x80, 0xf9, 0x02, 0x0f, 0x83,
    0x7d, 0x00, 0x00, 0x00, 0x41, 0x85, 0x6b, 0x28, 0x74, 0xb6, 0xeb, 0x05, 0x4d, 0x85, 0xdb, 0x74, 0x5a, 0x0f, 0xb6, 0x42,
    0x01, 0x49, 0x8b, 0x4b, 0x08, 0x4d, 0x8b, 0x43, 0x10, 0x4c, 0x89, 0xc2, 0x48, 0x29, 0xca, 0x48, 0xc1, 0xfa, 0x04, 0x48,
    0x39, 0xc2, 0x76, 0x0e, 0x48, 0xc1, 0xe0, 0x04, 0x48, 0x8b, 0x44, 0x01, 0x08, 0x48, 0x85, 0xc0, 0x75, 0x33, 0x49, 0x39,
    0xc8, 0x74, 0x2c, 0x41, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x45, 0x31, 0xc9, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x49, 0xc1, 0xe1, 0x04, 0x4a, 0x8b, 0x44, 0x09, 0x08, 0x48, 0x85, 0xc0, 0x75, 0x0f, 0x45, 0x89, 0xc1, 0x41, 0xff, 0xc0,
    0x4c, 0x39, 0xca, 0x77, 0xe7, 0xeb, 0x02, 0x31, 0xc0, 0x48, 0x83, 0xc4, 0x20, 0x5b, 0x5d, 0x5f, 0x5e, 0x41, 0x5c, 0x41,
    0x5e, 0x41, 0x5f, 0xc3, 0xe8, 0x7f, 0xd6, 0x35, 0xfe, 0xe8, 0x3a, 0xb9, 0x45, 0xfd, 0xcc,
};

static const uint8_t KB_D873A1C8[] = {
    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xec, 0x20, 0x4c, 0x8b, 0x59, 0x68, 0x48, 0x8b,
    0x71, 0x70, 0x49, 0x39, 0xf3, 0x0f, 0x84, 0xec, 0x00, 0x00, 0x00, 0x45, 0x0f, 0xb6, 0x10, 0xbf, 0x01, 0x00, 0x00, 0x00,
    0x44, 0x89, 0xd1, 0xd3, 0xe7, 0x41, 0x0f, 0xb6, 0x58, 0x01, 0xbd, 0x01, 0x00, 0x00, 0x00, 0x45, 0x0f, 0xb6, 0x48, 0x02,
    0x41, 0xbe, 0x01, 0x00, 0x00, 0x00, 0x44, 0x89, 0xc9, 0x41, 0xd3, 0xe6, 0x45, 0x0f, 0xb6, 0x78, 0x03, 0x45, 0x0f, 0xb6,
    0x60, 0x05, 0x41, 0x0f, 0xb6, 0x48, 0x04, 0xd3, 0xe5, 0x31, 0xc0, 0xeb, 0x10, 0x0f, 0x1f, 0x00, 0x49, 0x83, 0xc3, 0x30,
    0x49, 0x39, 0xf3, 0x0f, 0x84, 0xa0, 0x00, 0x00, 0x00, 0x84, 0xdb, 0x74, 0x10, 0x41, 0x80, 0xfa, 0x20, 0x0f, 0x83, 0xa1,
    0x00, 0x00, 0x00, 0x41, 0x85, 0x7b, 0x20, 0x74, 0xdf, 0x45, 0x84, 0xff, 0x74, 0x10, 0x41, 0x80, 0xf9, 0x20, 0x0f, 0x83,
    0x8c, 0x00, 0x00, 0x00, 0x45, 0x85, 0x73, 0x24, 0x74, 0xca, 0x45, 0x84, 0xe4, 0x74, 0x11, 0x80, 0xf9, 0x02, 0x0f, 0x83,
    0x7d, 0x00, 0x00, 0x00, 0x41, 0x85, 0x6b, 0x28, 0x74, 0xb6, 0xeb, 0x05, 0x4d, 0x85, 0xdb, 0x74, 0x5a, 0x0f, 0xb6, 0x42,
    0x01, 0x49, 0x8b, 0x4b, 0x08, 0x4d, 0x8b, 0x43, 0x10, 0x4c, 0x89, 0xc2, 0x48, 0x29, 0xca, 0x48, 0xc1, 0xfa, 0x04, 0x48,
    0x39, 0xc2, 0x76, 0x0e, 0x48, 0xc1, 0xe0, 0x04, 0x48, 0x8b, 0x44, 0x01, 0x08, 0x48, 0x85, 0xc0, 0x75, 0x33, 0x49, 0x39,
    0xc8, 0x74, 0x2c, 0x41, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x45, 0x31, 0xc9, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x49, 0xc1, 0xe1, 0x04, 0x4a, 0x8b, 0x44, 0x09, 0x08, 0x48, 0x85, 0xc0, 0x75, 0x0f, 0x45, 0x89, 0xc1, 0x41, 0xff, 0xc0,
    0x4c, 0x39, 0xca, 0x77, 0xe7, 0xeb, 0x02, 0x31, 0xc0, 0x48, 0x83, 0xc4, 0x20, 0x5b, 0x5d, 0x5f, 0x5e, 0x41, 0x5c, 0x41,
    0x5e, 0x41, 0x5f, 0xc3, 0xe8, 0xcf, 0xf5, 0x35, 0xfe, 0xe8, 0xca, 0xde, 0x45, 0xfd, 0xcc,
};

constexpr std::size_t SITE = 0xb1;
static const uint8_t SITE_BYTES[] = {0x0f, 0xb6, 0x42, 0x01, 0x49, 0x8b, 0x4b, 0x08, 0x4d, 0x8b, 0x43, 0x10};

struct Record {
    alignas(16) uint8_t bytes[0xd08] = {};
    std::string name;
};

struct Track {
    alignas(16) uint8_t bytes[0x290] = {};
};

struct Item {
    uint64_t allocator = 0;
    uint64_t pointer = 0;
};

struct Set {
    alignas(16) uint8_t bytes[0x30] = {};
};

struct World {
    std::vector<Record *> records;
    std::vector<std::vector<Item>> items;
    std::vector<std::vector<Track *>> tracks;
    std::vector<Set> sets;
    alignas(16) uint8_t system[0x230] = {};

    ~World()
    {
        for (Record *r : records)
            delete r;
        for (auto &v : tracks)
            for (Track *t : v)
                delete t;
    }
};

static void put64(uint8_t *at, uint64_t v) { std::memcpy(at, &v, sizeof v); }

static void put32(uint8_t *at, uint32_t v) { std::memcpy(at, &v, sizeof v); }

// Stat ids 0..43 as on aober420's profile: the stock tracks with track data, null for the dormant stock rows, and
// ashton.mza955 at 43. The saved Grand Prix is on stat id 44 (mapeditor.monzaovl, not on that profile).
static const char *const AOBER_SLOTS[44] = {
    "melbourne",
    nullptr,
    "shanghai",
    "bahrain",
    "catalunya",
    "monaco",
    "montreal",
    "silverstone",
    nullptr,
    "hungaroring",
    "spa_francorchamps",
    "monza",
    "singapore",
    "suzuka",
    "abu_dhabi",
    "texas",
    "brazil",
    "austria",
    nullptr,
    "mexico",
    "baku",
    nullptr,
    nullptr,
    nullptr,
    nullptr,
    nullptr,
    "zandvoort",
    "imola",
    nullptr,
    "jeddah",
    "miami",
    "las_vegas",
    "losail",
    nullptr,
    nullptr,
    nullptr,
    nullptr,
    nullptr,
    nullptr,
    "silverstone_reverse",
    "austria_reverse",
    "zandvoort_reverse",
    "madrid",
    "mza955",
};

// Makes the TrackDataSystem: one record per named slot (TrackDataSystem +0x28 is not read by GetTrack), and per set a
// 16-byte item per slot whose TrackDataTrack points back at its record. `without` leaves those slots out of a set.
static void make(World &w, const std::vector<std::vector<int>> &without, const std::vector<uint32_t> &modes)
{
    w.records.assign(44, nullptr);
    for (int i = 0; i < 44; ++i) {
        if (!AOBER_SLOTS[i])
            continue;
        Record *r = new Record;
        r->name = AOBER_SLOTS[i];
        r->bytes[0] = 1;
        r->bytes[tl::RECORD_STAT_ID] = uint8_t(i);
        put64(r->bytes + tl::RECORD_NAME, reinterpret_cast<uint64_t>(r->name.c_str()));
        w.records[std::size_t(i)] = r;
    }
    std::size_t n = modes.size();
    w.items.assign(n, std::vector<Item>(44));
    w.tracks.assign(n, std::vector<Track *>(44, nullptr));
    w.sets.assign(n, Set{});
    for (std::size_t s = 0; s < n; ++s) {
        for (int i = 0; i < 44; ++i) {
            bool left_out = false;
            for (int x : without[s])
                left_out = left_out || x == i;
            if (!w.records[std::size_t(i)] || left_out)
                continue;
            Track *t = new Track;
            put64(t->bytes + tl::TRACK_RECORD, reinterpret_cast<uint64_t>(w.records[std::size_t(i)]->bytes));
            w.tracks[s][std::size_t(i)] = t;
            w.items[s][std::size_t(i)].pointer = reinterpret_cast<uint64_t>(t->bytes);
        }
        uint8_t *set = w.sets[s].bytes;
        put64(set + tl::SET_ITEMS_BEGIN, reinterpret_cast<uint64_t>(w.items[s].data()));
        put64(set + tl::SET_ITEMS_END, reinterpret_cast<uint64_t>(w.items[s].data() + w.items[s].size()));
        put32(set + 0x20, modes[s]);
        put32(set + 0x24, (1u << 7) | (1u << 8));
        put32(set + 0x28, 3u);
    }
    put64(w.system + 0x68, reinterpret_cast<uint64_t>(w.sets.data()));
    put64(w.system + 0x70, reinterpret_cast<uint64_t>(w.sets.data() + w.sets.size()));
}

static uint64_t set_of(World &w, std::size_t s) { return reinterpret_cast<uint64_t>(w.sets[s].bytes); }

static uint64_t track_of(World &w, std::size_t s, int stat)
{
    return reinterpret_cast<uint64_t>(w.tracks[s][std::size_t(stat)]->bytes);
}

static uint64_t record_of(World &w, int stat) { return reinterpret_cast<uint64_t>(w.records[std::size_t(stat)]->bytes); }

static void check_pick()
{
    World w;
    make(w, {{}, {}}, {1u | (1u << 13), 1u << 20});
    tl::Pick p = tl::pick(set_of(w, 1), host_read, nullptr);
    expect(p.record == record_of(w, 0) && p.stat_id == 0 && p.name == "melbourne" && p.items == 44,
           "pick: aober420's 44 slots give melbourne's record (stat id 0)");
    expect(has(tl::line(p), "reads melbourne (stat id 0) of its track data set (44 track slot(s))"),
           "pick: the log line names melbourne and the slot count");

    World no_melbourne;
    make(no_melbourne, {{}, {0}}, {1u, 1u << 20});
    p = tl::pick(set_of(no_melbourne, 1), host_read, nullptr);
    expect(p.record == record_of(no_melbourne, 2) && p.stat_id == 2 && p.name == "shanghai",
           "pick: a set without melbourne gives its first track (shanghai, stat id 2)");

    World empty;
    make(empty, {{}, {}}, {1u, 1u << 20});
    empty.items[1].clear();
    put64(empty.sets[1].bytes + tl::SET_ITEMS_BEGIN, 0);
    put64(empty.sets[1].bytes + tl::SET_ITEMS_END, 0);
    p = tl::pick(set_of(empty, 1), host_read, nullptr);
    expect(!p.record && p.items == 0, "pick: an empty set gives no record");
    expect(has(tl::line(p), "reads the first track of its track data set (0 track slot(s), none readable)"),
           "pick: the log line for an empty set says GetTrack picks");

    uint8_t bad[0x30] = {};
    put64(bad + tl::SET_ITEMS_BEGIN, 0x100008);
    put64(bad + tl::SET_ITEMS_END, 0x100000);
    expect(!tl::pick(reinterpret_cast<uint64_t>(bad), host_read, nullptr).record, "pick: items end before begin gives no record");
    put64(bad + tl::SET_ITEMS_END, 0x100008 + 0x18);
    expect(!tl::pick(reinterpret_cast<uint64_t>(bad), host_read, nullptr).record,
           "pick: items not 16 bytes apart give no record");
    put64(bad + tl::SET_ITEMS_END, 0x100008 + 0x10 * 0x101);
    expect(!tl::pick(reinterpret_cast<uint64_t>(bad), host_read, nullptr).record, "pick: more than 256 items give no record");
    expect(!tl::pick(0, host_read, nullptr).record, "pick: no set gives no record");

    World unnamed;
    make(unnamed, {{}, {}}, {1u, 1u << 20});
    put64(unnamed.records[0]->bytes + tl::RECORD_NAME, 0);
    p = tl::pick(set_of(unnamed, 1), host_read, nullptr);
    expect(p.record == record_of(unnamed, 0) && p.name.empty(), "pick: a first record with no name is still the first track");

    tl::Pick q;
    expect(tl::record_at_site(record_of(w, 11), set_of(w, 1), 0x1234, host_read, nullptr, q) == record_of(w, 11) && !q.record,
           "site: a caller's own record is left alone");
    expect(tl::record_at_site(0, set_of(w, 1), 0x1234, host_read, nullptr, q) == record_of(w, 0),
           "site: a null record becomes melbourne's");
    expect(tl::record_at_site(0, set_of(empty, 1), 0x1234, host_read, nullptr, q) == 0x1234,
           "site: an empty set gets the placeholder");
}

#ifdef REAL_BYTES

using GetTrack = uint64_t(__attribute__((ms_abi)) *)(void *system, void *record, const uint8_t *query);

static const uint8_t PLACEHOLDER[2] = {0, tl::PLACEHOLDER_STAT_ID};

struct Trap {
    bool guard = false;
    int hits = 0;
    uint64_t rdx = 0, r11 = 0;
    bool null_record = false;
    tl::Pick pick;
} trap;

static uint64_t code_page = 0;

static void on_fault(int sig, siginfo_t *info, void *raw)
{
    auto *uc = static_cast<ucontext_t *>(raw);
    greg_t *r = uc->uc_mcontext.gregs;
    uint64_t rip = uint64_t(r[REG_RIP]);
    char text[512];
    int n = std::snprintf(text, sizeof text,
                          "FAIL: signal %d at rip 0x%llx (GetTrack +0x%llx) reading 0x%llx; rax 0x%llx rcx 0x%llx rdx 0x%llx "
                          "r8 0x%llx r11 0x%llx rsp 0x%llx; trap hits %d\n",
                          sig, (unsigned long long)rip,
                          (unsigned long long)(code_page && rip - code_page < 0x1000 ? rip - code_page : ~0ull),
                          (unsigned long long)info->si_addr, (unsigned long long)r[REG_RAX], (unsigned long long)r[REG_RCX],
                          (unsigned long long)r[REG_RDX], (unsigned long long)r[REG_R8], (unsigned long long)r[REG_R11],
                          (unsigned long long)r[REG_RSP], trap.hits);
    if (n > 0 && write(1, text, std::size_t(n)) < 0)
        _exit(3);
    _exit(2);
}

static void on_trap(int, siginfo_t *, void *raw)
{
    auto *uc = static_cast<ucontext_t *>(raw);
    greg_t *r = uc->uc_mcontext.gregs;
    trap.hits++;
    trap.rdx = uint64_t(r[REG_RDX]);
    trap.r11 = uint64_t(r[REG_R11]);
    uint64_t record = trap.rdx;
    if (trap.guard)
        record = tl::record_at_site(trap.rdx, trap.r11, reinterpret_cast<uint64_t>(PLACEHOLDER), host_read, nullptr, trap.pick);
    if (!record) {
        trap.null_record = true;
        record = reinterpret_cast<uint64_t>(PLACEHOLDER);
    }
    r[REG_RDX] = greg_t(record);
    r[REG_RAX] = greg_t(*reinterpret_cast<const uint8_t *>(record + tl::RECORD_STAT_ID));
    r[REG_RIP] = greg_t(uint64_t(r[REG_RIP]) - 1 + 4);
}

struct Call {
    uint64_t result = 0;
    bool null_record = false;
    int hits = 0;
    uint64_t rdx = 0, r11 = 0;
    tl::Pick pick;
};

static Call call(GetTrack fn, World &w, uint64_t record, const uint8_t (&query)[6], bool guard)
{
    trap = Trap{};
    trap.guard = guard;
    Call c;
    c.result = fn(w.system, reinterpret_cast<void *>(record), query);
    c.null_record = trap.null_record;
    c.hits = trap.hits;
    c.rdx = trap.rdx;
    c.r11 = trap.r11;
    c.pick = trap.pick;
    return c;
}

static void check_bytes(const char *build, const uint8_t *bytes, std::size_t size)
{
    std::string b = build;
    expect(size == 0x127 && std::memcmp(bytes + SITE, SITE_BYTES, sizeof SITE_BYTES) == 0,
           (b + ": the site pattern of site_Trk_TrackData_SetMatched is at GetTrack +0xb1").c_str());

    void *page = mmap(nullptr, 0x1000, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        expect(false, (b + ": map the code").c_str());
        return;
    }
    std::memcpy(page, bytes, size);
    static_cast<uint8_t *>(page)[SITE] = 0xcc;
    if (mprotect(page, 0x1000, PROT_READ | PROT_EXEC) != 0) {
        expect(false, (b + ": make the code executable").c_str());
        munmap(page, 0x1000);
        return;
    }
    auto fn = reinterpret_cast<GetTrack>(page);
    code_page = reinterpret_cast<uint64_t>(page);

    World w;
    make(w, {{}, {}}, {1u | (1u << 13), 1u << 20});
    const uint8_t swofty_f1[6] = {20, 1, 7, 1, 0, 1}, f1[6] = {0, 1, 7, 1, 0, 1}, nobody[6] = {5, 1, 7, 1, 0, 1};

    Call c = call(fn, w, record_of(w, 11), swofty_f1, true);
    expect(!c.null_record && c.hits == 1 && c.r11 == set_of(w, 1) && c.result == track_of(w, 1, 11),
           (b + ": monza's record reads monza's track of the set whose modes hold swofty_f1, r11 = that set").c_str());

    c = call(fn, w, 0, swofty_f1, false);
    expect(c.null_record && c.hits == 1 && c.rdx == 0 && c.r11 == set_of(w, 1),
           (b + ": without the guard a null record reaches the site with rdx 0 (aober420's crash, reading 0x1)").c_str());

    c = call(fn, w, 0, swofty_f1, true);
    expect(!c.null_record && c.result == track_of(w, 1, 0) && c.pick.name == "melbourne",
           (b + ": with the guard a null record reads melbourne's track of the matched set").c_str());

    c = call(fn, w, 0, f1, true);
    expect(!c.null_record && c.r11 == set_of(w, 0) && c.result == track_of(w, 0, 0),
           (b + ": a null record on another mode's set reads that set's melbourne").c_str());

    c = call(fn, w, 0, nobody, true);
    expect(!c.null_record && c.hits == 0 && c.result == 0,
           (b + ": no matching set returns null before the site, the game's own answer").c_str());

    World no_melbourne;
    make(no_melbourne, {{}, {0}}, {1u, 1u << 20});
    c = call(fn, no_melbourne, 0, swofty_f1, true);
    expect(!c.null_record && c.result == track_of(no_melbourne, 1, 2),
           (b + ": a set without melbourne reads its first track (shanghai)").c_str());

    World empty;
    make(empty, {{}, {}}, {1u, 1u << 20});
    put64(empty.sets[1].bytes + tl::SET_ITEMS_BEGIN, 0);
    put64(empty.sets[1].bytes + tl::SET_ITEMS_END, 0);
    c = call(fn, empty, 0, swofty_f1, true);
    expect(!c.null_record && c.hits == 1 && c.result == 0,
           (b + ": an empty matched set gets the placeholder and GetTrack returns null without a fault").c_str());

    World holes;
    make(holes, {{}, {}}, {1u, 1u << 20});
    for (Item &it : holes.items[1])
        it.pointer = 0;
    c = call(fn, holes, 0, swofty_f1, true);
    expect(!c.null_record && c.result == 0, (b + ": a matched set of null items gets the placeholder and reads null").c_str());

    c = call(fn, w, reinterpret_cast<uint64_t>(PLACEHOLDER), swofty_f1, false);
    expect(!c.null_record && c.result == track_of(w, 1, 0),
           (b + ": the placeholder's stat id 0xff is past the set and GetTrack falls back to its first track").c_str());

    munmap(page, 0x1000);
}

static void check_real_bytes()
{
    struct sigaction sa{};
    sa.sa_sigaction = on_trap;
    sa.sa_flags = SA_SIGINFO;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGTRAP, &sa, nullptr);
    struct sigaction fault{};
    fault.sa_sigaction = on_fault;
    fault.sa_flags = SA_SIGINFO;
    sigemptyset(&fault.sa_mask);
    for (int sig : {SIGSEGV, SIGBUS, SIGILL, SIGFPE})
        sigaction(sig, &fault, nullptr);
    check_bytes("Steam 26566308", STEAM_26566308, sizeof STEAM_26566308);
    check_bytes("KB d873a1c8", KB_D873A1C8, sizeof KB_D873A1C8);
    expect(std::memcmp(STEAM_26566308, KB_D873A1C8, 0x11d) == 0, "the two builds' GetTrack differ only in the assert calls");
}

#endif

int main()
{
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    check_pick();
#ifdef REAL_BYTES
    check_real_bytes();
#else
    std::printf("skip: the game's bytes run only on an x86-64 Linux host\n");
#endif
    if (failed) {
        std::printf("%d failure(s)\n", failed);
        return 1;
    }
    std::printf("all track data lookup checks passed\n");
    return 0;
}
