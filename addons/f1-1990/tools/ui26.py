#!/usr/bin/env python3
"""Writes erp/ui26.json and ui/: the team lists grown for 13 teams. Constructor standings get a 13th row; the
Vehicle Performance Comparison email, the R&D and part-fitting charts and the team select chart get rows up to 13 and
fit them into the height the stock layout gives 11 rows. The team select carousel moves to a copy of its scene whose
tiles fit 13 across. laptop_r_and_d.ui is left to rnd-tyres-thermal, which replaces it.

usage: ui26.py [--game <F1 25>] [--baseline <launcher baseline>/<build>]

The stock resources come from the baseline when it lists the archive, else from the game folder.
"""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(REPO / "tools"), str(REPO / "tools/swofty-ui")]

import idf as I  # noqa: E402
import scene as S  # noqa: E402
from erp import parse as erp_parse  # noqa: E402
from pkg import Arr, Obj, Pkg, Val  # noqa: E402
from uinode import CHILDREN, LAYOUT, PROPS, children, clone_node, copy_signals, fix_ids, is_node, name  # noqa: E402

UI = "2025_asset_groups/ui_package/"
HUB, HQ = UI + "flow_career_hub.erp", UI + "flow_team_hq.erp"
RND, SETUP = UI + "flow_r_and_d.erp", UI + "flow_career_season_setup.erp"
RND_DEPTS = UI + "screens/laptop_r_and_d_departments_container.erp"
DRIVER_UPGRADES, FIT_UPGRADES = UI + "screens/laptop_driver_upgrades.erp", UI + "screens/laptop_fit_upgrades.erp"
TS_SETUP, TS_HQ = UI + "screens/driver_career_team_select.erp", UI + "screens/team_hq_driver_career_team_select.erp"
TEAM_SELECTS = (SETUP, HQ, TS_SETUP, TS_HQ)
ART = "eaid://ui_package/art/"
SCREENS = "eaid://ui_package/screens/"
OUT = ROOT / "ui"
OPS = ROOT / "erp/ui26.json"
WHEN = {"cars": 26}
ROWS_STOCK = 11

# scene: (archives, [(source element, new element, y step)], (stacker, row height, gap) or None)
SCENES = {
    "components/career/laptop/sl_constructor_stack.idf": ((HUB, HQ), [("11", "12", -0.4)], None),
    "components/career/laptop/r_and_d_performance_compare_stack_attach.idf": (
        (HUB, HQ), [("pill_10", "pill_11", -0.45), ("pill_11", "pill_12", -0.45)], ("stacker", 0.4, 0.02)),
    "components/career/rnd_tree/rnd_perf_stack.idf": (
        (HUB, HQ, RND, RND_DEPTS), [("pill_10", "pill_12", -0.34)], ("stacker", 0.24, 0.05)),
    "components/team_hq/research/perf_stack.idf": (
        (HUB, HQ, RND, DRIVER_UPGRADES, FIT_UPGRADES), [("pill_10", "pill_12", -0.34)], ("stacker", 0.24, 0.05)),
    "components/team_select/dc_ts_perf_panel.idf": (
        TEAM_SELECTS, [("pill_10", "pill_11", -0.56), ("pill_11", "pill_12", -0.56)], ("stacker", 0.4, 0.066)),
}
CAROUSEL = ART + "components/tiles/tiles_general/1x1_sqr_carousel_team.idf"
CAROUSEL13 = ART + "components/tiles/tiles_general/1x1_sqr_carousel_team13.idf"
TEAM_SELECT = ART + "screens/team_select/dc_team_select.idf"
# (stacker, tile width, stock gap, gap)
CAROUSEL_FIT = ("item_spawner", 1.25, 0.205, 0.12)
# screen: (archives, [(list node path, source entry, new entry, scene element, view height or None)])
LISTS = {
    "laptop_standings.ui": ((HUB, HQ), [
        ("tab_content_championship_standings.constructor_championship.constructor_list", "11", "12", "12", None),
    ]),
    "laptop_overview.ui": ((HUB, HQ), [
        ("tab_content_emails_panel.reading_pane.attachments.performance_comparison.performance_list",
         "entry_11", "entry_12", "pill_12", 13),
    ]),
    "laptop_fit_upgrades.ui": ((RND, DRIVER_UPGRADES, FIT_UPGRADES), [
        ("laptop_fit_upgrades.teams_performance.performance_chart", "entry_11", "entry_12", "pill_12", 13),
    ]),
    "driver_career_team_select.ui": ((SETUP, TS_SETUP), [
        ("driver_career_team_select.content.performance_chart", "entry_10", "entry_11", "pill_11", 13),
        ("driver_career_team_select.content.performance_chart", "entry_11", "entry_12", "pill_12", 13),
    ]),
    "team_hq_driver_career_team_select.ui": ((HQ, TS_HQ), [
        ("team_hq_driver_career_team_select.content.performance_chart", "entry_10", "entry_11", "pill_11", 13),
        ("team_hq_driver_career_team_select.content.performance_chart", "entry_11", "entry_12", "pill_12", 13),
    ]),
}


