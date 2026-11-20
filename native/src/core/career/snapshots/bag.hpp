#pragma once
#include <cctype>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <map>
#include <string>

#include "core/career/snapshots/format.hpp"

// The game carries the payload as a property bag (one root child of the profile save, through the
// save splice): a "format" string and the payload in base64, split over "d000".."d999" string
// properties (type 'dstr': u32 length and the characters).

namespace career::snapshots::format {

constexpr size_t CHUNK = 32000;
constexpr size_t MAX_CHUNKS = 1000;
constexpr size_t MAX_PAYLOAD = 16u << 20;

constexpr uint32_t fourcc(const char (&s)[5])
{
    return uint32_t(uint8_t(s[0])) | uint32_t(uint8_t(s[1])) << 8 | uint32_t(uint8_t(s[2])) << 16 | uint32_t(uint8_t(s[3])) << 24;
}
constexpr uint32_t TYPE_BAG = fourcc("pbag");
constexpr uint32_t TYPE_DSTR = fourcc("dstr");

inline std::string base64(const Bytes &b)
{
    static const char *A = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    out.reserve((b.size() + 2) / 3 * 4);
    for (size_t i = 0; i < b.size(); i += 3) {
        uint32_t v =
            uint32_t(b[i]) << 16 | (i + 1 < b.size() ? uint32_t(b[i + 1]) << 8 : 0) | (i + 2 < b.size() ? uint32_t(b[i + 2]) : 0);
        out += A[v >> 18 & 63];
        out += A[v >> 12 & 63];
        out += i + 1 < b.size() ? A[v >> 6 & 63] : '=';
        out += i + 2 < b.size() ? A[v & 63] : '=';
    }
    return out;
}

inline bool unbase64(const std::string &s, Bytes &out)
{
    auto val = [](char c) -> int {
        if (c >= 'A' && c <= 'Z')
            return c - 'A';
        if (c >= 'a' && c <= 'z')
            return c - 'a' + 26;
        if (c >= '0' && c <= '9')
            return c - '0' + 52;
        return c == '+' ? 62 : c == '/' ? 63 : -1;
    };
    if (s.size() % 4)
        return false;
    out.clear();
    out.reserve(s.size() / 4 * 3);
    for (size_t i = 0; i < s.size(); i += 4) {
        int a = val(s[i]), b = val(s[i + 1]);
        int c = s[i + 2] == '=' ? -2 : val(s[i + 2]), d = s[i + 3] == '=' ? -2 : val(s[i + 3]);
        bool last = i + 4 == s.size();
        if (a < 0 || b < 0 || c == -1 || d == -1 || (c == -2 && d != -2) || ((c == -2 || d == -2) && !last))
            return false;
        uint32_t v = uint32_t(a) << 18 | uint32_t(b) << 12 | uint32_t(c < 0 ? 0 : c) << 6 | uint32_t(d < 0 ? 0 : d);
        out.push_back(uint8_t(v >> 16));
        if (c >= 0)
            out.push_back(uint8_t(v >> 8));
        if (d >= 0)
            out.push_back(uint8_t(v));
    }
    return true;
}

inline void key(Writer &w, const std::string &k)
{
    w.u8(uint8_t(k.size()));
    w.raw(k.data(), k.size());
}

inline void dstr(Writer &w, const std::string &name, const std::string &value)
{
    key(w, name);
    w.u32(TYPE_DSTR);
    w.u32(uint32_t(value.size()));
    w.raw(value.data(), value.size());
}

inline std::string chunk_name(size_t i)
{
    char n[8];
    snprintf(n, sizeof n, "d%03zu", i);
    return n;
}

// The bag body the save splice writes for the entry (the codec header 1, the entry count, then each
// entry), the same bytes Game_SaveData_WriteBag writes for a bag of these string properties.
inline bool bag_body(const Bytes &payload, Bytes &out)
{
    std::string b64 = base64(payload);
    size_t chunks = (b64.size() + CHUNK - 1) / CHUNK;
    if (chunks > MAX_CHUNKS || payload.size() > MAX_PAYLOAD)
        return false;
    Writer w;
    w.u64(1);
    w.u64(1 + chunks);
    dstr(w, "format", FORMAT_TAG);
    for (size_t i = 0; i < chunks; i++)
        dstr(w, chunk_name(i), b64.substr(i * CHUNK, CHUNK));
    out = std::move(w.out);
    return true;
}

// The payload out of the bag body the splice kept at a profile load (Game_SaveData_WriteBag's bytes,
// whose entry order need not be the order written). Anything but the string properties above is
// refused: this is not an entry this core wrote.
inline bool parse_bag(const Bytes &body, Bytes &payload, std::string &why)
{
    Reader r(body);
    if (r.u64() != 1)
        return why = "the bag header is not 1", false;
    uint64_t n = r.u64();
    if (!r.ok || n == 0 || n > MAX_CHUNKS + 1)
        return why = "the bag has " + std::to_string(n) + " entries", false;
    std::string format;
    std::map<size_t, std::string> chunks;
    for (uint64_t i = 0; i < n; i++) {
        size_t klen = r.u8();
        if (!r.need(klen))
            return why = "an entry name runs past the end", false;
        std::string k(reinterpret_cast<const char *>(body.data() + r.at), klen);
        r.at += klen;
        uint32_t type = r.u32();
        if (!r.ok || type != TYPE_DSTR)
            return why = "entry " + k + " is not a string", false;
        uint32_t len = r.u32();
        if (!r.need(len))
            return why = "entry " + k + " runs past the end", false;
        std::string v(reinterpret_cast<const char *>(body.data() + r.at), len);
        r.at += len;
        if (k == "format")
            format = v;
        else if (k.size() == 4 && k[0] == 'd' && isdigit(uint8_t(k[1])) && isdigit(uint8_t(k[2])) && isdigit(uint8_t(k[3])))
            chunks[size_t(std::stoul(k.substr(1)))] = std::move(v);
        else
            return why = "entry " + k + " is not one this core writes", false;
    }
    if (!r.done())
        return why = "bytes follow the last entry", false;
    if (format != FORMAT_TAG)
        return why = "its format is '" + format + "'", false;
    std::string all;
    size_t next = 0;
    for (auto &[i, c] : chunks) {
        if (i != next++)
            return why = "chunk " + chunk_name(next - 1) + " is missing", false;
        all += c;
    }
    if (!unbase64(all, payload))
        return why = "its data is not base64", false;
    return true;
}

}
