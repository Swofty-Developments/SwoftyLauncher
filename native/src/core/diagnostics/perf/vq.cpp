#include "core/diagnostics/perf/vq.hpp"

#include "core/diagnostics/perf/counters.hpp"
#include "core/runtime/imports.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <psapi.h>
#include <tlhelp32.h>

#include <algorithm>
#include <atomic>
#include <cwctype>

namespace perf::vq {

using namespace detail;

namespace {

struct Mod {
    uintptr_t base = 0, size = 0;
    std::string label;
    bool exe = false;
    int slots = 0;
};

struct alignas(64) Cell {
    U64 calls, ticks, game_calls, game_ticks;
};

struct Site {
    std::atomic<uintptr_t> at;
    U64 calls, ticks;
};

struct Made {
    std::atomic<DWORD> tid;
    std::atomic<uintptr_t> at;
};

using VqFn = SIZE_T(WINAPI *)(LPCVOID, PMEMORY_BASIC_INFORMATION, SIZE_T);
using ThreadFn = unsigned(__stdcall *)(void *);
using BeginFn = uintptr_t(__cdecl *)(void *, unsigned, ThreadFn, void *, unsigned, unsigned *);
using NtQitFn = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);

Mod mods[MODS];
std::atomic<int> mod_count{0};
std::atomic<uint32_t> mod_gen{0};
Cell cells[MODS + 1][SHARDS];
Site sites[SITES];
U64 site_spill_calls{0}, site_spill_ticks{0}, worst{0};
Made made[MADE];
VqFn real_vq, real_vq_base;
BeginFn real_begin;
NtQitFn nt_qit;
bool on, exe_hooked;
std::wstring root, game_dir;
thread_local uint32_t kind_gen;
thread_local bool kind_ours;

int mod_of(uintptr_t a)
{
    int n = mod_count.load(std::memory_order_acquire);
    for (int i = 0; i < n; i++)
        if (a - mods[i].base < mods[i].size)
            return i;
    return -1;
}

bool made_by_us(DWORD tid, uintptr_t *at)
{
    for (Made &m : made)
        if (m.tid.load(rlx) == tid) {
            if (at)
                *at = m.at.load(rlx);
            return true;
        }
    return false;
}

uintptr_t start_of(HANDLE thread)
{
    uintptr_t s = 0;
    if (!nt_qit || nt_qit(thread, 9, &s, sizeof s, nullptr) < 0)
        return 0;
    return s;
}

bool swofty_code(uintptr_t a)
{
    int i = mod_of(a);
    return i >= 0 && !mods[i].exe;
}

bool ours_thread()
{
    uint32_t g = mod_gen.load(rlx);
    if (kind_gen == g)
        return kind_ours;
    DWORD err = GetLastError();
    kind_ours = made_by_us(GetCurrentThreadId(), nullptr) || swofty_code(start_of(GetCurrentThread()));
    kind_gen = g;
    SetLastError(err);
    return kind_ours;
}

void count(uintptr_t ret, uint64_t dt)
{
    int i = mod_of(ret);
    bool exe = i >= 0 && mods[i].exe;
    bool game = exe || !ours_thread();
    Cell &c = cells[i < 0 ? MODS : i][(GetCurrentThreadId() >> 2) & (SHARDS - 1)];
    c.calls.fetch_add(1, rlx);
    c.ticks.fetch_add(dt, rlx);
    if (game) {
        c.game_calls.fetch_add(1, rlx);
        c.game_ticks.fetch_add(dt, rlx);
    }
    if (exe)
        return;
    raise_shared(worst, dt);
    uintptr_t site = ret;
    size_t h = logic::site_slot(site, SITE_BITS);
    for (size_t k = 0; k < 16; k++) {
        Site &s = sites[(h + k) & (SITES - 1)];
        uintptr_t at = s.at.load(rlx);
        if (!at && s.at.compare_exchange_strong(at, site, rlx))
            at = site;
        if (at != site)
            continue;
        s.calls.fetch_add(1, rlx);
        s.ticks.fetch_add(dt, rlx);
        return;
    }
    site_spill_calls.fetch_add(1, rlx);
    site_spill_ticks.fetch_add(dt, rlx);
}

SIZE_T WINAPI vq_hook(LPCVOID p, PMEMORY_BASIC_INFORMATION m, SIZE_T n)
{
    const void *ret = __builtin_return_address(0);
    uint64_t t0 = tsc();
    SIZE_T r = real_vq(p, m, n);
    uint64_t dt = tsc() - t0;
    DWORD err = GetLastError();
    count(uintptr_t(ret), dt);
    SetLastError(err);
    return r;
}

struct Start {
    ThreadFn fn;
    void *arg;
    uintptr_t at;
};

unsigned __stdcall thread_entry(void *p)
{
    Start s = *static_cast<Start *>(p);
    delete static_cast<Start *>(p);
    DWORD tid = GetCurrentThreadId();
    Made *slot = nullptr;
    for (Made &m : made) {
        DWORD free_ = 0;
        if (m.tid.load(rlx) == 0 && m.tid.compare_exchange_strong(free_, tid, rlx)) {
            m.at.store(s.at, rlx);
            slot = &m;
            break;
        }
    }
    unsigned r = s.fn(s.arg);
    if (slot) {
        slot->at.store(0, rlx);
        slot->tid.store(0, rlx);
    }
    return r;
}

uintptr_t __cdecl begin_hook(void *sec, unsigned stack, ThreadFn fn, void *arg, unsigned flags, unsigned *tid)
{
    if (!fn)
        return real_begin(sec, stack, fn, arg, flags, tid);
    void *frames[2] = {};
    USHORT n = RtlCaptureStackBackTrace(1, 2, frames, nullptr);
    uintptr_t at = n >= 2 && swofty_code(uintptr_t(frames[1])) ? uintptr_t(frames[1]) : uintptr_t(__builtin_return_address(0));
    auto *s = new (std::nothrow) Start{fn, arg, at};
    if (!s)
        return real_begin(sec, stack, fn, arg, flags, tid);
    uintptr_t h = real_begin(sec, stack, thread_entry, s, flags, tid);
    if (!h)
        delete s;
    return h;
}

std::atomic<bool> import_skip_logged{false};

int patch(const Mod &m, const char *fn, const void *const *match, int matches, void *repl)
{
    pe_patch p{};
    p.fn = fn;
    p.match = match;
    p.matches = matches;
    p.repl = repl;
    p.write = runtime::import_write;
    int done = pe_patch_imports(m.base, runtime::import_readable, nullptr, &p);
    if (p.skipped)
        note_import_skip(m.label.c_str(), p.first_skip, p.skipped);
    return done;
}

std::wstring lower(std::wstring s)
{
    for (auto &c : s)
        c = wchar_t(std::towlower(c));
    return s;
}

std::string narrow(const std::wstring &w)
{
    std::string out;
    for (wchar_t c : w)
        out += c < 128 ? char(c) : '?';
    return out;
}

bool hook(HMODULE h, const std::wstring &path, bool self)
{
    int n = mod_count.load(rlx);
    if (n >= MODS)
        return false;
    MODULEINFO mi{};
    if (!K32GetModuleInformation(GetCurrentProcess(), h, &mi, sizeof mi))
        return false;
    uintptr_t base = uintptr_t(mi.lpBaseOfDll);
    for (int i = 0; i < n; i++)
        if (mods[i].base == base)
            return false;
    bool exe = h == GetModuleHandleW(nullptr);
    std::wstring file = path.substr(path.find_last_of(L"\\/") + 1);
    Mod &m = mods[n];
    m.base = base;
    m.size = mi.SizeOfImage;
    m.exe = exe;
    m.label = self ? "core" : exe ? "game exe" : narrow(lower(file));
    mod_count.store(n + 1, std::memory_order_release);
    mod_gen.fetch_add(1, rlx);
    const void *vqs[] = {reinterpret_cast<void *>(real_vq), reinterpret_cast<void *>(real_vq_base)};
    m.slots = patch(m, "VirtualQuery", vqs, 2, reinterpret_cast<void *>(&vq_hook));
    if (!exe && real_begin) {
        const void *begins[] = {reinterpret_cast<void *>(real_begin)};
        patch(m, "_beginthreadex", begins, 1, reinterpret_cast<void *>(&begin_hook));
    }
    exe_hooked |= exe && m.slots;
    return true;
}

HMODULE self_module()
{
    HMODULE self = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&vq_hook), &self);
    return self;
}

