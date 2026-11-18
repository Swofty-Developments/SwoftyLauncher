#pragma once

// Engine settings (system/config.xml in game.dat) the game reads through its typed getters, forced to a value while a
// core feature flag is on. Every row is a developer switch or a tuning value the game already reads; the research
// behind each is in re/research/loading_pipeline.md and the full list of names in re/research/engine_settings.tsv.
//
// Getters on the KB build, (node, name, out*, default) each, default in r9 or xmm3, status 0 found / 2 missing:
//   bool  Ne_Get_SettingBool 0x143a43b90                       350 call sites
//   u32   Ne_Get_SettingU32 0x143a43240                         97
//   i32   Ne_Map_FindOrDefaultWithErrorContext 0x143a42f10      27 (strtol)
//   float Ne_Config_GetFloatOrDefault 0x143a44090               67 (4-byte store)
// Ne_Get_SettingDouble 0x143a43ea0 (8-byte store, 4 sites) has a binding record but no agree row yet, so it is not
// wrapped.

#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <span>

namespace runtime::game_settings {

enum class Type : uint8_t { boolean, u32, i32, f32 };
inline constexpr size_t TYPES = 4;

struct Override {
    const char *feature;
    const char *setting;
    Type type;
    uint32_t bits;
    const char *effect;
};

using Table = std::span<const Override>;

constexpr uint32_t of_bool(bool v) { return v ? 1 : 0; }

constexpr uint32_t of_f32(float v) { return std::bit_cast<uint32_t>(v); }

constexpr uint32_t of_i32(int32_t v) { return std::bit_cast<uint32_t>(v); }

// Game.IgnoreMinimumLoadingTime: Sess_SessionTransition_OnStateUpdate (0x1436f9890), Sess_Loader_RequestTrackLoad
// (0x142da7870) and Sess_StoryHub_OnStateEnter (0x1436fb5e0) each hold their state until 3.0 s have passed since it
// started, or 0.0 s with the setting on.
// Game.SkipIntroVideo: Sess_IntroVideo_ParseParams (0x142d6cc30) copies it to SessionStateIntroVideo+0xdc6, which
// lets Sess_IntroVideo_CanFinish (0x142d6d3b0) end the video before a session at once.
// Game.SkipGridRundown: Game_GridRundown_ShouldPlay (0x140ac6b30) returns false before any other test, so
// Sess_Loader_RequestLoadThumbnails (0x142d4d040) neither enters loader step 0x1e, asks the thumbnail system for the
// grid render (reason 3) nor queues the 32 grid slots' thumbnails, and the rundown does not play.
// Game.SkipTalkingPoints: SessionStateTalkingPoints' parse params (0x142d94e20, a jump to its body 0x14eb3abf0) stores
// +0xea1 = 1 with it on, so its enter (0x142d94f30) generates no broadcast sequence and its update takes the state's own
// `skipped` link on the first frame. The reader record (re/bindings/game/Game_SessionStateTalkingPoints_ParseParams_Thunk.json)
// has its author's proof only and still needs a second contributor's agree row.
inline constexpr std::array<Override, 4> OVERRIDES{{
    {"loading_skip_minimum_time", "Game.IgnoreMinimumLoadingTime", Type::boolean, of_bool(true),
     "the 3 s minimum of the session transition, track load and story hub loading screens is dropped"},
    {"loading_skip_session_intro_video", "Game.SkipIntroVideo", Type::boolean, of_bool(true),
     "the intro video before a session may end as soon as it starts"},
    {"loading_skip_grid_rundown", "Game.SkipGridRundown", Type::boolean, of_bool(true),
     "the race load queues no grid rundown thumbnails and the rundown does not play"},
    {"loading_skip_talking_points", "Game.SkipTalkingPoints", Type::boolean, of_bool(true),
     "the talking points before a race or practice session take their skipped link at once"},
}};

// Not an override: every distinct name and value the wrapped getters return is logged once (and again when it
// changes), so a run shows what system/config.xml sets, which the static read cannot see inside game.dat.
inline constexpr char LOG_FEATURE[] = "loading_log_settings";

constexpr uint32_t rows_all(Table t = OVERRIDES) { return t.size() >= 32 ? ~0u : (1u << t.size()) - 1; }

constexpr bool same(const char *a, const char *b)
{
    while (*a && *a == *b)
        a++, b++;
    return *a == *b;
}

// The rows of `feature`.
constexpr uint32_t rows_of(const char *feature, Table t = OVERRIDES)
{
    uint32_t bits = 0;
    for (size_t i = 0; i < t.size(); i++)
        if (same(t[i].feature, feature))
            bits |= 1u << i;
    return bits;
}

// The rows of `type` among `rows`.
constexpr uint32_t rows_typed(uint32_t rows, Type type, Table t = OVERRIDES)
{
    uint32_t bits = 0;
    for (size_t i = 0; i < t.size(); i++)
        if ((rows >> i & 1) && t[i].type == type)
            bits |= 1u << i;
    return bits;
}

// The armed row whose setting is `name` among `rows` (already narrowed to one getter's type), or nullptr. Every read
// of that getter passes through here; with no armed row it returns at once.
inline const Override *match(const char *name, uint32_t rows, Table t = OVERRIDES)
{
    if (!rows || !name)
        return nullptr;
    for (size_t i = 0; i < t.size(); i++)
        if ((rows >> i & 1) && name[0] == t[i].setting[0] && std::strcmp(name, t[i].setting) == 0)
            return &t[i];
    return nullptr;
}

inline size_t index_of(const Override *row, Table t = OVERRIDES) { return row ? size_t(row - t.data()) : t.size(); }

// The rows a comma-separated feature list names (the SWOFTY_LOADING_FEATURES variable that in-game measurements set,
// so a run can arm a flag without a launcher setting); `log` is set when it names LOG_FEATURE. Unknown names and
// empty items are ignored.
inline uint32_t parse_list(const char *list, bool *log = nullptr, Table t = OVERRIDES)
{
    uint32_t bits = 0;
    for (const char *p = list; p && *p;) {
        const char *end = std::strchr(p, ',');
        size_t n = end ? size_t(end - p) : std::strlen(p);
        while (n && (*p == ' ' || *p == '\t'))
            p++, n--;
        while (n && (p[n - 1] == ' ' || p[n - 1] == '\t'))
            n--;
        for (size_t i = 0; i < t.size(); i++)
            if (n && std::strlen(t[i].feature) == n && std::strncmp(t[i].feature, p, n) == 0)
                bits |= 1u << i;
        if (log && n == sizeof LOG_FEATURE - 1 && std::strncmp(LOG_FEATURE, p, n) == 0)
            *log = true;
        if (!end)
            break;
        p = end + 1;
    }
    return bits;
}

// The (name, type, value) triples already logged, keyed by the name's address: the game passes string literals, so a
// name is one address. seen() is true for a triple recorded with the same value; a new name or a changed value is
// recorded and returns false. Once the table is full every further new name returns true and is not logged. Not
// thread-safe: the caller holds a lock.
template <size_t N> struct SeenTable {
    static_assert(N && (N & (N - 1)) == 0, "a power of two");
    struct Slot {
        const void *name;
        uint8_t type;
        uint32_t bits;
    };
    std::array<Slot, N> slots{};
    size_t used = 0;

    bool seen(const void *name, Type type, uint32_t bits)
    {
        size_t h = (size_t(reinterpret_cast<uintptr_t>(name) >> 3) ^ size_t(type) * 0x9e3779b9u) & (N - 1);
        for (size_t k = 0; k < N; k++, h = (h + 1) & (N - 1)) {
            Slot &s = slots[h];
            if (!s.name) {
                if (used + 1 >= N)
                    return true;
                s = {name, uint8_t(type), bits};
                used++;
                return false;
            }
            if (s.name == name && s.type == uint8_t(type)) {
                if (s.bits == bits)
                    return true;
                s.bits = bits;
                return false;
            }
        }
        return true;
    }
};

}
