#include "core/career/career_ui.hpp"
#include "core/career/characters_logic.hpp"
#include "core/career/characters_state.hpp"
#include "core/registry/older.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/mixin_gate.hpp"

#include <atomic>
#include <chrono>
#include <mutex>
#include <set>
#include <string>
#include <vector>

// Per-career character overrides (SwoftyCharacters). Keyed by career data group; each entry acts
// only while the loaded career's data group is its group, so 2025 and 2026 careers stay stock.
//
// Strings. Each group's strings are runtime string overrides (lang_override.cpp), set when the
// group becomes active and cleared when it stops being active.
//
// The refs and the voices are core mixin sets of their own (runtime/loader/mixin_manager.cpp), offered only once an addon
// needs them: the refs after a swap, the voices after a mute, at the next mixin commit for a
// registration from an addon's init and right after the ON_LOAD handlers for one made there. A game
// where no addon swaps or mutes runs none of them, so an addon that wraps these functions itself
// keeps its wraps. A binding that does not resolve drops only that part.

namespace career::characters {

constexpr const char *REF_MIXIN = "swofty_core.dll (character refs)";
constexpr const char *VOICE_MIXIN = "swofty_core.dll (character voices)";

BuildRef *orig_build;
ListLoaded *orig_loaded;
PostLine *orig_post;
std::mutex lock;
std::string active;
std::set<void *> muted_emitters;
std::atomic<bool> any_swap{false}, any_mute{false};
runtime::loader::MixinPart ref{REF_MIXIN, "ref"}, voice{VOICE_MIXIN, "voice"};

}

using namespace career::characters;

namespace {

namespace cu = career::ui;
namespace mem = swofty::mem;

struct Swap {
    std::string group, character, replacement;
    HMODULE owner;
    std::string addon;
};

struct Mute {
    std::string group, speaker;
    HMODULE owner;
    std::string addon;
};

struct Text {
    std::string group, language, key, text;
    HMODULE owner;
    std::string addon;
};

std::vector<Swap> swaps, eswaps;
std::vector<Mute> mutes, emutes;
std::vector<Text> texts, etexts;
std::vector<std::pair<std::string, std::string>> applied;

namespace older = swofty_core::registry::older;

void rebuild_effective()
{
    auto &st = runtime::older_store();
    std::vector<Swap> sw;
    std::vector<Mute> mu;
    std::vector<Text> tx;
    for (auto &id : older::in_load_order(st, older::CHARACTER)) {
        older::CharacterState c = older::character_from(older::effective(st, older::CHARACTER, id));
        if (c.group.empty())
            continue;
        std::string addon = older::addon_of(id);
        for (auto &[character, with] : c.swaps)
            sw.push_back({c.group, character, with, nullptr, addon});
        for (auto &m : c.mutes)
            mu.push_back({c.group, m, nullptr, addon});
        for (auto &[language, kt] : c.strings)
            tx.push_back({c.group, language, kt.first, kt.second, nullptr, addon});
    }
    eswaps = std::move(sw);
    emutes = std::move(mu);
    etexts = std::move(tx);
}

void store_record(const std::string &addon, const std::string &group)
{
    older::CharacterState c;
    c.group = group;
    for (auto &w : swaps)
        if (w.addon == addon && w.group == group)
            c.swaps.push_back({w.character, w.replacement});
    for (auto &m : mutes)
        if (m.addon == addon && m.group == group)
            c.mutes.push_back(m.speaker);
    for (auto &t : texts)
        if (t.addon == addon && t.group == group)
            c.strings.push_back({t.language, {t.key, t.text}});
    auto &st = runtime::older_store();
    if (c.swaps.empty() && c.mutes.empty() && c.strings.empty())
        older::drop(st, addon, older::CHARACTER, group);
    else
        older::put(st, addon, older::CHARACTER, group, older::character_record(c), "characters");
    rebuild_effective();
}

bool subscribed;

void apply_strings(const std::string &group)
{
    const SwoftyLang *lg = runtime::lang_table();
    std::vector<std::pair<std::string, std::string>> drop;
    std::vector<Text> set;
    {
        std::lock_guard<std::mutex> g(lock);
        drop.swap(applied);
        for (auto &t : etexts)
            if (t.group == group)
                set.push_back(t);
        if (lg && lg->available())
            for (auto &t : set)
                applied.push_back({t.language, t.key});
    }
    if (drop.empty() && set.empty())
        return;
    if (!lg || !lg->available()) {
        if (!set.empty())
            swofty::log("[characters] %zu string(s) for '%s' stay stock: the core's string overrides are unavailable", set.size(),
                        group.c_str());
        return;
    }
    std::vector<runtime::LangOp> ops;
    ops.reserve(drop.size() + set.size());
    for (auto &[language, key] : drop)
        ops.push_back({true, language, key, nullptr});
    for (auto &t : set)
        ops.push_back({false, t.language, t.key, t.text.c_str()});
    auto began = std::chrono::steady_clock::now();
    size_t failed = runtime::lang_batch(ops);
    double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - began).count();
    swofty::log("[characters] strings: cleared %zu, set %zu for '%s' (%zu failed) in %.1f ms", drop.size(), set.size(),
                group.c_str(), failed, ms);
}