std::string site_label(uintptr_t at)
{
    int i = mod_of(at);
    return i < 0 ? logic::rva_label("?", at) : logic::rva_label(mods[i].label, at - mods[i].base);
}

uint64_t ft(FILETIME f) { return (uint64_t(f.dwHighDateTime) << 32) | f.dwLowDateTime; }

}

void note_import_skip(const char *module, const pe_import &first, int skipped)
{
    runtime::log_import_skip(import_skip_logged, "[perf]", module, first, skipped);
}

void install()
{
    if (HMODULE nt = GetModuleHandleW(L"ntdll.dll"))
        nt_qit = reinterpret_cast<NtQitFn>(GetProcAddress(nt, "NtQueryInformationThread"));
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_PERF_VQ", v, sizeof v);
    if (n && n < sizeof v && v[0] == '0') {
        swofty::log("[perf] VirtualQuery accounting off (SWOFTY_PERF_VQ=0)");
        return;
    }
    HMODULE k32 = GetModuleHandleW(L"kernel32.dll"), kb = GetModuleHandleW(L"kernelbase.dll");
    real_vq = k32 ? reinterpret_cast<VqFn>(GetProcAddress(k32, "VirtualQuery")) : nullptr;
    real_vq_base = kb ? reinterpret_cast<VqFn>(GetProcAddress(kb, "VirtualQuery")) : nullptr;
    if (!real_vq)
        real_vq = real_vq_base;
    if (HMODULE crt = GetModuleHandleW(L"ucrtbase.dll"))
        real_begin = reinterpret_cast<BeginFn>(GetProcAddress(crt, "_beginthreadex"));
    if (!real_vq)
        return;
    on = true;
    wchar_t path[MAX_PATH * 2] = {};
    HMODULE self = self_module();
    GetModuleFileNameW(self, path, MAX_PATH * 2);
    if (hook(self, path, true))
        swofty::log("%s", logic::vq_core_line(mods[0].slots, real_begin != nullptr).c_str());
}

