#pragma once

#include <cstddef>
#include <functional>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

#include "core/apply/applier.hpp"
#include "core/apply/ids.hpp"

namespace swofty_core::apply {

enum class Moment { boot, activate, deactivate, session_load, session_end, read };

const char *moment_name(Moment m);

struct Totals {
    int appliers = 0, applied = 0, put_back = 0, refused = 0;
};

struct SessionFacts {
    std::string mode, type, session, track;
};

class Scheduler {
public:
    explicit Scheduler(Store &store);

    void set_log(std::function<void(const std::string &)> fn);
    void set_facts(std::function<void(const Context &)> fn);

    // The boot plan. start_plan runs every boot applier's prepare and each count that is not counts_off_thread() on
    // this thread. With off_thread, the counts_off_thread() appliers then count on a worker; without it, every applier
    // counts here in applier order, as plan_boot does. Every call below waits for the worker first. The store's
    // generation is compared at that wait: a store that moved while the worker counted is counted again on the
    // waiting thread.
    void start_plan(bool off_thread);
    std::size_t plan_boot();
    std::size_t planned_keys();
    std::size_t planned_keys(const std::string &registry);
    bool boot_work();
    // Without waiting: true until the plan is counted, then boot_work(). When no counts_off_thread() applier may_count,
    // start_plan counts on this thread and the plan is counted when it returns.
    bool boot_may_work() const;
    bool boot_planned() const;
    Totals run_boot();
    bool booted() const;
    // The store's generation as the boot's appliers found it. The boot's own context publish is left out when it
    // changed no field of the context and the store has not moved since, so work made ahead of the boot can compare
    // the generation it started from with this one.
    uint64_t boot_generation() const;

    Totals career_loaded(const std::string &career, const std::string &mode, const std::string &data_group);
    Totals career_season(int year);
    // The year of a career loaded in a season that already started: the context takes it, no applier runs.
    bool career_dated(int year);
    Totals career_unloaded();
    Totals content_picked(const std::string &entrant_list);
    Totals content_picked(const std::string &entrant_list, const std::string &mode);
    Totals activate(const std::string &content, const std::string &why);
    Totals deactivate(const std::string &why);

    Totals session_load(const std::string &type, const std::string &track);
    Totals session_load(const SessionFacts &facts);
    std::string session_name() const;
    std::string facts_line() const;
    Totals session_end(const std::string &why);
    bool in_session() const;

    Totals install_read();

    Context context() const;
    std::string active() const;
    std::string find_package(const std::string &entrant_list);
    std::vector<std::string> chain_of(const std::string &registry, const std::string &id);

    void reset();

private:
    using Lock = std::unique_lock<std::recursive_mutex>;

    void publish(Lock &);
    Totals run(Moment m, Applies kind, const std::function<void(Applier &, Report &)> &call, std::vector<Applier *> *ran,
               bool reverse_order, const std::vector<Applier *> *only);
    struct OffThread;

    void log(const std::string &line);
    bool has_addon_content(const std::string &registry);
    void count_on_thread_locked();
    void note_count_locked(std::size_t i);
    void close_plan_locked();
    void await_count();
    void settle_locked(const OffThread &off);
    Lock settled_lock();
    Totals activate_locked(Lock &, const std::string &content, const std::string &label, const std::string &why);
    Totals deactivate_locked(Lock &, const std::string &why);
    Totals session_end_locked(Lock &, const std::string &why);
    Totals content_picked_locked(Lock &, const std::string &entrant_list, const std::string *mode);
    void log_facts();
    void log_unapplied();

    Store &store_;
    mutable std::recursive_mutex mu_;
    std::function<void(const std::string &)> log_;
    std::function<void(const Context &)> facts_;
    Context ctx_;
    bool have_ctx_ = false;
    bool planned_ = false, booted_ = false, read_ = false, session_ = false;
    uint64_t boot_from_ = 0, boot_published_ = 0;
    std::vector<std::pair<std::string, std::size_t>> keys_;
    bool work_ = false;
    bool started_ = false;
    std::vector<Applier *> boot_;
    std::vector<std::size_t> boot_keys_;
    std::vector<char> boot_content_;
    std::shared_ptr<OffThread> off_;
    std::string active_, active_label_;
    int career_year_ = 0;
    std::string session_name_;
    std::vector<Applier *> activated_, sessioned_;
    std::vector<std::string> boot_summary_;
};

Scheduler &scheduler();
void clear_appliers();
std::string summary(const Context &c);
int stat_id(Store &records, ids::Store &ids, const std::string &registry, const std::string &id, int64_t *out,
            const std::string &today, std::string *line);

}
