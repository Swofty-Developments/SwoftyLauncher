#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

// Read-only walk of the UI view-model registry for string leaves (ui_vm.cpp). Keys are the djb2 hashes of the node names; the
// walk reports the chain of hashes from the root and the node address.
struct UiProbeHit {
    uint64_t node = 0;
    int depth = 0;
    uint32_t chain[12] = {};
    char value[96] = {};
};

struct UiProbeStats {
    size_t visited = 0, maps = 0, strings = 0, matches = 0;
};

size_t ui_vm_probe_strings(const char *needle, UiProbeHit *out, size_t cap, UiProbeStats &stats);
bool ui_vm_set_path(const std::string &path, int type, const void *value);
