#include "core/assets/lang_scope.hpp"
#include "core/assets/lang_lookup.hpp"
#include "core/hooks/hooks.hpp"
#include "core/hooks/hot_table.hpp"
#include "core/registry/older.hpp"
#include "core/runtime/runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstring>
#include <map>
#include <mutex>
#include <set>
#include <string>
#include <string_view>
#include <unordered_map>
#include <unordered_set>
#include <vector>

// Runtime string overrides (SwoftyLang, API v8). Every lng_* lookup reaches Loc_LangSet_Find, the
// language set's virtual find (the set's other lookups call it through the vtable). Once an addon
// sets its first override the core wraps that function: while the map is non-empty a key in it
// returns the override text, otherwise the game's own lookup runs; with no override set the
// function is not hooked at all. Overrides apply to every language ("*") or to one language code,
// which wins while it is the game's current language (Loc_LocaleModule_Init's module +0x160 into
// g_Loc_LanguageInfo +0x60). Each distinct text is copied once into storage that is never freed,
// because the game may keep a returned pointer after the override is cleared. refresh() sends the current language
// to the module's listeners (+0x170), the notification the game sends after it loads a language.
// A key keeps one text variant per lang.string record in load order; a record's within is checked
// against the core's current facts at lookup, so a season's strings show only in that season.
// text() (SwoftyLang version 2) answers a key in the current language: the override when one applies,
// else the current language's set (module +0x28 + language * 0x10, Loc_LocaleModule_Init) through
// Loc_LangSet_Find mode 0, the trampoline once the function is hooked.

