#pragma once
#include <string>
#include <string_view>

namespace frontend::vm_path {

struct Split {
    std::string parent, leaf;
};

inline bool split(std::string_view path, Split &out)
{
    size_t dot = path.find_last_of('.');
    if (dot == std::string_view::npos || dot == 0 || dot + 1 >= path.size())
        return false;
    out.parent = std::string(path.substr(0, dot));
    out.leaf = std::string(path.substr(dot + 1));
    return true;
}

inline bool settable(std::string_view path)
{
    Split s;
    return split(path, s) && path.find_first_of(" []#") == std::string_view::npos;
}

}
