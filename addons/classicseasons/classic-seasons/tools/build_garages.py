#!/usr/bin/env python3
"""Season garage art (data/garages.json): the garage and pit lane textures of a source archive, extracted as DDS into
the season addon's garage/<name>/ (not committed), where its registry/garage.json team.garage record names them.

A source is an archive (`archive`, read from --downloads; a .rar is read with unrar) and the member in it that holds
the textures (`member`). tools/garage_textures.py writes every texture under the stock garage root with the stock
resource stem as its path, and checks each one against the stock surface layout.

    python3 tools/build_garages.py [--downloads ~/Downloads]
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
TOOLS = ROOT.parents[2] / "tools"
sys.path.insert(0, str(HERE))

from layout import year_dir

DATA = json.loads((ROOT / "data" / "garages.json").read_text())


def member(archive, name, out):
    if archive.suffix.lower() == ".rar":
        with open(out, "wb") as f:
            subprocess.run(["unrar", "p", "-inul", str(archive), name], stdout=f, check=True)
    else:
        import zipfile
        with zipfile.ZipFile(archive) as z, open(out, "wb") as f:
            f.write(z.read(name))
    if out.stat().st_size == 0:
        raise SystemExit(f"{archive}: no {name}")


def build(downloads):
    for g in DATA["garages"]:
        source = DATA["sources"][g["source"]]
        archive = Path(downloads) / source["archive"]
        if not archive.exists():
            raise SystemExit(f"{g['name']}: {archive} is missing")
        out = year_dir(g["season"]) / "garage" / g["name"]
        if out.exists():
            shutil.rmtree(out)
        out.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as tmp:
            erp = Path(tmp) / Path(source["member"]).name
            member(archive, source["member"], erp)
            subprocess.run([sys.executable, str(TOOLS / "garage_textures.py"), str(erp), str(out), "--check"],
                           check=True)
        files = sorted(p for p in out.rglob("*") if p.is_file())
        print(f"{g['season']} {g['entry']}: {len(files)} textures, {sum(p.stat().st_size for p in files) >> 20} MB "
              f"in {out.relative_to(ROOT.parent)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--downloads", default=str(Path.home() / "Downloads"))
    build(ap.parse_args().downloads)


if __name__ == "__main__":
    main()
