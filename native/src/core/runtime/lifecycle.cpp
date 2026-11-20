#include "core/runtime/lifecycle.hpp"

#include <cstdio>
#include <cstring>
#include <mutex>
#include <optional>
#include <span>

#include "core/diagnostics/steps.hpp"
#include "core/hooks/hooks.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/runtime/addon_api.hpp"
#include "core/runtime/loader/bindings.hpp"
#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/hook_pool.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/mixin_manager.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "core/runtime/loader/scanner.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/stage.hpp"
#include "core/services/services.hpp"

extern "C" __declspec(dllexport) int SwoftyCoreAttach(SwoftyApi *a);

namespace runtime {

namespace {

using namespace loader;

constexpr int RETRIES = 8;
constexpr DWORD RETRY_MS = 1000;
constexpr const char *RUNTIME_SETUP = "runtime setup (assets, resources, gamedb, dbread, overlay, lang, events, hud, telemetry)";
constexpr const char *ON_WORKER = " (on the arm worker)";

void run(const Subsystem &s, Start point, const char *suffix)
{
    if (point == Start::attach || point == Start::ready) {
        perf::Stopwatch step;
        s.install();
        loader::api->log(perf::STEP_LINE, step.total_ms(), (std::string("subsystem ") + s.name + suffix).c_str());
    } else
        when_code_ready(s.name, s.install);
}

// The subsystems from the one at open, marked Probe::open, to the next marked Probe::close or the end.
std::span<const Subsystem> probe_block(std::span<const Subsystem> all, size_t open)
{
    size_t end = open;
    while (end + 1 < all.size() && all[end].probe != Probe::close)
        end++;
    return all.subspan(open, end - open + 1);
}

void run_block(std::span<const Subsystem> block, Start point, const char *suffix)
{
    std::optional<render::d3d12::ProbeScope> probe;
    for (const Subsystem &s : block) {
        if (s.start != point)
            continue;
        if (s.probe == Probe::open)
            probe.emplace();
        run(s, point, suffix);
        if (s.probe == Probe::close)
            probe.reset();
    }
}

void ready_block_on_worker()
{
    std::span<const Subsystem> all = subsystems();
    for (size_t i = 0; i < all.size(); i++)
        if (all[i].start == Start::ready && all[i].probe == Probe::open) {
            run_block(probe_block(all, i), Start::ready, ON_WORKER);
            return;
        }
}

}

Runtime &Runtime::instance()
{
    static Runtime runtime;
    return runtime;
}

void Runtime::start(Start point)
{
    std::span<const Subsystem> all = subsystems();
    for (size_t i = 0; i < all.size(); i++) {
        const Subsystem &s = all[i];
        if (s.start != point)
            continue;
        if (s.probe != Probe::open) {
            run(s, point, "");
            continue;
        }
        std::span<const Subsystem> block = probe_block(all, i);
        i += block.size() - 1;
        perf::Stopwatch step;
        bool early = runtime::load_stage() == "early" && !runtime::window_reached();
        if (point == Start::ready && render::d3d12::arm_on_worker(ready_block_on_worker, early))
            loader::api->log(
                perf::STEP_LINE, step.total_ms(),
                (std::string("subsystems ") + block.front().name + " to " + block.back().name + ON_WORKER + ", scheduled")
                    .c_str());
        else
            run_block(block, point, "");
    }
}

int Runtime::attach(SwoftyApi *a)
{
    loader::api = api_ = a;
    image().attach(a->module_base());
    hook_pool().adopt(reinterpret_cast<void (*)()>(a->module_base));
    wchar_t path[MAX_PATH] = {};
    HMODULE self = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&SwoftyCoreAttach), &self);
    GetModuleFileNameW(self, path, MAX_PATH);
    dir_ = path;
    dir_.resize(dir_.find_last_of(L'\\') + 1);
    const std::wstring &dir = dir_;
    perf::Stopwatch steps;
    start(Start::attach);
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach subsystems");
    long long running_stamp = (long long)image().nt_headers()->FileHeader.TimeDateStamp;
    BindingSet &set = bindings();
    json::Value doc = set.load(dir, running_stamp);
    scanner().read_section_map(doc, set.file(), set.section_uses(), !set.empty());
    set.log_loaded(doc);
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach bindings load");
    long long kb_size = doc["build"]["size"].num(-1), exe_size = -1;
    wchar_t exe[MAX_PATH] = {};
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (GetModuleFileNameW(nullptr, exe, MAX_PATH) && GetFileAttributesExW(exe, GetFileExInfoStandard, &fa))
        exe_size = (long long)(((unsigned long long)fa.nFileSizeHigh << 32) | fa.nFileSizeLow);
    set.build.stamp = running_stamp, set.build.bytes = exe_size;
    if (!scanner().open(a, dir, exe_size))
        set.use_kb_addresses(false);
    set.plan_kb_addresses(kb_size, exe_size);
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach scan cache open");
    mixins().read_manifest(dir);
    publish_addon_api(a);
    mixins().add_core_sets(runtime::career_mixins());
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach addon api");
    set.build.sha = scanner().exe_sha256(exe, scanner().caches(dir) + L"exe-sha256.tsv", exe_size, running_stamp);
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach exe sha256");
    const auto &said = doc["build"]["packed"];
    auto cs = image().section_by_name(scanner().game_code());
    double ce = cs ? code().section_entropy(cs) : -1;
    code().set_packed(said.type == json::Value::Bool ? said.b : ce > CodeEncryption::ENCRYPTED);
    char cname[9] = {};
    if (cs)
        memcpy(cname, cs->Name, 8);
    loader::api->log("[core] build %s: %s (%s); code section %s entropy %.2f, reads %s", set.build.tag.c_str(),
                     code().packed() ? "packed" : "not packed",
                     said.type == json::Value::Bool ? "the set says so" : "measured at init", cname, ce < 0 ? 0 : ce,
                     CodeEncryption::reads(ce));
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach code entropy");
    {
        std::lock_guard<std::recursive_mutex> g(resolve_lock());
        set.attempt_unresolved();
        loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach binding pass");
        set.report("init");
        loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach binding report");
    }
    scanner().save();
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach scan cache save");
    start(Start::attach_code);
    loader::api->log(perf::STEP_LINE, steps.lap_ms(), "attach code subsystems");
    loader::api->log(perf::STEP_LINE, steps.total_ms(), "attach total");
    if (code().packed())
        runtime::on_stage(bind_stage);
    return 1;
}

