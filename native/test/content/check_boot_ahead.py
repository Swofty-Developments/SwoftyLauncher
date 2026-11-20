#!/usr/bin/env python3
"""The boot expansion made ahead on a worker (boot_ahead.hpp) against the boot's own path, on the game database:
rows_tool --ahead runs both. Classic Seasons, Classic Seasons over older addons' edits and a box world box are
adopted; a room that leaves an addon out and each --ahead-change are refused with the guard's reason. Either way the
database, the stat ids, the stat-id lines and the pool lines are the boot's own.

Usage: check_boot_ahead.py ROWS_TOOL GAMEDB OUT CS_ARGS... -- BOX_ARGS..."""

import filecmp
import json
import subprocess
import sys
from pathlib import Path

SAME = ["staged_records", "id_lines", "boot_lines", "ids", "lines", "records", "refused", "room_refused", "boot_fresh"]
GUARDS = {
    "registry": "the registry changed after the expansion ahead started",
    "ids": "the stat-id store changed after the expansion ahead started",
    "box-worlds": "SWOFTY_BOX_WORLDS is off at the boot and was on ahead",
    "day": "the date changed after the expansion ahead started",
    "edits": "the older addons' database edits changed after the expansion ahead started",
}
OLDER = ["--edits", "content/box_world/swofty_f1_edits.json"]


def main():
    tool, gamedb, out = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    rest = sys.argv[4:]
    cut = rest.index("--")
    cs, box = rest[:cut], rest[cut + 1:]

    def run(name, *args):
        edits, summary = out / f"ahead_{name}_edits.json", out / f"ahead_{name}_summary.json"
        subprocess.run([tool, "--registries", "../../packages/sdk/std/registries", "--db", gamedb, "--plan", "worker",
                        "--out", str(edits), "--summary", str(summary), *args],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        return edits, json.loads(summary.read_text())

    def compare(what, today, ahead, expect):
        (te, ts), (ae, sa) = today, ahead
        assert filecmp.cmp(te, ae, shallow=False), f"{what}: the database differs from the boot's own"
        differ = [k for k in SAME if ts.get(k) != sa.get(k)]
        assert not differ, f"{what}: {', '.join(differ)} differ from the boot's own"
        got = sa["ahead"]
        assert (got == "adopted") if expect == "adopted" else (expect in got), f"{what}: ahead {got!r}, wanted {expect!r}"
        print(f"boot ahead, {what}: {got}; the same {len(ts['staged_records'])} staged record(s), "
              f"{len(ts['id_lines'])} stat-id line(s) and stat ids as the boot's own path")

    cs_today = run("cs_today", *cs)
    compare("Classic Seasons", cs_today, run("cs", "--ahead", *cs), "adopted")
    compare("Classic Seasons over older addons' edits", run("older_today", *OLDER, *cs),
            run("older", "--ahead", *OLDER, *cs), "adopted")
    compare("a box world box", run("box_today", "--box-worlds", *OLDER, *box),
            run("box", "--ahead", "--box-worlds", *OLDER, *box), "adopted")
    room = str(sum(cs_today[1]["addon_fresh"].values()) - 1)
    room_today = run("room_today", "--room", room, *cs)
    assert room_today[1]["room_refused"], "the room run leaves no addon out"
    compare("a room that leaves an addon out", room_today, run("room", "--ahead", "--room", room, *cs), "out at the boot")
    for guard, reason in GUARDS.items():
        compare(f"--ahead-change {guard}", cs_today, run(f"guard_{guard}", "--ahead-change", guard, *cs), reason)


if __name__ == "__main__":
    main()
