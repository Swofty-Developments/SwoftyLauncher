// Wine test for the 1.6 handler guard (src/core/hooks/guard.cpp) built the way native/build.sh builds it:
// zig c++ -target x86_64-windows-gnu -fms-extensions. The host hooks test swaps in a C++ try/catch
// Platform, so nothing else runs the real __try/__except frames. Each case runs in its own process
// (guard_test.exe <case>) because a failing case takes the process down. Cases: a clean handler; an
// access violation with no C++ locals; an access violation in a handler that has C++ locals to destroy
// (every SDK handler does: std::function and swofty::events::Context); a libc++ throw with and without
// locals; an MSVC-code exception; an SDK-shaped after handler (std::function + a Context-like local)
// that faults; blamed_call letting the fault reach an outer frame.
#include <windows.h>

#include <atomic>
#include <cstdio>
#include <cstring>
#include <functional>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include <swofty/hooks.hpp>
#include <swofty/services.hpp>

#include "../../src/core/hooks/guard.hpp"

using namespace swofty_core::hooks::guard;

static void say(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    fflush(stdout);
}

static int fine(void *, SwoftyHookCall *)
{
    return 5;
}

static int av_plain(void *, SwoftyHookCall *)
{
    volatile int *p = nullptr;
    *p = 1;
    return 7;
}

__attribute__((noinline)) static void poke(size_t n)
{
    volatile size_t *p = nullptr;
    *p = n;
}

static int av_locals(void *, SwoftyHookCall *)
{
    std::string s(64, 'x');
    std::vector<int> v(16, 1);
    poke(s.size() + v.size());
    return 7;
}

static int throw_plain(void *, SwoftyHookCall *)
{
    throw 42;
}

static int throw_locals(void *, SwoftyHookCall *)
{
    return swofty::detail::guarded_entry([] {
        std::string s(64, 'x');
        std::vector<int> v(16, 1);
        if (s.size() + v.size())
            throw std::runtime_error("stoi");
        return 7;
    });
}

static int msvc_code(void *, SwoftyHookCall *)
{
    RaiseException(MSVC_CXX, 0, 0, nullptr);
    return 7;
}

struct FakeContext {
    explicit FakeContext(const void *h) : handle(h) {}
    const void *handle;
    std::optional<std::string> content;
    std::optional<std::vector<std::string>> participants;
};

struct Payload {
    uint32_t size, payload_version;
    int value;
};

static int sdk_after_entry(void *user, SwoftyHookCall *call)
{
    (*static_cast<std::function<void(const Payload &, const FakeContext &)> *>(user))(
        *static_cast<const Payload *>(call->payload), FakeContext(call->context));
    return 0;
}

static int blamed;

struct ServiceTable {
    static constexpr const char *name = "t.service";
    static constexpr uint32_t version = 1;
    uint32_t size;
};

static std::string blamed_what;

static void blame(const void *, unsigned long, uintptr_t, const char *what)
{
    blamed++;
    blamed_what = what ? what : "";
}

static int outer(SwoftyHookFn fn)
{
    __try {
        SwoftyHookCall c{sizeof c, 0, "t.point", nullptr, 0, nullptr};
        blamed_call(fn, nullptr, &c, blame, nullptr);
        return 0;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return 1;
    }
}

static bool isolated(SwoftyHookFn fn, void *user, unsigned long want, const char *what = nullptr)
{
    Payload pl{sizeof pl, 1, 3};
    SwoftyHookCall c{sizeof c, 2, "t.point", &pl, 0, nullptr};
    Seen s{0, 0, {}};
    int r = isolated_call(fn, user, &c, &s);
    printf("      returned %d, seen code 0x%08lx, what '%s'\n", r, s.code, s.what);
    if (what && strcmp(s.what, what))
        return false;
    bool caught = false;
    try {
        throw std::runtime_error("after");
    } catch (const std::exception &) {
        caught = true;
    }
    return r == 0 && s.code == want && caught;
}

