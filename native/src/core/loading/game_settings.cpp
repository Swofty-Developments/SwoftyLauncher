#include "core/loading/game_settings_logic.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

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
namespace bp = runtime::build_proof;
using gs::Type;

// The readers of Game.IgnoreMinimumLoadingTime the on-by-default rows answer, each told by the getter's return address
// (the instruction after its call of Ne_Get_SettingBool 0x143a43b90). Each one takes the elapsed seconds since its state
// started (now minus its QPC stamp, over the frequency), reads the setting into a stack byte with default 0, and holds
// its state while the elapsed time is below 3.0 s, or below 0.0 s with the setting on. Only the test after the call reads
// that byte; the function's later uses of the slot write it first.
// sigport.py --witness finds each function the same on the builds its proof names, with the offsets below at the same
// instructions, and the site pattern matches once on each (sigverify.py).
//   track load: Sess_Loader_RequestTrackLoad 0x142da7870 (Steam 1.0.145 0x142da9e00), stamp +0xea0, frequency +0xeb0;
//     725 of 725 instructions on Steam 1.0.145, the EA App KB build and EA App 1.0.145. Past the minimum it sets +0xe92
//     and goes on to its track request through +0xca0.
//   session transition: Sess_SessionTransition_OnStateUpdate 0x1436f9890 (0x1436fbe20), stamp +0xd98, frequency +0xda8;
//     89 of 89 on all three. Past the minimum it calls 0x1442627a0 on its +0xc80 member.
//   story hub: Sess_StoryHub_OnStateEnter 0x1436fb5e0 (0x1436fdb70), stamp +0xe20, frequency +0xe30; 664 of 664 on
//     all three (the EA App builds' 10 rip-relative vmovupd stores differ only in their displacement). Past the minimum
//     it sets +0xd40 and goes on through +0xc80.
constexpr uint32_t ALL_BUILDS =
    bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145) | bp::bit(bp::Build::ea) | bp::bit(bp::Build::ea_1_0_145);
constexpr bp::Proof TRACK_LOAD_READ{"the track load's minimum time read (+0xea0, +0xeb0, +0xe92)", ALL_BUILDS};
constexpr bp::Proof TRANSITION_READ{"the session transition's minimum time read (+0xd98, +0xda8, +0xc80)", ALL_BUILDS};
constexpr bp::Proof STORY_HUB_READ{"the story hub's minimum time read (+0xe20, +0xe30, +0xd40)", ALL_BUILDS};

constexpr runtime::Sig SIG_TRACK_LOAD_READ{
    "session.track_load_minimum_read",
    "80 7C 24 30 00 0F 84 ?? ?? ?? ?? C5 F9 57 C0 C5 F9 2E C6 0F 86 ?? ?? ?? ?? E9 ?? ?? ?? ?? C6 86 92 0E 00 00 01", 0x2da7921};
constexpr runtime::Sig SIG_TRANSITION_READ{"session.transition_minimum_read", "80 7C 24 26 00 0F 84 ?? ?? ?? ?? C5 F9 57 C0",
                                           0x36f9904};
constexpr runtime::Sig SIG_STORY_HUB_READ{
    "session.story_hub_minimum_read",
    "80 7C 24 30 00 0F 84 ?? ?? ?? ?? C5 F9 57 C0 C5 F9 2E C6 0F 86 ?? ?? ?? ?? E9 ?? ?? ?? ?? C6 86 40 0D 00 00 01", 0x36fb66c};

struct ReaderSite {
    bp::Gate gate;
    const runtime::Sig *sig;
};

constexpr std::array<ReaderSite, gs::READERS> READER_SITES{{
    {{"the track load minimum skip", &TRACK_LOAD_READ}, &SIG_TRACK_LOAD_READ},
    {{"the session transition skip", &TRANSITION_READ}, &SIG_TRANSITION_READ},
    {{"the story hub minimum skip", &STORY_HUB_READ}, &SIG_STORY_HUB_READ},
}};

std::array<std::atomic<const void *>, gs::READERS> reader_at{};
std::atomic<uintptr_t> image_lo{0}, image_hi{0};
std::atomic<bool> told_foreign{false};