int Runtime::init_addon(const wchar_t *name, SwoftyAddonInitFn init) { return mixins().init_addon(name, init); }

int Runtime::commit_pass()
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    mixins().offer_core_sets();
    size_t from = mixins().settled();
    size_t count[7] = {};
    mixins().install(from, count);
    if (!mixins().waiting(true))
        runtime::career_setup();
    if (!from)
        start(Start::commit_code);
    if (!mixins().waiting(true))
        mixins().settle_core_sets();
    mixins().run_on_load_from(from);
    if (mixins().offer_core_sets()) {
        loader::api->log("[core] commit: installing the core mixin sets that registrations at load asked for");
        mixins().install(mixins().settled(), count);
    }
    if (!mixins().waiting(true))
        mixins().settle_core_sets();
    mixins().close((from || runtime::load_stage() != "early") && !mixins().waiting(false));
    if (!from)
        when_code_ready(RUNTIME_SETUP, runtime::ready);
    size_t total = 0, active = mixins().active_from(from, &total);
    loader::api->log("[core] commit: %zu of %zu mixin addon(s) active; %zu wrap(s), %zu site(s), %zu vtable slot(s), %zu "
                     "redirect(s), %zu patch(es)",
                     active, total, count[SWOFTY_MIXIN_WRAP], count[SWOFTY_MIXIN_SITE], count[SWOFTY_MIXIN_VTABLE],
                     count[SWOFTY_MIXIN_REDIRECT], count[SWOFTY_MIXIN_PATCH]);
    scanner().save();
    if (pending() && runtime::load_stage() != "early")
        start_retries();
    if (from || runtime::load_stage() != "early")
        std16_window_ = true;
    return (int)active;
}

