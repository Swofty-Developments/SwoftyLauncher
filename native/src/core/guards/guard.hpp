#pragma once

#include "core/guards/env.hpp"
#include "core/guards/order.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace swofty_core::guards {

using ::runtime::loader::CoreSet;
using ::runtime::loader::CoreSetId;

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

// A guard that hooks game code through safetyhook itself: its setup runs once the game code is ready
// (runtime::when_code_ready, under `what`). Hook guards are armed in HookGuardId order.
class HookGuard {
public:
    HookGuard(HookGuardId id, const char *what, void (*setup)());
    HookGuard(const HookGuard &) = delete;
    HookGuard &operator=(const HookGuard &) = delete;

    HookGuardId id() const { return id_; }
    const char *what() const { return what_; }
    void arm() const;

private:
    HookGuardId id_;
    const char *what_;
    void (*setup_)();
};

// Arms the next registered hook guard in HookGuardId order. Each hook guard adds one SDK on-load entry that calls it
// (SWOFTY_HOOK_GUARD), so the core still registers one on-load mixin per hook guard and the order the guards arm in
// does not depend on which translation unit's entry the SDK lists first.
void arm_next_hook_guard();

// A guard that hooks game code through safetyhook itself when its owner first arms it, outside the core mixin sets
// and the on-load entries. install() runs on the first arm() only; every later arm() returns what that run returned.
class OnDemandGuard {
public:
    OnDemandGuard() = default;
    OnDemandGuard(const OnDemandGuard &) = delete;
    OnDemandGuard &operator=(const OnDemandGuard &) = delete;

    bool arm()
    {
        if (!tried_) {
            tried_ = true;
            armed_ = install();
        }
        return armed_;
    }

    bool armed() const { return armed_; }

protected:
    ~OnDemandGuard() = default;

    virtual bool install() = 0;

private:
    bool tried_ = false;
    bool armed_ = false;
};

}

#define SWOFTY_HOOK_GUARD(id, what, setup)                                                                                       \
    namespace {                                                                                                                  \
    const ::swofty_core::guards::HookGuard hook_guard{::swofty_core::guards::HookGuardId::id, what, setup};                      \
    }                                                                                                                            \
    SWOFTY_ON_LOAD { ::swofty_core::guards::arm_next_hook_guard(); }
