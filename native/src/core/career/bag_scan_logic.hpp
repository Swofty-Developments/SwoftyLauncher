#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

// Pure logic for the plain save store's work on the game's bag stream (docs/reference/save-format.md): a structural scan
// that finds where a bag ends, a split of the root's containers block into its entries, and a read stream the core owns
// (the game's bag reader only calls its slots +0x28 and +0x30). No game calls, so the unit test runs on the host.

namespace career::bag_scan {

constexpr size_t NPOS = size_t(-1);
constexpr int MAX_DEPTH = 64;
constexpr uint32_t PBAG = 0x67616270u;
constexpr uint32_t PLST = 0x74736c70u;

constexpr uint32_t fourcc(const char (&s)[5])
{
    return uint32_t(uint8_t(s[0])) | uint32_t(uint8_t(s[1])) << 8 | uint32_t(uint8_t(s[2])) << 16 | uint32_t(uint8_t(s[3])) << 24;
}

// Size of a fixed-size value type, 0 for a variable one, -1 for an unknown id.
inline int fixed_size(uint32_t id)
{
    struct Row {
        uint32_t id;
        int size;
    };
    static const Row rows[] = {{fourcc("si08"), 1},
                               {fourcc("ui08"), 1},
                               {fourcc("bool"), 1},
                               {fourcc("si16"), 2},
                               {fourcc("ui16"), 2},
                               {fourcc("si32"), 4},
                               {fourcc("ui32"), 4},
                               {fourcc("fp32"), 4},
                               {fourcc("rgba"), 4},
                               {fourcc("blwt"), 4},
                               {fourcc("si64"), 8},
                               {fourcc("ui64"), 8},
                               {fourcc("fp64"), 8},
                               {fourcc("auid"), 8},
                               {fourcc("vec2"), 8},
                               {fourcc("vec3"), 12},
                               {fourcc("vec4"), 16},
                               {fourcc("quat"), 16},
                               {fourcc("mat4"), 64},
                               {fourcc("shnm"), 16},
                               {fourcc("woid"), 16},
                               {fourcc("matp"), 16},
                               {fourcc("st32"), 0},
                               {fourcc("st64"), 0},
                               {fourcc("s128"), 0},
                               {fourcc("s256"), 0},
                               {fourcc("sstr"), 0},
                               {fourcc("dstr"), 0},
                               {fourcc("uist"), 0},
                               {fourcc("bffr"), 0},
                               {fourcc("dbrf"), 0},
                               {PBAG, 0},
                               {PLST, 0}};
    for (const Row &r : rows)
        if (r.id == id)
            return r.size;
    return -1;
}

inline bool get(const uint8_t *d, size_t n, size_t &pos, size_t need) { return pos <= n && n - pos >= need; }

inline uint64_t le(const uint8_t *p, int bytes)
{
    uint64_t v = 0;
    for (int i = 0; i < bytes; i++)
        v |= uint64_t(p[i]) << (8 * i);
    return v;
}

inline size_t skip_bag(const uint8_t *d, size_t n, size_t pos, int depth = 0);

// The end of one value of type id at pos, or NPOS when the bytes do not hold it.
inline size_t skip_value(const uint8_t *d, size_t n, size_t pos, uint32_t id, int depth)
{
    if (depth > MAX_DEPTH)
        return NPOS;
    if (id == PBAG)
        return skip_bag(d, n, pos, depth + 1);
    if (id == PLST) {
        if (!get(d, n, pos, 16) || le(d + pos, 8) > 1)
            return NPOS;
        uint64_t count = le(d + pos + 8, 8);
        pos += 16;
        if (count > 65535)
            return NPOS;
        for (uint64_t i = 0; i < count; i++) {
            if (!get(d, n, pos, 4))
                return NPOS;
            uint32_t t = uint32_t(le(d + pos, 4));
            pos = skip_value(d, n, pos + 4, t, depth + 1);
            if (pos == NPOS)
                return NPOS;
        }
        return pos;
    }
    int fixed = fixed_size(id);
    if (fixed < 0)
        return NPOS;
    if (fixed > 0)
        return get(d, n, pos, size_t(fixed)) ? pos + size_t(fixed) : NPOS;
    auto prefixed = [&](int lenbytes) -> size_t {
        if (!get(d, n, pos, size_t(lenbytes)))
            return NPOS;
        uint64_t len = le(d + pos, lenbytes);
        pos += size_t(lenbytes);
        return get(d, n, pos, size_t(len)) ? pos + size_t(len) : NPOS;
    };
    if (id == fourcc("st32") || id == fourcc("st64") || id == fourcc("s128"))
        return prefixed(1);
    if (id == fourcc("s256") || id == fourcc("sstr"))
        return prefixed(2);
    if (id == fourcc("dstr") || id == fourcc("uist") || id == fourcc("bffr"))
        return prefixed(4);
    if (id == fourcc("dbrf")) {
        if (!get(d, n, pos, 1))
            return NPOS;
        uint8_t flag = d[pos++];
        if (flag > 1)
            return NPOS;
        if (!flag)
            return pos;
        if (!get(d, n, pos, 1))
            return NPOS;
        size_t k = d[pos++];
        return get(d, n, pos, k * 4) ? pos + k * 4 : NPOS;
    }
    return NPOS;
}

// One bag at pos: version u64 (0 or 1), count u64, then entries (key length, key, type id, value). Returns its end or NPOS.
inline size_t skip_bag(const uint8_t *d, size_t n, size_t pos, int depth)
{
    if (depth > MAX_DEPTH || !get(d, n, pos, 16) || le(d + pos, 8) > 1)
        return NPOS;
    uint64_t count = le(d + pos + 8, 8);
    pos += 16;
    if (count > 65535)
        return NPOS;
    for (uint64_t i = 0; i < count; i++) {
        if (!get(d, n, pos, 1))
            return NPOS;
        size_t klen = d[pos++];
        if (klen > 63 || !get(d, n, pos, klen + 4))
            return NPOS;
        pos += klen;
        uint32_t t = uint32_t(le(d + pos, 4));
        pos = skip_value(d, n, pos + 4, t, depth);
        if (pos == NPOS)
            return NPOS;
    }
    return pos;
}

// A bag that fills the bytes exactly.
inline bool whole_bag(const uint8_t *d, size_t n) { return skip_bag(d, n, 0) == n; }

struct Entry {
    std::string name;
    size_t begin = 0, end = 0;
};

// The root's containers block as the game's save writes it after the count: `count` entries of key length, key, 'pbag' and the
// bag body. Succeeds only when exactly `count` entries end exactly at the end of the bytes.
inline bool split_entries(const uint8_t *d, size_t n, size_t count, std::vector<Entry> &out)
{
    size_t pos = 0;
    for (size_t i = 0; i < count; i++) {
        size_t start = pos;
        if (!get(d, n, pos, 1))
            return false;
        size_t klen = d[pos++];
        if (!klen || klen > 63 || !get(d, n, pos, klen + 4))
            return false;
        std::string name(reinterpret_cast<const char *>(d + pos), klen);
        pos += klen;
        if (le(d + pos, 4) != PBAG)
            return false;
        size_t body = pos + 4;
        size_t end = skip_bag(d, n, body);
        if (end == NPOS)
            return false;
        out.push_back({std::move(name), body, end});
        pos = end;
        (void)start;
    }
    return pos == n;
}

// ---- the read stream the core hands the game's bag reader ----

constexpr int READ_SLOTS = 27;
constexpr int READ_ERROR = 0x54;

struct ReadStream {
    void **vtable = nullptr;
    const uint8_t *data = nullptr;
    uint64_t size = 0;
    uint64_t pos = 0;
    uint64_t pad[4] = {};
    uint32_t bad_slot_calls = 0;
};

namespace slots {

inline int read_some(ReadStream *s, void *buf, uint64_t n, uint64_t *got)
{
    uint64_t take = s->size - s->pos < n ? s->size - s->pos : n;
    if (take)
        memcpy(buf, s->data + s->pos, size_t(take));
    s->pos += take;
    *got = take;
    return 0;
}

inline int read_exact(ReadStream *s, void *buf, uint64_t n)
{
    if (s->size - s->pos < n)
        return READ_ERROR;
    if (n)
        memcpy(buf, s->data + s->pos, size_t(n));
    s->pos += n;
    return 0;
}

inline int read_byte(ReadStream *s, void *buf) { return read_exact(s, buf, 1); }
inline uint64_t total(ReadStream *s) { return s->size; }
inline uint64_t position(ReadStream *s) { return s->pos; }
inline uint64_t is_open(ReadStream *) { return 1; }
inline int unused_ok(ReadStream *) { return 0; }
inline int unused_bad(ReadStream *s)
{
    s->bad_slot_calls++;
    return READ_ERROR;
}

}

// The vtable of the stream: the slots the reader uses are real, the rest return an error and count the call.
inline void **read_vtable()
{
    static void *vt[READ_SLOTS + 1] = {};
    static bool made = false;
    if (!made) {
        for (int i = 0; i < READ_SLOTS; i++)
            vt[i] = reinterpret_cast<void *>(&slots::unused_bad);
        vt[0] = reinterpret_cast<void *>(&slots::unused_ok);
        vt[0x28 / 8] = reinterpret_cast<void *>(&slots::read_some);
        vt[0x30 / 8] = reinterpret_cast<void *>(&slots::read_exact);
        vt[0x38 / 8] = reinterpret_cast<void *>(&slots::read_byte);
        vt[0xa8 / 8] = reinterpret_cast<void *>(&slots::total);
        vt[0xb0 / 8] = reinterpret_cast<void *>(&slots::position);
        vt[0xb8 / 8] = reinterpret_cast<void *>(&slots::is_open);
        made = true;
    }
    return vt;
}

inline void init_read(ReadStream &s, const uint8_t *data, uint64_t size)
{
    s = ReadStream();
    s.vtable = read_vtable();
    s.data = data;
    s.size = size;
}

// ---- the put-back decision for a registered mode's container ----

enum class Mode { leave, replace_empty, add_absent };

// The game's own container wins when it holds careers. Ours goes in when the game's has none (it was dropped and made again
// empty), or when the loaded tree has none at all, and only when ours holds careers.
inline Mode registered_decision(bool tree_has, size_t tree_careers, size_t ours_careers)
{
    if (!ours_careers)
        return Mode::leave;
    if (!tree_has)
        return Mode::add_absent;
    return tree_careers ? Mode::leave : Mode::replace_empty;
}

}