bool enabled() { return on; }

void scan()
{
    if (!on)
        return;
    if (root.empty()) {
        root = lower(runtime::swofty_root());
        size_t cut = root.find_last_of(L"\\/", root.size() >= 2 ? root.size() - 2 : 0);
        game_dir = cut == std::wstring::npos ? std::wstring() : root.substr(0, cut + 1);
    }
    HMODULE hs[1024];
    DWORD need = 0;
    if (!K32EnumProcessModules(GetCurrentProcess(), hs, sizeof hs, &need))
        return;
    HMODULE exe = GetModuleHandleW(nullptr);
    std::string added;
    for (DWORD i = 0; i < std::min<DWORD>(need / sizeof(HMODULE), 1024); i++) {
        wchar_t path[MAX_PATH * 2] = {};
        if (!GetModuleFileNameW(hs[i], path, MAX_PATH * 2))
            continue;
        std::wstring p = lower(path);
        bool ours = !root.empty() && p.rfind(root, 0) == 0;
        bool loader = !game_dir.empty() && p == game_dir + L"version.dll";
        if (hs[i] != exe && !ours && !loader)
            continue;
        if (!hook(hs[i], path, false))
            continue;
        const Mod &m = mods[mod_count.load(rlx) - 1];
        added += (added.empty() ? "" : ", ") + m.label + " (" + std::to_string(m.slots) + ")";
    }
    if (!added.empty())
        swofty::log("%s", logic::vq_modules_line(added).c_str());
}

