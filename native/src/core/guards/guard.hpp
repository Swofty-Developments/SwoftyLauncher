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

// True when the environment variable is set to 0 or off; `value` gets what it holds.
bool env_off(const char *name, char (&value)[16]);

// A binding a guard's handlers need the address of, resolved before the guard's sites.
struct Keep {
    const char *binding;
    uintptr_t *address;
};

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

// A guard made of site, wrap or patch mixins on game code: it is offered once the code is ready, unless its variable
// turns it off or a binding it needs does not resolve, and logs whether its mixins installed once they settle.
class SiteGuard : public CoreSet {
public:
    SiteGuard(CoreSetId id, SiteGuardSpec spec);

    std::vector<runtime::CoreMixins> offer() override;
    void commit() override;

protected:
    // A further condition, checked once the bindings resolve; a guard that refuses logs why.
    virtual bool admit() { return true; }

    const SiteGuardSpec &spec() const { return spec_; }
    bool active() const;

private:
    bool resolve();

    SiteGuardSpec spec_;
    bool offered_ = false, settled_ = false, wanted_ = false;
};

}
