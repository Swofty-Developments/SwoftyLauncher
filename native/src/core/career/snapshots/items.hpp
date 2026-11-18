#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

// The tagged items the swofty_registry entry is built from: a u16 tag, a u32 length and the body,
// little-endian, with the CRC-32 the entry header carries.

namespace career::snapshots::format {

using Bytes = std::vector<uint8_t>;

inline uint32_t crc32(const uint8_t *p, size_t n)
{
    uint32_t c = 0xffffffffu;
    for (size_t i = 0; i < n; i++) {
        c ^= p[i];
        for (int k = 0; k < 8; k++)
            c = (c >> 1) ^ (0xedb88320u & (0u - (c & 1u)));
    }
    return ~c;
}

struct Writer {
    Bytes out;
    void u8(uint8_t v) { out.push_back(v); }
    void raw(const void *p, size_t n)
    {
        auto *b = static_cast<const uint8_t *>(p);
        out.insert(out.end(), b, b + n);
    }
    void u16(uint16_t v) { u8(uint8_t(v)), u8(uint8_t(v >> 8)); }
    void u32(uint32_t v)
    {
        for (int i = 0; i < 4; i++)
            u8(uint8_t(v >> (8 * i)));
    }
    void u64(uint64_t v)
    {
        for (int i = 0; i < 8; i++)
            u8(uint8_t(v >> (8 * i)));
    }
    void item(uint16_t tag, const void *p, size_t n)
    {
        u16(tag);
        u32(uint32_t(n));
        raw(p, n);
    }
    void item(uint16_t tag, const Bytes &b) { item(tag, b.data(), b.size()); }
    void item(uint16_t tag, const std::string &s) { item(tag, s.data(), s.size()); }
};

struct Reader {
    const uint8_t *p;
    size_t n, at = 0;
    bool ok = true;
    Reader(const uint8_t *data, size_t size) : p(data), n(size) {}
    explicit Reader(const Bytes &b) : p(b.data()), n(b.size()) {}
    bool need(size_t k)
    {
        ok = ok && k <= n - at;
        return ok;
    }
    bool done() const { return at == n; }
    uint8_t u8() { return need(1) ? p[at++] : 0; }
    uint16_t u16()
    {
        if (!need(2))
            return 0;
        uint16_t v = uint16_t(p[at] | p[at + 1] << 8);
        at += 2;
        return v;
    }
    uint32_t u32()
    {
        if (!need(4))
            return 0;
        uint32_t v = 0;
        for (int i = 0; i < 4; i++)
            v |= uint32_t(p[at + i]) << (8 * i);
        at += 4;
        return v;
    }
    uint64_t u64()
    {
        if (!need(8))
            return 0;
        uint64_t v = 0;
        for (int i = 0; i < 8; i++)
            v |= uint64_t(p[at + i]) << (8 * i);
        at += 8;
        return v;
    }
};

// One tagged item: its tag and body, and the whole item's bytes for a byte-for-byte write-back.
struct Item {
    uint16_t tag = 0;
    Bytes body;
    Bytes encoded() const
    {
        Writer w;
        w.item(tag, body);
        return w.out;
    }
};

inline bool items(const uint8_t *p, size_t n, std::vector<Item> &out)
{
    Reader r(p, n);
    while (r.ok && !r.done()) {
        Item it;
        it.tag = r.u16();
        uint32_t len = r.u32();
        if (!r.need(len))
            return false;
        it.body.assign(p + r.at, p + r.at + len);
        r.at += len;
        out.push_back(std::move(it));
    }
    return r.ok;
}
inline bool items(const Bytes &b, std::vector<Item> &out) { return items(b.data(), b.size(), out); }

inline std::string text(const Bytes &b) { return std::string(b.begin(), b.end()); }

}
