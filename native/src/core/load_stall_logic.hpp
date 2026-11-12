#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>

// The load-stall watchdog's trigger (load_stall.cpp). Watch takes every flow state transition the core sees
// (career.cpp's flow site) and every UI screen the game opens (ui.cpp), and poll says when a load has sat still
// long enough for a report of every thread's stack.
//
// A load is in progress while the newest screen is a loading screen (its name holds "loading": loading_transitions,
// loading) or while StateLoadSession or StateUnloadSession is out of None. Progress is any flow transition or screen
// open; the stall is the time since the last one while a load is in progress. The threshold is kLoadingMs while only
// a loading screen holds the load (the hand-off from "State launch exited" to StateLoadSession took at most 1.3 s in
// 24 real loads) and kSessionLoadMs while a session load or unload flow is out of None (real StateLoadSession to
// StateSession took 8.9 s at the median and up to 169 s across 468 loads). A stall gets its first report at the
// threshold and a second at kRepeatFactor times it; progress starts a new stall; a process gets at most kMaxReports.

namespace runtime::load_stall {

constexpr uint64_t kLoadingMs = 45000;
constexpr uint64_t kSessionLoadMs = 180000;
constexpr uint64_t kRepeatFactor = 3;
constexpr int kReportsPerStall = 2;
constexpr int kMaxReports = 4;
constexpr size_t kName = 64;
constexpr size_t kSlots = 32;

enum Status : uint32_t { None = 0, Entering = 1, Updating = 2, Exiting = 3 };

inline const char *status_name(uint32_t s)
{
    static const char *const names[] = {"None", "Entering", "Updating", "Exiting"};
    return s < 4 ? names[s] : "?";
}

inline bool loading_screen(const char *name)
{
    return name && std::strstr(name, "loading");
}

inline bool session_load_flow(const char *name)
{
    return name && (!std::strcmp(name, "StateLoadSession") || !std::strcmp(name, "StateUnloadSession"));
}

struct Flow {
    uint32_t hash = 0;
    uint32_t status = None;
    uint64_t since = 0;
    char name[kName] = {};
    bool used = false;
};

struct Due {
    bool report = false;
    int index = 0;
    uint64_t stalled_ms = 0;
    uint64_t threshold_ms = 0;
};

inline void copy_name(char *dst, const char *src)
{
    std::snprintf(dst, kName, "%.*s", int(kName - 1), src ? src : "");
}

class Watch {
public:
    uint64_t loading_ms = kLoadingMs;
    uint64_t session_ms = kSessionLoadMs;

    void screen(const char *name, uint64_t now)
    {
        copy_name(screen_, name);
        screen_at_ = now;
        progress(now);
    }

    void flow(uint32_t hash, uint32_t to, const char *name, uint64_t now)
    {
        copy_name(last_flow_, name);
        last_to_ = to;
        last_flow_at_ = now;
        flows_seen_ = true;
        Flow *slot = find(hash);
        if (to == None) {
            if (slot)
                *slot = Flow{};
        } else {
            if (!slot)
                slot = take();
            if (slot->hash != hash || slot->status != to)
                slot->since = now;
            slot->hash = hash;
            slot->status = to;
            slot->used = true;
            copy_name(slot->name, name);
        }
        progress(now);
    }

    const Flow *session_flow() const
    {
        for (const Flow &f : flows_)
            if (f.used && session_load_flow(f.name))
                return &f;
        return nullptr;
    }

    bool in_loading_screen() const
    {
        return loading_screen(screen_);
    }

    uint64_t threshold() const
    {
        if (session_flow())
            return session_ms;
        return in_loading_screen() ? loading_ms : 0;
    }

    Due poll(uint64_t now)
    {
        Due d;
        uint64_t t = threshold();
        if (!t || total_ >= kMaxReports || stall_reports_ >= kReportsPerStall || now < progress_)
            return d;
        uint64_t stalled = now - progress_;
        uint64_t due = stall_reports_ == 0 ? t : t * kRepeatFactor;
        if (stalled < due)
            return d;
        stall_reports_++;
        total_++;
        d.report = true;
        d.index = stall_reports_;
        d.stalled_ms = stalled;
        d.threshold_ms = t;
        return d;
    }

    int reports() const
    {
        return total_;
    }

    uint64_t last_progress() const
    {
        return progress_;
    }

    size_t describe(char *out, size_t cap, uint64_t now) const
    {
        size_t at = 0;
        auto put = [&](const char *f, auto... a) {
            if (at + 1 >= cap)
                return;
            int n = std::snprintf(out + at, cap - at, f, a...);
            at = n < 0 ? at : (at + size_t(n) >= cap ? cap - 1 : at + size_t(n));
        };
        auto ago = [&](uint64_t t) { return double(now >= t ? now - t : 0) / 1000.0; };
        if (screen_[0])
            put("newest screen \"%s\", opened %.1f s ago%s\n", screen_, ago(screen_at_),
                in_loading_screen() ? " (a loading screen)" : "");
        else
            put("%s", "newest screen: none seen\n");
        if (flows_seen_)
            put("last flow transition: \"%s\" -> %s, %.1f s ago\n", last_flow_, status_name(last_to_), ago(last_flow_at_));
        else
            put("%s", "last flow transition: none seen\n");
        const Flow *s = session_flow();
        if (s)
            put("session load flow: \"%s\" in %s for %.1f s\n", s->name, status_name(s->status), ago(s->since));
        else
            put("%s", "session load flow: none (StateLoadSession and StateUnloadSession are in None)\n");
        int n = 0;
        for (const Flow &f : flows_)
            if (f.used && (f.status == Entering || f.status == Exiting))
                n++;
        put("flows in Entering or Exiting: %d\n", n);
        for (const Flow &f : flows_)
            if (f.used && (f.status == Entering || f.status == Exiting))
                put("  \"%s\" in %s for %.1f s\n", f.name, status_name(f.status), ago(f.since));
        n = 0;
        for (const Flow &f : flows_)
            if (f.used && f.status == Updating)
                n++;
        put("flows in Updating: %d\n", n);
        for (const Flow &f : flows_)
            if (f.used && f.status == Updating)
                put("  \"%s\" for %.1f s\n", f.name, ago(f.since));
        return at;
    }

private:
    char screen_[kName] = {};
    uint64_t screen_at_ = 0;
    char last_flow_[kName] = {};
    uint32_t last_to_ = None;
    uint64_t last_flow_at_ = 0;
    bool flows_seen_ = false;
    uint64_t progress_ = 0;
    int stall_reports_ = 0;
    int total_ = 0;
    Flow flows_[kSlots];

    void progress(uint64_t now)
    {
        progress_ = now;
        stall_reports_ = 0;
    }

    Flow *find(uint32_t hash)
    {
        for (Flow &f : flows_)
            if (f.used && f.hash == hash)
                return &f;
        return nullptr;
    }

    Flow *take()
    {
        Flow *oldest = &flows_[0];
        for (Flow &f : flows_) {
            if (!f.used)
                return &f;
            if (!session_load_flow(f.name) && (session_load_flow(oldest->name) || f.since < oldest->since))
                oldest = &f;
        }
        *oldest = Flow{};
        return oldest;
    }
};

}
