// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>

namespace f1990::intro {

constexpr size_t kBinkHeaderSize = 0x24;
constexpr double kMaxSeconds = 600.0;
constexpr double kTimeoutMargin = 15.0;

struct BinkInfo {
    int version;
    char revision;
    uint32_t frames;
    uint32_t width;
    uint32_t height;
    uint32_t fps_num;
    uint32_t fps_den;
};

inline uint32_t le32(const uint8_t *p)
{
    uint32_t v;
    std::memcpy(&v, p, sizeof v);
    return v;
}

// Bink 1 files start with "BIK" and a revision letter, Bink 2 files with "KB2"; both keep the frame
// count at +0x08, the size at +0x14/+0x18 and the frame rate (numerator, denominator) at +0x1c/+0x20.
inline std::optional<BinkInfo> parse_header(const uint8_t *p, size_t n)
{
    if (!p || n < kBinkHeaderSize)
        return std::nullopt;
    BinkInfo info{};
    if (p[0] == 'B' && p[1] == 'I' && p[2] == 'K')
        info.version = 1;
    else if (p[0] == 'K' && p[1] == 'B' && p[2] == '2')
        info.version = 2;
    else
        return std::nullopt;
    info.revision = static_cast<char>(p[3]);
    info.frames = le32(p + 0x08);
    info.width = le32(p + 0x14);
    info.height = le32(p + 0x18);
    info.fps_num = le32(p + 0x1c);
    info.fps_den = le32(p + 0x20);
    if (!info.frames || !info.width || !info.height || !info.fps_num || !info.fps_den)
        return std::nullopt;
    return info;
}

inline double seconds(const BinkInfo &info)
{
    return double(info.frames) * double(info.fps_den) / double(info.fps_num);
}

inline double fps(const BinkInfo &info)
{
    return double(info.fps_num) / double(info.fps_den);
}

enum class Verdict { play, not_bink, wrong_version, too_long };

// The game picks the decoder's colour matrix from the extension (.bik: Bink 1, .bk2: Bink 2), so
// the file has to be the version its name says.
inline Verdict check(const std::optional<BinkInfo> &info, int want_version)
{
    if (!info)
        return Verdict::not_bink;
    if (info->version != want_version)
        return Verdict::wrong_version;
    if (seconds(*info) > kMaxSeconds)
        return Verdict::too_long;
    return Verdict::play;
}

inline double timeout(const BinkInfo &info)
{
    return seconds(info) + kTimeoutMargin;
}

enum class Step { stay, ended, timed_out };

inline Step step(bool ended, double elapsed, double limit)
{
    if (ended)
        return Step::ended;
    if (elapsed >= limit)
        return Step::timed_out;
    return Step::stay;
}

inline bool ended_early(double elapsed, const BinkInfo &info)
{
    return elapsed + 1.0 < seconds(info);
}

}
