#include "core/loading/game_settings_logic.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <array>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <type_traits>
#include <vector>

// Game setting overrides: a wrap on each typed engine setting getter that answers the armed rows of OVERRIDES
// (game_settings_logic.hpp) with their forced value, and with loading_log_settings on logs what the game reads. The
// game then takes its own path; nothing else is patched. A getter is wrapped only while a row of its type is armed or
// the log is on, so with every feature off no mixin is offered.

namespace {

namespace gs = runtime::game_settings;
using gs::Type;

constexpr const char *MIXIN = "swofty_core.dll (game setting overrides)";
constexpr const char *ENV = "SWOFTY_LOADING_FEATURES";

template <Type K> struct Getter;
template <> struct Getter<Type::boolean> {
    using Value = uint8_t;
    static constexpr const char *target = "Ne_Get_SettingBool";
    static constexpr const char *name = "bool";
};
template <> struct Getter<Type::u32> {
    using Value = uint32_t;
    static constexpr const char *target = "Ne_Get_SettingU32";
    static constexpr const char *name = "u32";
};
template <> struct Getter<Type::i32> {
    using Value = int32_t;
    static constexpr const char *target = "Ne_Map_FindOrDefaultWithErrorContext";
    static constexpr const char *name = "i32";
};
template <> struct Getter<Type::f32> {
    using Value = float;
    static constexpr const char *target = "Ne_Config_GetFloatOrDefault";
    static constexpr const char *name = "float";
};

template <Type K> using Value = typename Getter<K>::Value;
template <Type K> using Fn = uint64_t(void *node, const char *name, Value<K> *out, Value<K> def);
template <Type K> Fn<K> *orig = nullptr;

std::array<std::atomic<uint32_t>, gs::TYPES> armed{};
std::atomic<bool> logging{false};
bool offered = false, settled = false;
std::array<std::atomic<uint64_t>, gs::OVERRIDES.size()> applied{};

SRWLOCK seen_lock = SRWLOCK_INIT;
gs::SeenTable<4096> seen;

template <class V> uint32_t bits_of(V v)
{
    uint32_t b = 0;
    std::memcpy(&b, &v, sizeof v);
    return b;
}

template <class V> std::string text(V v)
{
    char buf[48];
    if constexpr (std::is_same_v<V, float>)
        snprintf(buf, sizeof buf, "%g", double(v));
    else if constexpr (std::is_same_v<V, int32_t>)
        snprintf(buf, sizeof buf, "%d", v);
    else
        snprintf(buf, sizeof buf, "%u (0x%x)", unsigned(v), unsigned(v));
    return buf;
}

template <Type K> void log_read(const char *name, Value<K> v, uint64_t status, bool forced)
{
    AcquireSRWLockExclusive(&seen_lock);
    bool known = seen.seen(name, K, bits_of(v));
    ReleaseSRWLockExclusive(&seen_lock);
    if (!known)
        swofty::log("[settings] read %s %s = %s (status %llu%s)", Getter<K>::name, name, text(v).c_str(),
                    static_cast<unsigned long long>(status), forced ? ", forced by a loading feature" : "");
}

template <Type K> uint64_t get(void *node, const char *name, Value<K> *out, Value<K> def)
{
    uint64_t r = orig<K>(node, name, out, def);
    if (!out)
        return r;
    const gs::Override *row = gs::match(name, armed[size_t(K)].load(std::memory_order_relaxed));
    if (row) {
        Value<K> was = *out;
        if constexpr (sizeof(Value<K>) == 1)
            *out = Value<K>(row->bits != 0);
        else
            std::memcpy(out, &row->bits, sizeof(Value<K>));
        if (applied[gs::index_of(row)].fetch_add(1, std::memory_order_relaxed) == 0)
            swofty::log("[loading] %s read by the game (it had %s, status %llu): answered %s for the %s feature, so %s",
                        row->setting, text(was).c_str(), static_cast<unsigned long long>(r), text(*out).c_str(), row->feature,
                        row->effect);
    }
    if (name && logging.load(std::memory_order_relaxed))
        log_read<K>(name, *out, r, row != nullptr);
    return r;
}

template <Type K> void offer(std::vector<SwoftyMixin> &list, std::string &missing)
{
    if (!armed[size_t(K)].load() && !logging.load())
        return;
    if (!swofty::api->resolve(Getter<K>::target)) {
        armed[size_t(K)].store(0);
        missing += std::string(missing.empty() ? "" : ", ") + Getter<K>::target;
        return;
    }
    list.push_back({Getter<K>::target, SWOFTY_MIXIN_WRAP, 100, (void *)&get<K>, (void **)&orig<K>});
}

std::string feature_list(uint32_t rows, uint32_t from_env, bool log)
{
    std::string on;
    for (size_t i = 0; i < gs::OVERRIDES.size(); i++) {
        if (!(rows >> i & 1) || (gs::rows_of(gs::OVERRIDES[i].feature) & ((1u << i) - 1)))
            continue;
        on += std::string(on.empty() ? "" : ", ") + gs::OVERRIDES[i].feature;
        if (from_env >> i & 1)
            on += std::string(" (") + ENV + ")";
    }
    if (log)
        on += std::string(on.empty() ? "" : ", ") + gs::LOG_FEATURE;
    return on;
}

}

namespace runtime {

std::vector<CoreMixins> loading_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char env[512] = {};
    DWORD n = GetEnvironmentVariableA(ENV, env, sizeof env);
    bool log = false;
    uint32_t from_env = n && n < sizeof env ? gs::parse_list(env, &log) : 0;
    uint32_t rows = from_env;
    for (size_t i = 0; i < gs::OVERRIDES.size(); i++)
        if (!(rows >> i & 1) && enabled(gs::OVERRIDES[i].feature, nullptr))
            rows |= gs::rows_of(gs::OVERRIDES[i].feature);
    log = log || enabled(gs::LOG_FEATURE, nullptr);
    if (!rows && !log)
        return out;
    for (Type k : {Type::boolean, Type::u32, Type::i32, Type::f32})
        armed[size_t(k)].store(gs::rows_typed(rows, k));
    logging.store(log);
    std::vector<SwoftyMixin> list;
    std::string missing;
    offer<Type::boolean>(list, missing);
    offer<Type::u32>(list, missing);
    offer<Type::i32>(list, missing);
    offer<Type::f32>(list, missing);
    if (!missing.empty())
        swofty::log("[loading] game setting overrides: %s does not resolve on this build; its settings stay as read",
                    missing.c_str());
    if (list.empty())
        return out;
    swofty::log("[loading] game setting overrides wanted: %s (%zu getter(s))", feature_list(rows, from_env, log).c_str(),
                list.size());
    out.push_back({MIXIN, std::move(list)});
    return out;
}

void loading_commit()
{
    if (!offered || settled)
        return;
    settled = true;
    bool any = logging.load();
    for (auto &a : armed)
        any = any || a.load() != 0;
    if (!any)
        return;
    if (core_mixins_active(MIXIN)) {
        swofty::log("[loading] game setting overrides armed");
        return;
    }
    for (auto &a : armed)
        a.store(0);
    logging.store(false);
    swofty::log("[loading] game setting overrides did not install (see the [core] lines)");
}

static loader::FunctionSet loading_core_set{loader::CoreSetId::loading, loading_mixins, loading_commit};

}
