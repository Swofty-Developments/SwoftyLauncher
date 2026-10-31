// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace f1990 {

struct StopRules {
    static constexpr size_t kMandatoryCheck = 0x88, kValidLapsCheck = 0x89, kLengths = 0x8b, kLengthsEnd = 0xb2;

    bool held = false;
    uint8_t checks[2] = {};
    uint8_t lengths[kLengthsEnd - kLengths] = {};

    bool relax(uint8_t *mode)
    {
        if (held)
            return false;
        checks[0] = mode[kMandatoryCheck];
        checks[1] = mode[kValidLapsCheck];
        std::memcpy(lengths, mode + kLengths, sizeof lengths);
        mode[kMandatoryCheck] = 0;
        mode[kValidLapsCheck] = 0;
        std::memset(mode + kLengths, 0, sizeof lengths);
        held = true;
        return true;
    }

    bool restore(uint8_t *mode)
    {
        if (!held)
            return false;
        mode[kMandatoryCheck] = checks[0];
        mode[kValidLapsCheck] = checks[1];
        std::memcpy(mode + kLengths, lengths, sizeof lengths);
        held = false;
        return true;
    }
};

}