int Runtime::commit()
{
    int active = commit_pass();
    settle_std16(false);
    return active;
}

void Runtime::ready()
{
    if (!close_asset_registrations())
        return;
    start(Start::ready);
}

bool Runtime::code_decrypted(double &e)
{
    e = code().section_now(scanner().game_code());
    return e >= 0 && e <= CodeEncryption::PLAIN;
}

/* Setups that scan or hook game code without bindings wait for the whole code section, not a page, to read
   decrypted: a setup that ran on half-decrypted code would give up for good on what is still encrypted. */
size_t Runtime::run_code_tasks(const char *pass, bool final)
{
    double e;
    if (code_tasks_.empty() || (!code_decrypted(e) && !final))
        return 0;
    bool ok = e >= 0 && e <= CodeEncryption::PLAIN;
    auto tasks = std::move(code_tasks_);
    code_tasks_.clear();
    for (auto &t : tasks) {
        if (!ok) {
            loader::api->log("[core] %s: not set up; %s still reads %s (entropy %.2f) at the last retry, so it fails closed",
                             t.what.c_str(), scanner().game_code().c_str(), CodeEncryption::reads(e), e < 0 ? 0 : e);
            continue;
        }
        loader::api->log("[core] %s: %s reads decrypted (entropy %.2f) at the %s pass; setting it up now", t.what.c_str(),
                         scanner().game_code().c_str(), e, pass);
        perf::Stopwatch step;
        t.fn();
        runtime::stage_catch_up();
        loader::api->log(perf::STEP_LINE, step.total_ms(), ("code ready " + t.what + " (" + pass + " pass)").c_str());
    }
    return tasks.size();
}

void Runtime::late_pass(const char *pass, bool final)
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    std::lock_guard<std::recursive_mutex> r(resolve_lock());
    BindingSet &set = bindings();
    size_t before = set.deferred_count();
    if (!before && !mixins().waiting(false) && code_tasks_.empty()) {
        if (set.unreported())
            set.report(pass);
        return;
    }
    code().forget_samples();
    std::string sampled = set.sample_deferred_sections();
    set.retry_deferred(final);
    mixins().reinit_deferred(final);
    mixins().offer_core_sets();
    size_t count[7] = {};
    mixins().install_late(count);
    if (!mixins().waiting(true)) {
        runtime::career_setup();
        mixins().settle_core_sets();
    }
    size_t setups = run_code_tasks(pass, final);
    if (!mixins().waiting(false))
        mixins().close(true);
    size_t after = set.deferred_count(), installed = 0;
    for (size_t k : count)
        installed += k;
    if (after == before && !final && !installed && !setups && strcmp(pass, "window")) {
        loader::api->log("[core] %s pass: %zu binding(s) and %zu setup(s) still wait for their code to decrypt (%s)", pass, after,
                         code_tasks_.size(), sampled.c_str());
        return;
    }
    loader::api->log(
        "[core] %s pass: %s; %zu binding(s) came out of deferral, %zu still deferred; installed %zu wrap(s), %zu site(s), "
        "%zu vtable slot(s), %zu redirect(s), %zu patch(es)%s",
        pass, sampled.c_str(), before - after, after, count[SWOFTY_MIXIN_WRAP], count[SWOFTY_MIXIN_SITE],
        count[SWOFTY_MIXIN_VTABLE], count[SWOFTY_MIXIN_REDIRECT], count[SWOFTY_MIXIN_PATCH],
        mixins().waiting(false) ? "; some addons still wait" : "");
    set.report(pass);
    scanner().save();
}