bool group_used(const std::string &group) { return career::characters::group_in(eswaps, group); }

void set_active(std::string now, const char *why)
{
    bool refs;
    {
        std::lock_guard<std::mutex> g(lock);
        if (now == active)
            return;
        bool before = group_used(active);
        rebuild_effective();
        refs = before || group_used(now);
        active = now;
        muted_emitters.clear();
    }
    swofty::log("[characters] %s: active group '%s'", why, now.c_str());
    apply_strings(now);
    if (refs)
        rebuild(why);
}

void on_career(const SwoftyEvent *ev, void *)
{
    bool loaded = !strcmp(ev->name, SWOFTY_EVENT_CAREER_LOADED);
    set_active(loaded ? cu::loaded_group() : std::string(), loaded ? "career loaded" : "career unloaded");
}

void subscribe()
{
    if (subscribed)
        return;
    subscribed = true;
    const SwoftyEvents *ev = runtime::events_table();
    uint32_t token = 0;
    for (const char *name : {SWOFTY_EVENT_CAREER_LOADED, SWOFTY_EVENT_CAREER_UNLOADED})
        if (!ev || ev->subscribe(name, on_career, nullptr, &token) != SWOFTY_ASSET_OK)
            swofty::log("[characters] subscribe %s failed; entries apply after refresh() only", name);
}

void changed()
{
    bool s = false, m = false;
    {
        std::lock_guard<std::mutex> g(lock);
        s = !swaps.empty();
        m = !mutes.empty();
    }
    any_swap = s;
    any_mute = m;
}

int need(runtime::loader::MixinPart &p, bool want, const char *what)
{
    if (!runtime::enabled("career", nullptr))
        return SWOFTY_ASSET_UNAVAILABLE;
    if (want)
        p.wanted = true;
    if (p.usable(runtime::core_mixins_closed()))
        return SWOFTY_ASSET_OK;
    if (want && !p.gate.settled() && p.first_late())
        swofty::log("[characters] %s registered after the last mixin commit; the %s hooks are not installed", what, p.label);
    return SWOFTY_ASSET_UNAVAILABLE;
}

int api_available()
{
    return runtime::enabled("career", nullptr) && (ref.live || voice.live || !(ref.gate.settled() || voice.gate.settled()));
}

int api_career_group(char *out, size_t cap)
{
    std::lock_guard<std::mutex> g(lock);
    return cu::copy_out(active, out, cap);
}

int api_swap(const char *group, const char *character, const char *replacement)
{
    if (!cu::valid_name(group) || !cu::valid_name(character) || (replacement && !cu::valid_name(replacement)))
        return SWOFTY_ASSET_INVALID;
    HMODULE owner = cu::caller(__builtin_return_address(0));
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    bool now;
    {
        std::lock_guard<std::mutex> g(lock);
        std::erase_if(swaps, [&](const Swap &s) { return s.group == group && s.character == character && s.owner == owner; });
        for (auto &s : swaps)
            if (s.group == group && s.character == character)
                swofty::log("[characters] '%s' in '%s' is also swapped to '%s' by another addon; the last one wins", character,
                            group, s.replacement.c_str());
        if (replacement)
            swaps.push_back({group, character, replacement, owner, addon});
        store_record(addon, group);
        now = active == group;
    }
    changed();
    subscribe();
    swofty::log("[characters] %s '%s' in careers of '%s'%s%s", replacement ? "swap" : "restore", character, group,
                replacement ? " to " : "", replacement ? replacement : "");
    int r = need(ref, replacement != nullptr, "a swap");
    return r == SWOFTY_ASSET_OK && now ? SWOFTY_ASSET_LATE : r;
}

int api_mute(const char *group, const char *speaker, int on)
{
    if (!cu::valid_name(group) || !cu::valid_name(speaker))
        return SWOFTY_ASSET_INVALID;
    HMODULE owner = cu::caller(__builtin_return_address(0));
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    {
        std::lock_guard<std::mutex> g(lock);
        std::erase_if(mutes, [&](const Mute &m) { return m.group == group && m.speaker == speaker && m.owner == owner; });
        if (on)
            mutes.push_back({group, speaker, owner, addon});
        store_record(addon, group);
    }
    changed();
    subscribe();
    swofty::log("[characters] %s %s lines in careers of '%s'", on ? "mute" : "unmute", speaker, group);
    return need(voice, on != 0, "a mute");
}

