#include "core/runtime/addon_api.hpp"

#include <cstdlib>
#include <unordered_map>
#include <utility>

#include "core/hooks/hooks.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"
#include "core/services/services.hpp"

namespace runtime {

namespace {

using loader::api;

void *api_fields(const void *obj, size_t size)
{
    struct Key {
        const void *obj;
        HMODULE mod;
        bool operator==(const Key &) const = default;
    };
    struct Hash {
        size_t operator()(const Key &k) const { return std::hash<const void *>()(k.obj) ^ (uintptr_t)k.mod; }
    };
    static std::unordered_map<Key, std::pair<size_t, void *>, Hash> table;
    static SRWLOCK lock = SRWLOCK_INIT;
    HMODULE mod = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(__builtin_return_address(0)), &mod);
    void *out = nullptr;
    size_t held = 0;
    AcquireSRWLockExclusive(&lock);
    auto it = table.find({obj, mod});
    if (!size) {
        if (it != table.end())
            free(it->second.second), table.erase(it);
    } else if (it == table.end()) {
        out = calloc(1, size);
        table[{obj, mod}] = {size, out};
    } else if (it->second.first == size) {
        out = it->second.second;
    } else {
        held = it->second.first;
    }
    ReleaseSRWLockExclusive(&lock);
    if (held)
        api->log("[core] fields(%p, %zu): this addon already keeps %zu bytes on that object", obj, size, held);
    return out;
}

}

void publish_addon_api(SwoftyApi *a)
{
    a->resolve = loader::resolve_for_addon;
    a->register_mixins = loader::register_addon_mixins;
    a->fields = api_fields;
    a->setting = settings_get;
    a->watch_setting = settings_watch;
    a->assets = runtime::assets_table();
    a->events = runtime::events_table();
    a->overlay = runtime::overlay_table();
    a->telemetry = runtime::telemetry_table();
    a->db = runtime::db_table();
    a->storage = runtime::storage_table();
    a->career = runtime::career_table();
    a->popup = runtime::popup_table();
    a->lang = runtime::lang_table();
    a->career_content = runtime::career_content_table();
    a->characters = runtime::characters_table();
    a->career_wizard = runtime::wizard_table();
    a->era = runtime::era_table();
    a->registry = runtime::registry_table();
    a->hooks = swofty_core::hooks::table();
    a->services = swofty_core::services::table();
    a->version = SWOFTY_API_VERSION;
}

}
