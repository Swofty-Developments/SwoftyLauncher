#pragma once

#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/resolve.hpp"

#include <atomic>
#include <cstdint>
#include <initializer_list>
#include <mutex>
#include <string>
#include <utility>

#include <swofty/mixin.hpp>

namespace runtime::loader {

// The game's own function behind a core wrap. slot() is the address a SwoftyMixin entry's original pointer takes.
template <class Fn> class Original {
public:
    void **slot() { return reinterpret_cast<void **>(&fn_); }
    explicit operator bool() const { return fn_ != nullptr; }
    template <class... Args> decltype(auto) operator()(Args &&...args) const { return fn_(std::forward<Args>(args)...); }

private:
    Fn *fn_ = nullptr;
};

// One address a bind step resolves into `to`. A required id this build does not resolve is named in the step's
// reason; an optional one is left 0 without a word; a null id leaves 0.
struct Binding {
    const char *id;
    uintptr_t *to;
    bool required = true;
};

// The state a subsystem's core mixin set keeps beside its handlers: the MixinGate its offer and commit run through,
// the lock over the subsystem's own state, whether the hooks are live or off and why, and the bind step that
// resolves the addresses the handlers call. A subsystem derives from it and adds its wraps' originals.
template <class Lock = std::mutex> class HookSet {
public:
    MixinGate gate;
    Lock lock;

    bool live() const { return live_.load(); }
    bool off() const { return off_.load(); }
    const std::string &why() const { return why_; }

    void set_live() { live_ = true; }
    void turn_off(std::string why)
    {
        why_ = std::move(why);
        off_ = true;
    }

    // Resolves every binding in order; false with "needs <ids>, which this build does not resolve" in `why` when a
    // required id is missing.
    static bool bind(std::initializer_list<Binding> bindings, std::string &why)
    {
        std::string missing;
        for (const Binding &b : bindings)
            *b.to = !b.id ? 0 : b.required ? runtime::resolve_or_note(b.id, missing) : swofty::api->resolve(b.id);
        if (missing.empty())
            return true;
        why = "needs " + missing + ", which this build does not resolve";
        return false;
    }

private:
    std::atomic<bool> live_{false}, off_{false};
    std::string why_;
};

// One part of a core mixin set that addons ask for separately: the mixin's name, the label its lines use, whether an
// addon wants it, the MixinGate its offer and commit run through, whether its mixins installed, and whether its late
// registration has been logged.
struct MixinPart {
    const char *name, *label;
    std::atomic<bool> wanted{false};
    MixinGate gate{};
    std::atomic<bool> live{false}, late_logged{false};

    // True for the offer that offers the part, once an addon wants it.
    bool offer() { return wanted && gate.offer(); }
    // Whether an entry for the part can take effect: it is live, or it has not settled and commits remain.
    bool usable(bool commits_closed) const { return live || (!gate.settled() && !commits_closed); }
    // The commit for an offered part that has not settled: records installed() as live; true for the settling commit.
    template <class Installed> bool settle(Installed &&installed)
    {
        if (!gate.offered() || gate.settled())
            return false;
        live = installed();
        return gate.settle();
    }
    // The part's word on a setup line: "armed", "off" or "not used".
    const char *state() const { return live ? "armed" : gate.settled() ? "off" : "not used"; }
    // True the first time it is asked, for the one line a late registration logs.
    bool first_late() { return !late_logged.exchange(true); }
};

}
