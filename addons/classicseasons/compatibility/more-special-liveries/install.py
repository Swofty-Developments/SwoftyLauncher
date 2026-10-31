"""Prepare a NEW copy of More Special Liveries with the classic compatibility helper.

Build this folder with cargo build --release, then pass the original addon folder
and a new --output directory. Existing installations are never changed.
"""
import argparse
import hashlib
import json
import shutil
from pathlib import Path


def prepare(source, output, binary):
    source, output, binary = source.resolve(), output.resolve(), binary.resolve()
    if output.exists() or output.is_relative_to(source) or source.is_relative_to(output):
        raise ValueError("output must be a new directory outside the source addon")
    manifest = json.loads((source / "swofty.json").read_text())
    if manifest["id"] != "mildtdesign.more-special-liveries":
        raise ValueError("source must be the original More Special Liveries addon")
    for path in (source / "bin/more-special-liveries-helper.exe", source / "native/ai_livery.dll", binary):
        if not path.is_file():
            raise FileNotFoundError(path)
    for rel in ("classic-liveries.json", "registry/classic_unlocks.json"):
        if (source / rel).exists():
            raise ValueError(f"{rel} is already present; use an original addon copy")
    classic = Path(__file__).resolve().parents[2] / "classic-seasons/data"
    seasons = json.loads((classic / "seasons.json").read_text())
    catalog = json.loads((classic / "special_liveries.json").read_text())
    entries = []
    for year, liveries in catalog.items():
        teams = {t["folder"]: t["name"] for s in seasons["seasons"] if s["year"] == int(year) for t in s["teams"]}
        identity = f"swofty.classic-seasons-{year}"
        entries.extend({**entry, "addon": identity, "key": identity + "/" + entry["id"],
                        "year": int(year), "team_name": teams[entry["team"]]} for entry in liveries)
    name = "classic-liveries-compat-" + hashlib.sha256(binary.read_bytes()).hexdigest()[:12] + ".exe"
    shutil.copytree(source, output)
    shutil.copy2(binary, output / "bin" / name)
    (output / "classic-liveries.json").write_text(json.dumps(entries, indent=1) + "\n")
    (output / "registry").mkdir(exist_ok=True)
    (output / "registry/classic_unlocks.json").write_text('{"livery":{}}\n')
    manifest["launcher"]["program"]["windows"] = "bin/" + name
    (output / "swofty.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return len(entries)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parent / "target/release/classic-liveries-compat.exe")
    args = parser.parse_args()
    print(f"Prepared {prepare(args.source, args.output, args.binary)} classic liveries in {args.output}")
