import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import time
import types
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools'))
import coff_ctors  # noqa: E402
import init_check as ic  # noqa: E402

BODIES = {
    'SWOFTY_ON_LOAD': ' namespace { const ::swofty::detail::mixin reg{...}; } void SWOFTY_NS::handler()',
    'SWOFTY_HOOK_GUARD': ' namespace { HookGuard hook_guard{HookGuardId::id}; SWOFTY_ON_LOAD { arm_next_hook_guard(); } }',
    'SWOFTY_MID_HOOK_GUARD': ' SWOFTY_HOOK_GUARD(id, what, setup_mid(hook))',
    'SWOFTY_LATE_HOOK_GUARD': ' SWOFTY_MID_HOOK_GUARD(id, what, hook)',
}


class Unit:
    def __init__(self, name, decls, comdat=(), own=True):
        self.name = name
        self.entries = [ic.Entry(name, line, 'constructor', decl, 'SWOFTY_ON_LOAD') for line, decl in enumerate(decls, 1)]
        main = coff_ctors.Ctor(f'_GLOBAL__sub_I_{name}', False)
        main.initialized = [f'_ZN12_GLOBAL__N_13regE{n}' for n in range(len(decls))] if own else []
        self.ctors = [main]
        for symbol in comdat:
            c = coff_ctors.Ctor('__cxx_global_var_init', True)
            c.initialized = [symbol]
            self.ctors.append(c)


def section(text, title):
    rows, inside = [], False
    for line in text.splitlines():
        if not line.startswith(' '):
            inside = line == title
        elif inside:
            rows.append(line.strip())
    return rows


class OrderEffects(unittest.TestCase):
    def test_units_initialize_in_the_reverse_of_the_link_order(self):
        units = [Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('b.cpp', ['SWOFTY_ON_LOAD'])]
        text, _, _ = ic.order_effects(units, [], BODIES)
        self.assertEqual(section(text, 'init'), ['b.cpp', 'a.cpp'])
        self.assertEqual(section(text, 'on-load'), ['b.cpp  SWOFTY_ON_LOAD', 'a.cpp  SWOFTY_ON_LOAD'])

    def test_a_unit_that_only_appends_to_the_sdk_lists_is_listed(self):
        units = [Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('sdk_only.cpp', ['SWOFTY_ON_LOAD'], own=False)]
        text, _, _ = ic.order_effects(units, [], BODIES)
        self.assertEqual(section(text, 'init'), ['sdk_only.cpp', 'a.cpp'])
        self.assertEqual(section(text, 'mixins'), ['sdk_only.cpp  SWOFTY_ON_LOAD', 'a.cpp  SWOFTY_ON_LOAD'])

    def test_the_first_object_in_link_order_keeps_an_inline_variable(self):
        markets = '_ZN6swofty6career7marketsE'
        units = [Unit('a.cpp', ['SWOFTY_ON_LOAD'], [markets]), Unit('b.cpp', ['SWOFTY_ON_LOAD'], [markets])]
        text, _, _ = ic.order_effects(units, [], BODIES)
        self.assertEqual(section(text, 'inline'), ['a.cpp  swofty::career::markets'])

    def test_a_guard_spelled_through_another_macro_arms_a_hook_guard(self):
        units = [Unit('mid.cpp', ['SWOFTY_MID_HOOK_GUARD(mid, "mid", hook)']),
                 Unit('late.cpp', ['SWOFTY_LATE_HOOK_GUARD(late, "late", hook)']),
                 Unit('plain.cpp', ['SWOFTY_HOOK_GUARD(plain, "plain", setup)'])]
        text, _, misarmed = ic.order_effects(units, ['plain', 'late', 'mid'], BODIES)
        self.assertEqual(section(text, 'on-load'), ['plain.cpp  SWOFTY_HOOK_GUARD(plain), arms plain',
                                                    'late.cpp  SWOFTY_LATE_HOOK_GUARD(late), arms late',
                                                    'mid.cpp  SWOFTY_MID_HOOK_GUARD(mid), arms mid'])
        self.assertEqual(misarmed, [])

    def test_a_guard_that_arms_another_guard_is_reported(self):
        units = [Unit('first.cpp', ['SWOFTY_MID_HOOK_GUARD(first, "first", hook)']),
                 Unit('second.cpp', ['SWOFTY_HOOK_GUARD(second, "second", setup)'])]
        _, _, misarmed = ic.order_effects(units, ['first', 'second'], BODIES)
        self.assertEqual(misarmed, ['second.cpp  SWOFTY_HOOK_GUARD(second) arms first',
                                    'first.cpp  SWOFTY_MID_HOOK_GUARD(first) arms second'])

    def test_an_on_load_entry_that_is_no_guard_arms_nothing(self):
        units = [Unit('a.cpp', ['SWOFTY_ON_LOAD', 'SWOFTY_HOOK_GUARD(g, "g", setup)'])]
        text, _, misarmed = ic.order_effects(units, ['g'], BODIES)
        self.assertEqual(section(text, 'on-load'), ['a.cpp  SWOFTY_ON_LOAD', 'a.cpp  SWOFTY_HOOK_GUARD(g), arms g'])
        self.assertEqual(misarmed, [])


