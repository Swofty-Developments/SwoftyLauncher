#pragma once

#include "garage_setup_logic.hpp"

#include <cstddef>
#include <cstdint>

namespace runtime::tile_list_select {

using garage_setup::Read;
using garage_setup::user_pointer;
constexpr int LOG_FIRST = garage_setup::LOG_FIRST;

constexpr uint64_t COMPONENTS_AT = 0x30;
constexpr uint64_t CHILDREN_BEGIN_AT = 0x130;
constexpr uint64_t WIDGET_AT_RSP = 0x50;

enum class Child {
    Usable,
    Unreadable,
    NoComponents,
    NoComponent,
};

inline Child check_child(uint64_t cursor, Read read, void *user)
{
    uint64_t child = 0;
    if (!read(cursor, &child, sizeof child, user) || !user_pointer(child))
        return Child::Unreadable;
    uint64_t components = 0;
    if (!read(child + COMPONENTS_AT, &components, sizeof components, user))
        return Child::Unreadable;
    if (!components)
        return Child::NoComponents;
    if (!user_pointer(components))
        return Child::NoComponent;
    uint64_t first = 0;
    if (!read(components, &first, sizeof first, user) || !user_pointer(first))
        return Child::NoComponent;
    uint64_t vtable = 0;
    if (!read(first, &vtable, sizeof vtable, user) || !user_pointer(vtable))
        return Child::NoComponent;
    return Child::Usable;
}

struct Position {
    bool known = false;
    uint64_t index = 0;
    uint64_t count = 0;
};

inline Position position(uint64_t cursor, uint64_t end, uint64_t begin)
{
    Position p;
    if (!begin || cursor < begin || end < cursor || (cursor - begin) % 8 || (end - begin) % 8)
        return p;
    p.known = true;
    p.index = (cursor - begin) / 8;
    p.count = (end - begin) / 8;
    return p;
}

inline const char *why(Child c)
{
    switch (c) {
    case Child::Unreadable:
        return "is not readable";
    case Child::NoComponents:
        return "has no component array";
    case Child::NoComponent:
        return "has an unreadable first component";
    case Child::Usable:
        break;
    }
    return "is usable";
}

}