logic::VqMinute minute(Last &last, double secs)
{
    logic::VqMinute out;
    out.secs = secs;
    out.exe_hooked = exe_hooked;
    int n = mod_count.load(std::memory_order_acquire);
    for (int i = 0; i <= MODS; i++) {
        if (i < MODS && i >= n)
            continue;
        uint64_t now[4] = {};
        for (Cell &c : cells[i]) {
            now[0] += c.calls.load(rlx);
            now[1] += c.ticks.load(rlx);
            now[2] += c.game_calls.load(rlx);
            now[3] += c.game_ticks.load(rlx);
        }
        uint64_t d[4];
        for (int k = 0; k < 4; k++)
            d[k] = logic::delta(now[k], last.cell[i][k]), last.cell[i][k] = now[k];
        if (i < MODS && mods[i].exe) {
            out.exe_calls = d[0];
            out.exe_ms = ms_of(d[1]);
            continue;
        }
        out.calls += d[0];
        out.ms += ms_of(d[1]);
        out.game_calls += d[2];
        out.game_ms += ms_of(d[3]);
        out.modules.push_back({i < MODS ? mods[i].label : "other", d[0], ms_of(d[1])});
    }
    for (size_t k = 0; k < SITES; k++) {
        uintptr_t at = sites[k].at.load(rlx);
        if (!at)
            continue;
        uint64_t c = sites[k].calls.load(rlx), t = sites[k].ticks.load(rlx);
        uint64_t dc = logic::delta(c, last.site[k][0]), dt = logic::delta(t, last.site[k][1]);
        last.site[k][0] = c, last.site[k][1] = t;
        if (dc)
            out.sites.push_back({site_label(at), dc, ms_of(dt)});
    }
    uint64_t sc = site_spill_calls.load(rlx), st = site_spill_ticks.load(rlx);
    if (uint64_t dc = logic::delta(sc, last.spill[0]))
        out.sites.push_back({"other sites", dc, ms_of(logic::delta(st, last.spill[1]))});
    last.spill[0] = sc, last.spill[1] = st;
    out.worst_ms = ms_of(worst.exchange(0, rlx));
    swofty::mem::shared::Counts pages = swofty::mem::shared::counts();
    out.page_resident = logic::delta(pages.resident, last.page_resident);
    out.page_region = logic::delta(pages.region, last.page_region);
    last.page_resident = pages.resident, last.page_region = pages.region;
    return out;
}

logic::CpuMinute cpu(Cpu &st, double secs)
{
    logic::CpuMinute out;
    out.secs = secs;
    FILETIME c, e, k, u;
    if (GetProcessTimes(GetCurrentProcess(), &c, &e, &k, &u)) {
        out.user = logic::cores(logic::delta(ft(u), st.user), secs);
        out.kernel = logic::cores(logic::delta(ft(k), st.kernel), secs);
        st.user = ft(u), st.kernel = ft(k);
    }
    FILETIME sampled;
    GetSystemTimeAsFileTime(&sampled);
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snap == INVALID_HANDLE_VALUE)
        return out;
    for (auto &[tid, s] : st.seen)
        s.alive = false;
    DWORD pid = GetCurrentProcessId();
    uint64_t ou = 0, ok = 0;
    THREADENTRY32 te{};
    te.dwSize = sizeof te;
    for (BOOL more = Thread32First(snap, &te); more; more = Thread32Next(snap, &te)) {
        if (te.th32OwnerProcessID != pid)
            continue;
        DWORD tid = te.th32ThreadID;
        HANDLE h = OpenThread(THREAD_QUERY_INFORMATION, FALSE, tid);
        if (!h)
            h = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, tid);
        if (!h)
            continue;
        if (GetThreadTimes(h, &c, &e, &k, &u)) {
            auto [it, fresh] = st.seen.try_emplace(tid);
            Seen &s = it->second;
            if (fresh || s.created != ft(c)) {
                s = Seen();
                s.created = ft(c);
                uintptr_t at = 0;
                if (made_by_us(tid, &at)) {
                    s.ours = true;
                    s.what = site_label(at) + " (tid " + std::to_string(tid) + ", via _beginthreadex)";
                } else if (uintptr_t start = start_of(h); swofty_code(start)) {
                    s.ours = true;
                    s.what = site_label(start) + " (tid " + std::to_string(tid) + ")";
                }
                if (!st.primed || s.created < st.at)
                    s.user = ft(u), s.kernel = ft(k);
            }
            s.alive = true;
            uint64_t du = logic::delta(ft(u), s.user), dk = logic::delta(ft(k), s.kernel);
            if (s.ours) {
                ou += du, ok += dk;
                out.threads++;
                out.top.push_back({s.what, logic::cores(du + dk, secs)});
            } else if (double c = logic::cores(du + dk, secs); c > out.game_top) {
                out.game_top = c;
                out.game_top_what = "tid " + std::to_string(tid);
            }
            s.user = ft(u), s.kernel = ft(k);
        }
        CloseHandle(h);
    }
    CloseHandle(snap);
    std::erase_if(st.seen, [](const auto &kv) { return !kv.second.alive; });
    out.ours_user = logic::cores(ou, secs);
    out.ours_kernel = logic::cores(ok, secs);
    st.primed = true;
    st.at = ft(sampled);
    return out;
}

}
