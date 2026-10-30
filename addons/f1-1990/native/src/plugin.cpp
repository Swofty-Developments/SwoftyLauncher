// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 addon DLL entry. A refused init leaves every hook out, which is logged.

#include <swofty/mixin.hpp>

extern "C" __declspec(dllexport) const uint32_t SwoftyAddonApi = SWOFTY_API_VERSION;

extern "C" __declspec(dllexport) int SwoftyAddonInit(const SwoftyApi *a)
{
    int ok = ::swofty::detail::init(a, nullptr);
    if (!ok && a && a->log)
        a->log("[f1-1990] the f1_1990.dll hooks did not install (grid best-of, contract skip)");
    return ok;
}