class Stock:
    def __init__(self, game, baseline):
        self.game, self.files, self.objects = game, {}, None
        if baseline:
            self.files = json.loads((baseline / "baseline.json").read_text())["files"]
            self.objects = baseline.parent / "objects"

    def path(self, rel):
        entry = self.files.get(rel)
        if entry:
            obj = self.objects / entry["sha256"][:2] / entry["sha256"]
            if obj.exists():
                return obj
        if not self.game:
            sys.exit(f"{rel}: not in the baseline and no --game")
        p = self.game / rel
        if entry and hashlib.sha256(p.read_bytes()).hexdigest() != entry["sha256"]:
            sys.exit(f"{rel}: changed in the game folder and no stock copy in the baseline")
        return p

    def fragment(self, rel, resource):
        path = self.path(rel)
        meta, res = erp_parse(str(path))
        frags = {r[0]: r for r in res}[resource][3]
        _, off, _, _, comp, packed = frags[0]
        with open(path, "rb") as f:
            f.seek(meta["data_off"] + off)
            raw = f.read(packed)
        if comp in (0x11, 0x91):
            raw = subprocess.run(["zstd", "-dc"], input=raw, capture_output=True, check=True).stdout
        return raw


def element_key(doc, element):
    for si, sc in enumerate(doc.scenes):
        def walk(obj, key):
            for i, c in enumerate(S.child_objs(obj)):
                k = f"{key}.{i}"
                if I._val(c.get("name")) == element:
                    return k, c
                found = walk(c, k)
                if found:
                    return found
            return None
        found = walk(sc.entry.get("scene"), str(si))
        if found:
            return found
    raise KeyError(element)


def param(obj, name):
    for it in obj.get("params").items:
        if isinstance(it, Obj) and I._val(it.get("name")) == name:
            return I._val(it.get("value"))
    raise KeyError(name)


def row_index(obj):
    return int(I._val(obj.get("name")).rsplit("_", 1)[-1])


def fit(doc, stacker, height, gap):
    key, obj = element_key(doc, stacker)
    stock_gap = param(obj, "gap")
    items = obj.get("children").items
    items.sort(key=lambda o: row_index(o) if isinstance(o, Obj) else -1)
    rows = len(S.child_objs(obj))
    stock = ROWS_STOCK * height + (ROWS_STOCK - 1) * stock_gap
    scale = round(min(1.0, stock / (rows * height + (rows - 1) * gap)), 6)
    ops = [{"op": "param", "key": key, "name": "gap", "value": gap}]
    if scale < 1:
        ops.append({"op": "set", "key": key, "field": "scale", "value": [1.0, scale, 1.0]})
    for i, row in enumerate(S.child_objs(obj)):
        pos = row.get("position").value
        ops.append({"op": "set", "key": f"{key}.{i}", "field": "position",
                    "value": [pos[0], round(-i * (height + gap), 6), pos[2]]})
    S.apply_ops(doc, ops)
    return f"gap {stock_gap} -> {gap}" + (f", scale y {scale}" if scale < 1 else "")


