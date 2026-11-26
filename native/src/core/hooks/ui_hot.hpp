#pragma once
#include <cstring>
#include <optional>
#include <string>
#include <type_traits>

#include <swofty/systems/ui.hpp>

#include "core/hooks/dispatch.hpp"

namespace swofty_core::hooks::ui_hot {

inline constexpr const char *LANG_POINT = "lang.lookup";
inline constexpr const char *VM_POINT = "ui.vm.resolve";
inline constexpr size_t MAX_TEXT = 4096;

inline std::optional<std::string> fire_lang(Table &t, const char *key, const char *language, const char *stock)
{
    namespace ev = swofty::ui::events;
    struct Slot {
        std::string text;
        bool set = false;
        ev::LangLookup *payload = nullptr;
    } slot;
    slot.text = stock ? stock : "";
    ev::LangLookup e{};
    e.size = sizeof e;
    e.payload_version = ev::LangLookup::version;
    e.key_str = key;
    e.language_str = language ? language : "";
    e.text_str = slot.text.c_str();
    e.set_text_fn = [](void *user, const char *text) -> int32_t {
        auto *s = static_cast<Slot *>(user);
        if (!text || std::strlen(text) >= MAX_TEXT)
            return SWOFTY_ASSET_INVALID;
        s->text = text;
        s->set = true;
        s->payload->text_str = s->text.c_str();
        return SWOFTY_ASSET_OK;
    };
    e.user = &slot;
    slot.payload = &e;
    int replaced = 0;
    t.fire("", LANG_POINT, &e, &replaced);
    if (!slot.set)
        return std::nullopt;
    return slot.text;
}

// `node_of` gives the game's own node handle for another path, or null: what the payload's use_node_of calls. A handler
// that sets `node` itself or through use_node_of supplies it to the game once it validates.
template <class Valid, class NodeOf> void *fire_vm(Table &t, const char *path, Valid &&valid, NodeOf &&node_of)
{
    namespace ev = swofty::ui::events;
    struct Slot {
        std::remove_reference_t<NodeOf> *node_of;
    } slot{&node_of};
    ev::VmResolve e{};
    e.size = sizeof e;
    e.payload_version = ev::VmResolve::version;
    e.path_str = path;
    e.node = nullptr;
    e.node_of_fn = [](void *user, const char *other, void **out) -> int32_t {
        if (!other || !*other || !out)
            return SWOFTY_ASSET_INVALID;
        void *found = (*static_cast<Slot *>(user)->node_of)(other);
        if (!found)
            return SWOFTY_ASSET_UNAVAILABLE;
        *out = found;
        return SWOFTY_ASSET_OK;
    };
    e.user = &slot;
    int replaced = 0;
    t.fire("", VM_POINT, &e, &replaced);
    return e.node && valid(e.node) ? e.node : nullptr;
}

template <class Valid> void *fire_vm(Table &t, const char *path, Valid &&valid)
{
    return fire_vm(t, path, valid, [](const char *) -> void * { return nullptr; });
}

}