bool Runtime::pending()
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    return bindings().deferred_count() || mixins().waiting(false) || !code_tasks_.empty();
}

void Runtime::settle_std16(bool force)
{
    if (!std16_window_.load() || std16_settled_.load() || (!force && pending()))
        return;
    if (std16_settled_.exchange(true))
        return;
    swofty_core::hooks::window_stage();
    swofty_core::services::report_missing();
}

DWORD WINAPI Runtime::retry_worker(LPVOID)
{
    Runtime &self = instance();
    for (int n = 1; n <= RETRIES && self.pending(); n++) {
        Sleep(RETRY_MS);
        char pass[16];
        snprintf(pass, sizeof pass, "stage:%d", n);
        self.late_pass(pass, n == RETRIES);
        self.settle_std16(false);
    }
    self.settle_std16(true);
    return 0;
}

void Runtime::start_retries()
{
    if (InterlockedExchange(&retries_started_, 1))
        return;
    loader::api->log("[core] bindings: %zu binding(s) and %zu setup(s) still wait for their code to decrypt; retrying every %lu "
                     "ms, %d time(s), then they fail closed",
                     bindings().deferred_count(), code_tasks_.size(), (unsigned long)RETRY_MS, RETRIES);
    if (HANDLE t = CreateThread(nullptr, 0, retry_worker, nullptr, 0, nullptr)) {
        CloseHandle(t);
    } else {
        late_pass("stage:1", true);
        settle_std16(true);
    }
}

void Runtime::bind_stage(int stage)
{
    if (stage != SWOFTY_STAGE_WINDOW)
        return;
    Runtime &self = instance();
    self.late_pass("window", false);
    if (self.pending())
        self.start_retries();
    self.settle_std16(false);
}

/* Core setups that scan or hook game code without a binding run now, or on a packed build whose code still
   reads encrypted, at the first stage pass that reads it decrypted (the retries bound it like bindings). */
void Runtime::when_code_ready(const char *what, void (*fn)())
{
    std::lock_guard<std::recursive_mutex> c(commit_lock());
    {
        std::lock_guard<std::recursive_mutex> r(resolve_lock());
        double e = 0;
        if (code().packed() && !code_decrypted(e)) {
            code_tasks_.push_back({what, fn});
            loader::api->log("[core] %s waits: %s reads %s (entropy %.2f) on this packed build; it is set up once that code "
                             "decrypts",
                             what, scanner().game_code().c_str(), CodeEncryption::reads(e), e < 0 ? 0 : e);
            return;
        }
    }
    perf::Stopwatch step;
    fn();
    runtime::stage_catch_up();
    loader::api->log(perf::STEP_LINE, step.total_ms(), (std::string("code ready ") + what).c_str());
}

/* Lazy lookups ask this before caching a miss, so a miss on code that still reads encrypted is tried again. */
bool Runtime::code_ready()
{
    if (!code().packed() || code().plain_seen())
        return true;
    std::lock_guard<std::recursive_mutex> r(resolve_lock());
    double e;
    if (code_decrypted(e))
        code().mark_plain();
    return code().plain_seen();
}

void when_code_ready(const char *what, void (*fn)()) { Runtime::instance().when_code_ready(what, fn); }

bool code_ready() { return Runtime::instance().code_ready(); }

void ready() { Runtime::instance().ready(); }

std::string build_distribution() { return loader::bindings().build.tag; }

std::string build_exe_sha() { return loader::bindings().build.sha; }

}

extern "C" __declspec(dllexport) int SwoftyCoreAttach(SwoftyApi *a) { return runtime::Runtime::instance().attach(a); }

extern "C" __declspec(dllexport) int SwoftyCoreInit(const wchar_t *name, SwoftyAddonInitFn init)
{
    return runtime::Runtime::instance().init_addon(name, init);
}

extern "C" __declspec(dllexport) int SwoftyCoreCommit(void) { return runtime::Runtime::instance().commit(); }