class References(unittest.TestCase):
    def test_a_constructor_reaching_another_units_object_is_listed(self):
        u = Unit('race_hud.cpp', ['SWOFTY_ON_LOAD'])
        u.ctors[0].external = ['_ZN5popup8messagesE']
        self.assertEqual(ic.references([u]), [('race_hud.cpp', 'popup::messages')])

    def test_an_unlisted_reference_is_new_and_a_listed_one_that_went_away_is_stale(self):
        refs = [('race_hud.cpp', 'popup::messages'), ('toast.cpp', 'popup::messages')]
        allowed = {('race_hud.cpp', 'popup::messages'): 1, ('gone.cpp', 'perf::g_live'): 2}
        self.assertEqual(ic.unlisted_references(refs, allowed),
                         ([('toast.cpp', 'popup::messages')], [('gone.cpp', 'perf::g_live')]))

    def test_an_inline_variables_reference_is_listed_once_under_the_variable(self):
        first = Unit('first.cpp', ['SWOFTY_ON_LOAD'], ['_ZN6swofty6career7marketsE'])
        second = Unit('second.cpp', ['SWOFTY_ON_LOAD'], ['_ZN6swofty6career7marketsE'])
        for u in (first, second):
            u.ctors[1].external = ['_ZN5popup8messagesE']
        self.assertEqual(ic.references([first, second]), [('inline swofty::career::markets', 'popup::messages')])
        self.assertEqual(ic.references([second, first]), ic.references([first, second]))