namespace {

const runtime::Sig S_SET_FIND{"Loc_LangSet_Find", "41 57 41 56 41 54 56 57 53 48 83 EC 28 44 89 C7 49 89 D6 48 89 CB", 0x3acf800};
const runtime::Sig S_MODULE_INIT{"Loc_LocaleModule_Init", "41 57 41 56 41 55 41 54 56 57 53 48 81 EC A0 03 00 00", 0x4435970};
const runtime::Sig S_LANG_INFO{
    "g_Loc_LanguageInfo", "48 8D 0D ?? ?? ?? ?? 48 8B 5C 08 60", 0x62925e8, runtime::Sig::riprel, 0, 3, 7};

constexpr uint32_t LANGS = 0x13;
constexpr size_t MODULE_LANG = 0x160, MODULE_LISTENERS = 0x170, MODULE_SETS = 0x28, SET_STRIDE = 0x10, INFO_STRIDE = 0x100,
                 INFO_CODE = 0x60;

using Find_t = const char *(void *set, const char *key, int32_t mode);
using Init_t = int32_t(void *module);
using Listener_t = void(void *ctx, uint32_t language, uint32_t loaded);

struct Hash {
    using is_transparent = void;
    size_t operator()(std::string_view s) const noexcept { return std::hash<std::string_view>{}(s); }
};

struct Eq {
    using is_transparent = void;
    bool operator()(std::string_view a, std::string_view b) const noexcept { return a == b; }
};

namespace lang_scope = swofty_core::lang_scope;

struct Entry {
    std::vector<lang_scope::Variant> variants;
    bool scoped = false;
    bool seen = false;
    const char *last = nullptr;
};

struct Raw {
    std::string key;
    std::vector<std::pair<std::string, std::string>> texts;
    std::shared_ptr<const swofty_core::hooks::Selectors> within;
};

bool g_on;
std::once_flag g_setup_once;
uintptr_t g_find_at;
safetyhook::InlineHook h_find, h_init;
std::mutex g_install;
bool g_find_failed;
SRWLOCK g_lock = SRWLOCK_INIT;
std::atomic<size_t> g_count{0};
std::unordered_map<std::string, Entry, Hash, Eq> g_map;
std::unordered_set<std::string, Hash, Eq> g_texts;
std::atomic<uint8_t *> g_module{nullptr};
std::mutex g_seen;
uint8_t *g_info;

std::string lower(std::string_view s)
{
    std::string out(s);
    for (char &c : out)
        if (c >= 'A' && c <= 'Z')
            c = char(c - 'A' + 'a');
    return out;
}

bool all_languages(const char *language) { return !language || !*language || !strcmp(language, "*"); }

const char *current_code()
{
    uint8_t *m = g_module.load(std::memory_order_acquire);
    if (!m || !g_info)
        return nullptr;
    uint32_t lang = *reinterpret_cast<const uint32_t *>(m + MODULE_LANG);
    if (lang >= LANGS)
        return nullptr;
    return *reinterpret_cast<const char *const *>(g_info + size_t(lang) * INFO_STRIDE + INFO_CODE);
}

const char *lookup(const char *key)
{
    const char *out = nullptr;
    AcquireSRWLockShared(&g_lock);
    auto it = g_map.find(std::string_view(key));
    if (it != g_map.end()) {
        Entry &e = it->second;
        if (!e.scoped)
            out = lang_scope::choose(e.variants, nullptr, current_code());
        else {
            auto facts = swofty_core::hooks::core().current();
            out = lang_scope::choose(e.variants, facts.get(), current_code());
            std::lock_guard<std::mutex> g(g_seen);
            if (!e.seen || e.last != out) {
                e.seen = true;
                e.last = out;
                std::string where = facts ? swofty_core::hooks::summary(*facts) : std::string("no facts yet");
                if (out)
                    swofty::log("[lang] override: %s shows \"%s\" (%s)", key, out, where.c_str());
                else
                    swofty::log("[lang] override: %s shows the game's own text: no record for it applies here (%s)", key,
                                where.c_str());
            }
        }
    }
    ReleaseSRWLockShared(&g_lock);
    return out;
}

namespace older = swofty_core::registry::older;

std::mutex g_shim;
std::map<std::string, std::map<std::string, swofty::json::Value>> g_layers;

bool raw_of(const std::string &id, const swofty::json::Value &rec, Raw &out)
{
    out.key = older::key_of(id);
    out.texts = older::string_texts(rec);
    if (out.texts.empty())
        return false;
    std::string why;
    if (!lang_scope::compile_within(rec["within"], out.within, &why)) {
        swofty::log("[lang] override: %s is not used: its within does not compile (%s)", id.c_str(), why.c_str());
        return false;
    }
    return true;
}

using KeySet = std::unordered_set<std::string, Hash, Eq>;

std::vector<Raw> gather(const KeySet *only)
{
    auto &st = runtime::older_store();
    std::vector<Raw> out;
    for (auto &id : older::string_ids(st)) {
        if (only && !only->count(std::string_view(older::key_of(id))))
            continue;
        Raw r;
        if (raw_of(id, older::effective(st, older::STRING, id), r))
            out.push_back(std::move(r));
    }
    return out;
}

size_t install(std::vector<Raw> raws, const KeySet *only)
{
    size_t scoped = 0;
    AcquireSRWLockExclusive(&g_lock);
    if (only)
        for (auto &k : *only)
            g_map.erase(k);
    else
        g_map.clear();
    for (auto &r : raws) {
        lang_scope::Variant v;
        v.within = std::move(r.within);
        for (auto &[language, text] : r.texts) {
            const char *copy = g_texts.emplace(text).first->c_str();
            if (all_languages(language.c_str()))
                v.all = copy;
            else
                v.per.emplace_back(lower(language), copy);
        }
        Entry &e = g_map[r.key];
        e.scoped = e.scoped || v.within;
        e.variants.push_back(std::move(v));
    }
    for (auto &[k, e] : g_map)
        scoped += e.scoped;
    g_count.store(g_map.size(), std::memory_order_release);
    ReleaseSRWLockExclusive(&g_lock);
    return scoped;
}

void refresh_keys(const KeySet &keys) { install(gather(&keys), &keys); }

void refresh_key(const std::string &key) { refresh_keys(KeySet{key}); }

size_t refresh_all(size_t *scoped = nullptr)
{
    size_t s = install(gather(nullptr), nullptr);
    if (scoped)
        *scoped = s;
    return g_count.load(std::memory_order_acquire);
}

void put_layer(const std::string &addon, const std::string &key)
{
    auto &st = runtime::older_store();
    auto a = g_layers.find(addon);
    if (a == g_layers.end())
        return;
    auto k = a->second.find(key);
    if (k == a->second.end() || !k->second.is_object() || !k->second.size()) {
        older::string_layer(st, addon, key, swofty::json::Value());
        if (k != a->second.end())
            a->second.erase(k);
        return;
    }
    older::string_layer(st, addon, key, k->second);
}

void flush_keys(const std::map<std::string, std::set<std::string>> &changed, KeySet all)
{
    auto &st = runtime::older_store();
    for (auto &[addon, keys] : changed) {
        std::vector<std::pair<std::string, swofty::json::Value>> texts;
        texts.reserve(keys.size());
        auto a = g_layers.find(addon);
        for (auto &key : keys) {
            all.insert(key);
            swofty::json::Value v;
            if (a != g_layers.end()) {
                auto k = a->second.find(key);
                if (k != a->second.end()) {
                    if (k->second.is_object() && k->second.size())
                        v = k->second;
                    else
                        a->second.erase(k);
                }
            }
            texts.emplace_back(key, std::move(v));
        }
        if (a != g_layers.end())
            older::string_layers(st, addon, texts);
    }
    if (!all.empty())
        refresh_keys(all);
}

std::atomic<bool> g_lookup_on{false};
bool g_lookup_wanted;
swofty_core::hooks::HotTable<const char *> &hot_table()
{
    static swofty_core::hooks::HotTable<const char *> t;
    return t;
}

std::mutex g_hot_texts_lock;
std::unordered_set<std::string, Hash, Eq> g_hot_texts;
std::atomic<int> g_hot_told{0};
constexpr int HOT_LOGGED = 8;

const char *hot_intern(const std::string &text)
{
    std::lock_guard<std::mutex> g(g_hot_texts_lock);
    return g_hot_texts.emplace(text).first->c_str();
}

uint64_t hot_epoch()
{
    uint8_t *m = g_module.load(std::memory_order_acquire);
    uint64_t language = m ? *reinterpret_cast<const uint32_t *>(m + MODULE_LANG) & 0x1f : 0x1f;
    return runtime::lang_lookup_epoch() << 5 | language;
}

thread_local bool t_in_lookup;

const char *hot_find(void *set, const char *key)
{
    const char *own = g_count.load(std::memory_order_acquire) ? lookup(key) : nullptr;
    if (t_in_lookup)
        return own ? own : h_find.call<const char *>(set, key, 0);
    t_in_lookup = true;
    const char *supplied = hot_table().resolve(
        key, hot_epoch(),
        [&]() -> std::pair<const char *, bool> {
            const char *stock = own ? own : h_find.call<const char *>(set, key, 0);
            auto text = runtime::lang_lookup_fire(key, current_code() ? current_code() : "", stock);
            if (!text)
                return {nullptr, false};
            if (g_hot_told.fetch_add(1) < HOT_LOGGED)
                swofty::log("[hooks] lang.lookup (hot): %s -> \"%.80s\" supplied by a handler", key, text->c_str());
            return {hot_intern(*text), true};
        },
        [](const swofty_core::hooks::HotStats &old) {
            swofty::log("[hooks] lang.lookup (hot): table reset: %zu key(s) looked up, %zu supplied by handlers", old.entries,
                        old.supplied);
        });
    t_in_lookup = false;
    return supplied ? supplied : own ? own : h_find.call<const char *>(set, key, 0);
}

const char *find_hook(void *set, const char *key, int32_t mode)
{
    if (key && mode == 0 && g_lookup_on.load(std::memory_order_acquire))
        return hot_find(set, key);
    if (g_count.load(std::memory_order_acquire) && key)
        if (const char *text = lookup(key))
            return text;
    return h_find.call<const char *>(set, key, mode);
}

int32_t init_hook(void *module)
{
    int32_t r = h_init.call<int32_t>(module);
    g_module.store(static_cast<uint8_t *>(module), std::memory_order_release);
    swofty::log("[lang] locale module %p initialised (%d); current language %s", module, r,
                current_code() ? current_code() : "?");
    return r;
}

bool ensure_find_hook()
{
    std::lock_guard<std::mutex> g(g_install);
    if (h_find)
        return true;
    if (g_find_failed || !g_find_at)
        return false;
    auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(g_find_at), reinterpret_cast<void *>(&find_hook));
    if (!r) {
        g_find_failed = true;
        swofty::log("[lang] override: hook on Loc_LangSet_Find at base+0x%llx failed; string overrides off",
                    (unsigned long long)(g_find_at - swofty::api->module_base()));
        return false;
    }
    h_find = std::move(*r);
    swofty::log("[lang] override: Loc_LangSet_Find hooked at base+0x%llx",
                (unsigned long long)(g_find_at - swofty::api->module_base()));
    return true;
}

