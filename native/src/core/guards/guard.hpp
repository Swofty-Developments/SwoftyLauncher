#pragma once

#include "core/guards/env.hpp"
#include "core/guards/on_demand.hpp"
#include "core/guards/order.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/loader/core_rank.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace swofty_core::guards {

using ::runtime::loader::CoreSet;
using ::runtime::loader::CoreSetId;
using ::runtime::loader::install_state;
using ::runtime::loader::MixinGate;
using ::runtime::loader::NOT_INSTALLED;

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
    // True once build() has added the mixin set named `set` to the guard's offer.
    bool offered_set(const char *set) const;

private:
    MixinGate gate_;
    std::vector<const char *> offered_;
};

// A binding a guard's handlers need the address of, resolved before the guard's sites.
struct Keep {
    const char *binding;
    uintptr_t *address;
};

// Resolves every keep into its address, then (when all resolved) the needs that are not keeps, in order, until one
// fails. On a failure every keep's address is zeroed.
bool resolve(const std::vector<Keep> &keeps, const std::vector<const char *> &needs);

// A further mixin set a SiteGuard offers in its own set, under the guard's variable: offered when its own bindings
// resolve, whether or not the guard's set did, and settled with its own armed or did-not-install line.
struct SitePart {
    const char *set;
    const char *name;
    std::vector<Keep> keeps = {};
    std::vector<const char *> needs;
    std::vector<SwoftyMixin> mixins;
    const char *armed;
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
    std::vector<SitePart> parts = {};
    // Runs once, after the armed line, when the guard's own set installed.
    void (*on_armed)() = nullptr;
};

// A guard made of site, wrap or patch mixins on game code, in one mixin set and one more per part: it is off when its
// variable says so, a set is not offered when a binding it needs does not resolve, and the guard logs whether each
// offered set installed once they settle (lines.hpp).
class SiteGuard : public Guard {
public:
    SiteGuard(CoreSetId id, SiteGuardSpec spec);

    const SiteGuardSpec &spec() const { return spec_; }

protected:
    // A further condition, checked once the bindings resolve; a guard that refuses logs why.
    virtual bool admit() { return true; }
    // Adds optional mixins to the set before it is offered.
    virtual void extend(runtime::CoreMixins &) {}

private:
    void build(std::vector<runtime::CoreMixins> &out) override;
    void settle() override;

    SiteGuardSpec spec_;
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
// (SWOFTY_HOOK_GUARD), so the core still registers one on-load mixin per hook guard, in the same place in the on-load
// sequence, and the order the guards arm in is HookGuardId's whichever hook guard's entry the SDK runs first.
void arm_next_hook_guard();

}

#define SWOFTY_HOOK_GUARD(id, what, setup)                                                                                       \
    SWOFTY_CORE_RANK(id)                                                                                                         \
    namespace {                                                                                                                  \
    const ::swofty_core::guards::HookGuard hook_guard{::swofty_core::guards::HookGuardId::id, what, setup};                      \
    }                                                                                                                            \
    SWOFTY_ON_LOAD { ::swofty_core::guards::arm_next_hook_guard(); }
