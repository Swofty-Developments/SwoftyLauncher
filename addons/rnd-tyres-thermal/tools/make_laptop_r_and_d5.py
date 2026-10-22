"""Add the 5th R&D overview TILE (focusable.tyres) to screens/laptop_r_and_d.ui.

Clones focusable.aero, binds it to the --dept view-model child, attaches it to the
--slot scene element and gives focused_department a matching event. build_pie.py
then rebinds it from `fake` to `tyres` and adds the pie wiring.

The three team performance charts get a 13th row, entry_12 bound to pill_12, for the
26-car grid of f1-1990, which adds pill_12 to their scenes.
"""
import argparse

import common  # noqa: F401
import pkg as P
import uinode
from idf import djb2
from pkg import Arr, Obj, Val
from uitree import is_node

TAB = 'tab_content_rnd_upgrades_alt'
GROUP = f'{TAB}.focusable'
CHARTS = (
    'tab_content_rnd_upgrades_in_progress.refresh_performance_chart.performance_chart',
    f'{TAB}.performance_chart',
    'tab_content_research_overview.perf_info.performance_chart',
)


def find(x, path, want):
    if is_node(x):
        path = path + [x.items[0]['name'].value]
        if '.'.join(path) == want:
            return x
        for c in x.items[1].items if isinstance(x.items[1], Arr) else []:
            r = find(c, path, want)
            if r is not None:
                return r
        return None
    items = x.items if isinstance(x, (Arr, Obj)) else []
    for it in items:
        r = find(it[1] if isinstance(x, Obj) else it, path, want)
        if r is not None:
            return r
    return None


def clone(n):
    if isinstance(n, Obj):
        return Obj([(clone(k), clone(v)) for k, v in n.items], n.version)
    if isinstance(n, Arr):
        return Arr([clone(v) for v in n.items], n.tag)
    if isinstance(n, Val):
        return Val(n.type, n.value, None)
    return n


def rewrite(x, old, new):
    if isinstance(x, Obj):
        for i, (k, v) in enumerate(x.items):
            if isinstance(v, Val) and v.type == 'stri' and isinstance(v.value, str):
                if k.value == 'to_object_name' and (v.value == old or v.value.startswith(old + '.')):
                    path = new + v.value[len(old):]
                    x.items[i] = (k, Val('stri', path))
                    x.set('to_object', Val('ui32', djb2(path)))
            else:
                rewrite(v, old, new)
    elif isinstance(x, Arr):
        for v in x.items:
            rewrite(v, old, new)


def fix_ids(node, path):
    props = node.items[0]
    props.set('id', Val('ui32', djb2(path)))
    for c in node.items[1].items if isinstance(node.items[1], Arr) else []:
        if is_node(c):
            fix_ids(c, f"{path}.{c.items[0]['name'].value}")


def set_binding(props, key, dbentry):
    b = props.get(key)
    b.set('dbentry', Val('stri', dbentry))


def grow_chart(doc, path, src='entry_11', new='entry_12', element='pill_12', rows=13):
    chart = find(doc.root, [], path)
    if chart is None:
        raise SystemExit(f'{path} not found')
    kids = chart.items[1].items
    at = next(i for i, c in enumerate(kids) if is_node(c) and uinode.name(c) == src)
    if any(is_node(c) and uinode.name(c) == new for c in kids):
        raise SystemExit(f'{path}.{new} already exists')
    row = uinode.clone_node(kids[at], path, new)
    row.items[4].items[0].items[0]['debugName'].value = element
    uinode.fix_ids(row, path)
    kids.insert(at + 1, row)
    uinode.copy_signals(doc.root, f'{path}.{src}', f'{path}.{new}')
    chart.items[0].get('viewHeight')['value'].value = rows


def build(data, dept='fake', name='tyres', slot='tile_5'):
    doc = P.Pkg(data)
    assert doc.to_bytes() == data
    group = find(doc.root, [], GROUP)
    src = find(doc.root, [], f'{GROUP}.aero')
    if group is None or src is None:
        raise SystemExit('focusable.aero not found')
    if find(doc.root, [], f'{GROUP}.{name}') is not None:
        raise SystemExit(f'{name} already exists')
    tile = clone(src)
    props = tile.items[0]
    props.set('name', Val('stri', name))
    props.set('nameId', Val('ui32', djb2(name)))
    set_binding(props, 'dataContext', dept)
    tile.items[4] = Arr([Arr([Obj([(Val('stri', 'nodeId'), Val('ui32', djb2(slot))),
                                   (Val('stri', 'debugName'), Val('stri', slot))])])], tile.items[4].tag)
    rewrite(tile, f'{GROUP}.aero', f'{GROUP}.{name}')
    for ev in tile.items[3].items:
        for sig in ev.get('signals').items:
            cmd = sig.get('attributes') and sig.get('attributes').get('command')
            if cmd is not None and cmd.get('data') is not None:
                cmd.get('data').set('value', Val('stri', dept))
    fix_ids(tile, f'{GROUP}.{name}')
    kids = group.items[1].items
    at = next(i for i, c in enumerate(kids) if is_node(c) and c.items[0]['name'].value == 'focused_department')
    kids.insert(at, tile)
    focus = find(doc.root, [], f'{GROUP}.focused_department')
    events = focus.items[3].items
    chassis = next(e for e in events if e.get('eventName').value == 'chassis')
    ev = clone(chassis)
    ev.set('eventName', Val('stri', dept))
    ev.set('event', Val('ui32', djb2(dept)))
    sig = ev.get('signals').items[0]
    sig.set('from_pin_name', Val('stri', dept))
    sig.set('to_object_name', Val('stri', f'{GROUP}.{name}'))
    sig.set('to_object', Val('ui32', djb2(f'{GROUP}.{name}')))
    events.append(ev)
    for path in CHARTS:
        grow_chart(doc, path)
    out = doc.to_bytes()
    check = P.Pkg(out)
    assert check.to_bytes() == out
    assert find(check.root, [], f'{GROUP}.{name}') is not None
    return out


def main():
    ap = argparse.ArgumentParser(description='Add the 5th R&D overview tile to laptop_r_and_d.ui.')
    ap.add_argument('src', help='stock screens/laptop_r_and_d.ui')
    ap.add_argument('out')
    ap.add_argument('--dept', default='fake', help='VM child the 5th tile binds (dataContext) and the focus command data')
    ap.add_argument('--name', default='tyres', help='node name of the 5th TILE')
    ap.add_argument('--slot', default='tile_5', help='scene element the TILE attaches to')
    a = ap.parse_args()
    with open(a.src, 'rb') as f:
        out = build(f.read(), a.dept, a.name, a.slot)
    with open(a.out, 'wb') as f:
        f.write(out)
    print(f'wrote {a.out} ({len(out)} bytes): {GROUP}.{a.name} [TILE] -> {a.slot} (nodeId {djb2(a.slot)}), dataContext @{a.dept}')


if __name__ == '__main__':
    main()
