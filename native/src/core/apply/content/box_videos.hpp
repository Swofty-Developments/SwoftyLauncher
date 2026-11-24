#pragma once

#include "core/apply/content/lower.hpp"

#include <cstddef>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

// videos (content.package 1.9): the videos a box's sessions play in place of stock ones. Each entry names a stock path
// prefix below the game's videos/ folder ("track_map/26/") and the prefix that replaces it ("track_map/"), so a 1950
// box plays the stock 2025 track intro (videos/track_map/silverstone.bk2) instead of the 2026 one with active aero
// (videos/track_map/26/silverstone.bk2), or an addon's own loose video. The core wraps Ne_VideoPlayer_OpenFile, the
// one function every UI video opens through (talking points, the pre-race video, feature commands, red flag), and
// swaps the path while the box is the session's box (box_videos.cpp). Pure, for the host tests.

namespace swofty_core::apply::content {

struct VideoRedirect {
    std::string from, to;
};

struct VideosPick {
    std::string package;
    std::vector<VideoRedirect> redirects;
    std::string line;
};

// A prefix is a relative path below videos/ with / between folders: no leading videos/, no backslash, no "..", no
// drive. from is never empty; to may be empty only when from names a whole file (then nothing would be left).
inline bool video_prefix_ok(std::string_view p)
{
    if (p.empty() || p.front() == '/' || p.find('\\') != std::string_view::npos || p.find(':') != std::string_view::npos ||
        p.find("..") != std::string_view::npos || p.size() > 200)
        return false;
    return p.compare(0, 7, "videos/") != 0;
}

// The redirects of the box whose entrant list is the session's box: the last content.package (in the order given)
// with that entrant list, and its videos entries that are valid (others are skipped and counted in the line).
inline VideosPick videos_pick(const std::string &entrant_list, const std::vector<std::pair<std::string, const Value *>> &packages)
{
    VideosPick p;
    const Value *found = nullptr;
    for (auto &[id, rec] : packages)
        if (rec && (*rec)["entrant_list"].is_string() && (*rec)["entrant_list"].as_string() == entrant_list) {
            p.package = id;
            found = rec;
        }
    if (!found) {
        p.line = "box " + entrant_list + ": no content.package; stock videos";
        return p;
    }
    const Value &v = (*found)["videos"];
    std::size_t skipped = 0;
    for (std::size_t i = 0; i < v.size(); i++) {
        const Value &e = v[i];
        std::string from(e["from"].as_string()), to(e["to"].as_string());
        if (!e["from"].is_string() || !e["to"].is_string() || !video_prefix_ok(from) || (!to.empty() && !video_prefix_ok(to))) {
            skipped++;
            continue;
        }
        p.redirects.push_back({from, to});
    }
    p.line = "box " + entrant_list + " (" + p.package + "): " +
             (p.redirects.empty() ? std::string("stock videos") : std::to_string(p.redirects.size()) + " video redirect(s)") +
             (skipped ? ", " + std::to_string(skipped) + " invalid entr" + (skipped == 1 ? "y" : "ies") + " skipped" : "");
    return p;
}

// The path the game should open for path (as the caller hands it, with or without the videos/ prefix the game adds):
// the longest matching from prefix replaced by its to; nullopt when no redirect matches. Matching ignores ASCII case,
// as the game's file system does.
inline std::optional<std::string> video_redirect(const std::vector<VideoRedirect> &redirects, std::string_view path)
{
    std::string_view rest = path;
    bool had_prefix = false;
    auto lower = [](char c) { return c >= 'A' && c <= 'Z' ? char(c - 'A' + 'a') : c; };
    auto starts = [&](std::string_view s, std::string_view pre) {
        if (s.size() < pre.size())
            return false;
        for (std::size_t i = 0; i < pre.size(); i++)
            if (lower(s[i]) != lower(pre[i]))
                return false;
        return true;
    };
    if (starts(rest, "videos/")) {
        rest.remove_prefix(7);
        had_prefix = true;
    }
    const VideoRedirect *best = nullptr;
    for (auto &r : redirects)
        if (starts(rest, r.from) && (!best || r.from.size() > best->from.size()))
            best = &r;
    if (!best)
        return std::nullopt;
    std::string out = had_prefix ? "videos/" : "";
    out += best->to;
    out.append(rest.substr(best->from.size()));
    if (out.empty() || out.size() >= 0x100)
        return std::nullopt;
    return out;
}

}