bool add_text(const std::string &group, const std::string &language, const std::string &key, const char *text, HMODULE owner,
              const std::string &addon)
{
    std::erase_if(texts,
                  [&](const Text &t) { return t.group == group && t.language == language && t.key == key && t.owner == owner; });
    if (text)
        texts.push_back({group, language, key, text, owner, addon});
    return active == group;
}

int api_string_set(const char *group, const char *language, const char *key, const char *text)
{
    if (!cu::valid_name(group) || !cu::valid_name(key))
        return SWOFTY_ASSET_INVALID;
    HMODULE owner = cu::caller(__builtin_return_address(0));
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    bool now;
    {
        std::lock_guard<std::mutex> g(lock);
        now = add_text(group, career::characters::language_of(language), key, text, owner, addon);
        store_record(addon, group);
    }
    subscribe();
    if (now)
        apply_strings(group);
    return SWOFTY_ASSET_OK;
}

int api_strings_json(const char *group, const char *text, size_t len)
{
    if (!cu::valid_name(group) || !text)
        return SWOFTY_ASSET_INVALID;
    json::Value doc;
    if (!json::parse(std::string(text, len), doc) || doc.type != json::Value::Obj)
        return SWOFTY_ASSET_INVALID;
    for (auto &[language, keys] : doc.o)
        if (keys.type != json::Value::Obj)
            return SWOFTY_ASSET_INVALID;
    HMODULE owner = cu::caller(__builtin_return_address(0));
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    size_t n = 0;
    bool now = false;
    {
        std::lock_guard<std::mutex> g(lock);
        for (auto &[language, keys] : doc.o)
            for (auto &[key, value] : keys.o)
                if (value.type == json::Value::Str && cu::valid_name(key.c_str())) {
                    now = add_text(group, career::characters::language_of(language.c_str()), key, value.s.c_str(), owner, addon);
                    n++;
                }
        store_record(addon, group);
    }
    subscribe();
    swofty::log("[characters] %zu string(s) in %zu language(s) for careers of '%s'", n, doc.o.size(), group);
    if (now)
        apply_strings(group);
    return SWOFTY_ASSET_OK;
}

int api_clear(const char *group)
{
    if (!cu::valid_name(group))
        return SWOFTY_ASSET_INVALID;
    HMODULE owner = cu::caller(__builtin_return_address(0));
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    bool now;
    {
        std::lock_guard<std::mutex> g(lock);
        std::erase_if(swaps, [&](const Swap &s) { return s.group == group && s.owner == owner; });
        std::erase_if(mutes, [&](const Mute &m) { return m.group == group && m.owner == owner; });
        std::erase_if(texts, [&](const Text &t) { return t.group == group && t.owner == owner; });
        store_record(addon, group);
        now = active == group;
    }
    changed();
    if (now)
        apply_strings(group);
    return SWOFTY_ASSET_OK;
}

int api_refresh()
{
    std::string now = cu::loaded_group();
    {
        std::lock_guard<std::mutex> g(lock);
        if (now != active) {
            active.swap(now);
            muted_emitters.clear();
            now = active;
        }
    }
    apply_strings(now);
    return rebuild("refresh") ? SWOFTY_ASSET_OK : SWOFTY_ASSET_LATE;
}

const SwoftyCharacters table = {api_available,  api_career_group, api_swap,  api_mute,
                                api_string_set, api_strings_json, api_clear, api_refresh};

}

namespace career::characters {

std::string replacement_for(const std::string &character)
{
    std::lock_guard<std::mutex> g(lock);
    return career::characters::replacement_in(eswaps, active, character);
}

bool speaker_muted(const char *speaker)
{
    std::lock_guard<std::mutex> g(lock);
    return career::characters::muted_in(emutes, active, speaker);
}

}

namespace runtime {

const SwoftyCharacters *characters_table() { return &table; }

static std::vector<CoreMixins> characters_mixins()
{
    std::vector<CoreMixins> out;
    if (!enabled("career", nullptr))
        return out;
    if (ref.offer())
        out.push_back(
            {REF_MIXIN,
             {{"Game_EntrantsUi_BuildCharacterRef", SWOFTY_MIXIN_WRAP, 100, (void *)&build_ref, (void **)&orig_build},
              {"Game_EntrantsUi_OnCharacterListLoaded", SWOFTY_MIXIN_WRAP, 100, (void *)&list_loaded, (void **)&orig_loaded}}});
    if (voice.offer())
        out.push_back(
            {VOICE_MIXIN, {{"Game_SpeechSystem_PostLine", SWOFTY_MIXIN_WRAP, 100, (void *)&post_line, (void **)&orig_post}}});
    return out;
}

static void characters_commit()
{
    bool any = false;
    for (runtime::loader::MixinPart *p : {&ref, &voice})
        any |= p->settle([p] { return core_mixins_active(p->name); });
    if (any)
        swofty::log("[characters] setup: character refs %s, voices %s", ref.state(), voice.state());
}

static loader::FunctionSet characters_core_set{loader::CoreSetId::characters, characters_mixins, characters_commit};

}