namespace swofty::mem::guarded {
bool readable(uintptr_t p, size_t n);
bool copy(void *out, uintptr_t from, size_t n);
}

static bool mem_reads()
{
    namespace g = swofty::mem::guarded;
    static const uint64_t value = 0x1122334455667788ull;
    uint64_t got = 0;
    bool good = g::readable(uintptr_t(&value), 8) && g::copy(&got, uintptr_t(&value), 8) && got == value;
    bool null_refused = !g::readable(0, 8) && !g::copy(&got, 0, 8);
    void *freed = VirtualAlloc(nullptr, 0x2000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    VirtualFree(freed, 0, MEM_RELEASE);
    bool freed_refused = !g::readable(uintptr_t(freed), 8) && !g::copy(&got, uintptr_t(freed), 8);
    auto *span = static_cast<char *>(VirtualAlloc(nullptr, 0x3000, MEM_RESERVE, PAGE_NOACCESS));
    VirtualAlloc(span, 0x1000, MEM_COMMIT, PAGE_READWRITE);
    bool span_checked = g::readable(uintptr_t(span), 0x1000) && !g::readable(uintptr_t(span) + 0xff8, 16) &&
                        !g::readable(uintptr_t(span), 0x2000);
    auto *guarded_page = static_cast<char *>(VirtualAlloc(nullptr, 0x1000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE | PAGE_GUARD));
    bool guard_refused = !g::readable(uintptr_t(guarded_page), 8);
    MEMORY_BASIC_INFORMATION m{};
    VirtualQuery(guarded_page, &m, sizeof m);
    bool guard_kept = (m.Protect & PAGE_GUARD) != 0;
    bool flag_clear = !reading_now();
    uint32_t faults = read_fault_count();
    printf("      good %d, null %d, freed %d, span %d, guard refused %d kept %d, flag clear %d, faults %u\n", good, null_refused,
           freed_refused, span_checked, guard_refused, guard_kept, flag_clear, faults);
    return good && null_refused && freed_refused && span_checked && guard_refused && guard_kept && flag_clear && faults >= 5;
}

static LONG CALLBACK pass_on(EXCEPTION_POINTERS *)
{
    return EXCEPTION_CONTINUE_SEARCH;
}

struct Stress {
    char *edge;
    unsigned rounds;
    unsigned crossing, refused, good, read_back;
};

static void stress_body(Stress *a)
{
    namespace g = swofty::mem::guarded;
    static const size_t sizes[] = {1, 2, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 128, 255, 256, 257, 1000, 4096};
    static char out[8192];
    for (unsigned r = 0; r < a->rounds; r++) {
        for (size_t n : sizes) {
            for (size_t back : {size_t(1), size_t(3), size_t(8), n / 2, n - 1}) {
                if (!back || back >= n)
                    continue;
                uintptr_t from = uintptr_t(a->edge) - back;
                a->crossing += 2;
                a->refused += !g::copy(out, from, n);
                a->refused += !g::readable(from, n);
            }
            uintptr_t inside = uintptr_t(a->edge) - n - 16;
            a->good += g::copy(out, inside, n) && g::readable(inside, n);
            a->read_back += !memcmp(out, reinterpret_cast<const void *>(inside), n);
        }
    }
}

static DWORD WINAPI stress_thread(void *p)
{
    stress_body(static_cast<Stress *>(p));
    return 0;
}

static void *main_fiber;

static void WINAPI stress_fiber(void *p)
{
    stress_body(static_cast<Stress *>(p));
    SwitchToFiber(main_fiber);
}

static bool mem_stress()
{
    void *veh = AddVectoredExceptionHandler(1, pass_on);
    auto *span = static_cast<char *>(VirtualAlloc(nullptr, 0x4000, MEM_RESERVE, PAGE_NOACCESS));
    VirtualAlloc(span, 0x2000, MEM_COMMIT, PAGE_READWRITE);
    memset(span, 0x5a, 0x2000);
    char *edge = span + 0x2000;
    Stress runs[9] = {};
    HANDLE threads[8];
    for (int i = 0; i < 8; i++) {
        runs[i] = {edge, 60, 0, 0, 0, 0};
        threads[i] = CreateThread(nullptr, 0, stress_thread, &runs[i], 0, nullptr);
    }
    WaitForMultipleObjects(8, threads, TRUE, INFINITE);
    runs[8] = {edge, 20, 0, 0, 0, 0};
    main_fiber = ConvertThreadToFiber(nullptr);
    void *f = CreateFiber(64 * 1024, stress_fiber, &runs[8]);
    SwitchToFiber(f);
    DeleteFiber(f);
    ConvertFiberToThread();
    RemoveVectoredExceptionHandler(veh);
    bool ok = !reading_now();
    for (auto &r : runs) {
        printf("      crossing %u refused %u good %u read back %u\n", r.crossing, r.refused, r.good, r.read_back);
        ok = ok && r.crossing && r.refused == r.crossing && r.good == r.rounds * 21 && r.read_back == r.good;
    }
    return ok;
}

static std::atomic<uintptr_t> grower_limit{0};
static std::atomic<bool> grower_done{false};
static std::atomic<unsigned> grower_caught{0};

static int fault_here()
{
    volatile int *p = nullptr;
    return *p;
}

static __attribute__((noinline)) void grow(unsigned depth)
{
    volatile char pad[3000];
    pad[0] = char(depth);
    pad[sizeof pad - 1] = char(depth);
    grower_limit.store(uintptr_t(reinterpret_cast<NT_TIB *>(NtCurrentTeb())->StackLimit), std::memory_order_relaxed);
    if (depth) {
        grow(depth - 1);
        return;
    }
    __try {
        fault_here();
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        grower_caught.fetch_add(1, std::memory_order_relaxed);
    }
}

static DWORD WINAPI grower(void *)
{
    for (unsigned i = 0; i < 4000; i++)
        grow(8 + (i * 7) % 200);
    grower_done.store(true);
    return 0;
}

static bool mem_stack_guard()
{
    namespace g = swofty::mem::guarded;
    HANDLE t = CreateThread(nullptr, 0, grower, nullptr, 0, nullptr);
    unsigned probes = 0, readable = 0;
    while (!grower_done.load()) {
        uintptr_t limit = grower_limit.load(std::memory_order_relaxed);
        if (!limit)
            continue;
        for (uintptr_t page = limit - 0x3000; page <= limit; page += 0x1000) {
            readable += g::readable(page, 1);
            probes++;
        }
    }
    WaitForSingleObject(t, INFINITE);
    printf("      probes %u, readable %u, grower caught %u of 4000\n", probes, readable, grower_caught.load());
    return grower_caught.load() == 4000 && probes > 0;
}

int main(int argc, char **argv)
{
    const char *k = argc > 1 ? argv[1] : "";
    bool ok = false;
    if (!strcmp(k, "mem_reads")) {
        ok = mem_reads();
        say(ok, "mem_reads: guarded reads pass good memory, refuse null, freed, reserved and guard pages, and keep the guard armed");
    } else if (!strcmp(k, "mem_stack_guard")) {
        ok = mem_stack_guard();
        say(ok, "mem_stack_guard: probing another thread's stack guard pages leaves its stack growth and exception unwinding intact");
    } else if (!strcmp(k, "mem_stress")) {
        ok = mem_stress();
        say(ok, "mem_stress: reads that fault part way through a copy are refused on 8 threads and a fiber with a vectored handler installed");
    } else if (!strcmp(k, "fine")) {
        SwoftyHookCall c{sizeof c, 2, "t.point", nullptr, 0, nullptr};
        Seen s{0, 0, {}};
        ok = isolated_call(fine, nullptr, &c, &s) == 5 && !s.code;
        say(ok, "fine: a clean handler's result passes through");
    } else if (!strcmp(k, "av_plain")) {
        ok = isolated(av_plain, nullptr, EXCEPTION_ACCESS_VIOLATION);
        say(ok, "av_plain: an access violation with no C++ locals is isolated");
    } else if (!strcmp(k, "av_locals")) {
        ok = isolated(av_locals, nullptr, EXCEPTION_ACCESS_VIOLATION);
        say(ok, "av_locals: an access violation in a handler with C++ locals is isolated");
    } else if (!strcmp(k, "throw_plain")) {
        ok = isolated(throw_plain, nullptr, GCC_CXX);
        say(ok, "throw_plain: a libc++ throw with no locals is isolated");
    } else if (!strcmp(k, "throw_locals")) {
        ok = isolated(throw_locals, nullptr, HANDLER_THREW, "stoi");
        say(ok, "throw_locals: a libc++ throw from a handler with C++ locals, through the SDK's guarded entry, is isolated");
    } else if (!strcmp(k, "msvc_code")) {
        ok = isolated(msvc_code, nullptr, MSVC_CXX);
        say(ok, "msvc_code: an MSVC C++ exception code is isolated");
    } else if (!strcmp(k, "sdk_av")) {
        auto *fn = new std::function<void(const Payload &, const FakeContext &)>(
            [](const Payload &p, const FakeContext &) { poke(size_t(p.value)); });
        ok = isolated(sdk_after_entry, fn, EXCEPTION_ACCESS_VIOLATION);
        say(ok, "sdk_av: an SDK-shaped after handler (std::function + Context) that faults is isolated");
    } else if (!strcmp(k, "sdk_throw")) {
        auto *fn = new std::function<void(const Payload &, const swofty::hooks::Outcome &)>(
            [](const Payload &, const swofty::hooks::Outcome &) {
                std::string local(64, 'y');
                throw std::runtime_error("handler fault");
            });
        ok = isolated(&swofty::hooks::detail::after_entry<Payload>, fn, HANDLER_THREW, "handler fault");
        say(ok, "sdk_throw: an SDK after handler (hooks::after) that throws is isolated and its what() reaches the core");
    } else if (!strcmp(k, "sdk_service_throw")) {
        static const ServiceTable table{sizeof(ServiceTable)};
        auto *fn =
            new std::function<void(const ServiceTable &)>([](const ServiceTable &) { throw std::out_of_range("at"); });
        Seen s{0, 0, {}};
        bool done = isolated_service(&swofty::services::detail::when_entry<ServiceTable>, fn, &table, sizeof table, 1, &s);
        printf("      returned %d, seen code 0x%08lx, what '%s'\n", done, s.code, s.what);
        ok = !done && s.code == HANDLER_THREW && !strcmp(s.what, "at");
        say(ok, "sdk_service_throw: a services::when callback that throws is isolated");
    } else if (!strcmp(k, "sdk_blamed")) {
        auto *fn = new std::function<bool(Payload &)>([](Payload &) -> bool { throw std::runtime_error("replace fault"); });
        blamed = 0;
        int r = 0;
        __try {
            Payload pl{sizeof pl, 1, 3};
            SwoftyHookCall c{sizeof c, 1, "t.point", &pl, 0, nullptr};
            blamed_call(&swofty::hooks::detail::replace_entry<Payload>, fn, &c, blame, nullptr);
        } __except (EXCEPTION_EXECUTE_HANDLER) {
            r = 1;
        }
        ok = r == 1 && blamed == 1 && blamed_what == "replace fault";
        say(ok, "sdk_blamed: a throwing SDK replace handler is named (with what()) and takes the crash path");
    } else if (!strcmp(k, "blamed")) {
        blamed = 0;
        ok = outer(av_plain) == 1 && blamed == 1;
        blamed = 0;
        ok = ok && outer(fine) == 0 && blamed == 0;
        say(ok, "blamed: blamed_call names the handler and lets the fault reach the outer frame");
    } else {
        printf("usage: guard_test.exe fine|av_plain|av_locals|throw_plain|throw_locals|msvc_code|sdk_av|sdk_throw|"
               "sdk_service_throw|sdk_blamed|blamed\n");
        return 2;
    }
    return ok ? 0 : 1;
}
