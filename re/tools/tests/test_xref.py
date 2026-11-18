import importlib.util, os, subprocess, sys, unittest

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HAVE = all(importlib.util.find_spec(m) for m in ('numpy', 'pefile'))


def run(exe, needle):
    with open(exe, 'rb') as f:
        off = f.read().find(needle)
    out = subprocess.run([sys.executable, os.path.join(TOOLS, 'xref.py'), f'x={off:x}'], env={**os.environ, 'EXE': exe},
                         capture_output=True, text=True, check=True).stdout
    return out


@unittest.skipUnless(HAVE, 'xref.py needs numpy and pefile (re/tools/py has them)')
class ReadersOutsideTheMainCodeSection(unittest.TestCase):
    """Game.SkipTalkingPoints is read only from the protector's relocated code, which a scan of the main code section
    alone misses."""

    @unittest.skipUnless(os.environ.get('SWOFTY_STEAM_EXE'), 'set SWOFTY_STEAM_EXE to the KB build F1_25.exe')
    def test_the_kb_build_reader_in_data1(self):
        out = run(os.environ['SWOFTY_STEAM_EXE'], b'Game.SkipTalkingPoints\0')
        self.assertIn("refs=1 sections=['.data1'] sites=['0x14eb3ac1e'] funcs=['0x14eb3abf0']", out)

    @unittest.skipUnless(os.environ.get('SWOFTY_STEAM_145_EXE'), 'set SWOFTY_STEAM_145_EXE to the Steam 1.0.145 exe')
    def test_the_current_build_reader_in_trace(self):
        out = run(os.environ['SWOFTY_STEAM_145_EXE'], b'Game.SkipTalkingPoints\0')
        self.assertIn("sections=['.trace']", out)
        self.assertIn('0x14e6cc9de', out)


if __name__ == '__main__':
    unittest.main()
