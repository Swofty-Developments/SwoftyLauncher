#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdio>
#include <memory>
#include <string>

#include "core/assets/lang_lookup.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/hooks/game_point.hpp"
#include "core/hooks/hooks.hpp"
#include "core/hooks/hot_table.hpp"
#include "core/hooks/ui_hot.hpp"

namespace swofty_core::hooks {
namespace {

using Resolve_t = void *(void *registry, const char *path, const void *base, void *create);

Table *g_table;
std::unique_ptr<GamePoint> g_point;
HotTable<void *> &hot_table()
{
    static HotTable<void *> t;
    return t;
}

thread_local bool t_in_resolve;
std::atomic<int> g_told{0};
constexpr int LOGGED = 8;

bool handle_ok(void *handle)
{
    void *node = nullptr;
    return swofty::mem::readable(handle, sizeof(void *)) && swofty::mem::read(uintptr_t(handle), node) && node &&
           swofty::mem::readable(node, 0x68);
}

uint64_t epoch() { return g_table->epoch() << 24 ^ uint64_t(frontend::ui_vm_epoch()); }

void *d_resolve(void *registry, const char *path, const void *base, void *create)
{
    if (create || !path || !*path || t_in_resolve)
        return g_point->original<void *>(registry, path, base, create);
    uintptr_t from = 0;
    if (base && swofty::mem::readable(base, sizeof(uintptr_t)))
        swofty::mem::read(uintptr_t(base), from);
    thread_local std::string key;
    key.assign(path);
    if (from) {
        char tail[24];
        std::snprintf(tail, sizeof tail, "\x01%llx", (unsigned long long)from);
        key += tail;
    }
    t_in_resolve = true;
    void *supplied = hot_table().resolve(
        key, epoch(),
        [&]() -> std::pair<void *, bool> {
            void *node = ui_hot::fire_vm(*g_table, path, handle_ok, [registry](const char *other) -> void * {
                static const uintptr_t no_base = 0;
                void **handle = g_point->original<void **>(registry, other, &no_base, nullptr);
                return handle && handle_ok(handle) ? handle : nullptr;
            });
            if (node && g_told.fetch_add(1) < LOGGED)
                swofty::log("[hooks] ui.vm.resolve (hot): %s -> node %p supplied by a handler", path, node);
            return {node, node != nullptr};
        },
        [](const HotStats &old) {
            swofty::log("[hooks] ui.vm.resolve (hot): table reset: %zu path(s) resolved, %zu supplied by handlers", old.entries,
                        old.supplied);
        });
    t_in_resolve = false;
    return supplied ? supplied : g_point->original<void *>(registry, path, base, create);
}

}

}

std::optional<std::string> runtime::lang_lookup_fire(const char *key, const char *language, const char *stock)
{
    return swofty_core::hooks::ui_hot::fire_lang(swofty_core::hooks::core(), key, language, stock);
}

uint64_t runtime::lang_lookup_epoch() { return swofty_core::hooks::core().epoch(); }

namespace swofty_core::hooks {

void declare_ui_hot_points(Table &t)
{
    g_table = &t;
    g_point = std::make_unique<GamePoint>(ui_hot::VM_POINT, "Vm_Resolve_RegistryNode", reinterpret_cast<void *>(&d_resolve),
                                          game_ops());
    t.declare_game(ui_hot::VM_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::VmResolve::version, g_point.get(), true);
    t.declare_game(ui_hot::LANG_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::LangLookup::version, runtime::lang_lookup_link(),
                   true);
}

}
