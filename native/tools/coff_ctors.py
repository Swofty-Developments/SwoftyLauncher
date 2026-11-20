"""Reads what a COFF object's static constructors do, for tools/init_check.py.

An x86_64-windows-gnu object lists the functions the C++ runtime calls at load in its .ctors sections (.CRT$XC* and
.init_array* are read too): the unit's own _GLOBAL__sub_I_<file> and one per inline variable or static member of a
class template with vague linkage it initializes, in a COMDAT section of its own that the linker keeps from the first
object in link order that has it. ctors() lists them in section order and follows each, and the
__cxx_global_var_init functions it calls, through the relocations in its code. The unit's own writable objects the
code stores to, or passes to a call as the object (rcx), are the ones it initializes; one it only reads, or whose
address it takes into another register or stores, is not. A COMDAT constructor initializes the variable its guard
variable (_ZGV) names, and a __dtor_<object> it refers to is a destructor it registers with atexit. Another unit's
object it refers to, which a mingw object reaches through a .refptr.<object> slot, is listed as external."""

import re
import struct

WRITE = 0x80000000
LNK_NRELOC_OVFL = 0x01000000
CTOR_SECTION = re.compile(r'^(\.ctors(\.\d+)?|\.CRT\$XC[A-Z]*|\.init_array(\.\d+)?)$')
FOLLOW = re.compile(r'^(__cxx_global_var_init(\.\d+)?|__cxx_global_array_dtor(\.\d+)?)$')
BIGOBJ_CLASS = bytes.fromhex('c7a1bad1eebaa94baf20faf66aa4dcb8')


class Symbol:
    def __init__(self, name, value, section, kind, storage):
        self.name, self.value, self.section, self.kind, self.storage = name, value, section, kind, storage


class Ctor:
    def __init__(self, function, comdat):
        self.function, self.comdat = function, comdat
        self.initialized, self.destroyed, self.external = [], [], []


def read(path):
    data = open(path, 'rb').read()
    bigobj = data[:4] == b'\0\0\xff\xff' and data[12:28] == BIGOBJ_CLASS
    if bigobj:
        nsections, symtab, nsyms = struct.unpack_from('<III', data, 44)
        header, symsize, secfmt = 56, 20, '<i'
    else:
        nsections, = struct.unpack_from('<H', data, 2)
        symtab, nsyms, optsize = struct.unpack_from('<IIH', data, 8)
        header, symsize, secfmt = 20 + optsize, 18, '<h'
    strtab = symtab + nsyms * symsize

    def name_at(raw):
        if raw[:4] == b'\0\0\0\0':
            offset, = struct.unpack_from('<I', raw, 4)
            return data[strtab + offset:data.index(b'\0', strtab + offset)].decode()
        return raw.rstrip(b'\0').decode()

    sections = []
    for i in range(nsections):
        at = header + 40 * i
        raw = data[at:at + 8]
        name = raw.rstrip(b'\0').decode()
        if name.startswith('/'):
            offset = int(name[1:])
            name = data[strtab + offset:data.index(b'\0', strtab + offset)].decode()
        size, rawptr, relptr, _, nrel, _, flags = struct.unpack_from('<IIIIHHI', data, at + 16)
        if flags & LNK_NRELOC_OVFL:
            nrel, = struct.unpack_from('<I', data, relptr)
            relocs = [struct.unpack_from('<IIH', data, relptr + 10 * j) for j in range(1, nrel)]
        else:
            relocs = [struct.unpack_from('<IIH', data, relptr + 10 * j) for j in range(nrel)]
        sections.append({'name': name, 'size': size, 'flags': flags, 'relocs': relocs, 'comdat': bool(flags & 0x1000),
                         'data': data[rawptr:rawptr + size] if rawptr else b''})

    symbols, i = {}, 0
    while i < nsyms:
        at = symtab + i * symsize
        value, = struct.unpack_from('<I', data, at + 8)
        section, = struct.unpack_from(secfmt, data, at + 12)
        kind, storage, aux = struct.unpack_from('<HBB', data, at + 12 + (4 if bigobj else 2))
        symbols[i] = Symbol(name_at(data[at:at + 8]), value, section, kind, storage)
        i += 1 + aux
    return sections, symbols


LOADS = {0x8B, 0x8A, 0x3B, 0x3A, 0x39, 0x38, 0x85, 0x84, 0x63}
LOADS_0F = {0x10, 0x28, 0x6F, 0x7E, 0xB6, 0xB7, 0xBE, 0xBF, 0x12, 0x16, 0x2E, 0x2F, 0x5E, 0x59, 0x58, 0x5C, 0x54, 0x57}


