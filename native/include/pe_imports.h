#pragma once
#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* Walks a mapped PE32+ image's import table without trusting it. Another module can rewrite an image's import
   descriptors, name entries or slots at run time, so every descriptor, thunk, name RVA and string is checked
   against SizeOfImage and the caller's readable(address, bytes) before it is read. An entry that fails comes back
   as PE_IMPORT_SKIP and the walk goes on. Nothing here raises or catches an exception. Shared by the loader (C)
   and the core (C++). */

typedef int (*pe_readable_fn)(uintptr_t at, size_t n, void *ctx);

enum { PE_IMPORT_END, PE_IMPORT_ENTRY, PE_IMPORT_SKIP };
enum { PE_IMPORT_BY_NAME, PE_IMPORT_BY_ORDINAL, PE_IMPORT_UNNAMED };

#define PE_IMPORT_NAME_MAX 4096

typedef struct {
    int table;
    size_t index;
    const char *dll;
    uint64_t *slot;
    uint32_t slot_rva;
    int kind;
    const char *name;
    uint16_t ordinal;
    const char *why;
    uint64_t value;
    uint64_t value_rva;
} pe_import;

typedef struct {
    uintptr_t base;
    uint32_t size;
    pe_readable_fn readable;
    void *ctx;
    const char *only;
    uint64_t desc;
    uint64_t names, slots;
    int named;
    const char *dll;
    size_t index;
    int table;
    int done;
} pe_imports;

static inline uint16_t pe_imp_u16(uintptr_t at)
{
    uint16_t v;
    memcpy(&v, (const void *)at, sizeof v);
    return v;
}

static inline uint32_t pe_imp_u32(uintptr_t at)
{
    uint32_t v;
    memcpy(&v, (const void *)at, sizeof v);
    return v;
}

static inline uint64_t pe_imp_u64(uintptr_t at)
{
    uint64_t v;
    memcpy(&v, (const void *)at, sizeof v);
    return v;
}

static inline int pe_imp_span(const pe_imports *it, uint64_t rva, uint64_t n)
{
    return rva < it->size && n <= it->size - rva && it->readable(it->base + (uintptr_t)rva, (size_t)n, it->ctx);
}

static inline const char *pe_imp_string(const pe_imports *it, uint64_t rva)
{
    if (rva >= it->size)
        return NULL;
    uint64_t end = it->size - rva > PE_IMPORT_NAME_MAX ? rva + PE_IMPORT_NAME_MAX : it->size;
    for (uint64_t at = rva; at < end; at++) {
        uintptr_t p = it->base + (uintptr_t)at;
        if ((at == rva || !(p & 0xfff)) && !it->readable(p, 1, it->ctx))
            return NULL;
        if (!*(const char *)p)
            return at == rva ? NULL : (const char *)(it->base + (uintptr_t)rva);
    }
    return NULL;
}

static inline int pe_imp_ieq(const char *a, const char *b)
{
    for (;; a++, b++) {
        char x = *a >= 'A' && *a <= 'Z' ? (char)(*a + 32) : *a, y = *b >= 'A' && *b <= 'Z' ? (char)(*b + 32) : *b;
        if (x != y)
            return 0;
        if (!x)
            return 1;
    }
}

static inline int pe_imports_open(pe_imports *it, uintptr_t base, const char *only, pe_readable_fn readable, void *ctx)
{
    memset(it, 0, sizeof *it);
    it->done = 1;
    if (!base || !readable(base, 0x40, ctx) || pe_imp_u16(base) != 0x5A4D)
        return 0;
    uint32_t nt = pe_imp_u32(base + 0x3C);
    if (nt > 0x10000000 || !readable(base + nt, 0x98, ctx) || pe_imp_u32(base + nt) != 0x4550 ||
        pe_imp_u16(base + nt + 0x18) != 0x20B || pe_imp_u32(base + nt + 0x18 + 0x6C) <= 1)
        return 0;
    it->base = base;
    it->size = pe_imp_u32(base + nt + 0x18 + 0x38);
    it->readable = readable;
    it->ctx = ctx;
    it->only = only;
    it->desc = pe_imp_u32(base + nt + 0x18 + 0x78);
    if (!it->desc || (uint64_t)nt + 0x98 > it->size || base + it->size < base)
        return 0;
    it->done = 0;
    return 1;
}

