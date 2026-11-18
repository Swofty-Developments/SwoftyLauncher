"""Every core runtime::Sig on the KB build and on Steam 1.0.145 (re/tools/sigverify.py over native/src/core).

It reads both executables and writes nothing. The KB build is $SWOFTY_STEAM_EXE, else work/exe/F1_25.exe of this
checkout or of the repository a worktree belongs to; Steam 1.0.145 is $SWOFTY_STEAM_1_0_145_EXE, else sigverify.py's
Steam path. Each must carry its PE timestamp from native/src/core/runtime/build_proof.hpp. The test skips when either
is missing or another build, or when re/tools/py finds no Python with pefile, numpy and capstone.
"""

import os
import re
import struct
import subprocess
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SIGVERIFY = os.path.join(ROOT, 're', 'tools', 'sigverify.py')
PY = os.path.join(ROOT, 're', 'tools', 'py')
BUILD_PROOF = os.path.join(ROOT, 'native', 'src', 'core', 'runtime', 'build_proof.hpp')


def known_timestamps():
    text = open(BUILD_PROOF).read()
    return {b: int(t) for b, t in re.findall(r'\{Build::(\w+), "[^"]*", (\d+)u', text)}


def pe_timestamp(path):
    with open(path, 'rb') as f:
        head = f.read(0x400)
    if head[:2] != b'MZ':
        return None
    pe = struct.unpack_from('<I', head, 0x3c)[0]
    if pe + 12 > len(head) or head[pe:pe + 4] != b'PE\0\0':
        return None
    return struct.unpack_from('<I', head, pe + 8)[0]


def kb_candidates():
    if os.environ.get('SWOFTY_STEAM_EXE'):
        return [os.environ['SWOFTY_STEAM_EXE']]
    out = [os.path.join(ROOT, 'work', 'exe', 'F1_25.exe')]
    try:
        common = subprocess.run(['git', '-C', ROOT, 'rev-parse', '--path-format=absolute', '--git-common-dir'],
                                capture_output=True, text=True, timeout=30).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        common = ''
    if common:
        out.append(os.path.join(os.path.dirname(common), 'work', 'exe', 'F1_25.exe'))
    return out


def port_candidate():
    if os.environ.get('SWOFTY_STEAM_1_0_145_EXE'):
        return os.environ['SWOFTY_STEAM_1_0_145_EXE']
    m = re.search(r"^STEAM = '([^']*)'", open(SIGVERIFY).read(), re.M)
    return m.group(1) if m else ''


class Signatures(unittest.TestCase):
    def test_every_core_signature_matches_once_at_the_same_code(self):
        stamps = known_timestamps()
        kb = next((p for p in kb_candidates() if os.path.isfile(p)), None)
        if not kb:
            self.skipTest('no KB build F1_25.exe (set SWOFTY_STEAM_EXE)')
        if pe_timestamp(kb) != stamps.get('kb'):
            self.skipTest(f'{kb} is not the KB build')
        port = port_candidate()
        if not port or not os.path.isfile(port):
            self.skipTest('no Steam 1.0.145 F1_25.exe (set SWOFTY_STEAM_1_0_145_EXE)')
        if pe_timestamp(port) != stamps.get('steam_1_0_145'):
            self.skipTest(f'{port} is not Steam 1.0.145')
        probe = subprocess.run([PY, '-c', 'import pefile, numpy, capstone'], capture_output=True, text=True)
        if probe.returncode:
            self.skipTest('re/tools/py has no Python with pefile, numpy and capstone')
        r = subprocess.run([PY, '-I', SIGVERIFY, '--ref', kb, '--port', port, '--build', 'steam-26566308'],
                           capture_output=True, text=True, cwd=ROOT)
        lines = r.stdout.splitlines()
        failing = [line for line in lines if line.startswith('FAIL')]
        summary = lines[-1] if lines else r.stderr.strip()
        print(summary)
        self.assertEqual(r.returncode, 0, '\n'.join(failing) or r.stderr)
        self.assertFalse(failing, '\n'.join(failing))


if __name__ == '__main__':
    unittest.main()
