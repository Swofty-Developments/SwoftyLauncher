#!/usr/bin/env python3
"""Writes data/pit-phases.json: the phase frames of every stock F1 pit stop crew sequence.

usage: pit_phases.py [pitstop_26.erp]

Per sequence (key: the NeSequence name, lower case, without its leading "f1"): the PitStopLogic clip's Start,
Stop and JC event frames, the frames of every play_anim_tyre_gun_on audio key and the first play_anim_jack_move or
play_anim_jack_down audio key between Start and JC, and the frame rate. Only sequences with Start, Stop and JC are
written.
"""

import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "tools"))

from erp import parse  # noqa: E402

ERP = "/mnt/gaming/SteamLibrary/steamapps/common/F1 25/2025_asset_groups/animation_package/scenes/pitstops/f1/pitstop_26.erp"
OUT = os.path.join(HERE, "..", "data", "pit-phases.json")
FPS = 30


def fragments(path):
    meta, res = parse(path)
    with open(path, "rb") as f:
        for name, rtype, _, frags, _ in res:
            if rtype != "Sequence" or not frags:
                continue
            _, off, _, _, comp, packed = frags[0]
            f.seek(meta["data_off"] + off)
            raw = f.read(packed)
            if comp in (0x11, 0x91):
                r = subprocess.run(["zstd", "-dc"], input=raw, capture_output=True)
                raw = r.stdout if r.returncode == 0 else raw
            yield name, raw


def phases(data):
    text = " ".join(t.decode("latin1") for t in re.findall(rb"[\x20-\x7e]+", data))
    head = re.search(r"NeSequence name (\S+)", text)
    if not head:
        return None, None
    events, guns, jacks = {}, set(), []
    for clip in text.split("NeSequenceClip name ")[1:]:
        if clip.split(" ")[0] == "PitStopLogic":
            for name, frame in re.findall(r"EventKey name (\w+) time_f (\d+)", clip):
                events.setdefault(name, int(frame))
        for frame, sound in re.findall(r"Key time_f (\d+) play (play_anim_\w+)", clip):
            if sound == "play_anim_tyre_gun_on":
                guns.add(int(frame))
            elif sound in ("play_anim_jack_move", "play_anim_jack_down"):
                jacks.append(int(frame))
    if not all(k in events for k in ("Start", "Stop", "JC")):
        return head.group(1), None
    start, stop, jc = events["Start"], events["Stop"], events["JC"]
    end = max(stop, jc)
    jack = min((j for j in jacks if start < j <= end + 4), default=-1)
    return head.group(1), {
        "start": start,
        "stop": stop,
        "jc": jc,
        "guns": sorted(g for g in guns if start < g < end),
        "jack": jack,
    }


def key(name):
    k = name.lower()
    return k[2:] if k.startswith("f1") else k


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else ERP
    table = {}
    for _, data in fragments(path):
        name, row = phases(data)
        if row:
            table[key(name)] = row
    doc = {"fps": FPS, "sequences": dict(sorted(table.items()))}
    with open(OUT, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"{OUT}: {len(table)} sequences")


if __name__ == "__main__":
    main()
