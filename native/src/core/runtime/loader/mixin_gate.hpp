#pragma once

#include <atomic>

// A core mixin set's offer and commit (core_sets.hpp's FunctionSet) each run once: the offer marks the gate offered,
// and the commit settles it once after an offer and logs the set's state through install_state.
namespace runtime::loader {

inline constexpr const char *NOT_INSTALLED = "did not install (see the [core] lines)";

class MixinGate {
public:
    // True for the call that offered the set, false once it had been offered.
    bool offer() { return !offered_.exchange(true); }
    // True for the one commit after an offer that settles the set.
    bool settle() { return offered_.load() && !settled_.exchange(true); }
    bool offered() const { return offered_.load(); }
    bool settled() const { return settled_.load(); }

private:
    std::atomic<bool> offered_{false}, settled_{false};
};

// A commit line's state: `armed` ("armed: <what the set does>") when the set's mixins installed, `off` when not.
constexpr const char *install_state(bool installed, const char *armed, const char *off = NOT_INSTALLED)
{
    return installed ? armed : off;
}

}
