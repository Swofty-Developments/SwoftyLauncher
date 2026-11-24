#pragma once
#include <cstddef>
#include <cstdint>

namespace runtime::iat_scan {

constexpr size_t MIN_RUN = 8;

constexpr bool module_pointer(uint64_t v, uint64_t exe_lo, uint64_t exe_hi)
{
    return v >= 0x10000 && v < 0x800000000000ull && !(v >= exe_lo && v < exe_hi);
}

inline bool in_table(const uint64_t *qw, size_t n, size_t at, uint64_t exe_lo, uint64_t exe_hi)
{
    if (at >= n || !module_pointer(qw[at], exe_lo, exe_hi))
        return false;
    size_t a = at, b = at + 1;
    while (a > 0 && module_pointer(qw[a - 1], exe_lo, exe_hi))
        a--;
    while (b < n && module_pointer(qw[b], exe_lo, exe_hi))
        b++;
    return b - a >= MIN_RUN;
}

}
