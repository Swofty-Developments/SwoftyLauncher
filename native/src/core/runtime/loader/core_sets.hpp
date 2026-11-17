#pragma once

#include "core/runtime/loader/core_set_order.hpp"

#include <vector>

namespace runtime {
struct CoreMixins;
}

namespace runtime::loader {

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

    std::vector<runtime::CoreMixins> offer() override;
    void commit() override;

private:
    Offer offer_;
    Commit commit_;
};

// Hands every registered core set's offer to `add`, in CoreSetId order.
void offer_core_sets(void (*add)(std::vector<runtime::CoreMixins>));
// Settles every registered core set, in COMMIT_ORDER.
void commit_core_sets();

}
