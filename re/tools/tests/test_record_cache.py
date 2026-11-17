import json, os, shutil, sys, tempfile, unittest
from unittest import mock

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
import binding_record as br

SAMPLES = ('session/site_Sess_StintTyreCheckPredictCall.json', 'core/Ne_Get_SettingDouble.json')


def sample(rel):
    with open(os.path.join(br.RECORDS, rel), 'rb') as f:
        return f.read()


def strict_parse_forbidden():
    return mock.patch.object(br.Record, 'from_bytes', side_effect=AssertionError('parsed a cached record'))


class RecordCacheTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.dir)
        self.root = os.path.join(self.dir, 'bindings')
        self.cache = os.path.join(self.dir, 'cache', 'records.json')
        for rel in SAMPLES:
            self.write(rel, sample(rel))

    def write(self, rel, data, root=None):
        path = os.path.join(root or self.root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(data)
        return path

    def load(self, root=None):
        return br.Store(root or self.root, cache_path=self.cache)

    def fresh(self, root=None):
        with mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': '0'}):
            return br.Store(root or self.root)

    def entries(self):
        if not os.path.exists(self.cache):
            return {}
        with open(self.cache) as f:
            return json.load(f)['entries']

    def view(self, store):
        return sorted((r.relpath, r.to_json()) for r in store.records), sorted(store.errors)

    def same_as_fresh(self, root=None):
        cached = self.load(root)
        self.assertEqual(self.view(cached), self.view(self.fresh(root)))
        return cached

    def cached_load(self):
        self.load()
        self.assertEqual(sorted(self.entries()), sorted(SAMPLES))

    def same_size_edit(self, rel):
        doc = br.parse_json(sample(rel))
        notes = doc['row']['notes']
        doc['row']['notes'] = ('X' if notes[0] != 'X' else 'Y') + notes[1:]
        data = br.Record(doc, rel).to_json().encode('utf-8')
        self.assertEqual(len(data), len(sample(rel)))
        self.assertNotEqual(data, sample(rel))
        return data, doc['row']['notes']

    def test_a_second_load_is_served_from_the_cache(self):
        self.cached_load()
        with strict_parse_forbidden():
            store = self.load()
        self.assertEqual(len(store.records), len(SAMPLES))
        self.assertEqual(self.view(store), self.view(self.fresh()))

    def test_a_same_size_edit_right_after_a_load_is_parsed_again(self):
        self.cached_load()
        rel = SAMPLES[0]
        data, notes = self.same_size_edit(rel)
        self.write(rel, data)
        store = self.same_as_fresh()
        self.assertEqual(store.by_relpath(rel).doc['row']['notes'], notes)

    def test_a_record_replaced_with_its_old_size_and_mtime_is_parsed_again(self):
        self.cached_load()
        rel = SAMPLES[0]
        path = os.path.join(self.root, rel)
        old = os.lstat(path)
        data, notes = self.same_size_edit(rel)
        tmp = os.path.join(self.dir, 'replacement.json')
        with open(tmp, 'wb') as f:
            f.write(data)
        os.utime(tmp, ns=(old.st_atime_ns, old.st_mtime_ns))
        os.replace(tmp, path)
        new = os.lstat(path)
        self.assertEqual((new.st_size, new.st_mtime_ns), (old.st_size, old.st_mtime_ns))
        store = self.same_as_fresh()
        self.assertEqual(store.by_relpath(rel).doc['row']['notes'], notes)

    def test_an_added_record_is_served(self):
        self.cached_load()
        doc = json.loads(sample(SAMPLES[0]))
        doc['name'] = 'site_Cache_Added'
        self.write('session/site_Cache_Added.json', json.dumps(doc, indent=1).encode())
        store = self.same_as_fresh()
        self.assertIsNotNone(store.by_relpath('session/site_Cache_Added.json'))

    def test_a_deleted_record_is_gone_and_leaves_the_cache(self):
        self.cached_load()
        os.unlink(os.path.join(self.root, SAMPLES[1]))
        store = self.same_as_fresh()
        self.assertIsNone(store.by_relpath(SAMPLES[1]))
        self.assertEqual(sorted(self.entries()), [SAMPLES[0]])

    def test_a_record_that_breaks_reports_its_error_and_one_that_heals_loads(self):
        self.cached_load()
        rel = SAMPLES[0]
        self.write(rel, sample(rel)[:-3])
        broken = self.same_as_fresh()
        self.assertIsNone(broken.by_relpath(rel))
        self.assertTrue(any(e.startswith(f'bindings/{rel}: invalid JSON') for e in broken.errors))
        self.assertTrue(self.entries()[rel][1])
        with strict_parse_forbidden():
            again = self.load()
        self.assertEqual(self.view(again), self.view(broken))
        self.write(rel, sample(rel))
        healed = self.same_as_fresh()
        self.assertIsNotNone(healed.by_relpath(rel))
        self.assertEqual(healed.errors, [])

    def test_the_same_bytes_under_another_name_are_parsed_again(self):
        self.cached_load()
        other = 'session/site_Cache_Renamed.json'
        os.rename(os.path.join(self.root, SAMPLES[0]), os.path.join(self.root, other))
        with mock.patch.object(br.Record, 'from_bytes', wraps=br.Record.from_bytes) as parsed:
            store = self.load()
        self.assertEqual(parsed.call_count, 1)
        self.assertEqual(self.view(store), self.view(self.fresh()))

    def test_a_symlinked_or_oversized_record_is_never_cached(self):
        self.cached_load()
        link = os.path.join(self.root, 'session', 'site_Cache_Link.json')
        os.symlink(os.path.join(self.root, SAMPLES[0]), link)
        self.write('session/site_Cache_Big.json', b' ' * (br.MAX_BYTES + 1))
        store = self.same_as_fresh()
        self.assertTrue(any('site_Cache_Link.json: not a regular file' in e for e in store.errors))
        self.assertTrue(any('site_Cache_Big.json: file is' in e for e in store.errors))
        self.assertEqual(sorted(self.entries()), sorted(SAMPLES))

    def test_a_parser_change_drops_every_entry(self):
        self.cached_load()
        with mock.patch.object(br, 'parser_digest', return_value='0' * 64), \
                mock.patch.object(br.Record, 'from_bytes', wraps=br.Record.from_bytes) as parsed:
            self.load()
        self.assertEqual(parsed.call_count, len(SAMPLES))

    def test_an_entry_without_its_file_is_never_a_record(self):
        self.cached_load()
        with open(self.cache) as f:
            doc = json.load(f)
        doc['entries']['session/site_Cache_Ghost.json'] = doc['entries'][SAMPLES[0]]
        with open(self.cache, 'w') as f:
            json.dump(doc, f)
        store = self.same_as_fresh()
        self.assertIsNone(store.by_relpath('session/site_Cache_Ghost.json'))
        self.assertNotIn('session/site_Cache_Ghost.json', self.entries())

    def test_a_forged_entry_for_changed_bytes_is_not_trusted(self):
        self.cached_load()
        rel = SAMPLES[0]
        self.write(rel, b'{"schema": 1}')
        with open(self.cache) as f:
            doc = json.load(f)
        doc['entries'][rel] = ['f' * 64, []]
        with open(self.cache, 'w') as f:
            json.dump(doc, f)
        store = self.same_as_fresh()
        self.assertIsNone(store.by_relpath(rel))
        self.assertTrue(any(e.startswith(f'bindings/{rel}:') for e in store.errors))

    def test_an_unreadable_or_foreign_cache_is_ignored(self):
        self.cached_load()
        for body in ('{not json', json.dumps({'schema': br.CACHE_SCHEMA, 'code': '0' * 64, 'entries': {}}),
                     json.dumps({'schema': br.CACHE_SCHEMA + 1, 'code': br.parser_digest(), 'entries': {}}),
                     json.dumps([1, 2, 3])):
            with open(self.cache, 'w') as f:
                f.write(body)
            self.same_as_fresh()
            self.assertEqual(sorted(self.entries()), sorted(SAMPLES))

    def test_a_copy_of_the_tree_is_served_from_the_same_cache(self):
        self.cached_load()
        copy = os.path.join(self.dir, 'copy')
        shutil.copytree(self.root, copy)
        with strict_parse_forbidden():
            store = self.load(copy)
        self.assertEqual(self.view(store), self.view(self.fresh(copy)))

    def test_the_environment_turns_the_cache_off_or_points_it_elsewhere(self):
        other = os.path.join(self.dir, 'elsewhere.json')
        with mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': other}):
            br.Store(self.root)
        self.assertTrue(os.path.exists(other))
        with mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': '0'}):
            self.assertIsNone(br.cache_path_for(br.RECORDS))
        with mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': ''}):
            self.assertEqual(br.cache_path_for(br.RECORDS), br.CACHE)
            self.assertIsNone(br.cache_path_for(self.root))

    def test_the_real_records_load_the_same_with_and_without_the_cache(self):
        shared = br.Store.get()
        cache = br.cache_path_for(br.RECORDS)
        warm = br.Store(br.RECORDS, cache_path=cache) if cache else shared
        plain = self.fresh(br.RECORDS)
        self.assertEqual(self.view(shared), self.view(plain))
        self.assertEqual(self.view(warm), self.view(plain))


if __name__ == '__main__':
    unittest.main()
