#pragma once
#include <emmintrin.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static const uint8_t BYTE_RANK[256] = {
    255, 248, 243, 229, 239, 244, 182, 183, 230, 159, 121, 107, 198, 221, 112, 249, 231, 215, 80,  93,  170, 200, 94,  82,
    204, 76,  60,  50,  118, 104, 49,  187, 219, 78,  35,  39,  250, 129, 41,  38,  224, 193, 52,  110, 92,  57,  151, 36,
    212, 209, 62,  125, 99,  75,  25,  43,  211, 214, 47,  174, 101, 177, 28,  61,  220, 241, 141, 167, 235, 216, 181, 168,
    254, 238, 150, 128, 245, 213, 155, 134, 223, 133, 83,  126, 189, 135, 169, 195, 203, 142, 29,  119, 175, 109, 176, 172,
    166, 24,  20,  59,  87,  115, 226, 15,  156, 117, 9,   44,  122, 16,  45,  96,  171, 33,  98,  77,  242, 218, 105, 111,
    208, 100, 79,  53,  160, 51,  95,  138, 210, 201, 81,  234, 232, 240, 186, 145, 192, 252, 56,  253, 163, 246, 149, 137,
    178, 26,  63,  11,  148, 40,  85,  22,  127, 21,  4,   10,  73,  3,   65,  19,  130, 66,  1,   7,   30,  2,   14,  6,
    120, 5,   72,  17,  55,  0,   13,  67,  144, 68,  8,   18,  97,  12,  188, 84,  185, 146, 164, 69,  102, 34,  123, 64,
    236, 227, 179, 197, 217, 233, 202, 225, 196, 205, 165, 108, 247, 86,  131, 103, 190, 153, 191, 113, 54,  70,  71,  91,
    161, 147, 114, 132, 23,  27,  31,  42,  180, 136, 154, 88,  74,  37,  46,  48,  237, 222, 140, 207, 152, 106, 58,  90,
    206, 173, 158, 139, 32,  89,  162, 143, 228, 199, 184, 116, 124, 157, 194, 251,
};

static inline int pick_anchors(const int *pat, int n, int *a1, int *a2)
{
    *a1 = *a2 = -1;
    for (int i = 0; i < n; i++) {
        if (pat[i] < 0)
            continue;
        if (*a1 < 0 || BYTE_RANK[pat[i]] < BYTE_RANK[pat[*a1]])
            *a2 = *a1, *a1 = i;
        else if (*a2 < 0 || BYTE_RANK[pat[i]] < BYTE_RANK[pat[*a2]])
            *a2 = i;
    }
    if (*a1 < 0)
        return 0;
    if (*a2 < 0)
        *a2 = *a1;
    return 1;
}

static inline int pattern_hit(const uint8_t *c, const int *pat, int n)
{
    int i = 0;
    while (i < n && (pat[i] < 0 || c[i] == pat[i]))
        i++;
    return i == n;
}

static inline int scan_range(const uint8_t *lo, const uint8_t *hi, const int *pat, int n, int a1, int a2, uintptr_t *first)
{
    int hits = 0;
    if (hi - lo < n)
        return 0;
    size_t span = (size_t)(hi - lo) - n + 1, i = 0;
    const __m128i v1 = _mm_set1_epi8((char)pat[a1]), v2 = _mm_set1_epi8((char)pat[a2]);
    for (; i + 16 <= span; i += 16) {
        __m128i x1 = _mm_loadu_si128((const __m128i *)(lo + i + a1));
        __m128i x2 = _mm_loadu_si128((const __m128i *)(lo + i + a2));
        unsigned m = (unsigned)_mm_movemask_epi8(_mm_and_si128(_mm_cmpeq_epi8(x1, v1), _mm_cmpeq_epi8(x2, v2)));
        while (m) {
            const uint8_t *c = lo + i + __builtin_ctz(m);
            m &= m - 1;
            if (pattern_hit(c, pat, n) && hits++ == 0)
                *first = (uintptr_t)c;
        }
    }
    for (; i < span; i++)
        if (pattern_hit(lo + i, pat, n) && hits++ == 0)
            *first = (uintptr_t)(lo + i);
    return hits;
}

static inline int parse_pattern(const char *p, int *out, int max)
{
    int n = 0;
    while (*p) {
        while (*p == ' ')
            p++;
        if (!*p)
            break;
        if (n == max)
            return -1;
        if (*p == '?') {
            out[n++] = -1;
            while (*p == '?')
                p++;
        } else {
            char *end;
            long v = strtol(p, &end, 16);
            if (end == p || end - p > 2 || v < 0 || v > 255)
                return -1;
            out[n++] = (int)v;
            p = end;
        }
    }
    return n;
}

/* Many patterns over the same memory in one pass. Each pattern is keyed on its rarest run of four fixed
   bytes; every position's four bytes go through a bit filter and a hash table of those keys, and a key hit
   checks the whole pattern where it would start. A hit is counted exactly when scan_range would count it,
   in the same address order, so hits[i] and first[i] are what scan_range gives pattern i on each range. A
   pattern without four fixed bytes in a row is scanned on its own with scan_range. */