def access(code, at):
    """How the instruction whose RIP-relative displacement sits at `at` uses its object: 'write' for a store, a
    read-modify-write or its address loaded into rcx for a call (the object a constructor or member function is
    given, in the Windows x64 convention), 'address' for its address taken into another register or stored by the
    next instruction, 'read' for a load or a compare. A displacement it cannot place counts as a write."""
    if at < 2 or code[at - 1] & 0xC7 != 0x05:
        return 'write'
    modrm, op = code[at - 1], code[at - 2]
    rex = code[at - 3] if at >= 3 and 0x40 <= code[at - 3] <= 0x4F else 0
    if op == 0x8D:
        nxt = code[at + 4:at + 7]
        stored = len(nxt) == 3 and nxt[0] == 0x48 and nxt[1] == 0x89 and nxt[2] >> 6 != 3 and (nxt[2] >> 3) & 7 == 1
        return 'write' if (modrm >> 3) & 7 == 1 and not rex & 0x04 and not stored else 'address'
    if op in LOADS:
        return 'read'
    if op in (0x80, 0x81, 0x83) and (modrm >> 3) & 7 == 7 or op in (0xF6, 0xF7) and (modrm >> 3) & 7 == 0:
        return 'read'
    if at >= 3 and code[at - 3] == 0x0F and op in LOADS_0F:
        return 'read'
    return 'write'


def ctors(path):
    sections, symbols = read(path)
    by_section = {}
    for s in symbols.values():
        if s.section > 0 and s.name != sections[s.section - 1]['name']:
            by_section.setdefault(s.section, []).append(s)

    def code(fn):
        sec = sections[fn.section - 1]
        later = [s.value for s in by_section.get(fn.section, []) if s.value > fn.value]
        end = min(later, default=sec['size'])
        return [(symbols[index], access(sec['data'], offset)) for offset, index, _ in sec['relocs']
                if fn.value <= offset < end and index in symbols]

    def own(s):
        sec = sections[s.section - 1] if s.section > 0 else None
        return sec and sec['flags'] & WRITE and not sec['comdat'] and s.kind & 0xf0 != 0x20 and s.name != sec['name']

    out = []
    for sec in sections:
        if not CTOR_SECTION.match(sec['name']):
            continue
        for _, index, _ in sec['relocs']:
            fn = symbols[index]
            ctor = Ctor(fn.name, sec['comdat'])
            seen, todo = {fn.name}, [(fn, False)]
            while todo:
                f, in_dtor = todo.pop(0)
                for s, how in code(f):
                    if s.name.startswith('__dtor_'):
                        name = s.name[len('__dtor_'):]
                        if name not in ctor.destroyed:
                            ctor.destroyed.append(name)
                    elif FOLLOW.match(s.name) and s.section > 0 and s.name not in seen:
                        seen.add(s.name)
                        todo.append((s, in_dtor or s.name.startswith('__cxx_global_array_dtor')))
                    elif s.name.startswith('_ZGV') and ctor.comdat:
                        name = '_Z' + s.name[len('_ZGV'):]
                        if name not in ctor.initialized:
                            ctor.initialized.append(name)
                    elif s.name.startswith('.refptr.') and not in_dtor:
                        if s.name[len('.refptr.'):] not in ctor.external:
                            ctor.external.append(s.name[len('.refptr.'):])
                    elif own(s) and not ctor.comdat and (in_dtor or how == 'write'):
                        target = ctor.destroyed if in_dtor else ctor.initialized
                        if s.name not in target:
                            target.append(s.name)
            out.append(ctor)
    return out


def leaf(symbol):
    """The unqualified name of a variable from its mangled symbol, or the symbol when it is not mangled: the source
    name its mangling ends in."""
    if not symbol.startswith('_Z'):
        return symbol
    s = symbol[:-1] if symbol.endswith('E') else symbol
    for q in range(len(s) - 1, 0, -1):
        if s[q - 1].isdigit() and not s[q].isdigit():
            p = q - 1
            while p > 0 and s[p - 1].isdigit():
                p -= 1
            for start in range(p, q):
                if int(s[start:q]) == len(s) - q:
                    return s[q:]
    return symbol


def readable(symbol):
    """A demangled spelling of a variable's nested name, with template arguments shown as <...>."""
    if not symbol.startswith('_Z'):
        return symbol
    parts, i, depth = [], 2, 0
    while i < len(symbol):
        c = symbol[i]
        if depth == 0 and c.isdigit():
            j = i
            while symbol[j].isdigit():
                j += 1
            n = int(symbol[i:j])
            name = symbol[j:j + n]
            parts.append('(anonymous namespace)' if name.startswith('_GLOBAL__N') else name)
            i = j + n
            continue
        if c == 'I':
            if depth == 0 and parts:
                parts[-1] += '<...>'
            depth += 1
        elif c == 'E' and depth:
            depth -= 1
        i += 1
    return '::'.join(parts) if parts else symbol
