#pragma once

#include <mutex>

namespace runtime::loader {

// commit_lock serialises the addon commit and every later install pass; resolve_lock serialises binding resolution
// and packed-build scans. Code that takes both takes commit_lock first.
inline std::recursive_mutex &commit_lock()
{
    static std::recursive_mutex lock;
    return lock;
}

inline std::recursive_mutex &resolve_lock()
{
    static std::recursive_mutex lock;
    return lock;
}

}
