"""The core's signature declarations, read from source: runtime::Sig and the structs that share its leading fields.

sigverify.py and native/tools/patch_sites.py both read them through this module. Standard library only.

  declarations(text, types)       (type, name, line, args) for each `const|constexpr [runtime::]TYPE NAME{...}`
  fields(args, layout, ...)       the declaration's values by field name; ValueError when one is not a literal or a
                                  constant the caller passes
"""
import re

SIG_LAYOUT = ('what', 'pattern', 'rva', 'kind', 'insn', 'anchor', 'len', 'section')
DEFAULTS = {'rva': 0, 'kind': 'addr', 'insn': 0, 'anchor': 0, 'len': 0, 'section': '.xtls'}
TEXT_FIELDS = ('what', 'pattern', 'section')

DECL = re.compile(r'(?:const|constexpr)\s+(?:runtime::)?(?P<type>\w+)\s+(?P<name>[A-Za-z_]\w*)\s*\{')
STRING = r'(?:\s*"(?:[^"\\]|\\.)*")+'
STRING_DECL = re.compile(r'(?:constexpr|const)\s+(?:static\s+)?(?:const\s+)?char\s*(?:const\s*)?'
                         r'(?:\*\s*(?:const\s+)?(?P<p>\w+)|\s(?P<a>\w+)\s*\[\s*\])\s*=\s*(?P<v>' + STRING + r')')
INT_DECL = re.compile(r'(?:constexpr|const)\s+(?:static\s+)?(?:std::)?(?:u?int\d+_t|size_t|uintptr_t|unsigned|int|long)\s+'
                      r'[A-Za-z_]\w*\s*=[^;]*;')
INT_PART = re.compile(r'([A-Za-z_]\w*)\s*=\s*(0[xX][0-9A-Fa-f]+|\d+)[uUlL]*\b')


def strip_comments(text):
    text = re.sub(r'/\*.*?\*/', lambda m: '\n' * m.group(0).count('\n'), text, flags=re.S)
    return re.sub(r'//[^\n]*', '', text)


def split_args(body):
    out, cur, depth, quoted = [], '', 0, False
    for i, ch in enumerate(body):
        if ch == '"' and (i == 0 or body[i - 1] != '\\'):
            quoted = not quoted
        if not quoted and ch in '({[':
            depth += 1
        elif not quoted and ch in ')}]':
            depth -= 1
        elif ch == ',' and not quoted and depth == 0:
            out.append(cur.strip())
            cur = ''
            continue
        cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def braced(text, at):
    depth = 0
    for i in range(at, len(text)):
        if text[i] == '{':
            depth += 1
        elif text[i] == '}':
            depth -= 1
            if depth == 0:
                return text[at + 1:i]
    raise ValueError(f'unclosed brace at offset {at}')


def literal(text):
    """The joined value of one or more adjacent string literals, else None."""
    if not re.fullmatch(STRING + r'\s*', text):
        return None
    return ''.join(re.findall(r'"((?:[^"\\]|\\.)*)"', text))


def int_arg(text, names=None):
    """An integer argument: a literal, a constant in names, or a sum of literals and casts such as
    `uintptr_t(0) - 0x31c`."""
    if names and text.strip() in names:
        return names[text.strip()]
    expr = re.sub(r'\b(?:uintptr_t|size_t|uint32_t|int32_t|int)\s*\(\s*([^()]*)\s*\)', r'\1', text)
    terms = re.findall(r'\s*([+-]?)\s*(0[xX][0-9a-fA-F]+|\d+)[uUlL]*', expr)
    if not terms or re.sub(r'\s|[+-]|0[xX][0-9a-fA-F]+[uUlL]*|\d+[uUlL]*', '', expr):
        raise ValueError(f'not an integer expression: {text}')
    return sum(-int(v, 0) if sign == '-' else int(v, 0) for sign, v in terms)


def string_consts(text):
    return {m['p'] or m['a']: literal(m['v']) for m in STRING_DECL.finditer(strip_comments(text))}


def int_consts(text):
    out = {}
    for d in INT_DECL.finditer(strip_comments(text)):
        for name, v in INT_PART.findall(d.group(0)):
            out.setdefault(name, int(v, 0))
    return out


def declarations(text, types=('Sig',)):
    """(type, name, line, args) for each declaration of one of types whose first value is a string literal and that
    has a pattern and an rva."""
    text = strip_comments(text)
    for m in DECL.finditer(text):
        if m['type'] not in types:
            continue
        args = split_args(braced(text, m.end() - 1))
        if len(args) < 3 or not args[0].startswith('"'):
            continue
        yield m['type'], m['name'], text.count('\n', 0, m.start()) + 1, args


def fields(args, layout=SIG_LAYOUT, strings=None, ints=None):
    out = dict(DEFAULTS)
    for field, v in zip(layout, args):
        if field in TEXT_FIELDS:
            s = literal(v)
            if s is None and strings and v.strip() in strings:
                s = strings[v.strip()]
            if s is None:
                raise ValueError(f'{field} {v} is not a literal or a known constant')
            out[field] = ' '.join(s.split()) if field == 'pattern' else s
        elif field == 'kind':
            out['kind'] = v.split('::')[-1]
        else:
            out[field] = int_arg(v, ints)
    return out