static inline int pe_imports_skip(pe_imports *it, pe_import *e, const char *why, uint64_t value, uint64_t value_rva)
{
    e->table = it->table;
    e->dll = it->dll;
    e->why = why;
    e->value = value;
    e->value_rva = value_rva;
    return PE_IMPORT_SKIP;
}

static inline int pe_imports_next(pe_imports *it, pe_import *e)
{
    memset(e, 0, sizeof *e);
    while (!it->done) {
        if (!it->dll) {
            uint64_t at = it->desc;
            if (!pe_imp_span(it, at, 20)) {
                it->done = 1;
                return pe_imports_skip(it, e, "import descriptor outside the image", 0, at);
            }
            uint32_t oft = pe_imp_u32(it->base + (uintptr_t)at), name = pe_imp_u32(it->base + (uintptr_t)at + 12),
                     ft = pe_imp_u32(it->base + (uintptr_t)at + 16);
            if (!name) {
                it->done = 1;
                break;
            }
            it->desc += 20;
            const char *dll = pe_imp_string(it, name);
            if (!dll)
                return pe_imports_skip(it, e, "import DLL name outside the image", name, at + 12);
            if (it->only && !pe_imp_ieq(dll, it->only))
                continue;
            it->table++;
            it->dll = dll;
            it->named = oft != 0;
            it->names = oft ? oft : ft;
            it->slots = ft;
            it->index = 0;
            continue;
        }
        uint64_t k = it->index, name_rva = it->names + 8 * k, slot_rva = it->slots + 8 * k;
        if (!pe_imp_span(it, name_rva, 8)) {
            pe_imports_skip(it, e, "import name table runs outside the image", 0, name_rva);
            e->index = (size_t)k;
            it->dll = NULL;
            return PE_IMPORT_SKIP;
        }
        uint64_t v = pe_imp_u64(it->base + (uintptr_t)name_rva);
        if (!v) {
            it->dll = NULL;
            continue;
        }
        it->index++;
        e->table = it->table;
        e->index = (size_t)k;
        e->dll = it->dll;
        if (!pe_imp_span(it, slot_rva, 8)) {
            pe_imports_skip(it, e, "import slot outside the image", v, slot_rva);
            it->dll = NULL;
            return PE_IMPORT_SKIP;
        }
        e->slot = (uint64_t *)(it->base + (uintptr_t)slot_rva);
        e->slot_rva = (uint32_t)slot_rva;
        if (!it->named) {
            e->kind = PE_IMPORT_UNNAMED;
            return PE_IMPORT_ENTRY;
        }
        if (v >> 63) {
            e->kind = PE_IMPORT_BY_ORDINAL;
            e->ordinal = (uint16_t)(v & 0xFFFF);
            return PE_IMPORT_ENTRY;
        }
        const char *fn = v < it->size ? pe_imp_string(it, v + 2) : NULL;
        if (!fn) {
            e->slot = NULL;
            e->slot_rva = 0;
            return pe_imports_skip(it, e, "import name outside the image", v, name_rva);
        }
        e->kind = PE_IMPORT_BY_NAME;
        e->name = fn;
        return PE_IMPORT_ENTRY;
    }
    return PE_IMPORT_END;
}

/* Points every slot imported under fn (or imported with no name table) that still holds one of match[] at repl,
   through write(). Returns the slots written; skipped counts the entries left out as corrupt, first_skip is the
   first of them. */
typedef struct {
    const char *fn;
    const void *const *match;
    int matches;
    const void *repl;
    int (*write)(uint64_t *slot, const void *value, void *ctx);
    int skipped;
    pe_import first_skip;
} pe_patch;

static inline int pe_patch_imports(uintptr_t base, pe_readable_fn readable, void *ctx, pe_patch *p)
{
    pe_imports it;
    pe_import e;
    int done = 0, r;
    if (!pe_imports_open(&it, base, NULL, readable, ctx))
        return 0;
    while ((r = pe_imports_next(&it, &e)) != PE_IMPORT_END) {
        if (r == PE_IMPORT_SKIP) {
            if (!p->skipped++)
                p->first_skip = e;
            continue;
        }
        if (e.kind == PE_IMPORT_BY_ORDINAL || (e.kind == PE_IMPORT_BY_NAME && strcmp(e.name, p->fn)))
            continue;
        const void *now = (const void *)(uintptr_t)*e.slot;
        int known = 0;
        for (int j = 0; j < p->matches; j++)
            known |= p->match[j] && now == p->match[j];
        if (known && p->write(e.slot, p->repl, ctx))
            done++;
    }
    return done;
}
