#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

namespace runtime {

uint64_t fnv64(const char *s, size_t n);
inline uint64_t fnv64(const std::string &s) { return fnv64(s.data(), s.size()); }
uint32_t fnv32(const char *s);
void md5(const void *data, size_t n, uint8_t out[16]);
std::string hex16(uint64_t v);

}