def checked(out, component_ids):
    checks = S.verify_bytes(out, component_ids=component_ids)
    if not checks["ok"]:
        sys.exit(f"scene check failed: {checks}")
    return out


def grow_scene(data, clones, spec):
    doc = I.Idf(data)
    for src, new, step in clones:
        key, obj = element_key(doc, src)
        pos = list(obj.get("position").value)
        S.apply_ops(doc, [{"op": "clone", "key": key, "name": new, "position": [pos[0], round(pos[1] + step, 6), pos[2]]}])
    note = fit(doc, *spec) if spec else ""
    return checked(doc.to_bytes(), [sc.name for sc in I.Idf(data).scenes]), note


def carousel13(data):
    doc = I.Idf(data)
    doc.scenes[0].entry.set("name", Val("ui64", I.component_id(CAROUSEL13)))
    stacker, width, stock_gap, gap = CAROUSEL_FIT
    key, obj = element_key(doc, stacker)
    if param(obj, "gap") != stock_gap:
        sys.exit(f"{CAROUSEL}: {stacker} gap is not {stock_gap}")
    s = round((ROWS_STOCK * width + (ROWS_STOCK - 1) * stock_gap) / (13 * width + 12 * gap), 6)
    S.apply_ops(doc, [{"op": "param", "key": key, "name": "gap", "value": gap},
                      {"op": "set", "key": key, "field": "scale", "value": [s, s, 1.0]}])
    return checked(doc.to_bytes(), [I.component_id(CAROUSEL13)]), f"{stacker} gap {stock_gap} -> {gap}, scale {s}"


def team_select(data):
    doc = I.Idf(data)
    key, _ = element_key(doc, "carousel")
    S.apply_ops(doc, [{"op": "retarget", "key": key, "fileRef": CAROUSEL13}])
    return checked(doc.to_bytes(), [sc.name for sc in I.Idf(data).scenes])


def screen_roots(o):
    if is_node(o):
        yield o
        return
    for x in (v for _, v in o.items) if isinstance(o, Obj) else (o.items if isinstance(o, Arr) else []):
        yield from screen_roots(x)


def find(pkg, path):
    first, *rest = path.split(".")
    for r in screen_roots(pkg.root):
        if name(r) != first:
            continue
        n = r
        for part in rest:
            n = next(c for c in children(n) if is_node(c) and name(c) == part)
        return n
    raise KeyError(path)


def bind(node, element):
    for grp in node.items[LAYOUT].items:
        for b in grp.items if isinstance(grp, Arr) else []:
            if isinstance(b, Obj) and b.get("debugName") is not None:
                b["debugName"].value = element
                return
    raise KeyError(f"{name(node)} has no scene binding")


def grow_screen(data, lists):
    pkg = Pkg(data)
    for path, src, new, element, view in lists:
        lst = find(pkg, path)
        kids = lst.items[CHILDREN].items
        at = next(i for i, c in enumerate(kids) if is_node(c) and name(c) == src)
        if any(is_node(c) and name(c) == new for c in kids):
            sys.exit(f"{path}.{new} already exists")
        clone = clone_node(kids[at], path, new)
        bind(clone, element)
        fix_ids(clone, path)
        kids.insert(at + 1, clone)
        copy_signals(pkg.root, f"{path}.{src}", f"{path}.{new}")
        if view is not None:
            vh = lst.items[PROPS].get("viewHeight")
            vh["value"].value = view
    out = pkg.to_bytes()
    if Pkg(out).to_bytes() != out:
        sys.exit("edited screen does not round-trip")
    return out


def tree(pkg):
    out = {}

    def walk(n, path):
        p = f"{path}.{name(n)}" if path else name(n)
        out[p] = n
        for c in children(n):
            if is_node(c):
                walk(c, p)
    for r in screen_roots(pkg.root):
        walk(r, "")
    return out


