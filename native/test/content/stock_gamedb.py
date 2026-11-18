#!/usr/bin/env python3
"""Print the path of a stock gamedatabases.2025.bin for the tests that read the game database.

  stock_gamedb.py            the path on stdout and exit 0, or the reason on stderr and exit 2

SWOFTY_TEST_GAMEDB names a fixture file and wins when set. Otherwise the database comes from the launcher's
baseline store (<data dir>/baseline, the data dir found as the launcher finds it: SWOFTY_DATA_DIR, the Flatpak
Steam data folder, XDG_DATA_HOME or ~/.local/share, then swofty): the record of every baseline whose database
Steam's manifest vouched for (source depot), newest build first, whose object is in the store with the recorded
size and SHA-256. The game folder is never read: it holds the launcher's modded database while the game is set up
to play.
"""
import hashlib
import json
import os
import sys
from pathlib import Path

REL = 'game_package/gamedatabases/gamedatabases.2025.bin'


def data_dir():
    if os.environ.get('SWOFTY_DATA_DIR'):
        return Path(os.environ['SWOFTY_DATA_DIR'])
    home = Path(os.environ.get('HOME', '.'))
    flatpak = home / '.var/app/com.valvesoftware.Steam/data'
    if flatpak.is_dir() and not (home / '.local/share/Steam').exists():
        return flatpak / 'swofty'
    return Path(os.environ.get('XDG_DATA_HOME') or home / '.local/share') / 'swofty'


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def build_order(baseline):
    build = str(baseline.get('build_id', ''))
    return (build.isdigit(), int(build) if build.isdigit() else 0, build)


def from_baselines(root):
    store = root / 'baseline'
    if not store.is_dir():
        return None, f'no launcher baseline store at {store}'
    found = []
    for p in sorted(store.glob('*/baseline.json')):
        try:
            b = json.loads(p.read_text())
        except (OSError, ValueError):
            continue
        r = b.get('files', {}).get(REL)
        if r and r.get('source') == 'depot' and len(r.get('sha256', '')) == 64:
            found.append((build_order(b), b.get('build_id'), r))
    if not found:
        return None, f'no baseline in {store} records a Steam-verified {REL}'
    reasons = []
    for _, build, r in sorted(found, key=lambda f: f[0], reverse=True):
        obj = store / 'objects' / r['sha256'][:2] / r['sha256']
        if not obj.is_file():
            reasons.append(f'build {build}: object {obj.name[:12]} not in the store')
        elif obj.stat().st_size != r.get('size'):
            reasons.append(f'build {build}: object {obj.name[:12]} has the wrong size')
        elif sha256(obj) != r['sha256']:
            reasons.append(f'build {build}: object {obj.name[:12]} does not hash to its name')
        else:
            return obj, None
    return None, '; '.join(reasons)


def main():
    fixture = os.environ.get('SWOFTY_TEST_GAMEDB')
    if fixture:
        if Path(fixture).is_file():
            print(fixture)
            return 0
        print(f'SWOFTY_TEST_GAMEDB={fixture} is not a file', file=sys.stderr)
        return 2
    path, why = from_baselines(data_dir())
    if path is None:
        print(f'{why}; set SWOFTY_TEST_GAMEDB to a stock {Path(REL).name}', file=sys.stderr)
        return 2
    print(path)
    return 0


if __name__ == '__main__':
    sys.exit(main())
