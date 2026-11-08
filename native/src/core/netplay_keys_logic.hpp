#pragma once
#include <cstdint>

// Session key material. NetworkSubsystem::requestSSLKeyMaterial queues an OS-service operation
// (Game_OsService_SubmitRequest) whose completion callback is a closure {vtable, NetworkSubsystem*, ...}
// stored inline in a std::function-style object (callback[7] == callback). Its vtable slot 2 is
// Game_NetworkSubsystem_UpdateKeyMaterial(closure, const bool *success, const SSLKeyMaterial *), which sets the
// fetch status the InviteManager's join step waits on (NetSub+0x10d0: 2 done, 3 failed) and copies the three
// strings of the material (+0, +0x28, +0x50; capacity at +8, characters at +0x10 of each) into the subsystem.

namespace runtime::netplay_keys {

struct Str {
    const void *vtable;
    uint64_t capacity;
    const char *chars;
    const void *allocator;
    char inline_buffer[8];
};

static_assert(sizeof(Str) == 0x28);

struct Material {
    Str part[3];
};

static_assert(sizeof(Material) == 0x78);

using UpdateFn = void(void *closure, const bool *success, const void *material);

inline constexpr char placeholder[] = "x";

inline void fill(Material &m)
{
    for (Str &s : m.part) {
        s = {};
        s.capacity = sizeof placeholder + 1;
        s.chars = placeholder;
    }
}

inline bool is_key_callback(void *const *callback, const void *update)
{
    if (!callback || callback[7] != static_cast<const void *>(callback))
        return false;
    auto vtable = static_cast<const void *const *>(callback[0]);
    return vtable && update && vtable[2] == update;
}

inline bool answer(void **callback, const void *update)
{
    if (!is_key_callback(callback, update))
        return false;
    Material material;
    fill(material);
    const bool success = true;
    reinterpret_cast<UpdateFn *>(const_cast<void *>(update))(callback, &success, &material);
    callback[7] = nullptr;
    return true;
}

}
