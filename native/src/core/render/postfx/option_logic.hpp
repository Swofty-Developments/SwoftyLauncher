#pragma once
#include <cctype>
#include <cstdlib>
#include <string>
#include <string_view>

namespace render::postfx {

// Win32 virtual-key codes for a toggle option's key names; 0 for None and unknown names.
inline int key_code(std::string_view key)
{
    if (key.size() == 1 && ((key[0] >= 'A' && key[0] <= 'Z') || (key[0] >= '0' && key[0] <= '9')))
        return key[0];
    if (key.size() == 1 && key[0] >= 'a' && key[0] <= 'z')
        return key[0] - 'a' + 'A';
    if (key.size() >= 2 && key.size() <= 3 && (key[0] == 'F' || key[0] == 'f')) {
        int n = 0;
        for (char c : key.substr(1)) {
            if (c < '0' || c > '9')
                return 0;
            n = n * 10 + (c - '0');
        }
        return n >= 1 && n <= 24 ? 0x70 + n - 1 : 0;
    }
    struct Named {
        std::string_view name;
        int vk;
    };
    static constexpr Named NAMED[] = {{"Insert", 0x2D},    {"Home", 0x24},        {"End", 0x23},  {"Page Up", 0x21},
                                      {"Page Down", 0x22}, {"Scroll Lock", 0x91}, {"Pause", 0x13}};
    for (auto &n : NAMED)
        if (n.name == key)
            return n.vk;
    return 0;
}

// An option's value from the text of its current state in the settings: true and false read as 1 and
// 0, numbers as themselves; anything else is not a value.
inline bool option_value(std::string_view text, float &out)
{
    if (text == "true" || text == "false") {
        out = text == "true";
        return true;
    }
    std::string s(text);
    char *end = nullptr;
    double v = std::strtod(s.c_str(), &end);
    if (s.empty() || end != s.c_str() + s.size())
        return false;
    out = float(v);
    return true;
}

}
