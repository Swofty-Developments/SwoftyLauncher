"""Build db/audio.json: the 1990 engine sounds, used only by the 1990 cars.

- four `audio.audio_bundle_settings` rows (`v12_1990`, `v10_1990`, `v8_1990`, `v8j_1990`, cloned from
  `ferrari_2026`) for the bundles `operations.files` adds as new files under `audio/2025/bundles/`;
- one `audio.audio_car_data.<label>` row per 1990 car (put-record, every field the audio loader reads), copied from
  the donor car's stock row with `bundle_name` set to the car's engine (V12, V10, V8, and a second V8 for Judd), or
  with option `engine_sounds` 2026 the donor's stock 2026 bundle.

The labels come from audio.py: the donor's `switchName` (read from the stock 2026 carmodel layer) in other letter
case, which keeps the donor's Wwise switch state. The car pack writes the same label into each 1990 car's carmodel
layer. Nothing stock is changed: every key
the ops write must be absent from the stock database. Without --write it builds into build/ and exits 1 when that
differs from the shipped file.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ADDON = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(ADDON))
for _p in (os.path.join(REPO, 'tools', 'swofty-ui'), os.path.join(REPO, 'tools'), HERE):
    if _p not in sys.path:
        sys.path.insert(0, _p)

import audio
import cars as C
import erpw
import gamedb
from build_cars import GAMEDB, game_root

OUT = 'db/audio.json'
OPTION = 'engine_sounds'
REGISTRY = 'audio.audio_bundle_settings'
REGISTRY_ROW = f'{REGISTRY}.audio_bundle_settings'
REGISTRY_FIELDS = ['_metadata_', 'bundle_name', 'era', 'engine', 'exhaust', 'cabin']
CAR_DATA = 'audio.audio_car_data'
FIELDS = [
    '_metadata_', 'bundle_name', 'detonations_category', 'safety_car', 'cabin_radio', 'left_right_panning',
    'frontwing_offset_x', 'frontwing_offset_y', 'frontwing_offset_z',
    'rearwing_offset_x', 'rearwing_offset_y', 'rearwing_offset_z',
    'windscreen_offset_x', 'windscreen_offset_y', 'windscreen_offset_z',
    'engine_offset_x', 'engine_offset_y', 'engine_offset_z', 'engine_rotation_y',
    'exhaust_offset_x', 'exhaust_offset_y', 'exhaust_offset_z', 'exhaust_rotation_y',
    'cabin_offset_x', 'cabin_offset_y', 'cabin_offset_z', 'cabin_rotation_y',
    'gearbox_offset_x', 'gearbox_offset_y', 'gearbox_offset_z', 'gearbox_rotation_y',
    'braking_slowdown', 'engine_braking_scale', 'exhaust_braking_scale',
    'engine_downshift_load_lag', 'exhaust_downshift_load_lag',
    'engine_torque_throttle_mixer', 'exhaust_torque_throttle_mixer',
    'judder_wobble_scale', 'surface_load_wobble_scale',
    'off_load_decell_attenuation_idle', 'off_load_decell_attenuation_redline', 'throttle_blend_power_base',
]


def donor_labels(root):
    out = {}
    for c in C.CARS:
        path = os.path.join(root, '2025_asset_groups', C.SOURCE_PACKAGE, 'teams', c.donor, 'wep', f'{c.donor}.erp')
        e = erpw.Erp(path)
        layer = e.find(audio.carmodel_layer(C.SOURCE_PACKAGE, c.donor))
        out[c.team] = audio.switch_name(e.frag_data(layer, 0))
    return out


def labels(root):
    donors = donor_labels(root)
    out = {c.team: (donors[c.team], audio.label_1990(c, donors[c.team])) for c in C.CARS}
    new = [n for _, n in out.values()]
    if len(set(new)) != len(new):
        raise ValueError(f'1990 audio labels collide: {new}')
    return out


def build(root):
    names = labels(root)
    recs = gamedb.read(open(os.path.join(root, GAMEDB), 'rb').read())
    by = {r.key: r for r in recs}
    ops = []
    for name in audio.BUNDLES.values():
        if any(r.value == name.encode() for r in recs if r.type == 2):
            raise ValueError(f'{name} is already in the stock database')
        ops.append({'op': 'add-row', 'table': REGISTRY,
                    'from': f'{REGISTRY_ROW}[_metadata_={audio.BUNDLE_TEMPLATE}]',
                    'set': {'_metadata_': name, 'bundle_name': name}})
    for c in C.CARS:
        old, new = names[c.team]
        for field in FIELDS:
            src = by.get(gamedb.fnv1a64(f'{CAR_DATA}.{old}.{field}'))
            if src is None:
                raise ValueError(f'{old}: no {field}')
            key = f'{CAR_DATA}.{new}.{field}'
            if gamedb.fnv1a64(key) in by:
                raise ValueError(f'{key} is in the stock database')
            if field == 'bundle_name':
                for choice, value in (('1990', audio.bundle_file(c).encode()), ('2026', src.value)):
                    ops.append({'op': 'put-record', 'key': key, 'type': src.type, 'value': value.hex(),
                                'when': {OPTION: choice}})
                continue
            value = new.encode() if field == '_metadata_' else src.value
            ops.append({'op': 'put-record', 'key': key, 'type': src.type, 'value': value.hex()})
        print(f'{c.team:13} {c.folder:15} {old:22} -> {new:22} {audio.bundle_file(c)}')
    return {'schema_version': 1, 'fields': {REGISTRY_ROW: REGISTRY_FIELDS}, 'ops': ops}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    ap.add_argument('--game', help='the F1 25 folder with the stock database and 2026 team archives')
    ap.add_argument('--write', action='store_true', help='replace the shipped file with the build')
    args = ap.parse_args()
    root = game_root(args.game)
    blob = (json.dumps(build(root), indent=1) + '\n').encode()
    dst = os.path.join(ADDON, OUT) if args.write else os.path.join(ADDON, 'build', OUT)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(dst, 'wb') as f:
        f.write(blob)
    if args.write:
        return 0
    shipped = os.path.join(ADDON, OUT)
    if not os.path.exists(shipped) or open(shipped, 'rb').read() != blob:
        print(f'differs: {OUT}')
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
