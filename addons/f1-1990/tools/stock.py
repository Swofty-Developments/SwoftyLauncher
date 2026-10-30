import hashlib
import json
import subprocess
import sys
from functools import lru_cache
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools"))
from erp import parse as erp_parse  # noqa: E402


class Stock:
    def __init__(self, game, baseline=None):
        self.game = Path(game)
        self.files, self.objects = {}, None
        if baseline:
            self.files = json.loads((Path(baseline) / "baseline.json").read_text())["files"]
            self.objects = Path(baseline).parent / "objects"

    @lru_cache(None)
    def path(self, rel):
        p = self.game / rel
        entry = self.files.get(rel)
        if entry and hashlib.sha256(p.read_bytes()).hexdigest() != entry["sha256"]:
            obj = self.objects / entry["sha256"][:2] / entry["sha256"]
            if not obj.exists():
                sys.exit(f"{rel}: changed in the game folder and no stock copy in the baseline")
            return obj
        return p


def fragments(path, name):
    meta, res = erp_parse(str(path))
    r = {x[0]: x for x in res}[name]
    out = []
    with open(path, "rb") as f:
        for fn, off, size, flags, comp, packed in r[3]:
            f.seek(meta["data_off"] + off)
            raw = f.read(packed)
            if comp == 0x11:
                raw = subprocess.run(["zstd", "-dc"], input=raw, capture_output=True, check=True).stdout
            out.append(raw)
    return out