void setup_once();

bool start_lookup()
{
    if (!ensure_find_hook())
        return false;
    g_lookup_on.store(true, std::memory_order_release);
    return true;
}

class LookupLink final : public swofty_core::hooks::GameLink {
public:
    bool install(bool late, const std::string &subscribers) override
    {
        setup_once();
        g_lookup_wanted = true;
        if (!g_on || !g_find_at) {
            if (runtime::code_ready())
                swofty::log("[hooks] lang.lookup unavailable: Loc_LangSet_Find does not resolve on this build; subscribers %s "
                            "not run",
                            subscribers.c_str());
            return false;
        }
        if (g_lookup_on.load() || start_lookup()) {
            swofty::log("[hooks] lang.lookup live (hot) on Loc_LangSet_Find%s: lookups run the handlers once per key and "
                        "content; subscribers %s",
                        late ? " (installed late)" : " (window stage)", subscribers.c_str());
            return true;
        }
        swofty::log("[hooks] lang.lookup unavailable: the hook on Loc_LangSet_Find failed; subscribers %s not run",
                    subscribers.c_str());
        return false;
    }

    void uninstall() override
    {
        g_lookup_wanted = false;
        if (g_lookup_on.exchange(false)) {
            hot_table().clear();
            swofty::log("[hooks] lang.lookup has no subscriber left; lookups skip the handlers");
        }
    }

