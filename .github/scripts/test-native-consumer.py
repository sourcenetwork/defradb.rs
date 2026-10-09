#!/usr/bin/env python3
"""Offline driver regressions; no Cargo, native processes or network access."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("native_consumer", Path(__file__).with_name("native-consumer.py"))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


def success(names):
    return "".join("test " + n + " ... ok\n" for n in names) + f"test result: ok. {len(names)} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"


class Tests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)

    def test_failed_phase_exports_location_without_private_paths_or_assertion_values(self):
        log = self.root / 'private.log'
        log.write_text("thread 'policy' panicked at /private/keys/poll.rs:47:9:\n"
                       "assertion failed: secret=private-key-value\n")
        site = driver.panic_site(log)
        self.assertEqual(site, {'file': 'poll.rs', 'line': 47, 'column': 9})
        record = {'phase': 'policy-lifecycle', 'exit_code': 101, 'seconds': 30.4,
                  'panic_site': site, 'command': ['--secret', 'private-key-value'],
                  'log': str(log)}
        public = json.dumps(driver.public_phase(record))
        self.assertNotIn('private-key-value', public)
        self.assertNotIn('/private/keys', public)
        self.assertNotIn(str(self.root), public)

    def test_panic_location_accepts_relative_and_windows_paths_but_not_messages(self):
        log = self.root / 'private.log'
        for path in ('src/poll.rs', r'C:\private\poll.rs', 'poll.rs'):
            log.write_text("thread 'test' panicked at " + path + ":47:9:\nprivate assertion\n")
            self.assertEqual(driver.panic_site(log), {'file': 'poll.rs', 'line': 47, 'column': 9})
        log.write_text('qualification failed without a Rust panic\n')
        self.assertIsNone(driver.panic_site(log))

    def test_sdk_pin_reader_requires_all_seven_exact_matching_declarations(self):
        for file, count in zip(driver.SDK_MANIFESTS, (3, 4)):
            path = self.root / file
            path.parent.mkdir(parents=True)
            path.write_text(('vera = { git = "https://github.com/sourcenetwork/vera.rs", rev = "' + 'a' * 40 + '" }\n') * count)
        self.assertEqual(driver.revision(self.root), 'a' * 40)
        path.write_text(path.read_text().replace('a' * 40, 'b' * 40, 1))
        with self.assertRaises(ValueError):
            driver.revision(self.root)
        path.write_text('')
        with self.assertRaises(ValueError):
            driver.revision(self.root)

    def test_test_output_rejects_zero_extra_ignored_and_duplicate_passes(self):
        log = self.root / 'test.log'
        log.write_text(success(['expected']))
        self.assertEqual(driver.passed_tests(log, ['expected']), 1)
        for output in (success([]), success(['expected', 'extra']), success(['expected', 'expected']),
                       success(['expected']).replace('0 ignored', '1 ignored'),
                       success(['expected']).replace('... ok', '... FAILED')):
            log.write_text(output)
            with self.assertRaises(ValueError):
                driver.passed_tests(log, ['expected'])

    def test_artifact_requires_unique_release_executable_under_owned_target(self):
        target = self.root / 'target'
        target.mkdir()
        binary = target / 'defra'
        binary.write_bytes(b'fixture')
        item = {'reason': 'compiler-artifact', 'target': {'name': 'defra'},
                'profile': {'debug_assertions': False}, 'executable': str(binary)}
        log = self.root / 'build.log'
        log.write_text(json.dumps(item) + '\n')
        self.assertEqual(driver.artifact(log, 'defra', target), binary.resolve())
        for entries in ([item, item], [{**item, 'profile': {'debug_assertions': True}}],
                        [{**item, 'executable': str(self.root / 'outside')}], []):
            log.write_text(''.join(json.dumps(x) + '\n' for x in entries))
            with self.assertRaises(ValueError):
                driver.artifact(log, 'defra', target)

    def test_environment_removes_profile_compiler_runtime_and_deadline_overrides(self):
        poison = {key: 'poison' for key in ('CARGO_PROFILE_RELEASE_LTO', 'RUSTC', 'RUSTDOC',
                  'CARGO_BUILD_RUSTC', 'RUSTFLAGS', 'CARGO_TARGET_DIR', 'VERAD_BINARY',
                  'DEFRA_RUST_BINARY', 'VERA_E2E_DEADLINE_SCALE', 'ORBIS_LOCAL_STORAGE_KDF_M_COST')}
        env = driver.clean_environment({**poison, 'PATH': '/usr/bin'})
        self.assertFalse(set(poison) & set(env))
        self.assertEqual(env['PATH'], '/usr/bin')
        self.assertEqual(env['CARGO_BUILD_JOBS'], '2')

    def test_numeric_summary_cannot_include_commands_or_private_output(self):
        runner = object.__new__(driver.Runner)
        runner.private = self.root
        runner.summary = self.root / 'summary.json'
        runner.sources, runner.binaries = {}, {}
        runner.records = [{'phase': 'reader-update', 'command': ['private-key'], 'log': 'private-path',
                           'exit_code': 0, 'seconds': 1.5, 'passed': 1}]
        runner.save()
        self.assertEqual(json.loads(runner.summary.read_text()), {'phases': [
            {'phase': 'reader-update', 'exit_code': 0, 'seconds': 1.5, 'passed': 1}]})

    def test_real_runner_records_failure_without_streaming_raw_log(self):
        runner = object.__new__(driver.Runner)
        runner.root = runner.private = self.root
        runner.summary = self.root / 'summary.json'
        runner.env, runner.sources, runner.binaries, runner.records = {}, {}, {}, []
        with patch.object(runner, 'guard'), patch.object(driver.subprocess, 'Popen') as popen, \
                patch.object(driver, 'stop_group'), patch('builtins.print') as output:
            popen.return_value.wait.return_value = 7
            with self.assertRaises(RuntimeError):
                runner.run('build-defra', ['cargo', 'private-argument'])
            text = ''.join(str(c) for c in output.call_args_list)
            self.assertNotIn('private-argument', text)
            self.assertEqual(runner.records[0]['exit_code'], 7)

    def test_real_runner_retains_panic_location_after_closing_private_log(self):
        runner = object.__new__(driver.Runner)
        runner.root = runner.private = self.root
        runner.summary = self.root / 'summary.json'
        runner.env, runner.sources, runner.binaries, runner.records = {}, {}, {}, []

        def process(*args, **kwargs):
            kwargs['stdout'].write("thread 'policy' panicked at /private/poll.rs:47:9:\nsecret-value\n")
            child = unittest.mock.Mock()
            child.wait.return_value = 101
            return child

        with patch.object(runner, 'guard'), patch.object(driver.subprocess, 'Popen', side_effect=process), \
                patch.object(driver, 'stop_group'), patch('builtins.print'):
            with self.assertRaises(RuntimeError):
                runner.run('policy-lifecycle', ['test-binary'])
        public = json.loads(runner.summary.read_text())
        self.assertEqual(public['phases'][0]['panic_site'], {'file': 'poll.rs', 'line': 47, 'column': 9})
        self.assertNotIn('secret-value', runner.summary.read_text())

    def test_pipeline_stages_normal_binaries_before_tests_and_runs_exact_cases_once(self):
        class Fake:
            def __init__(self, root):
                self.root = self.vera = root
                self.target = root / 'target'
                self.target.mkdir()
                self.env, self.calls = {}, []
            def run(self, phase, args, cwd=None):
                self.calls.append(('run', phase, args, dict(self.env)))
                return phase
            def stage(self, log, target, name):
                self.calls.append(('stage', name))
                return '/staged/' + name
            def test(self, phase, binary, selectors, expected):
                self.calls.append(('test', phase, binary, selectors, expected))
        runner = Fake(self.root)
        driver.qualify(runner)
        runs = {c[1]: c for c in runner.calls if c[0] == 'run'}
        self.assertEqual(runs['build-defra'][2][runs['build-defra'][2].index('--features') + 1], 'vera')
        first_test = next(i for i, c in enumerate(runner.calls) if c[0] == 'test')
        self.assertLess(runner.calls.index(('stage', 'defra')), first_test)
        self.assertLess(runner.calls.index(('stage', 'verad')), first_test)
        self.assertEqual(runs['compile-native'][3]['DEFRA_RUST_BINARY'], '/staged/defra')
        self.assertEqual(runs['compile-native'][3]['VERAD_BINARY'], '/staged/verad')
        tests = [c for c in runner.calls if c[0] == 'test']
        self.assertEqual(len(tests), 8)
        self.assertEqual(len(tests[0][4]), 14)
        self.assertEqual({c[1] for c in tests[1:]}, {phase for phase, _ in driver.LIVE_CASES})
        for _, _, _, selectors, expected in tests[1:]:
            self.assertEqual(selectors, [expected[0], '--exact'])
            self.assertNotIn('--ignored', selectors)
        for _, phase, args, _ in runs.values():
            if not phase.startswith('fetch-'):
                self.assertIn('--release', args)
                self.assertIn('--frozen', args)
        self.assertFalse(runner.target.exists())


if __name__ == '__main__':
    unittest.main()
