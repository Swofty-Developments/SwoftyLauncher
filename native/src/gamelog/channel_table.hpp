#pragma once

#include <algorithm>
#include <cstdio>
#include <string>
#include <string_view>
#include <vector>

namespace gamelog::table {

constexpr const char *UNAVAILABLE = "[gamelog] game log channels unavailable";

struct Section {
    unsigned long stamp = 0;
    std::string_view rows;
};

inline std::vector<Section> sections(std::string_view text)
{
    std::vector<Section> out;
    std::vector<size_t> from;
    size_t pos = 0;
    while (pos < text.size()) {
        size_t end = text.find('\n', pos);
        if (end == std::string_view::npos)
            end = text.size();
        std::string line(text.substr(pos, end - pos));
        unsigned long stamp = 0;
        char sha[32] = {};
        if (line.starts_with("# F1_25.exe ") && sscanf(line.c_str(), "# F1_25.exe %31s %lu", sha, &stamp) == 2) {
            if (!out.empty())
                out.back().rows = text.substr(from.back(), pos - from.back());
            out.push_back({stamp, {}});
            from.push_back(std::min(end + 1, text.size()));
        }
        pos = end + 1;
    }
    if (!out.empty())
        out.back().rows = text.substr(from.back());
    return out;
}

inline const Section *find(const std::vector<Section> &all, unsigned long stamp)
{
    for (const Section &s : all)
        if (s.stamp == stamp)
            return &s;
    return nullptr;
}

inline std::string covered(const std::vector<Section> &all)
{
    std::string s;
    for (const Section &x : all)
        s += (s.empty() ? "" : ", ") + std::to_string(x.stamp);
    return s.empty() ? "none" : s;
}

}
