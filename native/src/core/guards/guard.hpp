#pragma once

#include "core/guards/order.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace swofty_core::guards {

// One of the core's own mixin sets. Each instance registers itself under its id when it is built; the commit pass
// offers every registered set in CoreSetId order and settles them in COMMIT_ORDER (offer_core_sets,
// commit_core_sets).
class CoreSet {
public:
    explicit CoreSet(CoreSetId id);
    virtual ~CoreSet() = default;
    CoreSet(const CoreSet &) = delete;
    CoreSet &operator=(const CoreSet &) = delete;

    CoreSetId id() const { return id_; }

    // Called on every commit pass; answers the sets to install once, and nothing on later passes.
    virtual std::vector<runtime::CoreMixins> offer() = 0;
    // Called after each install pass that waited on no core set.
    virtual void commit() {}

private:
    CoreSetId id_;
};

// A core set its subsystem still exports as an offer and a commit function.
class FunctionSet final : public CoreSet {
public:
    using Offer = std::vector<runtime::CoreMixins> (*)();
    using Commit = void (*)();

    FunctionSet(CoreSetId id, Offer offer, Commit commit = nullptr);

    std::vector<runtime::CoreMixins> offer() override { return offer_(); }
    void commit() override
    {
        if (commit_)
            commit_();
    }

private:
    Offer offer_;
    Commit commit_;
};

// True when the environment variable holds a value `off_value` accepts (by default 0 or off); `value` gets what
// it holds.
bool env_off(const char *name, char (&value)[16], bool (*off_value)(const char *) = nullptr);

// A guard the core offers once its code is ready (and ready() holds) and settles once, after the first install
// pass that follows the offer.
class Guard : public CoreSet {
public:
    using CoreSet::CoreSet;

    std::vector<runtime::CoreMixins> offer() final;
    void commit() final;

protected:
    virtual bool ready() const { return true; }
    // Adds the guard's mixin sets to `out`; a guard that turns itself off logs why and adds none.
    virtual void build(std::vector<runtime::CoreMixins> &out) = 0;
    // Logs what installed.
    virtual void settle() = 0;

private:
    bool offered_ = false, settled_ = false;
};

// A binding a guard's handlers need the address of, resolved before the guard's sites.
struct Keep {
    const char *binding;
    uintptr_t *address;
};

// Resolves every keep into its address, then (when all resolved) the needs that are not keeps, in order, until one
// fails. On a failure every keep's address is zeroed.
bool resolve(const std::vector<Keep> &keeps, const std::vector<const char *> &needs);

struct SiteGuardSpec {
    // The core mixin set's name, as the [core] lines print it.
    const char *set;
    // The start of every line the guard logs: its channel and what it calls itself ("[ers] mode guard").
    const char *name;
    // The environment variable whose 0 or off turns the guard off.
    const char *env;
    // Resolved first, each into its address; the guard is off when one does not resolve.
    std::vector<Keep> keeps = {};
    // Every binding the guard needs, in the order its off line names them. Those that are not keeps are resolved
    // after the keeps, in this order, until one fails.
    std::vector<const char *> needs;
    std::vector<SwoftyMixin> mixins;
    // What the armed line says the guard does.
    const char *armed;
    // Logs the settle line even when the guard turned itself off before offering its mixins.
    bool settles_when_off = false;
};

// A guard made of site, wrap or patch mixins on game code, in one mixin set: it is off when its variable says so or
// a binding it needs does not resolve, and logs whether its mixins installed once they settle (lines.hpp).
class SiteGuard : public Guard {
public:
    SiteGuard(CoreSetId id, SiteGuardSpec spec);

    const SiteGuardSpec &spec() const { return spec_; }

protected:
    // A further condition, checked once the bindings resolve; a guard that refuses logs why.
    virtual bool admit() { return true; }
    // Adds optional mixins to the set before it is offered.
    virtual void extend(runtime::CoreMixins &) {}

    bool wanted() const { return wanted_; }

private:
    void build(std::vector<runtime::CoreMixins> &out) override;
    void settle() override;

    SiteGuardSpec spec_;
    bool wanted_ = false;
};

}
