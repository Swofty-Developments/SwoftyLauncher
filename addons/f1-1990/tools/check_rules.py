#!/usr/bin/env python3
"""Checks a built database against the stock one: the stock rows the 1990 rules clone from must be unchanged.

usage: check_rules.py <stock gamedatabases.2025.bin> <built gamedatabases.2025.bin>

Protected: every stock row of the setup modes, the game-setup-mode option tags, the compound classes, the visual
compounds, the allocation and mandatory tyre presets, the DRS lap groups, the points presets, the track layers and
the light probes. Allowed changes: the row counts of the tables that gain a 1990 row, the option tag lists (stock
value plus gamesetup_f1_1990) and the light probe list (stock value plus f1_1990). Exit code 1
when a protected key changed or went missing, a 1990 row is missing, or the light probes lack a garage slot of a
13-team grid.
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import career  # noqa: E402
import rules  # noqa: E402

NEW_ROWS = [
    ("game_setup.game_setup_modes", "game_setup_mode", rules.NAME),
    (rules.TAG_FOLDER.replace("[_metadata_=game_setup_mode]", "_2"), "game_setup_mode_tag", rules.TAG_NAME),
    ("tyres.compound_classes", "CompoundClass", rules.NAME),
    *[("tyres.visual_compounds", "VisualCompound", name) for name in rules.VISUAL_STAT_IDS],
    ("tyres.allocation_tyre_presets", "TyresPerSessionPreset", rules.ALLOCATION_NAME),
    ("tyres.mandatory_tyre_presets", "TyresPerSessionPreset", rules.MANDATORY_NAME),
    ("drs.drs_allowed_lap", "drs_allowed_lap_group", rules.NAME),
    ("game_setup.session_standings", "session_standing_preset", rules.STANDINGS_NAME),
    ("track_data.game_setup", "TrackDataSet", rules.NAME),
]
PROTECTED = [(c, w) for c, w, _ in NEW_ROWS if c != "track_data.game_setup"] + [
    ("track_data.track_layers", "TrackLayer"),
    ("tracks.ibl_probes", "IblProbes"),
]


def h(path):
    return career.gamedb.fnv1a64(path.encode())


def under(db, row, rest):
    segs = rest.split(".")
    i = next((k for k, seg in enumerate(segs) if seg.endswith("_N")), None)
    if i is None:
        return [rest] if db.get(f"{row}.{rest}") is not None else []
    pre, word, tail = ".".join(segs[:i]), segs[i][:-2], ".".join(segs[i + 1:])
    parent = f"{row}.{pre}" if pre else row
    n = db.count(parent, word)
    found = []
    k = 0
    while k < n if n is not None else db.get(f"{parent}.{word}_{k}._metadata_") is not None:
        node = f"{pre}.{word}_{k}" if pre else f"{word}_{k}"
        if not tail:
            if db.get(f"{row}.{node}") is not None:
                found.append(node)
        else:
            found += [f"{node}.{sub}" for sub in under(db, f"{row}.{node}", tail)]
        k += 1
    return found


def protected(db):
    out = {}
    for container, word in dict.fromkeys(PROTECTED):
        template = f"{re.sub(r'_[0-9]+', '_N', container)}.{word}_N."
        rests = sorted({k[len(template):] for k in db.cat if k.startswith(template)})
        for i in range(db.count(container, word) or 0):
            row = f"{container}.{word}_{i}"
            label = db.get(f"{row}._metadata_")
            for rest in rests:
                for field in under(db, row, rest):
                    out[h(f"{row}.{field}")] = f"{row}.{field} ({label})"
    return out


def allowed(stock):
    ok = {h(f"{c}._{w}_instancecount_"): None for c, w, _ in NEW_ROWS}
    for key in rules.FIELDS["tag_lists"]:
        ok[h(key)] = f"{stock.get(key)},{rules.TAG_NAME}"
    key = f"{rules.PROBES}.game_setup_modes"
    ok[h(key)] = f"{stock.get(key)},{rules.NAME}"
    return ok


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    stock, built = career.Db(sys.argv[1]), career.Db(sys.argv[2])
    guard, ok = protected(stock), allowed(stock)
    bad, other = [], 0
    for key, (tag, value) in stock.d.items():
        new = built.d.get(key)
        if new == (tag, value):
            continue
        if key in ok and new is not None:
            want = ok[key]
            if want is not None and new[1] != want:
                bad.append(f"{guard.get(key, hex(key))}: {value!r} -> {new[1]!r}, want {want!r}")
            continue
        if key in guard:
            bad.append(f"{guard[key]}: {value!r} -> {new[1] if new else 'MISSING'!r}")
        else:
            other += 1
    for container, word, label in NEW_ROWS:
        try:
            built.row_index(container, word, label)
        except KeyError:
            bad.append(f"missing 1990 row {container}.{word}[_metadata_={label}]")
    for slot in range(rules.GARAGES):
        if built.get(f"{rules.PROBES}.{rules.garage(slot)}") is None:
            bad.append(f"missing light probe set {rules.PROBES}.{rules.garage(slot)}")
    print(f"protected keys: {len(guard)}, other changed stock keys (other lanes): {other}")
    if bad:
        print(f"FAIL: {len(bad)} protected change(s)")
        for line in bad[:50]:
            print("  " + line)
        sys.exit(1)
    print("ok: stock setup modes, option tags, tyre rows, DRS groups, points presets, layers and probes unchanged")


if __name__ == "__main__":
    main()
