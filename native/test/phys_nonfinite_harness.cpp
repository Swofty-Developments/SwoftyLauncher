// The physics non-finite log on the game's own code, under Wine (run_phys_nonfinite_test.sh). The harness maps one
// F1_25.exe image (a PE file, or a flat dump whose file offset is the RVA) at its image base, finds the functions by
// their binding patterns in its executable sections, hooks the game's Ne_PhysUpdateStage_FinishStepNotifyBodies with
// safetyhook as the core's wrap mixin does, and runs it on a world it builds: the game's code takes its step-done path
// (state 7 -> 0), flushes its substep records and walks its constraint list, then the core's checker reads the bodies.
// The game's Ne_PhysBody_ApplyImpulse and its two velocity setters write the values that are not finite. Then it times
// the walk with 500 and 5000 bodies.
//
//   phys_nonfinite_harness.exe <label> <image> <pe|flat> <patterns.tsv>

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <map>
#include <random>
#include <string>
#include <vector>

#include <safetyhook.hpp>

#include "core/guards/engine/phys_nonfinite_logic.hpp"

namespace pn = runtime::phys_nonfinite;

namespace {

constexpr uintptr_t IMAGE_BASE = 0x140000000;
constexpr size_t BODY_SIZE = 0x4b0, WORLD_SIZE = 0x1200;
constexpr uintptr_t WORLD_COLLISION = 0x30, WORLD_CONSTRAINTS = 0x40, COLLISION_WORLD = 0x10e0, COLLISION_THREADS = 0xf58;
constexpr uintptr_t COLLISION_PENDING = 0xf4c, COLLISION_BLOCKS = 0xf60, STAGE_NOTIFY = 0x69;
constexpr uintptr_t BODY_FLAGS = 0x46c, BODY_MAX_SPEED = 0x400, BODY_AXIS_LIMITS = 0x3f4, BODY_CENTRE = 0x378;
constexpr uintptr_t BODY_INERTIA = 0xd0, BODY_ROTATION = 0x150, BODY_TRANSFORM = 0x190;
constexpr uint32_t NO_LIMIT = 0x5f7fffff;
constexpr uint32_t STATE_DONE = 7;

int fails = 0;
const char *label = "";

void expect(bool ok, const char *what)
{
    std::printf("%s: %s: %s\n", ok ? "ok  " : "FAIL", label, what);
    std::fflush(stdout);
    fails += !ok;
}

std::vector<std::string> lines;

void sink(const std::string &line) { lines.push_back(line); }

pn::Owner no_owner(uintptr_t)
{
    pn::Owner o;
    o.table = true;
    return o;
}

pn::Checker *checker = nullptr;

using FinishStep = void(void *);
safetyhook::InlineHook hook;

void finish_step(void *stage)
{
    uintptr_t world = *reinterpret_cast<const uintptr_t *>(reinterpret_cast<uintptr_t>(stage) + pn::STAGE_WORLD);
    bool pending = world && *reinterpret_cast<const uint32_t *>(world + pn::WORLD_STATE) != 0;
    hook.call<void>(stage);
    if (!pending)
        return;
    uint64_t ticks = 0;
    checker->step(world, ticks);
}

struct Pattern {
    std::vector<int> bytes;
};

Pattern parse(const std::string &text)
{
    Pattern p;
    for (size_t i = 0; i < text.size();) {
        if (text[i] == ' ') {
            i++;
            continue;
        }
        if (text[i] == '?') {
            p.bytes.push_back(-1);
            while (i < text.size() && text[i] == '?')
                i++;
            continue;
        }
        p.bytes.push_back(int(std::strtoul(text.substr(i, 2).c_str(), nullptr, 16)));
        i += 2;
    }
    return p;
}

std::vector<uintptr_t> scan(const uint8_t *from, size_t size, const Pattern &p)
{
    std::vector<uintptr_t> hits;
    size_t n = p.bytes.size();
    for (size_t i = 0; i + n <= size; i++) {
        if (from[i] != p.bytes[0])
            continue;
        size_t k = 1;
        while (k < n && (p.bytes[k] < 0 || from[i + k] == p.bytes[k]))
            k++;
        if (k == n)
            hits.push_back(reinterpret_cast<uintptr_t>(from + i));
    }
    return hits;
}

bool map_image(const char *path, bool flat, uint8_t *&base, size_t &size)
{
    FILE *f = std::fopen(path, "rb");
    if (!f)
        return false;
    std::fseek(f, 0, SEEK_END);
    size_t file = size_t(_ftelli64(f));
    std::fseek(f, 0, SEEK_SET);
    std::vector<uint8_t> raw(file);
    bool ok = std::fread(raw.data(), 1, file, f) == file;
    std::fclose(f);
    if (!ok || file < 0x1000)
        return false;
    auto *dos = reinterpret_cast<IMAGE_DOS_HEADER *>(raw.data());
    auto *nt = reinterpret_cast<IMAGE_NT_HEADERS64 *>(raw.data() + dos->e_lfanew);
    size = flat ? file : nt->OptionalHeader.SizeOfImage;
    base = static_cast<uint8_t *>(
        VirtualAlloc(reinterpret_cast<void *>(IMAGE_BASE), size, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE));
    if (base != reinterpret_cast<uint8_t *>(IMAGE_BASE)) {
        std::printf("FAIL: %s: VirtualAlloc at 0x%llx for 0x%zx bytes gave %p (error %lu)\n", label,
                    (unsigned long long)IMAGE_BASE, size, static_cast<void *>(base), GetLastError());
        return false;
    }
    if (flat) {
        std::memcpy(base, raw.data(), file);
        return true;
    }
    std::memcpy(base, raw.data(), nt->OptionalHeader.SizeOfHeaders);
    auto *sec = IMAGE_FIRST_SECTION(nt);
    for (unsigned i = 0; i < nt->FileHeader.NumberOfSections; i++) {
        size_t n =
            std::min<size_t>(sec[i].SizeOfRawData, sec[i].Misc.VirtualSize ? sec[i].Misc.VirtualSize : sec[i].SizeOfRawData);
        if (sec[i].PointerToRawData + n <= file)
            std::memcpy(base + sec[i].VirtualAddress, raw.data() + sec[i].PointerToRawData, n);
    }
    return true;
}

struct Section {
    std::string name;
    uint8_t *from;
    size_t size;
};

std::vector<Section> code_sections(uint8_t *base)
{
    std::vector<Section> out;
    auto *dos = reinterpret_cast<IMAGE_DOS_HEADER *>(base);
    auto *nt = reinterpret_cast<IMAGE_NT_HEADERS64 *>(base + dos->e_lfanew);
    auto *sec = IMAGE_FIRST_SECTION(nt);
    for (unsigned i = 0; i < nt->FileHeader.NumberOfSections; i++) {
        if (!(sec[i].Characteristics & IMAGE_SCN_MEM_EXECUTE))
            continue;
        char n[9] = {};
        std::memcpy(n, sec[i].Name, 8);
        out.push_back({n, base + sec[i].VirtualAddress, std::max(sec[i].Misc.VirtualSize, sec[i].SizeOfRawData)});
    }
    return out;
}

void *aligned(size_t size, size_t align = 64)
{
    void *p = VirtualAlloc(nullptr, size + align, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    return reinterpret_cast<void *>((reinterpret_cast<uintptr_t>(p) + align - 1) & ~(align - 1));
}

template <class T> void put(uintptr_t at, T v) { std::memcpy(reinterpret_cast<void *>(at), &v, sizeof v); }

template <class T> T get(uintptr_t at)
{
    T v;
    std::memcpy(&v, reinterpret_cast<const void *>(at), sizeof v);
    return v;
}

void put4(uintptr_t at, float x, float y, float z, float w)
{
    float v[4] = {x, y, z, w};
    std::memcpy(reinterpret_cast<void *>(at), v, sizeof v);
}

struct Scene {
    uintptr_t stage = 0, world = 0, collision = 0, constraint = 0;
    std::vector<uintptr_t> bodies, handles;
    uint8_t *slab = nullptr;

    Scene(size_t n, bool shuffled)
    {
        stage = reinterpret_cast<uintptr_t>(aligned(0x100));
        world = reinterpret_cast<uintptr_t>(aligned(WORLD_SIZE));
        collision = reinterpret_cast<uintptr_t>(aligned(WORLD_SIZE));
        constraint = reinterpret_cast<uintptr_t>(aligned(0x400));
        put(stage + pn::STAGE_WORLD, world);
        put<uint8_t>(stage + STAGE_NOTIFY, 1);
        put(world + WORLD_COLLISION, collision);
        put(world + WORLD_CONSTRAINTS, constraint);
        put<float>(world + pn::WORLD_STEP_TIME, 1.f / 60.f);
        put(collision + COLLISION_WORLD, world);
        put<uint32_t>(collision + COLLISION_THREADS, 1);
        put<uint32_t>(collision + COLLISION_PENDING, 0);
        put<uintptr_t>(collision + COLLISION_BLOCKS, 0);
        slab = static_cast<uint8_t *>(aligned(n * BODY_SIZE + 0x100 * n));
        for (size_t i = 0; i < n; i++) {
            uintptr_t b = reinterpret_cast<uintptr_t>(slab) + i * BODY_SIZE;
            uintptr_t h = reinterpret_cast<uintptr_t>(slab) + n * BODY_SIZE + i * 0x100;
            uintptr_t centre = h + 0x40;
            for (int r = 0; r < 3; r++) {
                put4(b + BODY_ROTATION + 0x10 * r, r == 0, r == 1, r == 2, 0);
                put4(b + BODY_TRANSFORM + 0x10 * r, r == 0, r == 1, r == 2, 0);
                put4(b + BODY_INERTIA + 0x10 * r, r == 0 ? 2.f : 0, r == 1 ? 2.f : 0, r == 2 ? 2.f : 0, 0);
            }
            put4(b + BODY_INERTIA + 0x30, 0, 0, 0, 0);
            put4(b + pn::BODY_POSITION, float(i), 0.3f, 2.f * float(i), 1);
            put4(centre, float(i), 0.3f, 2.f * float(i), 1);
            put4(b + pn::BODY_LINEAR, 20.f, 0, 70.f, 0);
            put4(b + pn::BODY_ANGULAR, 0, 0.25f, 0, 0);
            put<float>(b + pn::BODY_INV_MASS, 1.f / 798.f);
            put<uint32_t>(b + BODY_FLAGS, 3);
            put<uint32_t>(b + BODY_MAX_SPEED, NO_LIMIT);
            for (int a = 0; a < 3; a++)
                put<uint32_t>(b + BODY_AXIS_LIMITS + 4 * a, NO_LIMIT);
            put(b + BODY_CENTRE, centre);
            put(b + pn::BODY_HANDLE, h);
            put(h + pn::HANDLE_BODY, b);
            bodies.push_back(b);
            handles.push_back(h);
        }
        std::vector<uintptr_t> order = bodies;
        if (shuffled)
            std::shuffle(order.begin(), order.end(), std::mt19937_64(26566308));
        else
            std::reverse(order.begin(), order.end());
        for (size_t i = 0; i < order.size(); i++)
            put<uintptr_t>(order[i] + pn::BODY_NEXT, i + 1 < order.size() ? order[i + 1] : 0);
        put<uintptr_t>(world + pn::WORLD_BODIES, order.empty() ? 0 : order[0]);
    }

    std::vector<uint8_t> snapshot() const
    {
        std::vector<uint8_t> out(slab, slab + bodies.size() * BODY_SIZE);
        auto *w = reinterpret_cast<const uint8_t *>(world);
        out.insert(out.end(), w, w + WORLD_SIZE);
        return out;
    }
};

struct Game {
    FinishStep *finish = nullptr;
    void (*impulse)(void *, const float *, const float *) = nullptr;
    void (*set_angular)(void *, const float *) = nullptr;
    void (*set_linear)(void *, const float *) = nullptr;
};

void run_step(Game &g, Scene &s)
{
    put<uint32_t>(s.world + pn::WORLD_STATE, STATE_DONE);
    g.finish(reinterpret_cast<void *>(s.stage));
}

size_t lines_with(const std::string &needle)
{
    size_t n = 0;
    for (auto &l : lines)
        n += l.find(needle) != std::string::npos;
    return n;
}

void proofs(Game &g)
{
    Scene s(40, false);
    pn::Checker c{&sink, &no_owner};
    checker = &c;
    lines.clear();
    auto before = s.snapshot();
    run_step(g, s);
    expect(get<uint32_t>(s.world + pn::WORLD_STATE) == 0,
           "the game's FinishStepNotifyBodies takes its step-done path (state 7 to 0)");
    expect(c.steps() == 1 && lines.size() == 1 && lines_with("finished its first step (step 1) with 40 bodies") == 1,
           "the checker runs after the game's code and logs the world's first step once");
    auto after = s.snapshot();
    uint32_t zero = 0;
    std::memcpy(before.data() + s.bodies.size() * BODY_SIZE + pn::WORLD_STATE, &zero, sizeof zero);
    expect(after == before, "a finite step changes no body byte and no world byte but the game's own state write (7 to 0)");

    g.finish(reinterpret_cast<void *>(s.stage));
    expect(c.steps() == 1, "with no step pending (state 0) the checker does not read the bodies");

    const float nan = std::numeric_limits<float>::quiet_NaN(), inf = std::numeric_limits<float>::infinity();
    alignas(16) float j_nan[4] = {nan, 0, 0, 0}, at[4] = {0, 0, 0, 1};
    put<uint32_t>(s.bodies[3] + BODY_FLAGS, 1);
    g.impulse(reinterpret_cast<void *>(s.bodies[3]), j_nan, at);
    expect(pn::fields(s.bodies[3]) == pn::linear,
           "the game's ApplyImpulse (linear bit) puts the NaN in linear velocity +0x220 only");
    run_step(g, s);
    expect(lines.size() == 2 && lines_with("body 36 of 40") == 1 && lines_with("linear velocity is not finite") == 1,
           "the body is logged once, naming linear velocity");
    alignas(16) float w_inf[4] = {inf, 0, 0, 0};
    g.set_angular(reinterpret_cast<void *>(s.handles[11]), w_inf);
    expect(pn::fields(s.bodies[11]) == pn::angular, "the game's +0x210 setter puts the infinity in angular velocity only");
    run_step(g, s);
    expect(lines.size() == 3 && lines_with("angular velocity is not finite; position (11, 0.3, 22)") == 1,
           "the angular velocity body is logged once");
    alignas(16) float v_ninf[4] = {0, -inf, 0, 0};
    g.set_linear(reinterpret_cast<void *>(s.handles[20]), v_ninf);
    expect(pn::fields(s.bodies[20]) == pn::linear, "the game's +0x220 setter puts minus infinity in linear velocity only");
    put<uint32_t>(s.bodies[25] + BODY_FLAGS, 2);
    g.impulse(reinterpret_cast<void *>(s.bodies[25]), j_nan, at);
    expect(pn::fields(s.bodies[25]) & pn::angular, "the game's ApplyImpulse (angular bit) puts the NaN in angular velocity");
    put<float>(s.bodies[30] + pn::BODY_POSITION + 4, nan);
    run_step(g, s);
    expect(lines.size() == 6 && lines_with("body 19 of 40") == 1 && lines_with("body 14 of 40") == 1 &&
               lines_with("body 9 of 40") == 1,
           "three new bodies in one step are each logged once");
    expect(lines_with("position is not finite") == 1, "a position NaN names the position");
    expect(lines.back().find("5 bodies in this step are not finite") != std::string::npos, "the step counts every such body");
    auto known = s.snapshot();
    for (int i = 0; i < 50; i++)
        run_step(g, s);
    auto later = s.snapshot();
    expect(lines.size() == 6, "fifty more steps add no line");
    expect(known == later, "steps over bodies that are not finite change no byte (the world's state goes 7 to 0 each step)");
    for (auto &l : lines)
        std::printf("      %s\n", l.c_str());
}

double qpc_ns()
{
    static LARGE_INTEGER f{};
    if (!f.QuadPart)
        QueryPerformanceFrequency(&f);
    LARGE_INTEGER q;
    QueryPerformanceCounter(&q);
    return double(q.QuadPart) * 1e9 / double(f.QuadPart);
}

double ns_per_tick()
{
    double q0 = qpc_ns();
    uint64_t t0 = __builtin_ia32_rdtsc();
    Sleep(200);
    double q1 = qpc_ns();
    uint64_t t1 = __builtin_ia32_rdtsc();
    return (q1 - q0) / double(t1 - t0);
}

volatile uint64_t sink_v;

void evict(std::vector<uint8_t> &big)
{
    uint64_t s = 0;
    for (size_t i = 0; i < big.size(); i += 64) {
        big[i]++;
        s += big[i];
    }
    sink_v = s;
}

struct Timing {
    double game, hooked, walk, walk_max;
};

enum class Cache { warm, touched, cold };

const char *cache_name(Cache c) { return c == Cache::warm ? "warm" : c == Cache::touched ? "solver-touched" : "cold"; }

// Before a step: nothing (warm: the bodies stay in cache from the last step), the 64 MB eviction then the game's
// ApplyImpulse with a zero impulse on each body in list order (touched: as the solver writes each body's velocity and
// reads its limits just before the update finishes), or the eviction alone (cold: every body read misses to memory).
void prepare(Game &g, Scene &s, Cache c, std::vector<uint8_t> &big)
{
    if (c == Cache::warm)
        return;
    evict(big);
    if (c == Cache::cold)
        return;
    alignas(16) static const float zero[4] = {}, at[4] = {0, 0, 0, 1};
    for (uintptr_t b = get<uintptr_t>(s.world + pn::WORLD_BODIES); b; b = get<uintptr_t>(b + pn::BODY_NEXT))
        g.impulse(reinterpret_cast<void *>(b), zero, at);
}

Timing time_steps(Game &g, Scene &s, int steps, Cache cache, std::vector<uint8_t> &big, double npt)
{
    pn::Checker c{&sink, &no_owner};
    checker = &c;
    std::vector<double> game, hooked;
    (void)hook.disable();
    for (int i = 0; i < steps; i++) {
        prepare(g, s, cache, big);
        double t0 = qpc_ns();
        run_step(g, s);
        game.push_back(qpc_ns() - t0);
    }
    (void)hook.enable();
    c.take();
    for (int i = 0; i < steps; i++) {
        prepare(g, s, cache, big);
        double t0 = qpc_ns();
        run_step(g, s);
        hooked.push_back(qpc_ns() - t0);
    }
    pn::Window w = c.take();
    std::sort(game.begin(), game.end());
    std::sort(hooked.begin(), hooked.end());
    return {game[game.size() / 2], hooked[hooked.size() / 2], double(w.ticks) * npt / double(w.steps), double(w.max_ticks) * npt};
}

void perf(Game &g)
{
    double npt = ns_per_tick();
    std::vector<uint8_t> big(64u << 20);
    std::printf("perf: %s: timestamp counter %.3f ns per tick; game/hooked = median wall time per step "
                "(QueryPerformanceCounter, 100 ns steps under Wine), walk = the checker's own rdtsc time per step\n",
                label, npt);
    for (size_t n : {size_t(500), size_t(5000)})
        for (bool shuffled : {false, true})
            for (Cache cache : {Cache::warm, Cache::touched, Cache::cold}) {
                Scene s(n, shuffled);
                int steps = cache == Cache::warm ? 2000 : 200;
                Timing t = time_steps(g, s, steps, cache, big, npt);
                std::printf("perf: %s: n=%zu %s %s: game step %.0f ns, hooked %.0f ns; checker walk %.0f ns per step (max "
                            "%.0f ns) = %.1f ns per body; at 60 steps/s %.1f us per s (%.3f%% of a frame), at 240 steps/s "
                            "%.1f us per s\n",
                            label, n, shuffled ? "shuffled" : "pool-order", cache_name(cache), t.game, t.hooked, t.walk,
                            t.walk_max, t.walk / double(n), t.walk * 60 / 1000, t.walk / 16666666.7 * 100, t.walk * 240 / 1000);
            }
    {
        Scene s(500, false);
        pn::Checker c{&sink, &no_owner};
        checker = &c;
        put<float>(s.bodies[100] + pn::BODY_LINEAR, std::numeric_limits<float>::quiet_NaN());
        lines.clear();
        run_step(g, s);
        c.take();
        for (int i = 0; i < 2000; i++)
            run_step(g, s);
        pn::Window w = c.take();
        std::printf("perf: %s: n=500 with one logged body that stays NaN: checker %.0f ns per step (the walk and one pass "
                    "that skips the logged body)\n",
                    label, double(w.ticks) * npt / double(w.steps));
    }
}

}

int main(int argc, char **argv)
{
    if (argc < 5) {
        std::printf("usage: %s <label> <image> <pe|flat> <patterns.tsv>\n", argv[0]);
        return 2;
    }
    label = argv[1];
    uint8_t *base = nullptr;
    size_t size = 0;
    if (!map_image(argv[2], !std::strcmp(argv[3], "flat"), base, size)) {
        std::printf("FAIL: %s: could not map %s\n", label, argv[2]);
        return 1;
    }
    std::vector<Section> code = code_sections(base);
    std::map<std::string, uintptr_t> at;
    FILE *f = std::fopen(argv[4], "r");
    char line[1024];
    while (f && std::fgets(line, sizeof line, f)) {
        std::string l = line;
        while (!l.empty() && (l.back() == '\n' || l.back() == '\r'))
            l.pop_back();
        size_t tab = l.find('\t');
        if (tab == std::string::npos)
            continue;
        std::string name = l.substr(0, tab);
        Pattern p = parse(l.substr(tab + 1));
        std::vector<uintptr_t> hits;
        std::string where;
        for (const Section &c : code)
            for (uintptr_t h : scan(c.from, c.size, p)) {
                hits.push_back(h);
                where = c.name;
            }
        char what[200];
        std::snprintf(what, sizeof what, "%s matches once in the executable sections (%zu hit%s, rva 0x%llx in %s)", name.c_str(),
                      hits.size(), hits.size() == 1 ? "" : "s",
                      hits.size() == 1 ? (unsigned long long)(hits[0] - IMAGE_BASE) : 0ull,
                      hits.size() == 1 ? where.c_str() : "-");
        expect(hits.size() == 1, what);
        if (hits.size() == 1)
            at[name] = hits[0];
    }
    if (f)
        std::fclose(f);
    Game g;
    g.finish = reinterpret_cast<FinishStep *>(at["Ne_PhysUpdateStage_FinishStepNotifyBodies"]);
    g.impulse = reinterpret_cast<void (*)(void *, const float *, const float *)>(at["Ne_PhysBody_ApplyImpulse"]);
    g.set_angular = reinterpret_cast<void (*)(void *, const float *)>(at["Ne_PhysBody_SetLinearVelocity"]);
    g.set_linear = reinterpret_cast<void (*)(void *, const float *)>(at["Ne_PhysBody_SetAngularVelocity"]);
    if (!g.finish || !g.impulse || !g.set_angular || !g.set_linear)
        return 1;
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(g.finish), reinterpret_cast<void *>(&finish_step));
    expect(bool(h), "safetyhook hooks the game's FinishStepNotifyBodies as the core's wrap mixin does");
    if (!h)
        return 1;
    hook = std::move(*h);
    proofs(g);
    perf(g);
    std::printf("%s: %s\n", fails ? "FAILED" : "all passed", label);
    return fails ? 1 : 0;
}
