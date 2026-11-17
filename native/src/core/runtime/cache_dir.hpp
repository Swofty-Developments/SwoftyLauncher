#pragma once

#include <string>

namespace runtime {

// The folder the core keeps its caches in, ending in a backslash: swofty\cache, or %LOCALAPPDATA%\Swofty\cache\game
// when the game can't create files there.
const std::wstring &cache_dir();

}
