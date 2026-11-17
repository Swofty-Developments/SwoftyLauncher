#pragma once

#include "core/runtime/json.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/scan_cache.hpp"
#include "swofty_api.h"

#include <string>
#include <unordered_map>

namespace runtime::loader {

// Pattern scans of the game exe by core code, bindings and addons: the per-exe scan cache, the set's section map that
// sends Steam section names to this build's, and the refusals that keep a packed build's scans off encrypted code.
class Scanner {
public:
    // Wraps the loader's scan; false when SWOFTY_RESOLVE=scan turns the KB addresses and the scan cache off.
    bool open(SwoftyApi *a, const std::wstring &dir, long long exe_bytes);
    void save();
    int scan(const char *section, const char *pattern, uintptr_t *out);
    int addon_scan(const char *section, const char *pattern, uintptr_t *out);
    void read_section_map(const json::Value &doc, const std::string &file, const std::unordered_map<std::string, size_t> &uses,
                          bool any_binding);
    std::string game_code() const;
    const std::wstring &caches(const std::wstring &dir);
    std::string exe_sha256(const std::wstring &exe, const std::wstring &cache, long long exe_bytes, long long exe_stamp);

private:
    bool refused(const char *section, const char *pattern, uintptr_t hit);

    int (*loader_scan_)(const char *, const char *, uintptr_t *) = nullptr;
    scancache::Cache cache_;
    std::wstring cache_path_, cache_dir_;
    const char *cache_name_ = "swofty\\cache";
    bool cache_on_ = false;
    std::unordered_map<std::string, std::string> section_alias_;
    SRWLOCK lock_ = SRWLOCK_INIT;
};

Scanner &scanner();

std::string read_whole_file(const std::wstring &path);

}
