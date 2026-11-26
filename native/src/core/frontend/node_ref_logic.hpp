#pragma once
#include <cstdint>

#include "core/boxes/node_handle.hpp"

namespace frontend::node_ref {

enum Result : int { OK = 0, NO_PARENT = 1, EMPTY_TARGET = 2, REFUSED = 3 };

inline const char *describe(int r)
{
    switch (r) {
    case OK:
        return "bound";
    case NO_PARENT:
        return "the parent node did not resolve";
    case EMPTY_TARGET:
        return "the target path resolves to no node";
    case REFUSED:
        return "the leaf exists and is not a reference (Vm_Set_NodeRef 0x14083c270 refused it)";
    }
    return "unknown";
}

// The FillTile 0x141a404c0 recipe: a temporary handle linked at (*target)+0x28 with +0x24 bumped, handed to
// Vm_Set_NodeRef (which copies it into the leaf), then unlinked and released so the count and the holder list
// keep only the leaf's own handle. target and empty are the slots Vm_Resolve_RegistryNode and
// k_Vm_EmptyNodeHandle give; set_ref takes the handle and returns the leaf slot; release is Vm_Release_Node.
template <class SetRef, class Release> int bind(void **target, void **empty, SetRef set_ref, Release release)
{
    if (!target || target == empty || !*target)
        return EMPTY_TARGET;
    runtime::era::Handle handle{};
    runtime::era::link(handle, static_cast<uint8_t *>(*target));
    void **leaf = set_ref(&handle);
    runtime::era::unlink(handle, release);
    return !leaf || leaf == empty ? REFUSED : OK;
}

}