def check_screen(stock, edited, lists):
    a, b = tree(Pkg(stock)), tree(Pkg(edited))
    added = sorted(set(b) - set(a))
    expect = {f"{p}.{new}" for p, _, new, _, _ in lists}
    if set(a) - set(b) or not all(any(x == e or x.startswith(e + ".") for e in expect) for x in added):
        sys.exit(f"unexpected node changes: removed {sorted(set(a) - set(b))[:5]}, added {added[:5]}")
    changed = [p for p in a if _node_bytes(a[p]) != _node_bytes(b[p])]
    lists_paths = {p for p, *_ in lists}
    if any(p not in lists_paths for p in changed):
        sys.exit(f"stock nodes changed: {[p for p in changed if p not in lists_paths][:5]}")
    return len(added), changed


def _node_bytes(n):
    props = n.items[PROPS]
    own = [(k.value, repr(v.value) if isinstance(v, Val) else _flat(v)) for k, v in props.items]
    return repr(own) + repr([_flat(n.items[i]) for i in range(len(n.items)) if i not in (PROPS, CHILDREN)])


def _flat(o):
    if isinstance(o, Val):
        return repr(o.value)
    if isinstance(o, Obj):
        return "{" + ",".join(f"{k.value}:{_flat(v)}" for k, v in o.items) + "}"
    if isinstance(o, Arr):
        return "[" + ",".join(_flat(x) for x in o.items) + "]"
    return "?"


def same_stock(stock, archives, resource):
    data = stock.fragment(archives[0], resource)
    for rel in archives[1:]:
        if stock.fragment(rel, resource) != data:
            sys.exit(f"{resource}: the stock copies in {archives} differ")
    return data


def replace(archives, resource, source):
    return [{"op": "replace-resource", "when": WHEN, "erp": rel, "resource": resource, "source": source}
            for rel in archives]


def build(stock):
    OUT.mkdir(exist_ok=True)
    ops, lines = [], []
    for res, (archives, clones, spec) in SCENES.items():
        data = same_stock(stock, archives, ART + res)
        out, note = grow_scene(data, clones, spec)
        name = res.rsplit("/", 1)[-1]
        (OUT / name).write_bytes(out)
        ops += replace(archives, ART + res, f"ui/{name}")
        lines.append(f"ui/{name}: {len(data)} -> {len(out)} bytes, +{', '.join(n for _, n, _ in clones)}"
                     + (f"; {note}" if note else ""))
    data = stock.fragment(SETUP, CAROUSEL)
    out, note = carousel13(data)
    name = CAROUSEL13.rsplit("/", 1)[-1]
    (OUT / name).write_bytes(out)
    ops += [{"op": "add-resource", "when": WHEN, "erp": rel, "resource": CAROUSEL13, "type": "UILayout",
             "sources": [f"ui/{name}"]} for rel in TEAM_SELECTS]
    lines.append(f"ui/{name}: copy of {CAROUSEL.rsplit('/', 1)[-1]}, {note}")
    data = same_stock(stock, TEAM_SELECTS, TEAM_SELECT)
    out = team_select(data)
    name = TEAM_SELECT.rsplit("/", 1)[-1]
    (OUT / name).write_bytes(out)
    ops += replace(TEAM_SELECTS, TEAM_SELECT, f"ui/{name}")
    lines.append(f"ui/{name}: carousel -> {CAROUSEL13.rsplit('/', 1)[-1]}")
    for res, (archives, lists) in LISTS.items():
        data = same_stock(stock, archives, SCREENS + res)
        out = grow_screen(data, lists)
        added, changed = check_screen(data, out, lists)
        (OUT / res).write_bytes(out)
        ops += replace(archives, SCREENS + res, f"ui/{res}")
        lines.append(f"ui/{res}: {len(data)} -> {len(out)} bytes, {added} node(s) added, changed {', '.join(changed)}")
    OPS.write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    lines.append(f"erp/ui26.json: {len(ops)} ops")
    return lines


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", type=Path)
    ap.add_argument("--baseline", type=Path)
    a = ap.parse_args(argv[1:])
    if not a.game and not a.baseline:
        sys.exit(__doc__)
    for line in build(Stock(a.game, a.baseline)):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
