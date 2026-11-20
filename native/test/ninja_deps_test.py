import os
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools'))
import ninja_deps as nd  # noqa: E402


def path_record(path, node):
    body = path.encode()
    body += b'\0' * (-len(body) % 4)
    body += struct.pack('<I', ~node & 0xFFFFFFFF)
    return struct.pack('<I', len(body)) + body


def deps_record(out, mtime, deps):
    body = struct.pack('<iII', out, mtime & 0xFFFFFFFF, mtime >> 32) + struct.pack(f'<{len(deps)}i', *deps)
    return struct.pack('<I', len(body) | 0x80000000) + body


def log(*records, version=4):
    return nd.HEADER + struct.pack('<i', version) + b''.join(records)


GOOD = log(path_record('src/a.cpp', 0), path_record('build/obj/a.o', 1), deps_record(1, 1794902189359590085, [0]))
# Two ninjas writing one build directory: each gave its new path the id 2, so the second path's checksum names 2 where
# the log expects 3 (the damage this worktree's log had, at src/core/guards/on_demand.hpp).
TWO_WRITERS = GOOD + path_record('src/b.hpp', 2) + path_record('src/core/guards/on_demand.hpp', 2)


class Damage(unittest.TestCase):
    def test_a_whole_log_reads(self):
        self.assertIsNone(nd.damage(GOOD))
        self.assertIsNone(nd.damage(log()))

    def test_two_writers_give_one_id_twice(self):
        self.assertEqual(nd.damage(TWO_WRITERS),
                         f'the path record for src/core/guards/on_demand.hpp at byte {len(GOOD) + len(path_record('src/b.hpp', 2))} has node id 2, not 3')

    def test_a_cut_record_is_damage(self):
        self.assertIn('runs past the log', nd.damage(GOOD + path_record('src/c.hpp', 2)[:-4]))
        self.assertIn('cut short', nd.damage(GOOD + b'\x08\x00'))

    def test_deps_on_an_undefined_node_are_damage(self):
        self.assertIn('names a node', nd.damage(GOOD + deps_record(1, 5, [7])))

    def test_another_version_is_left_to_ninja(self):
        self.assertIsNone(nd.damage(log(path_record('x', 5), version=3)))

    def test_no_header_is_damage(self):
        self.assertEqual(nd.damage(b'garbage'), 'no ninja deps header')


class Check(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.path = os.path.join(self.dir, '.ninja_deps')

    def tearDown(self):
        shutil.rmtree(self.dir)

    def write(self, data):
        with open(self.path, 'wb') as f:
            f.write(data)

    def test_a_damaged_log_is_removed_with_one_line(self):
        self.write(TWO_WRITERS)
        line = nd.check(self.path, 'build.sh')
        self.assertFalse(os.path.exists(self.path))
        self.assertTrue(line.startswith(f'build.sh: removed {self.path} (the path record for src/core/guards/on_demand'))
        self.assertTrue(line.endswith('; this build recompiles everything once'))

    def test_a_whole_log_stays(self):
        self.write(GOOD)
        self.assertIsNone(nd.check(self.path))
        with open(self.path, 'rb') as f:
            self.assertEqual(f.read(), GOOD)

    def test_no_log_is_fine(self):
        self.assertIsNone(nd.check(self.path))

    @unittest.skipUnless(shutil.which('ninja'), 'no ninja on PATH')
    def test_ninja_keeps_losing_records_after_the_damage_until_the_log_goes(self):
        build = os.path.join(self.dir, 'build')
        os.mkdir(build)
        with open(os.path.join(self.dir, 'build.ninja'), 'w') as f:
            f.write('builddir = build\n'
                    'rule cc\n'
                    "  command = sh -c 'echo \"$out: $in\" > $out.d; cp $in $out'\n"
                    '  depfile = $out.d\n'
                    '  deps = gcc\n'
                    'build out.o: cc in.c\n')
        with open(os.path.join(self.dir, 'in.c'), 'w') as f:
            f.write('x\n')
        deps = os.path.join(build, '.ninja_deps')
        with open(deps, 'wb') as f:
            f.write(TWO_WRITERS)

        def ninja():
            return subprocess.run(['ninja'], cwd=self.dir, capture_output=True, text=True).stdout

        ninja()
        self.assertNotIn('no work to do', ninja())
        self.assertIsNotNone(nd.check(deps))
        ninja()
        self.assertIn('no work to do', ninja())


if __name__ == '__main__':
    unittest.main()
