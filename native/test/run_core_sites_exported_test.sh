#!/usr/bin/env bash
# Host-only. Every "site_*" name in native/src/core that the core resolves by name must be exported in
# native/include/swofty/game/*.hpp, or sit in native/test/core_sites_unexported.txt with a reason. A name
# written as Sig{"site_X", "<byte pattern>", ...} is found by signature scan and needs no export.
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 - <<'PY'
import glob, re, sys

allow = {}
for line in open('native/test/core_sites_unexported.txt'):
    if line.strip() and not line.startswith('#'):
        name, _, why = line.rstrip('\n').partition('\t')
        if not why.strip():
            sys.exit(f'core_sites_unexported.txt: {name} has no reason')
        allow[name] = why

exported = set()
for f in glob.glob('native/include/swofty/game/*.hpp'):
    exported |= set(re.findall(r'"(site_\w+)"', open(f).read()))

named, scanned = {}, set()
for f in glob.glob('native/src/core/**/*.[ch]pp', recursive=True):
    text = open(f).read()
    for m in re.finditer(r'"(site_\w+)"', text):
        named.setdefault(m.group(1), set()).add(f)
    scanned |= set(re.findall(r'"(site_\w+)"\s*,\s*"(?:[0-9A-Fa-f?]{2}\s*)+', text))

bad = [f'{n} ({", ".join(sorted(fs))}): not exported and resolved by name' for n, fs in sorted(named.items())
       if n not in exported and n not in scanned and n not in allow]
bad += [f'{n}: in the allowlist but {"exported now" if n in exported else "no longer referenced" if n not in named else "found by signature scan"}; remove it'
        for n in sorted(allow) if n in exported or n not in named or n in scanned]
if bad:
    print('\n'.join(bad))
    sys.exit(1)
print(f'core sites: {len(named)} names, {len(allow)} allowlisted, all others exported or found by signature scan')
PY