class ExplicitOrder(unittest.TestCase):
    MARKS = {'a.cpp': [(1, 'a')], 'b.cpp': [(1, 'b')]}

    def explicit(self, units, ranks, marks=None, hooks=()):
        marks = marks if marks is not None else self.MARKS
        return ic.explicit_order(units, list(hooks), BODIES, ranks, lambda name, bodies: marks.get(name, []))

    def test_ranks_that_list_units_in_their_run_order_give_that_order(self):
        text, problems = self.explicit([Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('b.cpp', ['SWOFTY_ON_LOAD'])], ['b', 'a'])
        self.assertEqual(section(text, 'explicit'), ['b  b.cpp  SWOFTY_ON_LOAD', 'a  a.cpp  SWOFTY_ON_LOAD'])
        self.assertEqual(problems, [])

    def test_the_rank_order_does_not_follow_the_link_order(self):
        a, b = Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('b.cpp', ['SWOFTY_ON_LOAD'])
        self.assertEqual(self.explicit([a, b], ['b', 'a'])[0], self.explicit([b, a], ['b', 'a'])[0])

    def test_ranks_in_another_order_than_the_run_order_fail(self):
        _, problems = self.explicit([Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('b.cpp', ['SWOFTY_ON_LOAD'])], ['a', 'b'])
        self.assertTrue(problems and 'otherwise than the run order' in problems[0])

    def test_a_registering_unit_without_a_rank_fails(self):
        _, problems = self.explicit([Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('c.cpp', ['SWOFTY_ON_LOAD'])], ['a'])
        self.assertEqual(problems, ['c.cpp: registers mixins or on-load entries but names no rank with SWOFTY_CORE_RANK'])

    def test_a_rank_named_after_the_first_registration_fails(self):
        _, problems = self.explicit([Unit('a.cpp', ['SWOFTY_ON_LOAD', 'SWOFTY_ON_LOAD'])], ['a'], {'a.cpp': [(2, 'a')]})
        self.assertEqual(problems, ["a.cpp:2: the rank is named after the unit's first registration (line 1)"])

    def test_a_rank_no_registering_unit_names_fails(self):
        _, problems = self.explicit([Unit('a.cpp', ['SWOFTY_ON_LOAD'])], ['a', 'gone'])
        self.assertEqual(problems, [f"{ic.RANK_HEADER}: CoreRank::gone is no registering unit's rank"])

    def test_two_units_with_one_rank_fail(self):
        _, problems = self.explicit([Unit('a.cpp', ['SWOFTY_ON_LOAD']), Unit('b.cpp', ['SWOFTY_ON_LOAD'])], ['a'],
                                    {'a.cpp': [(1, 'a')], 'b.cpp': [(1, 'a')]})
        self.assertEqual(problems, ['a.cpp:1: b.cpp names the same rank, a'])

    def test_a_hook_guard_in_rank_order_arms_its_own_guard(self):
        units = [Unit('g.cpp', ['SWOFTY_HOOK_GUARD(g, "g", setup)']), Unit('h.cpp', ['SWOFTY_MID_HOOK_GUARD(h, "h", hook)'])]
        text, problems = self.explicit(units, ['h', 'g'], {'g.cpp': [(1, 'g')], 'h.cpp': [(1, 'h')]}, ['h', 'g'])
        self.assertEqual(section(text, 'explicit'), ['h  h.cpp  SWOFTY_MID_HOOK_GUARD(h), arms h',
                                                     'g  g.cpp  SWOFTY_HOOK_GUARD(g), arms g'])
        self.assertEqual(problems, [])

    def test_a_hook_guards_rank_is_read_through_its_macro(self):
        bodies = dict(BODIES, SWOFTY_HOOK_GUARD=' SWOFTY_CORE_RANK(id) ' + BODIES['SWOFTY_HOOK_GUARD'])
        lines = ic.source_lines.__defaults__[0]
        lines['g.cpp'] = ['#include "core/guards/guard.hpp"', '', 'SWOFTY_MID_HOOK_GUARD(gp, "gp", hook)',
                          '    // SWOFTY_CORE_RANK(comment)', 'SWOFTY_CORE_RANK(own)']
        try:
            self.assertEqual(ic.rank_marks('g.cpp', bodies), [(3, 'gp'), (5, 'own')])
        finally:
            del lines['g.cpp']


class FindDecl(unittest.TestCase):
    def setUp(self):
        self.saved = dict(ic.source_lines.__defaults__[0])
        ic.source_lines.__defaults__[0]['unit.cpp'] = [
            'void report()',
            '{',
            '    swofty::log("[tyres] slot guard (%s): no usable slot", name);',
            '    if (guard == nullptr)',
            '}',
            'guards::SiteGuard guard{',
            'static std::map<std::string, int> rules;',
        ]

    def tearDown(self):
        ic.source_lines.__defaults__[0].clear()
        ic.source_lines.__defaults__[0].update(self.saved)

    def test_a_declaration_is_found_past_a_statement_that_names_it(self):
        self.assertEqual(ic.find_decl('unit.cpp', 'guard'), (6, 'guards::SiteGuard guard{'))

    def test_a_declaration_with_template_arguments_is_found(self):
        self.assertEqual(ic.find_decl('unit.cpp', 'rules'), (7, 'static std::map<std::string, int> rules;'))

    def test_a_name_that_is_never_declared_is_not_found(self):
        self.assertEqual(ic.find_decl('unit.cpp', 'name'), (0, ''))