    bool installed() const override { return g_lookup_on.load(); }
};

int lang_available()
{
    setup_once();
    return g_on && g_find_at && !g_find_failed;
}

int lang_override_set(const char *language, const char *key, const char *text)
{
    setup_once();
    if (!g_on || !g_find_at)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!key || !*key || !text)
        return SWOFTY_ASSET_INVALID;
    if (!ensure_find_hook())
        return SWOFTY_ASSET_UNAVAILABLE;
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    {
        std::lock_guard<std::mutex> g(g_shim);
        swofty::json::Value &t = g_layers[addon][key];
        if (!t.is_object())
            t = swofty::json::Value::object();
        t[all_languages(language) ? std::string("*") : lower(language)] = text;
        put_layer(addon, key);
        refresh_key(key);
    }
    return SWOFTY_ASSET_OK;
}

int lang_override_clear(const char *language, const char *key)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!key || !*key)
        return SWOFTY_ASSET_INVALID;
    std::lock_guard<std::mutex> g(g_shim);
    for (auto &[addon, keys] : g_layers) {
        auto k = keys.find(key);
        if (k == keys.end())
            continue;
        if (all_languages(language))
            k->second = swofty::json::Value();
        else
            k->second.erase(lower(language));
        put_layer(addon, key);
    }
    refresh_key(key);
    return SWOFTY_ASSET_OK;
}