typedef struct {
    const int *pat;
    int n, a1, a2, key_off;
    uint32_t key;
} ScanPattern;

typedef struct {
    ScanPattern *pats;
    int count, solos;
    unsigned table_bits;
    uint32_t *filter;
    int *heads, *next, *solo;
} MultiScan;

#define MULTI_FILTER_BITS 21

static inline uint32_t multi_hash(uint32_t k) { return k * 2654435761u; }

static inline void multi_free(MultiScan *ms)
{
    free(ms->filter);
    free(ms->heads);
    free(ms->next);
    free(ms->solo);
    ms->filter = NULL, ms->heads = ms->next = ms->solo = NULL;
}

static inline int multi_key_offset(const int *pat, int n)
{
    int best = -1, best_cost = INT_MAX;
    for (int o = 0; o + 4 <= n; o++) {
        if (pat[o] < 0 || pat[o + 1] < 0 || pat[o + 2] < 0 || pat[o + 3] < 0)
            continue;
        int cost = BYTE_RANK[pat[o]] + BYTE_RANK[pat[o + 1]] + BYTE_RANK[pat[o + 2]] + BYTE_RANK[pat[o + 3]];
        if (cost < best_cost)
            best = o, best_cost = cost;
    }
    return best;
}

static inline int multi_prepare(MultiScan *ms, ScanPattern *pats, int count)
{
    memset(ms, 0, sizeof *ms);
    ms->pats = pats;
    ms->count = count;
    unsigned bits = 4;
    while ((1u << bits) < 2u * (unsigned)count)
        bits++;
    ms->table_bits = bits;
    ms->filter = (uint32_t *)calloc((size_t)1 << (MULTI_FILTER_BITS - 5), sizeof(uint32_t));
    ms->heads = (int *)malloc(sizeof(int) << bits);
    ms->next = (int *)malloc(sizeof(int) * (size_t)(count ? count : 1));
    ms->solo = (int *)malloc(sizeof(int) * (size_t)(count ? count : 1));
    if (!ms->filter || !ms->heads || !ms->next || !ms->solo) {
        multi_free(ms);
        return 0;
    }
    for (size_t i = 0; i < ((size_t)1 << bits); i++)
        ms->heads[i] = -1;
    for (int j = count - 1; j >= 0; j--) {
        ScanPattern *p = &pats[j];
        p->key_off = multi_key_offset(p->pat, p->n);
        if (p->key_off < 0) {
            ms->solo[ms->solos++] = j;
            continue;
        }
        const int *k = p->pat + p->key_off;
        p->key = (uint32_t)k[0] | (uint32_t)k[1] << 8 | (uint32_t)k[2] << 16 | (uint32_t)k[3] << 24;
        uint32_t h = multi_hash(p->key), f = h >> (32 - MULTI_FILTER_BITS);
        ms->filter[f >> 5] |= 1u << (f & 31);
        unsigned slot = h >> (32 - bits);
        ms->next[j] = ms->heads[slot];
        ms->heads[slot] = j;
    }
    return 1;
}

/* Keyed patterns that start in [lo, hi) and end by hi, for key positions q in [qlo, qhi): a range split
   into pieces gives the same hits piece by piece, in order. */
static inline void multi_scan_keys(const MultiScan *ms, const uint8_t *lo, const uint8_t *hi, const uint8_t *qlo,
                                   const uint8_t *qhi, int *hits, uintptr_t *first)
{
    if (hi - lo < 4)
        return;
    const uint8_t *last = hi - 4;
    if (qhi > last + 1)
        qhi = last + 1;
    const unsigned shift = 32 - ms->table_bits;
    for (const uint8_t *q = qlo; q < qhi; q++) {
        uint32_t k;
        memcpy(&k, q, 4);
        uint32_t h = multi_hash(k), f = h >> (32 - MULTI_FILTER_BITS);
        if (!(ms->filter[f >> 5] & (1u << (f & 31))))
            continue;
        for (int j = ms->heads[h >> shift]; j >= 0; j = ms->next[j]) {
            const ScanPattern *p = &ms->pats[j];
            if (p->key != k || q - lo < p->key_off)
                continue;
            const uint8_t *c = q - p->key_off;
            if (hi - c < p->n || !pattern_hit(c, p->pat, p->n))
                continue;
            if (hits[j]++ == 0)
                first[j] = (uintptr_t)c;
        }
    }
}

static inline void multi_scan_solos(const MultiScan *ms, const uint8_t *lo, const uint8_t *hi, int *hits, uintptr_t *first)
{
    for (int s = 0; s < ms->solos; s++) {
        int j = ms->solo[s];
        const ScanPattern *p = &ms->pats[j];
        uintptr_t f = 0;
        int h = scan_range(lo, hi, p->pat, p->n, p->a1, p->a2, &f);
        if (h && !hits[j])
            first[j] = f;
        hits[j] += h;
    }
}

static inline void multi_scan_range(const MultiScan *ms, const uint8_t *lo, const uint8_t *hi, int *hits, uintptr_t *first)
{
    multi_scan_keys(ms, lo, hi, lo, hi, hits, first);
    multi_scan_solos(ms, lo, hi, hits, first);
}
