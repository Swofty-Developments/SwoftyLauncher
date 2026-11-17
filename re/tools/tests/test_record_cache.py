import json, os, shutil, sys, tempfile, time, unittest
from unittest import mock

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
import binding_record as br

SAMPLES = ('session/site_Sess_StintTyreCheckPredictCall.json', 'core/Ne_Get_SettingDouble.json')
WINDOW_NS = 50_000_000


def sample(rel):
    with open(os.path.join(br.RECORDS, rel), 'rb') as f:
        return f.read()


class RecordCacheTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.root = os.path.join(self.dir, 'bindings')
        self.cache = os.path.join(self.dir, 'cache', 'records.json')
        for rel in SAMPLES:
            self.write(rel, sample(rel))
        patcher = mock.patch.object(br.RecordCache, 'RACY_NS', WINDOW_NS)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.addCleanup(shutil.rmtree, self.dir)

    def write(self, rel, data):
        path = os.path.join(self.root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(data)
        return path

    def settle(self):
        time.sleep(WINDOW_NS * 2 / 1e9)

    def load(self):
        return br.Store(self.root, cache_path=self.cache)

    def fresh(self):
        return br.Store(self.root)

    def entries(self):
        if not os.path.exists(self.cache):
            return {}
        with open(self.cache) as f:
            return json.load(f)['entries']

    def view(self, store):
        return sorted((r.relpath, r.to_json()) for r in store.records), sorted(store.errors)

    def same_as_fresh(self):
        cached = self.load()
        self.assertEqual(self.view(cached), self.view(self.fresh()))
        return cached

    def cached_load(self):
        self.settle()
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

    def test_a_settled_load_is_served_from_the_cache(self):
        self.cached_load()
        with mock.patch.object(br.Record, 'load', side_effect=AssertionError('parsed a cached record')):
            store = self.load()
        self.assertEqual(len(store.records), len(SAMPLES))
        self.assertEqual(self.view(store), self.view(self.fresh()))

    def test_a_changed_record_of_the_same_size_is_parsed_again(self):
        self.cached_load()
        rel = SAMPLES[0]
        data, swapped = self.same_size_edit(rel)
        self.write(rel, data)
        store = self.same_as_fresh()
        self.assertEqual(store.by_relpath(rel).doc['row']['notes'], swapped)

    def test_a_record_replaced_with_its_old_size_and_mtime_is_parsed_again(self):
        self.cached_load()
        rel = SAMPLES[0]
        path = os.path.join(self.root, rel)
        old = os.lstat(path)
        data, swapped = self.same_size_edit(rel)
        tmp = os.path.join(self.dir, 'replacement.json')
        with open(tmp, 'wb') as f:
            f.write(data)
        os.utime(tmp, ns=(old.st_atime_ns, old.st_mtime_ns))
        os.replace(tmp, path)
        new = os.lstat(path)
        self.assertEqual((new.st_size, new.st_mtime_ns), (old.st_size, old.st_mtime_ns))
        store = self.same_as_fresh()
        self.assertEqual(store.by_relpath(rel).doc['row']['notes'], swapped)

    def test_a_record_written_just_before_a_load_is_not_stored(self):
        rel = SAMPLES[0]
        self.write(rel, sample(rel))
        with mock.patch.object(br.RecordCache, 'RACY_NS', 60_000_000_000):
            self.load()
        self.assertNotIn(rel, self.entries())
        data, swapped = self.same_size_edit(rel)
        self.write(rel, data)
        store = self.same_as_fresh()
        self.assertEqual(store.by_relpath(rel).doc['row']['notes'], swapped)

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
        self.settle()
        self.load()
        self.assertTrue(self.entries()[rel]['errors'])
        self.write(rel, sample(rel))
        healed = self.same_as_fresh()
        self.assertIsNotNone(healed.by_relpath(rel))
        self.assertEqual(healed.errors, [])

    def test_a_parser_change_drops_every_entry(self):
        self.cached_load()
        with mock.patch.object(br, 'parser_digest', return_value='0' * 64), \
                mock.patch.object(br.Record, 'load', wraps=br.Record.load) as load:
            self.load()
        self.assertEqual(load.call_count, len(SAMPLES))

    def test_an_entry_without_its_file_is_never_a_record(self):
        self.cached_load()
        with open(self.cache) as f:
            doc = json.load(f)
        ghost = dict(doc['entries'][SAMPLES[0]])
        ghost['doc'] = dict(ghost['doc'], name='site_Cache_Ghost')
        doc['entries']['session/site_Cache_Ghost.json'] = ghost
        with open(self.cache, 'w') as f:
            json.dump(doc, f)
        store = self.same_as_fresh()
        self.assertIsNone(store.by_relpath('session/site_Cache_Ghost.json'))
        self.assertNotIn('session/site_Cache_Ghost.json', self.entries())

    def test_an_unreadable_or_foreign_cache_is_ignored(self):
        self.cached_load()
        for body in ('{not json', json.dumps({'schema': br.CACHE_SCHEMA, 'code': br.parser_digest(),
                                               'root': os.path.join(self.dir, 'elsewhere'), 'entries': {}}),
                     json.dumps([1, 2, 3])):
            with open(self.cache, 'w') as f:
                f.write(body)
            self.same_as_fresh()
            self.assertEqual(sorted(self.entries()), sorted(SAMPLES))

    def test_the_real_records_load_the_same_with_and_without_the_cache(self):
        cache = os.path.join(self.dir, 'real.json')
        first = br.Store(br.RECORDS, cache_path=cache)
        again = br.Store(br.RECORDS, cache_path=cache)
        with mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': '0'}):
            plain = br.Store(br.RECORDS)
        self.assertEqual(self.view(first), self.view(plain))
        self.assertEqual(self.view(again), self.view(plain))


if __name__ == '__main__':
    unittest.main()
