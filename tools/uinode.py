import sys, os, copy
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pkg import Pkg, Obj, Arr, Val

PROPS, CHILDREN, COMMANDS, EVENTS, LAYOUT, COMPONENTS, BEHAVIOURS, EXTRA = range(8)


def djb2(s):
    h = 5381
    for c in s.encode():
        h = (h * 33 + c) & 0xFFFFFFFF
    return h


def is_node(n):
    return isinstance(n, Arr) and len(n.items) == 8 and isinstance(n.items[0], Obj) and n.items[0].get('name') is not None


def name(n):
    return n.items[PROPS]['name'].value


def children(n):
    return n.items[CHILDREN].items


def child(n, nm):
    for c in children(n):
        if name(c) == nm:
            return c
    raise KeyError(nm)


def setv(o, key, val):
    v = o.get(key)
    if isinstance(v, Val):
        v.value = val
    else:
        o.set(key, val)


def prop(n, key):
    return n.items[PROPS].get(key)


def set_prop_value(n, key, value, type='stri'):
    o = n.items[PROPS].get(key)
    if o is None:
        n.items[PROPS].set(key, Obj([(Val('stri', 'value'), Val(type, value))]))
    else:
        o['value'].value = value


def drop_key(o, key):
    o.items = [(k, v) for k, v in o.items if k.value != key]


def fix_signals(arr):
    if not isinstance(arr, Arr):
        return
    for e in arr.items:
        if not isinstance(e, Obj):
            continue
        if e.get('eventName') is not None and e.get('event') is not None:
            e['event'].value = djb2(e['eventName'].value)
        if e.get('commandName') is not None and e.get('command') is not None:
            e['command'].value = djb2(e['commandName'].value)
        sigs = e.get('signals')
        for s in (sigs.items if isinstance(sigs, Arr) else []):
            s['to_object'].value = djb2(s['to_object_name'].value)
            s['to_pin'].value = djb2(s['to_pin_name'].value)


def fix_ids(n, parent=None):
    p = n.items[PROPS]
    nm = p['name'].value
    path = nm if parent is None else f'{parent}.{nm}'
    if p.get('id') is not None:
        p['id'].value = djb2(path)
    if p.get('nameId') is not None:
        p['nameId'].value = djb2(nm)
    fix_signals(n.items[COMMANDS])
    fix_signals(n.items[EVENTS])
    lay = n.items[LAYOUT]
    if isinstance(lay, Arr):
        for grp in lay.items:
            for b in (grp.items if isinstance(grp, Arr) else []):
                if isinstance(b, Obj) and b.get('nodeId') is not None and b.get('debugName') is not None:
                    b['nodeId'].value = djb2(b['debugName'].value)
    for c in children(n):
        if is_node(c):
            fix_ids(c, path)


def rename_refs(n, old, new):
    def walk(x):
        if isinstance(x, Val):
            if x.type == 'stri' and isinstance(x.value, str) and (x.value == old or x.value.startswith(old + '.')):
                x.value = new + x.value[len(old):]
        elif isinstance(x, Obj):
            for k, v in x.items:
                walk(v)
        elif isinstance(x, Arr):
            for v in x.items:
                walk(v)
    walk(n)


def clone_node(src, parent_path, new_name):
    n = copy.deepcopy(src)
    old_path = f'{parent_path}.{name(src)}'
    rename_refs(n, old_path, f'{parent_path}.{new_name}')
    n.items[PROPS]['name'].value = new_name
    fix_ids(n, parent_path)
    return n


def copy_signals(root, old, new):
    count = 0

    def visit(n, parent):
        nonlocal count
        path = name(n) if parent is None else f'{parent}.{name(n)}'
        if any(path == p or path.startswith(p + '.') for p in (old, new)):
            return
        for slot in (COMMANDS, EVENTS):
            for e in n.items[slot].items if isinstance(n.items[slot], Arr) else []:
                sigs = e.get('signals') if isinstance(e, Obj) else None
                if not isinstance(sigs, Arr):
                    continue
                out = []
                for sg in sigs.items:
                    out.append(sg)
                    to = sg['to_object_name'].value
                    if to == old or to.startswith(old + '.'):
                        c = copy.deepcopy(sg)
                        rename_refs(c, old, new)
                        c['to_object'].value = djb2(c['to_object_name'].value)
                        out.append(c)
                        count += 1
                sigs.items[:] = out
        for c in children(n):
            if is_node(c):
                visit(c, path)

    def top(x):
        if is_node(x):
            visit(x, None)
        elif isinstance(x, Obj):
            for _, v in x.items:
                top(v)
        elif isinstance(x, Arr):
            for v in x.items:
                top(v)
    top(root)
    return count


def signal(to_object_name, to_pin_name, from_pin_name, attributes=None):
    items = [
        (Val('stri', 'to_object'), Val('ui32', djb2(to_object_name))),
        (Val('stri', 'to_object_name'), Val('stri', to_object_name)),
        (Val('stri', 'to_pin_name'), Val('stri', to_pin_name)),
        (Val('stri', 'to_pin'), Val('ui32', djb2(to_pin_name))),
        (Val('stri', 'from_pin_name'), Val('stri', from_pin_name)),
    ]
    if attributes:
        items.append((Val('stri', 'attributes'), Obj([(Val('stri', k), Obj([(Val('stri', 'value'), Val('stri', v))])) for k, v in attributes.items()])))
    return Obj(items)
