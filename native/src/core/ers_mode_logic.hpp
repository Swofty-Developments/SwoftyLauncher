#pragma once
#include <cstddef>
#include <cstdint>

// Where the ERS system keeps the car's ERS mode (CarErs) on each build it was found on: the KB build reads it at
// +0x1a68 (Car_Powertrain_Step), and Steam 1.0.145 (26566308) reads it and the ERS Reset stores it at +0x1a6c
// (re/research/live-steam-26566308.md). No other build has a witness for it, so the ERS mode guard there neither
// reads the mode back after the reset nor writes it: mode_offset is 0. A build is the running exe's PE
// TimeDateStamp and SizeOfImage (runtime::image_id), matched as stage.cpp's build checks match them.

namespace runtime::ers_mode {

struct Known {
    uint32_t stamp, size;
    // The KB build's entry takes any image larger than size: its packed and unpacked images differ in size.
    bool larger;
    size_t mode;
};

inline constexpr Known KNOWN[] = {
    {1789118616u, 0x6000000, true, 0x1a68},
    {1794312636u, 0x18434000, false, 0x1a6c},
};

constexpr size_t mode_offset(uint32_t stamp, uint32_t size)
{
    for (const Known &k : KNOWN)
        if (stamp == k.stamp && (k.larger ? size > k.size : size == k.size))
            return k.mode;
    return 0;
}

}