int lang_override_clear_all()
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> g(g_shim);
    for (auto &[addon, keys] : g_layers) {
        std::vector<std::string> names;
        for (auto &[k, t] : keys)
            names.push_back(k);
        for (auto &k : names) {
            keys[k] = swofty::json::Value();
            put_layer(addon, k);
        }
    }
    g_layers.clear();
    AcquireSRWLockExclusive(&g_lock);
    g_map.clear();
    g_count.store(0, std::memory_order_release);
    ReleaseSRWLockExclusive(&g_lock);
    refresh_all();
    return SWOFTY_ASSET_OK;
}

int lang_refresh()
{
    uint8_t *m = g_module.load(std::memory_order_acquire);
    if (!g_on || !m)
        return SWOFTY_ASSET_UNAVAILABLE;
    auto *list = *reinterpret_cast<uint8_t **>(m + MODULE_LISTENERS);
    if (!list || !runtime::readable(list, 0x18))
        return SWOFTY_ASSET_UNAVAILABLE;
    auto *b = *reinterpret_cast<uint8_t **>(list + 0x08), *e = *reinterpret_cast<uint8_t **>(list + 0x10);
    uint32_t lang = *reinterpret_cast<const uint32_t *>(m + MODULE_LANG);
    size_t n = 0;
    for (; b && b < e; b += 0x10, n++) {
        auto fn = *reinterpret_cast<Listener_t **>(b);
        if (fn)
            fn(*reinterpret_cast<void **>(b + 0x08), lang, 1);
    }
    swofty::log("[lang] override: refresh sent language %u to %zu listener(s)", lang, n);
    return SWOFTY_ASSET_OK;
}

int copy_out(const char *text, char *out, size_t cap, size_t *len)
{
    size_t n = strlen(text);
    if (len)
        *len = n;
    if (!out || n >= cap)
        return SWOFTY_TOO_SMALL;
    memcpy(out, text, n + 1);
    return SWOFTY_ASSET_OK;
}

const char *game_text(uint8_t *m, const char *key)
{
    uint32_t lang = *reinterpret_cast<const uint32_t *>(m + MODULE_LANG);
    if (lang >= LANGS)
        return nullptr;
    auto *slot = m + MODULE_SETS + size_t(lang) * SET_STRIDE;
    void *set = runtime::readable(slot, sizeof(void *)) ? *reinterpret_cast<void **>(slot) : nullptr;
    if (!set || !runtime::readable(set, sizeof(void *)))
        return nullptr;
    bool hooked;
    {
        std::lock_guard<std::mutex> g(g_install);
        hooked = bool(h_find);
    }
    if (hooked)
        return h_find.call<const char *>(set, key, 0);
    return reinterpret_cast<Find_t *>(g_find_at)(set, key, 0);
}

int lang_text(const char *key, char *out, size_t cap, size_t *len)
{
    setup_once();
    uint8_t *m = g_module.load(std::memory_order_acquire);
    if (!g_on || !g_find_at || !m)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!key || !*key)
        return SWOFTY_ASSET_INVALID;
    const char *text = g_count.load(std::memory_order_acquire) ? lookup(key) : nullptr;
    if (!text)
        text = game_text(m, key);
    if (!text)
        return SWOFTY_ASSET_INVALID;
    return copy_out(text, out, cap, len);
}

int lang_language(char *out, size_t cap, size_t *len)
{
    setup_once();
    const char *code = g_on ? current_code() : nullptr;
    if (!code || !runtime::readable(code, 1))
        return SWOFTY_ASSET_UNAVAILABLE;
    return copy_out(code, out, cap, len);
}

