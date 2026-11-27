#include "core/apply/scheduler.hpp"

#include "core/registry/table.hpp"
#include "core/apply/facts.hpp"
#include "core/diagnostics/steps.hpp"

#include <algorithm>
#include <condition_variable>
#include <cstdio>
#include <system_error>
#include <thread>

namespace swofty_core::apply {

namespace {

std::mutex list_mu;

constexpr double STEP_FLOOR_MS = 1.0;

std::string step_line(double ms, const std::string &name)
{
    char line[320];
    std::snprintf(line, sizeof line, perf::STEP_LINE, ms, name.c_str());
    return line;
}

std::vector<std::unique_ptr<Applier>> &list()
{
    static std::vector<std::unique_ptr<Applier>> l;
    return l;
}

std::string counts(const Report &r)
{
    return std::to_string(r.applied) + " applied, " + std::to_string(r.put_back) + " put back, " + std::to_string(r.refused) +
           " refused";
}

std::string counts(const Totals &t)
{
    return std::to_string(t.applied) + " applied, " + std::to_string(t.put_back) + " put back, " + std::to_string(t.refused) +
           " refused";
}

}

void add(std::unique_ptr<Applier> applier)
{
    if (!applier)
        return;
    std::lock_guard g(list_mu);
    list().push_back(std::move(applier));
}

std::vector<Applier *> appliers()
{
    std::lock_guard g(list_mu);
    std::vector<Applier *> out;
    for (auto &a : list())
        out.push_back(a.get());
    return out;
}

void clear_appliers()
{
    std::lock_guard g(list_mu);
    list().clear();
}

const char *moment_name(Moment m)
{
    switch (m) {
    case Moment::boot:
        return "boot";
    case Moment::activate:
        return "activate";
    case Moment::deactivate:
        return "put back";
    case Moment::session_load:
        return "session load";
    case Moment::session_end:
        return "session end";
    case Moment::read:
        return "read";
    }
    return "?";
}

std::string summary(const Context &c)
{
    std::string s = "profile " + c.profile;
    if (!c.career.empty())
        s += ", career " + c.career;
    if (!c.mode.empty())
        s += ", mode " + c.mode;
    if (!c.data_group.empty())
        s += ", data group " + c.data_group;
    if (!c.content.empty()) {
        s += ", content " + c.content.front();
        if (c.content.size() > 1) {
            s += " (from";
            for (std::size_t i = 1; i < c.content.size(); i++)
                s += " " + c.content[i];
            s += ")";
        }
    }
    if (c.has_year)
        s += ", year " + std::to_string(c.year);
    if (!c.setup_mode.empty())
        s += ", setup mode " + c.setup_mode;
    if (!c.session_type.empty())
        s += ", session " + c.session_type;
    if (!c.track.empty())
        s += ", track " + c.track;
    if (!c.vehicle_package.empty())
        s += ", vehicle package " + c.vehicle_package;
    return s;
}

Scheduler::Scheduler(Store &store) : store_(store) {}

void Scheduler::set_log(std::function<void(const std::string &)> fn)
{
    Lock g(mu_);
    log_ = std::move(fn);
}

void Scheduler::set_facts(std::function<void(const Context &)> fn)
{
    Lock g(mu_);
    facts_ = std::move(fn);
}

void Scheduler::log(const std::string &line)
{
    if (log_)
        log_(line);
}

void Scheduler::reset()
{
    Lock g = settled_lock();
    ctx_ = Context();
    have_ctx_ = false;
    planned_ = booted_ = read_ = session_ = loaded_from_flow_ = work_ = false;
    started_ = false;
    boot_.clear();
    boot_keys_.clear();
    boot_content_.clear();
    keys_.clear();
    active_.clear();
    active_label_.clear();
    career_year_ = 0;
    session_name_.clear();
    activated_.clear();
    sessioned_.clear();
    boot_summary_.clear();
}

Context Scheduler::context() const
{
    Lock g(mu_);
    if (!have_ctx_)
        return store_.context();
    return ctx_;
}

std::string Scheduler::active() const
{
    Lock g(mu_);
    return active_label_;
}

bool Scheduler::in_session() const
{
    Lock g(mu_);
    return session_;
}

bool Scheduler::booted() const
{
    Lock g(mu_);
    return booted_;
}

bool Scheduler::boot_planned() const
{
    Lock g(mu_);
    return planned_;
}

void Scheduler::publish(Lock &)
{
    ctx_.profile = store_.context().profile;
    have_ctx_ = true;
    store_.set_context(ctx_);
    if (facts_)
        facts_(ctx_);
}

Totals Scheduler::run(Moment m, Applies kind, const std::function<void(Applier &, Report &)> &call, std::vector<Applier *> *ran,
                      bool reverse_order, const std::vector<Applier *> *only)
{
    std::vector<Applier *> todo;
    if (only)
        todo = *only;
    else
        for (Applier *a : appliers())
            if (a->applies() == kind)
                todo.push_back(a);
    if (reverse_order)
        std::reverse(todo.begin(), todo.end());
    Totals t;
    perf::Stopwatch steps;
    for (Applier *a : todo) {
        Report r;
        steps.lap_ms();
        call(*a, r);
        double took = steps.lap_ms();
        if (m == Moment::boot || took >= STEP_FLOOR_MS)
            log(step_line(took, std::string("applier ") + a->registry() + " " + moment_name(m)));
        t.appliers++;
        t.applied += r.applied;
        t.put_back += r.put_back;
        t.refused += r.refused;
        log(std::string("[apply] ") + a->registry() + " " + moment_name(m) + ": " + counts(r));
        if (m == Moment::boot && (r.applied || !r.summary.empty()))
            boot_summary_.push_back(std::string(a->registry()) + " " + std::to_string(r.applied) +
                                    (r.summary.empty() ? std::string() : " (" + r.summary + ")"));
        for (auto &line : r.lines)
            log(std::string("[apply] ") + a->registry() + " " + moment_name(m) + ":   " + line);
        if (ran)
            ran->push_back(a);
    }
    if (!todo.empty())
        log(step_line(steps.total_ms(), std::string("appliers ") + moment_name(m) + " (" + std::to_string(todo.size()) + ")"));
    return t;
}

bool Scheduler::has_addon_content(const std::string &registry)
{
    // The core's own records are game/<key>, or <prefix>/<key> in a shared registry (add_base), such as the venues
    // it seeds at prepare; neither is addon content.
    std::string core = "game/";
    if (auto d = store_.def(registry); d && d->shared && !d->shared_prefix.empty())
        core = d->shared_prefix + "/";
    for (auto &id : store_.list(registry, ""))
        if (id.rfind(core, 0) != 0)
            return true;
    for (auto &addon : store_.order())
        if (store_.layer_count(addon, registry))
            return true;
    return false;
}

struct Scheduler::OffThread {
    std::mutex mu;
    std::condition_variable cv;
    bool done = false;
    uint64_t generation = 0;
    std::vector<std::size_t> picks, keys;
    std::string error;
};

void Scheduler::start_plan(bool off_thread)
{
    Lock g(mu_);
    if (started_)
        return;
    started_ = true;
    perf::Stopwatch steps;
    boot_.clear();
    for (Applier *a : appliers())
        if (a->applies() == Applies::boot)
            boot_.push_back(a);
    for (Applier *a : boot_)
        a->prepare(store_);
    boot_keys_.assign(boot_.size(), 0);
    boot_content_.assign(boot_.size(), 0);
    if (!off_thread) {
        count_on_thread_locked();
        log(step_line(steps.total_ms(), "boot plan on this thread (" + std::to_string(boot_.size()) + ")"));
        return;
    }
    log(step_line(steps.lap_ms(), "boot plan prepare (" + std::to_string(boot_.size()) + ")"));
    auto off = std::make_shared<OffThread>();
    std::vector<Applier *> todo;
    bool counts = false;
    for (std::size_t i = 0; i < boot_.size(); i++) {
        Applier *a = boot_[i];
        if (a->counts_off_thread()) {
            counts = a->may_count(store_) || counts;
            off->picks.push_back(i);
            todo.push_back(a);
            continue;
        }
        boot_keys_[i] = a->database_keys(store_);
        boot_content_[i] = has_addon_content(a->registry());
    }
    log(step_line(steps.lap_ms(), "boot plan counts on this thread (" + std::to_string(boot_.size() - todo.size()) + ")"));
    off->generation = store_.generation();
    auto here = [&](const char *why) {
        for (Applier *a : todo)
            off->keys.push_back(a->database_keys(store_));
        off->done = true;
        log(step_line(steps.lap_ms(),
                      std::string("boot plan counts on this thread, ") + why + " (" + std::to_string(todo.size()) + ")"));
        settle_locked(*off);
    };
    if (!counts) {
        here("none can count a key");
        return;
    }
    std::function<void(const std::string &)> say = log_;
    Store &st = store_;
    try {
        std::thread([off, todo, &st, say] {
            perf::Stopwatch took;
            try {
                for (Applier *a : todo)
                    off->keys.push_back(a->database_keys(st));
            } catch (const std::exception &e) {
                off->error = e.what();
            } catch (...) {
                off->error = "an unknown exception";
            }
            if (say)
                say(step_line(took.total_ms(), "boot plan count off the thread (" + std::to_string(todo.size()) + ")"));
            {
                std::lock_guard l(off->mu);
                off->done = true;
            }
            off->cv.notify_all();
        }).detach();
        off_ = off;
    } catch (const std::system_error &) {
        here("no worker thread");
    }
}

void Scheduler::count_on_thread_locked()
{
    for (std::size_t i = 0; i < boot_.size(); i++) {
        boot_keys_[i] = boot_[i]->database_keys(store_);
        boot_content_[i] = has_addon_content(boot_[i]->registry());
        note_count_locked(i);
    }
    close_plan_locked();
}

void Scheduler::note_count_locked(std::size_t i)
{
    std::size_t k = boot_keys_[i];
    bool content = boot_content_[i];
    keys_.push_back({boot_[i]->registry(), k});
    work_ = work_ || k || content;
    log(std::string("[apply] ") + boot_[i]->registry() + " boot: " + std::to_string(k) + " new database key(s) planned" +
        (content ? "" : "; no addon records or layers"));
}

void Scheduler::close_plan_locked()
{
    std::size_t total = 0;
    for (auto &[reg, k] : keys_)
        total += k;
    planned_ = true;
    log("[apply] boot plan: " + std::to_string(boot_.size()) + " boot applier(s), " + std::to_string(total) +
        " new database key(s) for the value map" + (work_ ? "" : "; nothing for the database to apply"));
}

void Scheduler::settle_locked(const OffThread &off)
{
    if (off_.get() == &off)
        off_.reset();
    perf::Stopwatch took;
    bool moved = store_.generation() != off.generation;
    if (moved || !off.error.empty() || off.keys.size() != off.picks.size()) {
        log(std::string("[apply] boot plan: ") +
            (moved ? std::string("the registry changed after the count off the thread started")
                   : "the count off the thread failed (" + (off.error.empty() ? std::string("no count") : off.error) + ")") +
            "; counted again on this thread");
        for (std::size_t i : off.picks)
            boot_[i]->forget_count();
        for (std::size_t i : off.picks)
            boot_keys_[i] = boot_[i]->database_keys(store_);
        log(step_line(took.lap_ms(), "boot plan count again on this thread (" + std::to_string(off.picks.size()) + ")"));
    } else
        for (std::size_t n = 0; n < off.picks.size(); n++)
            boot_keys_[off.picks[n]] = off.keys[n];
    for (std::size_t i : off.picks)
        boot_content_[i] = has_addon_content(boot_[i]->registry());
    for (std::size_t i = 0; i < boot_.size(); i++)
        note_count_locked(i);
    close_plan_locked();
}

void Scheduler::await_count()
{
    std::shared_ptr<OffThread> off;
    {
        Lock g(mu_);
        off = off_;
    }
    if (!off)
        return;
    perf::Stopwatch waited;
    {
        std::unique_lock l(off->mu);
        off->cv.wait(l, [&] { return off->done; });
    }
    double ms = waited.total_ms();
    Lock g(mu_);
    log(step_line(ms, "boot plan wait for the count off the thread"));
    if (off_ == off)
        settle_locked(*off);
}

Scheduler::Lock Scheduler::settled_lock()
{
    for (;;) {
        await_count();
        Lock g(mu_);
        if (!off_)
            return g;
    }
}

std::size_t Scheduler::plan_boot()
{
    start_plan(false);
    Lock g = settled_lock();
    std::size_t n = 0;
    for (auto &[reg, k] : keys_)
        n += k;
    return n;
}

std::size_t Scheduler::planned_keys()
{
    Lock g = settled_lock();
    std::size_t n = 0;
    for (auto &[reg, k] : keys_)
        n += k;
    return n;
}

std::size_t Scheduler::planned_keys(const std::string &registry)
{
    Lock g = settled_lock();
    for (auto &[reg, k] : keys_)
        if (reg == registry)
            return k;
    return 0;
}

bool Scheduler::boot_work()
{
    Lock g = settled_lock();
    return work_;
}

bool Scheduler::boot_may_work() const
{
    Lock g(mu_);
    return !planned_ || work_;
}

Totals Scheduler::run_boot()
{
    Lock g = settled_lock();
    if (booted_)
        return {};
    if (!planned_)
        plan_boot();
    booted_ = true;
    uint64_t from = store_.generation();
    Context was = store_.context();
    publish(g);
    boot_published_ = store_.generation();
    boot_from_ = boot_published_ == from + 1 && store_.context() == was ? from : boot_published_;
    Totals t = run(Moment::boot, Applies::boot, [&](Applier &a, Report &r) { a.boot(store_, r); }, nullptr, false, nullptr);
    log("[apply] boot: " + std::to_string(t.appliers) + " applier(s); " + counts(t) + " (" + summary(ctx_) + ")");
    if (!boot_summary_.empty()) {
        std::string line;
        for (auto &part : boot_summary_)
            line += (line.empty() ? "" : ", ") + part;
        log("[registry] applied at boot: " + line);
        boot_summary_.clear();
    }
    log_unapplied();
    return t;
}

uint64_t Scheduler::boot_generation() const
{
    Lock g(mu_);
    uint64_t now = store_.generation();
    return booted_ && now == boot_published_ ? boot_from_ : now;
}

void Scheduler::log_unapplied()
{
    static const char *const READ_BY_CORE[] = {"addon.option",     "career.character",
                                               "career.content",   "career.driver_ratings",
                                               "career.season",    "career.wizard",
                                               "content.package",  "driver.head",
                                               "driver.identity",  "driver.look",
                                               "driver.person",    "driver.stats",
                                               "engine_sound",     "era.box",
                                               "gloves",           "helmet",
                                               "hub.tab",          "lang.string",
                                               "livery",           "suit",
                                               "team.constructor", "team.identity",
                                               "team.logo",        "team.pit_times",
                                               "ui.route",         "ui.screen",
                                               "ui.tile",          "vehicle",
                                               "vehicle.package",  "wearable.season_set"};
    std::vector<std::string> names = store_.registries();
    std::sort(names.begin(), names.end());
    for (auto &name : names) {
        bool handled = false;
        for (Applier *a : appliers())
            handled = handled || name == a->registry();
        for (const char *known : READ_BY_CORE)
            handled = handled || name == known;
        if (handled)
            continue;
        std::size_t records = store_.list(name, "").size();
        if (records)
            log("[registry] " + name + ": " + std::to_string(records) + " record(s); this core applies nothing from it");
    }
}

std::vector<std::string> Scheduler::chain_of(const std::string &registry, const std::string &id)
{
    auto c = store_.chain(registry, id);
    if (c.empty())
        c.push_back(id);
    return c;
}

std::string Scheduler::find_package(const std::string &entrant_list)
{
    if (entrant_list.empty())
        return {};
    std::string found;
    for (auto &id : store_.list("content.package", "")) {
        registry::Record r = store_.get("content.package", id);
        if (r.status != registry::Status::ok)
            continue;
        if (r.value["entrant_list"].as_string() == entrant_list)
            found = id;
    }
    return found;
}

Totals Scheduler::deactivate_locked(Lock &g, const std::string &why)
{
    if (active_label_.empty() && activated_.empty())
        return {};
    std::vector<Applier *> was;
    was.swap(activated_);
    Totals t =
        run(Moment::deactivate, Applies::content, [&](Applier &a, Report &r) { a.deactivate(store_, r); }, nullptr, true, &was);
    std::string label = active_label_;
    active_.clear();
    active_label_.clear();
    ctx_.content.clear();
    ctx_.setup_mode.clear();
    ctx_.has_year = career_year_ != 0;
    ctx_.year = career_year_;
    publish(g);
    log("[apply] put back: " + label + " ended (" + why + "); " + std::to_string(t.appliers) + " applier(s), " + counts(t));
    return t;
}

Totals Scheduler::activate_locked(Lock &g, const std::string &content, const std::string &label, const std::string &why)
{
    Totals put = deactivate_locked(g, "another becomes active");
    active_ = content;
    active_label_ = label;
    ctx_.content.clear();
    if (!content.empty()) {
        ctx_.content = chain_of("content.package", content);
        registry::Record r = store_.get("content.package", content);
        if (r.status == registry::Status::ok) {
            if (r.value["season"].is_int()) {
                ctx_.year = int(r.value["season"].as_int());
                ctx_.has_year = true;
            }
            ctx_.setup_mode = std::string(r.value["setup_mode"].as_string("swofty_f1"));
        }
    }
    publish(g);
    Totals t = run(
        Moment::activate, Applies::content, [&](Applier &a, Report &r) { a.activate(store_, ctx_, r); }, &activated_, false,
        nullptr);
    t.put_back += put.put_back;
    log("[apply] activate: " + label + " active (" + why + "); " + std::to_string(t.appliers) + " applier(s), " + counts(t) +
        " (" + summary(ctx_) + ")");
    return t;
}

Totals Scheduler::activate(const std::string &content, const std::string &why)
{
    Lock g(mu_);
    return activate_locked(g, content, content.empty() ? "stock content" : "content " + content, why);
}

Totals Scheduler::deactivate(const std::string &why)
{
    Lock g(mu_);
    return deactivate_locked(g, why);
}

Totals Scheduler::content_picked_locked(Lock &g, const std::string &entrant_list, const std::string *mode)
{
    if (mode && ctx_.career.empty())
        ctx_.mode = mode->empty() ? std::string("unknown") : *mode;
    std::string id = find_package(entrant_list);
    std::string label = id.empty() ? "stock box " + entrant_list : "content " + id;
    Totals t = activate_locked(g, id, label, "box " + entrant_list + " picked");
    log_facts();
    return t;
}

Totals Scheduler::content_picked(const std::string &entrant_list)
{
    Lock g(mu_);
    return content_picked_locked(g, entrant_list, nullptr);
}

Totals Scheduler::content_picked(const std::string &entrant_list, const std::string &mode)
{
    Lock g(mu_);
    return content_picked_locked(g, entrant_list, &mode);
}

std::string Scheduler::session_name() const
{
    Lock g(mu_);
    return session_name_;
}

std::string Scheduler::facts_line() const
{
    Lock g(mu_);
    return facts::line(ctx_.mode, ctx_.session_type, session_name_, ctx_.track);
}

void Scheduler::log_facts() { log(facts_line()); }

Totals Scheduler::career_loaded(const std::string &career, const std::string &mode, const std::string &data_group)
{
    Lock g(mu_);
    if (session_)
        session_end_locked(g, "the career loaded");
    ctx_.career = career;
    ctx_.mode = mode;
    ctx_.data_group = data_group;
    Totals t = activate_locked(g, "", "career " + (career.empty() ? std::string("(no key)") : career), "career loaded");
    log_facts();
    return t;
}

Totals Scheduler::career_season(int year)
{
    Lock g(mu_);
    career_year_ = year;
    ctx_.year = year;
    ctx_.has_year = year != 0;
    std::string label = active_label_.empty() ? "career " + ctx_.career : active_label_;
    Totals t = activate_locked(g, active_, label, "season " + std::to_string(year) + " started");
    log_facts();
    return t;
}

bool Scheduler::career_dated(int year)
{
    Lock g(mu_);
    if (!year || year == career_year_ || ctx_.career.empty())
        return false;
    career_year_ = year;
    if (active_.empty()) {
        ctx_.year = year;
        ctx_.has_year = true;
    }
    publish(g);
    log("[apply] career " + ctx_.career + ": season " + std::to_string(year) + ", which already started");
    log_facts();
    return true;
}

Totals Scheduler::career_unloaded()
{
    Lock g(mu_);
    Totals t;
    if (session_)
        t = session_end_locked(g, "the career unloaded");
    Totals p = deactivate_locked(g, "the career unloaded");
    ctx_.career.clear();
    ctx_.mode.clear();
    ctx_.data_group.clear();
    career_year_ = 0;
    ctx_.year = 0;
    ctx_.has_year = false;
    publish(g);
    t.put_back += p.put_back;
    t.appliers += p.appliers;
    log_facts();
    return t;
}

Totals Scheduler::session_load(const std::string &type, const std::string &track)
{
    return session_load(SessionFacts{"", type, "", track});
}

Totals Scheduler::session_load(const SessionFacts &f)
{
    Lock g(mu_);
    if (session_)
        session_end_locked(g, "another session loads");
    session_ = true;
    if (!f.mode.empty() && ctx_.career.empty())
        ctx_.mode = f.mode;
    ctx_.session_type = f.type;
    ctx_.track = f.track;
    session_name_ = f.session;
    publish(g);
    log_facts();
    Totals t = run(
        Moment::session_load, Applies::session, [&](Applier &a, Report &r) { a.session_load(store_, ctx_, r); }, &sessioned_,
        false, nullptr);
    log("[apply] session load: " + std::to_string(t.appliers) + " applier(s); " + counts(t) + " (" + summary(ctx_) + ")");
    return t;
}

bool Scheduler::session_enter(const SessionFacts &f, bool from_flow)
{
    Lock g(mu_);
    if (facts::session_start(from_flow, session_, loaded_from_flow_) == facts::SessionStart::facts) {
        session_started(f);
        return false;
    }
    session_load(f);
    loaded_from_flow_ = from_flow;
    return true;
}

bool Scheduler::session_started(const SessionFacts &f)
{
    Lock g(mu_);
    if (!session_)
        return false;
    bool other_track = !f.track.empty() && !ctx_.track.empty() && f.track != ctx_.track;
    if (other_track)
        log("[apply] session start: the weekend names track " + f.track + ", not the loaded " + ctx_.track +
            "; the session is left unknown");
    std::string type = other_track ? std::string() : f.type, name = other_track ? std::string() : f.session;
    std::string track = ctx_.track.empty() ? f.track : ctx_.track;
    if (type == ctx_.session_type && name == session_name_ && track == ctx_.track)
        return false;
    ctx_.session_type = type;
    session_name_ = name;
    ctx_.track = track;
    publish(g);
    log_facts();
    return true;
}

Totals Scheduler::session_end_locked(Lock &g, const std::string &why)
{
    loaded_from_flow_ = false;
    if (!session_)
        return {};
    std::vector<Applier *> was;
    was.swap(sessioned_);
    Totals t =
        run(Moment::session_end, Applies::session, [&](Applier &a, Report &r) { a.session_end(store_, r); }, nullptr, true, &was);
    session_ = false;
    ctx_.session_type.clear();
    ctx_.track.clear();
    session_name_.clear();
    publish(g);
    log("[apply] session end (" + why + "): " + std::to_string(t.appliers) + " applier(s); " + counts(t));
    log_facts();
    return t;
}

Totals Scheduler::session_end(const std::string &why)
{
    Lock g(mu_);
    return session_end_locked(g, why);
}

Totals Scheduler::install_read()
{
    Lock g(mu_);
    if (read_)
        return {};
    read_ = true;
    publish(g);
    Totals t =
        run(Moment::read, Applies::read, [&](Applier &a, Report &r) { a.install_read(store_, r); }, nullptr, false, nullptr);
    log("[apply] read: " + std::to_string(t.appliers) + " read path(s) installed at the window stage; " + counts(t));
    return t;
}

int stat_id(Store &records, ids::Store &ids, const std::string &registry, const std::string &id, int64_t *out,
            const std::string &today, std::string *line)
{
    if (out)
        *out = -1;
    registry::Record r = records.get(registry, id);
    if (r.status != registry::Status::ok) {
        registry::set_last_error("stat_id: " + registry + " " + id + " is " + registry::status_name(r.status));
        return r.status == registry::Status::no_registry ? SWOFTY_ASSET_UNAVAILABLE : SWOFTY_ASSET_INVALID;
    }
    const auto &v = r.value["stat_id"];
    ids::Result got;
    if (v.is_int() && v.as_int() >= 0) {
        got = ids.pin(registry, id, v.as_int(), today);
    } else {
        got = ids.allocate(registry, id, today);
        if (got.id < 0) {
            registry::set_last_error("stat_id: " + got.line.substr(got.line.find(':') + 2));
            if (line)
                *line = got.line;
            return SWOFTY_ASSET_UNAVAILABLE;
        }
    }
    if (line && got.how != "kept")
        *line = got.line;
    if (out)
        *out = got.id;
    return SWOFTY_ASSET_OK;
}

Scheduler &scheduler()
{
    static Scheduler s(registry::store());
    return s;
}

}
