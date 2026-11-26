#pragma once
#include <swofty/game.hpp>

#include <string>

#include "core/frontend/tile_image_logic.hpp"
#include "core/runtime/runtime.hpp"

namespace frontend::tile_image {

inline bool template_built()
{
    static const bool built = !runtime::read_file(runtime::under_swofty(FLAG), 16).empty();
    return built;
}

// The FillTile recipe (Game_CreateSessionDlcSelectTab_FillTile 0x141a404c0): the node handle slot the registry gives for
// `path`, a temporary handle linked into that node's holder list with its count bumped, Vm_Set_NodeRef on the tile node's
// image leaf (overwrite), then the temporary unlinked and released. False when the path resolves to no node.
inline bool set(void *tile_node, const std::string &path)
{
    using swofty::game::Vm_Release_Node;
    using swofty::game::Vm_Resolve_RegistryNode;
    using swofty::game::Vm_Set_NodeRef;
    void *reg = *swofty::game::g_Vm_Registry;
    const void *none = reinterpret_cast<const void *>(swofty::game::k_Vm_EmptyNodeHandle.address());
    auto **slot = static_cast<uint8_t **>(Vm_Resolve_RegistryNode(reg, path.c_str(), none, nullptr));
    if (!slot || !*slot)
        return false;
    runtime::era::Handle handle{};
    runtime::era::link(handle, *slot);
    Vm_Set_NodeRef(reg, static_cast<uint8_t *>(tile_node) + 8, LEAF, &handle, 1);
    runtime::car_category::unlink(handle, [reg](uint8_t *node) { Vm_Release_Node(reg, node); });
    return true;
}

}