uintptr_t find(const runtime::Sig &s)
{
    uintptr_t at = 0;
    if (swofty::api->scan(".xtls", s.pattern, &at) != 1 || !at)
        return 0;
    if (s.kind == runtime::Sig::riprel)
        return at + s.insn + s.len + *reinterpret_cast<const int32_t *>(at + s.anchor);
    return at + s.anchor;
}

void setup()
{
    if (!runtime::enabled("lang", nullptr))
        return;
    g_on = true;
    g_find_at = find(S_SET_FIND);
    g_info = reinterpret_cast<uint8_t *>(find(S_LANG_INFO));
    if (uintptr_t at = find(S_MODULE_INIT)) {
        auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(&init_hook));
        if (r)
            h_init = std::move(*r);
        else
            swofty::log("[lang] override: hook on Loc_LocaleModule_Init failed; per-language overrides and refresh off");
    }
    swofty::log("[lang] override: %s (find %s, language table %s, module hook %s)",
                g_find_at ? "ready, hooked on first use" : "off: Loc_LangSet_Find not found", g_find_at ? "found" : "missing",
                g_info ? "found" : "missing", h_init ? "on" : "off");
    if (!g_find_at)
        return;
    if (g_lookup_wanted && start_lookup())
        swofty::log("[hooks] lang.lookup live (hot) on Loc_LangSet_Find (set up after the first subscriber)");
    std::lock_guard<std::mutex> g(g_shim);
    size_t scoped = 0;
    if (size_t n = refresh_all(&scoped)) {
        swofty::log("[lang] override: %zu string(s) from lang.string records, %zu of them only within their scope", n, scoped);
        ensure_find_hook();
    }
}

void setup_once()
{
    if (runtime::code_ready())
        std::call_once(g_setup_once, setup);
}

const SwoftyLang table = {lang_available, lang_override_set, lang_override_clear, lang_override_clear_all,
                          lang_refresh,   lang_text,         lang_language};

}

extern "C" __declspec(dllexport) uint32_t SwoftyCoreLangVersion(void) { return SWOFTY_LANG_VERSION; }

const SwoftyLang *runtime::lang_table() { return &table; }

void runtime::lang_override_setup() { setup_once(); }

swofty_core::hooks::GameLink *runtime::lang_lookup_link()
{
    static LookupLink link;
    return &link;
}

size_t runtime::lang_batch(const std::vector<runtime::LangOp> &ops)
{
    setup_once();
    if (ops.empty())
        return 0;
    if (!g_on || !g_find_at)
        return ops.size();
    bool any_set = false;
    for (auto &o : ops)
        any_set = any_set || (!o.clear && !o.key.empty() && o.text);
    if (any_set && !ensure_find_hook())
        return ops.size();
    std::string addon = runtime::older_addon(__builtin_return_address(0));
    size_t failed = 0;
    std::lock_guard<std::mutex> g(g_shim);
    std::map<std::string, std::set<std::string>> changed;
    KeySet keys_seen;
    for (auto &o : ops) {
        if (o.key.empty() || (!o.clear && !o.text)) {
            failed++;
            continue;
        }
        keys_seen.insert(o.key);
        const char *language = o.language.c_str();
        if (o.clear) {
            for (auto &[owner, keys] : g_layers) {
                auto k = keys.find(o.key);
                if (k == keys.end())
                    continue;
                if (all_languages(language))
                    k->second = swofty::json::Value();
                else
                    k->second.erase(lower(language));
                changed[owner].insert(o.key);
            }
            continue;
        }
        swofty::json::Value &t = g_layers[addon][o.key];
        if (!t.is_object())
            t = swofty::json::Value::object();
        t[all_languages(language) ? std::string("*") : lower(language)] = o.text;
        changed[addon].insert(o.key);
    }
    flush_keys(changed, std::move(keys_seen));
    return failed;
}
