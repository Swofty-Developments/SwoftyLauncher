#pragma once

#include "core/career/snapshots/snapshots.hpp"

#include <cstddef>
#include <cstdio>
#include <map>
#include <set>
#include <string>
#include <utility>

// What the book's entry and career moments (snapshots.cpp) and its freezes (freeze.cpp) share: the decoded
// seasons and record majors the reads use, and the size text of the book's lines.
namespace career::snapshots {

struct Book::Cache {
    struct Decoded {
        bool ok = false;
        swofty::json::Value records;
        std::set<std::string> loose;
        int major = 1;
    };
    std::map<std::pair<int, std::string>, Decoded> seasons;
    std::map<std::string, int> majors;
    std::set<std::string> logged;
};

inline std::string kb(size_t n)
{
    char b[32];
    if (n < 1024)
        snprintf(b, sizeof b, "%zu B", n);
    else
        snprintf(b, sizeof b, "%.1f KB", double(n) / 1024.0);
    return b;
}

}
