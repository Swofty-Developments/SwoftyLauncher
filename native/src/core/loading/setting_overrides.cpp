#include "core/loading/setting_overrides_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <array>
#include <atomic>
#include <cstdint>
#include <string>
#include <vector>

// Loading-time setting overrides: one wrap on Ne_Get_SettingBool that answers the settings in
// setting_overrides_logic.hpp with their forced value while each one's feature is on. The game then takes its own
// developer path; nothing else is patched. With every feature off no mixin is offered.

namespace {

namespace loading = runtime::loading;

constexpr const char *MIXIN = "swofty_core.dll (loading setting overrides)";
constexpr const char *TARGET = "Ne_Get_SettingBool";

using GetBool = uint64_t(void *node, const char *name, uint8_t *out, uint8_t def);

GetBool *orig_get_bool;
std::atomic<uint32_t> active{0};
bool offered = false, settled = false;
std::array<std::atomic<uint64_t>, loading::OVERRIDES.size()> applied{};

uint64_t get_bool(void *node, const char *name, uint8_t *out, uint8_t def)
{
    uint64_t r = orig_get_bool(node, name, out, def);
    const loading::SettingOverride *row = loading::match(name, active.load(std::memory_order_relaxed));
    if (!row || !out)
        return r;
    uint8_t was = *out;
    *out = row->value ? 1 : 0;
    if (applied[loading::index_of(row)].fetch_add(1, std::memory_order_relaxed) == 0)
        swofty::log("[loading] %s read by the game (it had %u, status %llu): answered %u for the %s feature, so %s", row->setting,
                    unsigned(was), static_cast<unsigned long long>(r), unsigned(*out), row->feature, row->effect);
    return r;
}

}

namespace runtime {

std::vector<CoreMixins> loading_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char env[256] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_LOADING_FEATURES", env, sizeof env);
    uint32_t from_env = n && n < sizeof env ? loading::parse_list(env) : 0;
    std::string on;
    for (size_t i = 0; i < loading::OVERRIDES.size(); i++)
        if ((from_env >> i & 1) || enabled(loading::OVERRIDES[i].feature, nullptr)) {
            active.fetch_or(1u << i);
            on += std::string(on.empty() ? "" : ", ") + loading::OVERRIDES[i].feature +
                  ((from_env >> i & 1) ? " (SWOFTY_LOADING_FEATURES)" : "");
        }
    if (!active.load())
        return out;
    if (!swofty::api->resolve(TARGET)) {
        active.store(0);
        swofty::log("[loading] setting overrides off (%s asked for): %s does not resolve on this build", on.c_str(), TARGET);
        return out;
    }
    swofty::log("[loading] setting overrides wanted: %s", on.c_str());
    out.push_back({MIXIN, {{TARGET, SWOFTY_MIXIN_WRAP, 100, (void *)&get_bool, (void **)&orig_get_bool}}});
    return out;
}

void loading_commit()
{
    if (!offered || settled || !active.load())
        return;
    settled = true;
    if (core_mixins_active(MIXIN)) {
        swofty::log("[loading] setting overrides armed on %s", TARGET);
        return;
    }
    active.store(0);
    swofty::log("[loading] setting overrides did not install (see the [core] lines)");
}

}