// A read of a reader row's setting whose caller is outside the game: another wrap of the getter runs before the core's,
// so the return address names that wrap, not the reader, and the reader rows answer nothing. Logged once.
void foreign_caller(const char *name, const void *ret)
{
    auto at = reinterpret_cast<uintptr_t>(ret);
    uintptr_t lo = image_lo.load(std::memory_order_relaxed);
    if (!lo || (at >= lo && at < image_hi.load(std::memory_order_relaxed)) || told_foreign.load(std::memory_order_relaxed))
        return;
    if (!name || std::strcmp(name, "Game.IgnoreMinimumLoadingTime") || told_foreign.exchange(true))
        return;
    std::string dll;
    settings_addon(ret, dll);
    swofty::log("[loading] %s read through %s, which wraps Ne_Get_SettingBool before the core does: the core cannot tell "
                "which loading screen asks, so the minimum time skips answer nothing this session",
                name, dll.empty() ? "an unknown module" : dll.c_str());
}

int reader_of(const void *ret)
{
    for (int i = 0; i < gs::READERS; i++)
        if (ret && reader_at[i].load(std::memory_order_relaxed) == ret)
            return i;
    return gs::READERS;
}

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
    const void *ret = __builtin_return_address(0);
    uint64_t r = orig<K>(node, name, out, def);
    if (!out)
        return r;
    const uint32_t rows = armed[size_t(K)].load(std::memory_order_relaxed);
    const gs::Override *row = gs::match(name, rows, reader_of(ret));
    if (!row && rows && K == Type::boolean)
        foreign_caller(name, ret);
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

gs::Toggle player_toggle()
{
    SwoftySettingValue v{};
    if (!::settings_get(gs::PLAYER_OPTION, SWOFTY_SETTING_TOGGLE, &v))
        return gs::Toggle::unset;
    return v.toggle ? gs::Toggle::on : gs::Toggle::off;
}

// Resolves the readers of `row` its proofs cover, and says in `missing` which are left out and why; false when none is
// left.
bool resolve_readers(const gs::Override &row, std::string &missing)
{
    int found = 0;
    for (int i = 0; i < gs::READERS; i++) {
        if (!(row.readers >> i & 1))
            continue;
        const ReaderSite &site = READER_SITES[size_t(i)];
        const char *fault = !runtime::holds(site.gate) ? " is not proven on this build" : nullptr;
        uintptr_t at = fault ? 0 : runtime::find(*site.sig, site.gate.feature, "loading");
        if (!at) {
            missing += std::string(missing.empty() ? "" : "; ") + site.gate.feature + (fault ? fault : " did not resolve");
            continue;
        }
        reader_at[size_t(i)].store(reinterpret_cast<const void *>(at));
        found++;
        if (!image_lo.load()) {
            uintptr_t base = swofty::api->module_base();
            image_hi.store(base + runtime::running_exe().size_of_image);
            image_lo.store(base);
        }
    }
    return found > 0;
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

static std::vector<CoreMixins> loading_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char env[512] = {};
    DWORD n = GetEnvironmentVariableA(ENV, env, sizeof env);
    bool log = false;
    uint32_t from_env = n && n < sizeof env ? gs::parse_list(env, &log) : 0;
    gs::Toggle player = player_toggle();
    uint32_t rows = 0;
    for (size_t i = 0; i < gs::OVERRIDES.size(); i++) {
        const gs::Override &row = gs::OVERRIDES[i];
        gs::State st = gs::feature_state(row.feature, player, from_env >> i & 1, enabled(row.feature, nullptr));
        std::string missing;
        if (st.on && row.readers && !resolve_readers(row, missing))
            st = {false, "off"};
        if (st.on)
            rows |= 1u << i;
        if (gs::on_by_default(row.feature))
            swofty::log("[loading] %s %s%s%s", row.feature, st.why,
                        missing.empty() ? ""
                        : st.on         ? ", except "
                                        : ": ",
                        missing.c_str());
    }
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

static void loading_commit()
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