class ChangedDuringBuild(unittest.TestCase):
    def setUp(self):
        self.native, self.root = ic.NATIVE, tempfile.mkdtemp()
        ic.NATIVE = self.root
        self.write('src/a.cpp', 'int a = f();\n')
        self.write('src/a.hpp', 'int f();\n')

    def tearDown(self):
        ic.NATIVE = self.native
        shutil.rmtree(self.root)

    def write(self, path, text):
        os.makedirs(os.path.dirname(os.path.join(self.root, path)), exist_ok=True)
        with open(os.path.join(self.root, path), 'w') as f:
            f.write(text)

    def unit(self, compiled, built, paths=()):
        entries = [ic.Entry(p, 1, 'constructor', 'int a = f();', '') for p in ('src/a.cpp',) + tuple(paths)]
        return types.SimpleNamespace(name='src/a.cpp', compiled=compiled, built=built, entries=entries)

    def test_a_source_as_it_was_compiled_has_not_changed(self):
        u = self.unit(ic.source_hash('src/a.cpp'), time.time() + 60, ['src/a.hpp'])
        self.assertEqual(ic.changed_during_build(u), [])

    def test_a_source_edited_after_its_compile_started_changed(self):
        u = self.unit(ic.source_hash('src/a.cpp'), time.time() + 60)
        self.write('src/a.cpp', 'int b;\nint a = f();\n')
        self.assertEqual(ic.changed_during_build(u), ['src/a.cpp'])

    def test_a_header_with_an_entry_that_is_newer_than_the_report_changed(self):
        built = os.path.getmtime(os.path.join(self.root, 'src/a.hpp')) - 60
        self.assertEqual(ic.changed_during_build(self.unit(ic.source_hash('src/a.cpp'), built, ['src/a.hpp'])),
                         ['src/a.hpp'])

    def test_a_report_without_a_hash_does_not_judge_its_source(self):
        u = self.unit(None, time.time() + 60)
        self.write('src/a.cpp', 'int b;\n')
        self.assertEqual(ic.changed_during_build(u), [])

    def test_the_compile_wrapper_records_the_hash_of_the_source_before_it_compiles(self):
        report = os.path.join(self.root, 'a.o.init')
        source = os.path.join(self.root, 'src/a.cpp')
        wrapper = os.path.join(self.native, 'tools/cxx_init.sh')
        subprocess.run(['sh', wrapper, report, source, 'sh', '-c', f'printf "int c;\\n" >> {source}'], check=True)
        self.assertEqual(ic.compiled_hash(report), hashlib.sha256(b'int a = f();\n').hexdigest())
        self.assertEqual(ic.changed_during_build(self.unit(ic.compiled_hash(report), os.path.getmtime(report))),
                         ['src/a.cpp'])


class Expansion(unittest.TestCase):
    def test_expands_to_follows_macros_to_any_depth(self):
        self.assertTrue(ic.expands_to('SWOFTY_LATE_HOOK_GUARD', 'SWOFTY_HOOK_GUARD', BODIES))
        self.assertFalse(ic.expands_to('SWOFTY_ON_LOAD', 'SWOFTY_HOOK_GUARD', BODIES))

    def test_a_macro_that_names_itself_ends(self):
        self.assertFalse(ic.expands_to('SWOFTY_SELF', 'SWOFTY_HOOK_GUARD', {'SWOFTY_SELF': ' SWOFTY_SELF(x)'}))


class Leaf(unittest.TestCase):
    def test_leaf_reads_the_name_a_mangling_ends_in(self):
        self.assertEqual(coff_ctors.leaf('_ZN12_GLOBAL__N_15guardE'), 'guard')
        self.assertEqual(coff_ctors.leaf('_ZN7runtime16loading_core_setE'), 'loading_core_set')
        self.assertEqual(coff_ctors.leaf('_ZL5guard'), 'guard')
        self.assertEqual(coff_ctors.leaf('plain'), 'plain')


if __name__ == '__main__':
    unittest.main()
