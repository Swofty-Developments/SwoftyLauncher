#pragma once
#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

namespace frontend::node_calls {

struct Calls {
    swofty::game::Vm_Resolve_RegistryNode_t *resolve = nullptr;
    swofty::game::Vm_Set_NodeRef_t *set_ref = nullptr;
    swofty::game::Vm_Release_Node_t *release = nullptr;
    void **empty = nullptr;
    explicit operator bool() const { return resolve && set_ref && release && empty; }
};

inline Calls load()
{
    Calls c;
    c.resolve = reinterpret_cast<swofty::game::Vm_Resolve_RegistryNode_t *>(swofty::api->resolve("Vm_Resolve_RegistryNode"));
    c.set_ref = reinterpret_cast<swofty::game::Vm_Set_NodeRef_t *>(swofty::api->resolve("Vm_Set_NodeRef"));
    c.release = reinterpret_cast<swofty::game::Vm_Release_Node_t *>(swofty::api->resolve("Vm_Release_Node"));
    c.empty = reinterpret_cast<void **>(swofty::api->resolve("k_Vm_EmptyNodeHandle"));
    return c;
}

}
