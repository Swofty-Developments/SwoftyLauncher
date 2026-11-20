#pragma once

#include "core/runtime/loader/mixin_gate.hpp"

#include <string>
#include <vector>

// The lines a SiteGuard or a MidHookSpec guard logs, each starting with the guard's name ("[ers] mode guard").
namespace swofty_core::guards::lines {

inline std::string listed(const std::vector<const char *> &bindings)
{
    std::string out;
    for (size_t i = 0; i < bindings.size(); i++) {
        if (i)
            out += i + 1 == bindings.size() ? " and " : ", ";
        out += bindings[i];
    }
    return out;
}

inline std::string off(const std::string &name, const std::string &env, const std::string &value)
{
    return name + " off (" + env + "=" + value + ")";
}

inline std::string unresolved(const std::string &name, const std::vector<const char *> &needs)
{
    return name + " off: needs " + listed(needs) + ", which this build does not resolve";
}

inline std::string armed(const std::string &name, const std::string &what) { return name + " armed: " + what; }

inline std::string hook_failed(const std::string &name, const char *site, const std::string &error)
{
    return name + ": " + (site ? std::string(site) + ": " : std::string()) + "the hook failed: " + error;
}

inline std::string not_installed(const std::string &name) { return name + " " + ::runtime::loader::NOT_INSTALLED; }

}
