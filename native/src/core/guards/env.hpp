#pragma once

namespace swofty_core::guards {

inline bool switch_off(const char *value)
{
    if (!value)
        return false;
    return (value[0] == '0' && value[1] == 0) || (value[0] == 'o' && value[1] == 'f' && value[2] == 'f' && value[3] == 0);
}

}
