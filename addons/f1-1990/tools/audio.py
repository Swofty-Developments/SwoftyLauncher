"""1990 engine audio shared by build_audio.py and the car pack's build_pack.py.

A car's engine sound is picked by the `switchName` of the AudioF1Car component in its `<stem>-carmodel.layer`
(stock 2026: `f1_m_<audio name>_26`). The AudioF1Car setup 0x1409f4fa0 uses the label twice:
- its Wwise ID (0x1449087e0: FNV-1 32 with A-Z lowered) is the engine switch state it sets on the car's emitters,
  so a label the soundbanks don't hold leaves the engine silent;
- it reads `audio.audio_car_data.<switchName>.*` from the game database by FNV-1a 64 of the exact bytes, case kept
  (a table keyed by that label with no row index or count), and loads the row's `bundle_name` from
  `audio/2025/bundles/`, which must be listed in `audio.audio_bundle_settings` (registry 0x1409f1b50).

The 1990 cars get their donor's label in other letter case (`F1_M_<name>_26`; `F1_M_<NAME>_26` for a car built on a
team donor for another folder, so Larrousse and Dallara differ from Williams and Arrows): the donor's Wwise switch
state and the car's own database rows. The length is the donor's, so the car pack rewrites the layer byte for byte.
"""
import re

PREFIX = 'f1_m_'
SUFFIX = '_26'
BUNDLES = {'V12': 'v12_1990', 'V10': 'v10_1990', 'V8': 'v8_1990', 'V8 Judd': 'v8j_1990'}
BUNDLE_TEMPLATE = 'ferrari_2026'
SWITCH = re.compile(rb'\nswitchName\x00(.)', re.DOTALL)


def carmodel_layer(package, stem):
    return f'eaid://{package}/teams/{stem}/wep/{stem}-carmodel.layer'


def switch_name(data):
    hits = list(SWITCH.finditer(data))
    if len(hits) != 1:
        raise ValueError(f'{len(hits)} switchName entries in the carmodel layer')
    m = hits[0]
    return data[m.end():m.end() + data[m.end() - 1]].decode()


def wwise_id(label):
    h = 0x811c9dc5
    for c in label.encode():
        if 0x41 <= c <= 0x5a:
            c += 0x20
        h = ((h * 0x1000193) & 0xffffffff) ^ c
    return h


def label_1990(car, donor_label):
    if not (donor_label.startswith(PREFIX) and donor_label.endswith(SUFFIX)):
        raise ValueError(f'{car.team}: unexpected audio label {donor_label}')
    name = donor_label[len(PREFIX):-len(SUFFIX)]
    if car.stem != car.donor:
        if name != car.donor:
            raise ValueError(f'{car.team}: audio name {name} is not the donor {car.donor}')
        name = name.upper()
    new = f'{PREFIX.upper()}{name}{SUFFIX}'
    if len(new) != len(donor_label) or new == donor_label or wwise_id(new) != wwise_id(donor_label):
        raise ValueError(f'{car.team}: {new} does not keep the Wwise switch of {donor_label}')
    return new


def rewrite_switch(data, old, new):
    a = b'\nswitchName\x00' + bytes([len(old)]) + old.encode() + b'\x00'
    b = b'\nswitchName\x00' + bytes([len(new)]) + new.encode() + b'\x00'
    if data.count(a) != 1 or len(a) != len(b):
        raise ValueError(f'switchName {old} not found once')
    return data.replace(a, b)


def sound(car):
    return 'V8 Judd' if car.engine.startswith('Judd') else car.family


def bundle_file(car):
    return f'{BUNDLES[sound(car)]}.bdl'
