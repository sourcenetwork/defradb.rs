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


URL = 'https://github.com/sourcenetwork/vera.rs'
PIN, OTHER = 'a' * 40, 'b' * 40
DEPENDENCIES = '[dependencies]\n'
SDK = DEPENDENCIES + 'vera-client = "0.1.2"\nvera-domain = "0.1.2"\nvera-modules = "0.1.2"\n'
HARNESS = 'vera-harness = { git = "' + URL + '", rev = "' + PIN + '" }\n'
# A node manifest with one Vera dependency at the harness pin, and one with two.
PINNED = DEPENDENCIES + 'vera-jsonrpc = { git = "' + URL + '", rev = "' + PIN + '" }\n'
NODE = PINNED + 'vera-node = { git = "' + URL + '", rev = "' + PIN + '" }\n'


class Tests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)

    def sdk_fixture(self):
        for file, harness in zip(driver.SDK_MANIFESTS, (False, True)):
            path = self.root / file
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(SDK + (HARNESS if harness else ''))
        for file in driver.NODE_MANIFESTS:
            path = self.root / file
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(NODE)

    def test_runtime_revision_rejects_mutable_malformed_and_nonstring_values(self):
        for value in ('', 'main', 'a' * 7, 'A' * 40, 'a' * 40 + '\n',
                      ' a' * 20, None, 17):
            with self.subTest(value=value), self.assertRaises(ValueError):
                driver.runtime_revision(value)

    def test_explicit_runtime_keeps_sdk_pin_and_records_both_sources(self):
        self.sdk_fixture()
        runtime = self.root / 'runtime'
        sources = {self.root: {'revision': 'c' * 40, 'lock_sha256': 'd' * 64},
                   runtime: {'revision': 'b' * 40, 'lock_sha256': 'e' * 64}}
        with patch.object(driver, 'source', side_effect=sources.__getitem__):
            runner = driver.Runner(self.root, runtime, self.root, 'b' * 40)
            runner.guard()
            summary = json.loads(runner.summary.read_text())
            manifest = json.loads((runner.private / 'manifest.json').read_text())
            self.assertEqual(summary['sdk_revision'], 'a' * 40)
            self.assertEqual(summary['sources']['vera'], sources[runtime])
            self.assertEqual(summary['sources']['defra'], sources[self.root])
            self.assertEqual(manifest['sdk_revision'], summary['sdk_revision'])
            sources[runtime] = {**sources[runtime], 'revision': 'f' * 40}
            with self.assertRaises(ValueError):
                runner.guard()

    def test_default_and_explicit_runtime_reject_wrong_checkout_before_staging(self):
        self.sdk_fixture()
        for selected, actual in ((None, 'b' * 40), ('b' * 40, 'a' * 40)):
            with self.subTest(selected=selected), patch.object(driver, 'source',
                    return_value={'revision': actual, 'lock_sha256': 'c' * 64}), \
                    self.assertRaises(ValueError):
                driver.Runner(self.root, self.root / 'runtime', self.root, selected)
        self.assertEqual(list(self.root.glob('defra-native-consumer-*')), [])
        with patch.object(driver, 'source', return_value={'revision': 'a' * 40, 'lock_sha256': 'c' * 64}):
            runner = driver.Runner(self.root, self.root / 'runtime', self.root)
        self.assertEqual(runner.sdk_revision, 'a' * 40)

    def test_runtime_override_does_not_bypass_conflicting_sdk_declarations(self):
        self.sdk_fixture()
        path = self.root / driver.SDK_MANIFESTS[1]
        path.write_text(path.read_text().replace('vera-domain = "0.1.2"', 'vera-domain = "0.1.3"'))
        with patch.object(driver, 'source') as source, self.assertRaises(ValueError):
            driver.Runner(self.root, self.root / 'runtime', self.root, 'b' * 40)
        source.assert_not_called()

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

    def test_sdk_reader_requires_published_declarations_and_one_immutable_harness_pin(self):
        self.sdk_fixture()
        self.assertEqual(driver.revision(self.root), 'a' * 40)
        sdk, harness = (self.root / name for name in driver.SDK_MANIFESTS)
        changes = [
            (sdk, SDK.replace('vera-domain = "0.1.2"', 'vera-domain = "0.1.3"')),
            (sdk, SDK.replace('vera-modules = "0.1.2"\n', '')),
            (sdk, SDK.replace('vera-client = "0.1.2"', 'vera-client = { git = "https://github.com/sourcenetwork/vera.rs", rev = "' + 'a' * 40 + '" }')),
            (harness, SDK),
            (harness, SDK + HARNESS + HARNESS.replace('a' * 40, 'b' * 40)),
            (harness, SDK + HARNESS.replace('rev = "' + 'a' * 40 + '"', 'tag = "v0.1.2"')),
        ]
        for path, content in changes:
            original = path.read_text()
            path.write_text(content)
            with self.assertRaises(ValueError):
                driver.revision(self.root)
            path.write_text(original)
        self.assertEqual(driver.revision(self.root), 'a' * 40)

    def test_node_crates_must_pin_the_harness_revision_on_every_vera_dependency(self):
        self.sdk_fixture()
        node = self.root / driver.NODE_MANIFESTS[0]
        for content in (
            NODE.replace(PIN, OTHER),
            NODE.replace(PIN, OTHER, 1),
            NODE.replace('rev = "' + PIN + '"', 'tag = "v0.1.2"', 1),
            NODE.replace('rev = "' + PIN + '"', 'branch = "main"'),
            NODE.replace(PIN, PIN[:39]),
            DEPENDENCIES + 'vera-jsonrpc = "0.1.2"\n',
            PINNED.replace(DEPENDENCIES, ''),
            PINNED + 'vera-node = { git = ',
            '',
        ):
            node.write_text(content)
            with self.subTest(content=content), self.assertRaises(ValueError):
                driver.revision(self.root)
        for content in (NODE, PINNED):
            node.write_text(content)
            self.assertEqual(driver.revision(self.root), PIN)

    def second_dependency(self, forms):
        """Each form, added to a pinned node manifest, is accepted at the harness pin and refused at another."""
        self.sdk_fixture()
        node = self.root / driver.NODE_MANIFESTS[0]
        for form in forms:
            with self.subTest(form=form):
                self.assertEqual(driver.git_pins(PINNED + form % OTHER), [PIN, OTHER])
                node.write_text(PINNED + form % PIN)
                self.assertEqual(driver.revision(self.root), PIN)
                node.write_text(PINNED + form % OTHER)
                with self.assertRaises(ValueError):
                    driver.revision(self.root)
        return node

    def test_vera_pins_are_read_whatever_the_spacing_or_table_form(self):
        node = self.second_dependency((
            'vera-node={git="' + URL + '",rev="%s"}\n',
            '[dev-dependencies.vera-node]\ngit = "' + URL + '"\nrev = "%s"\n',
            '[dev-dependencies.vera-node]\ngit="' + URL + '"\nrev="%s"\n',
            '[build-dependencies]\nvera-node = { rev = "%s", git = "' + URL + '" }\n',
            "[target.'cfg(unix)'.dependencies.vera-node]\ngit = '" + URL + "'\nrev = '%s'\n",
            'vera-node.git = "' + URL + '"\nvera-node.rev = "%s"\n',
            'vera-node . git="' + URL + '"\nvera-node . rev="%s"\n',
            '"vera-node".git = "' + URL + '"\nvera-node.rev = "%s"\n',
            '[dev-dependencies]\n"vera-node".git = "' + URL + '"\n"vera-node".rev = "%s"\n',
        ))
        node.write_text(PINNED + '# vera-node={git="' + URL + '",rev="' + OTHER + '"}\n')
        self.assertEqual(driver.revision(self.root), PIN)

    def test_a_vera_dependency_needs_a_revision_of_its_own(self):
        self.sdk_fixture()
        node = self.root / driver.NODE_MANIFESTS[0]
        for unpinned in (
            '[dependencies.vera-node]\ngit = "' + URL + '"\ntag = "v0.1.2"\n',
            '[dependencies.vera-node]\ngit="' + URL + '"\nbranch="main"\n',
            '[dependencies.vera-node]\ngit = "' + URL + '"\n[dependencies.other]\nrev = "' + PIN + '"\n',
            '[dependencies.vera-node]\ngit = "' + URL + '"\nother = { rev = "' + PIN + '" }\n',
            '[dependencies.vera-node]\ngit = "' + URL + '"\nvera-jsonrpc.rev = "' + PIN + '"\n',
            'vera-node={git="' + URL + '",tag="v0.1.2"}\n',
            "vera-node = { git = '" + URL + ".git', branch = 'main' }\n",
            'vera-node = { git = "https://github.com:443/sourcenetwork/vera.rs", branch = "main" }\n',
            'vera-node = { git = "' + URL + '", branch = "main", "not.rev" = "' + PIN + '" }\n',
            'vera-node = { git = "' + URL + '", branch = "main", not.rev = "' + PIN + '" }\n',
            'vera-node = { git = "' + URL + '", branch = "main", x = { rev = "' + PIN + '" } }\n',
            'vera-node = { git = "' + URL + '", rev = ["' + PIN + '"] }\n',
            'vera-node.git = "' + URL + '"\n',
            'vera-node.git = "' + URL + '"\nvera-other.rev = "' + PIN + '"\n',
            'vera-node.git = "' + URL + '"\nrev = "' + PIN + '"\n',
        ):
            node.write_text(PINNED + unpinned)
            with self.subTest(unpinned=unpinned), self.assertRaises(ValueError):
                driver.revision(self.root)

    def test_vera_is_recognised_under_every_spelling_cargo_resolves_to_it(self):
        node = self.second_dependency((
            'vera-node = { git = "https://github.com/SourceNetwork/vera.rs", rev = "%s" }\n',
            'vera-node = { git = "http://github.com/sourcenetwork/vera.rs", rev = "%s" }\n',
            'vera-node = { "git" = "https://github.com/sourcenetwork/vera.rs", rev = "%s" }\n',
            "vera-node = { 'git' = 'HTTPS://GITHUB.COM/SOURCENETWORK/VERA.RS.GIT/', 'rev' = '%s' }\n",
            'vera-node = { "g\\u0069t" = "https://github.com/sourcenetwork/vera.rs", "r\\u0065v" = "%s" }\n',
            'vera-node = { git = "ssh://git@github.com/sourcenetwork/vera.rs.git", rev = "%s" }\n',
            'vera-node = { git = "git@github.com:sourcenetwork/vera.rs", rev = "%s" }\n',
            'vera-node = { git = "git://www.github.com/sourcenetwork/vera.rs", "rev" = "%s" }\n',
            'vera-node = { git = "https://user:token@github.com/sourcenetwork/vera.rs", rev = "%s" }\n',
            'vera-node = { git = "https://github.com:443/sourcenetwork/vera.rs", rev = "%s" }\n',
            'vera-node = { git = "ssh://git@github.com:22/sourcenetwork/vera.rs.git", rev = "%s" }\n',
            'vera-node = { git = "https://github.com/sourcenetwork/vera.rs?ref=main#readme", rev = "%s" }\n',
            '[dev-dependencies.vera-node]\n"git" = "http://github.com/Sourcenetwork/Vera.rs"\n"rev" = "%s"\n',
            'vera-node."git" = "https://github.com/sourcenetwork/vera.rs"\nvera-node."rev" = "%s"\n',
        ))
        for unrelated in (
            'other = { git = "https://github.com/sourcenetwork/vera.rs-tools", rev = "' + OTHER + '" }\n',
            'other = { git = "https://github.com/sourcenetwork/backbone.git", rev = "' + OTHER + '" }\n',
            'other = { git = "https://example.com/github.com.sourcenetwork/vera.rs", rev = "' + OTHER + '" }\n',
            'other = { git = "https://example.com/github.com/sourcenetwork/vera.rs", rev = "' + OTHER + '" }\n',
            'other = { git = "https://notgithub.com/sourcenetwork/vera.rs", rev = "' + OTHER + '" }\n',
            'other = { git = "https://example.com:443/github.com/sourcenetwork/vera.rs", rev = "' + OTHER + '" }\n',
            'other = { git = "https://github.com.example.org/sourcenetwork/vera.rs", branch = "main" }\n',
            'digit = "https://github.com/sourcenetwork/vera.rs"\n',
            '[package.metadata.mirror]\nupstream = { git = "' + URL + '", branch = "main" }\n',
            '[package.metadata.dependencies]\nvera-node = { git = "' + URL + '", rev = "' + OTHER + '" }\n',
        ):
            node.write_text(PINNED + unrelated)
            with self.subTest(unrelated=unrelated):
                self.assertEqual(driver.revision(self.root), PIN)

    def test_harness_pin_and_sdk_versions_are_read_whatever_their_form(self):
        self.sdk_fixture()
        sdk, harness = (self.root / name for name in driver.SDK_MANIFESTS)
        harness.write_text(SDK + 'vera-harness={git="' + URL + '",rev="' + PIN + '"}\n')
        self.assertEqual(driver.revision(self.root), PIN)
        for second in (
            'vera-node={git="' + URL + '",rev="' + OTHER + '"}\n',
            'vera-node = { git = "' + URL + '", rev = "' + PIN + '" }\n',
            '[dev-dependencies.vera-node]\ngit="' + URL + '"\nrev="' + OTHER + '"\n',
        ):
            harness.write_text(SDK + HARNESS + second)
            with self.subTest(second=second), self.assertRaises(ValueError):
                driver.revision(self.root)
        harness.write_text(SDK + HARNESS)
        sdk.write_text(SDK.replace('vera-domain = "0.1.2"', 'vera-domain="0.1.2"  # published'))
        self.assertEqual(driver.revision(self.root), PIN)
        for extra in (
            '[dev-dependencies]\nvera-client="0.1.1"\n',
            '[dev-dependencies]\n"vera-client" = "0.1.2"\n',
            '[dev-dependencies]\nvera-client = { version = "0.1.2" }\n',
            '[dev-dependencies]\nvera-client.version = "0.1.2"\n',
            '[dev-dependencies.vera-client]\nversion = "0.1.2"\n',
            "[target.'cfg(unix)'.dependencies]\nvera-client = { workspace = true }\n",
            '[workspace.dependencies]\nvera-client = "^0.1.2"\n',
            '[dev-dependencies]\nsdk = { package = "vera-client", version = "9.9.9" }\n',
            '[dev-dependencies]\nsdk = { version = "0.1.2", "package" = \'vera-domain\' }\n',
            '[dev-dependencies]\nsdk.package = "vera-modules"\nsdk.version = "9.9.9"\n',
            '[dev-dependencies.sdk]\npackage = "vera-client"\nversion = "9.9.9"\n',
        ):
            sdk.write_text(SDK + extra)
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                driver.revision(self.root)
        for unrelated in (
            'vera-client-extras = "9.9.9"\nnot-vera-client = "9.9.9"\n',
            'other = { package = "vera-client-extras", version = "9.9.9" }\n',
            '[features]\nfull = [\n    "vera-client/serde",\n]\n',
            '[features]\nfull = ["vera-client/serde"]\nvera-client = ["dep:vera-client"]\n',
            '[package.metadata.docs]\nvera-client = "9.9.9"\npackage = "vera-client"\n',
            '[package.metadata.dependencies]\nvera-client = { version = "9.9.9" }\n',
            '[dependencies.other]\nversion = "1.0.0"\nfeatures = [\n    "vera-domain",\n]\n',
        ):
            sdk.write_text(SDK + unrelated)
            with self.subTest(unrelated=unrelated):
                self.assertEqual(driver.revision(self.root), PIN)

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
        binary = target / 'defradb'
        binary.write_bytes(b'fixture')
        item = {'reason': 'compiler-artifact', 'target': {'name': 'defradb'},
                'profile': {'debug_assertions': False}, 'executable': str(binary)}
        log = self.root / 'build.log'
        log.write_text(json.dumps(item) + '\n')
        self.assertEqual(driver.artifact(log, 'defradb', target), binary.resolve())
        for entries in ([item, item], [{**item, 'profile': {'debug_assertions': True}}],
                        [{**item, 'executable': str(self.root / 'outside')}], []):
            log.write_text(''.join(json.dumps(x) + '\n' for x in entries))
            with self.assertRaises(ValueError):
                driver.artifact(log, 'defradb', target)

    def test_environment_removes_profile_compiler_runtime_and_deadline_overrides(self):
        poison = {key: 'poison' for key in ('CARGO_PROFILE_RELEASE_LTO', 'RUSTC', 'RUSTDOC',
                  'CARGO_BUILD_RUSTC', 'RUSTFLAGS', 'CARGO_TARGET_DIR', 'VERAD_BINARY',
                  'DEFRA_RUST_BINARY', 'VERA_E2E_DEADLINE_SCALE', 'ORBIS_LOCAL_STORAGE_KDF_M_COST')}
        env = driver.clean_environment({**poison, 'PATH': '/usr/bin'})
        self.assertFalse(set(poison) & set(env))
        self.assertEqual(env['PATH'], '/usr/bin')
        self.assertEqual(env['CARGO_BUILD_JOBS'], '2')

    def test_node_readiness_events_remain_enabled_with_inherited_log_filters(self):
        for inherited in ('warn', 'error', 'off', 'trace', 'defra_http=off'):
            env = driver.clean_environment({'RUST_LOG': inherited})
            self.assertEqual(env['RUST_LOG'], 'info')

    def test_numeric_summary_cannot_include_commands_or_private_output(self):
        runner = object.__new__(driver.Runner)
        runner.private = self.root
        runner.summary = self.root / 'summary.json'
        runner.sources, runner.binaries = {}, {}
        runner.sdk_revision = 'a' * 40
        runner.records = [{'phase': 'reader-update', 'command': ['private-key'], 'log': 'private-path',
                           'exit_code': 0, 'seconds': 1.5, 'passed': 1}]
        runner.save()
        self.assertEqual(json.loads(runner.summary.read_text()), {'sources': {}, 'sdk_revision': 'a' * 40, 'phases': [
            {'phase': 'reader-update', 'exit_code': 0, 'seconds': 1.5, 'passed': 1}]})

    def test_real_runner_records_failure_without_streaming_raw_log(self):
        runner = object.__new__(driver.Runner)
        runner.root = runner.private = self.root
        runner.summary = self.root / 'summary.json'
        runner.env, runner.sources, runner.binaries, runner.records = {}, {}, {}, []
        runner.sdk_revision = 'a' * 40
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
        runner.sdk_revision = 'a' * 40

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

    def test_cached_runtime_requires_exact_source_configuration_and_bytes(self):
        self.sdk_fixture()
        sources = {"revision": "a" * 40, "lock_sha256": "b" * 64}
        with patch.object(driver, 'source', return_value=sources):
            runner = driver.Runner(self.root, self.root / 'vera', self.root,
                                   cache_dir=self.root / 'cache')
            self.assertFalse(runner.restore_runtime())
            binary = runner.private / 'verad'
            binary.write_bytes(b'normal-release')
            runner.binaries['verad'] = {'sha256': driver.digest(binary)}
            runner.cache_runtime()
            binary.unlink()
            self.assertTrue(runner.restore_runtime())
            self.assertEqual(binary.read_bytes(), b'normal-release')
            manifest = runner.cache_dir / 'vera-runtime/manifest.json'
            original = json.loads(manifest.read_text())
            for key, value in [('profile', 'dev'), ('rust', 'changed'),
                               ('source', {'revision': 'c' * 40, 'lock_sha256': 'b' * 64})]:
                manifest.write_text(json.dumps({**original, key: value}))
                self.assertFalse(runner.restore_runtime())
            manifest.write_text(json.dumps(original))
            (runner.cache_dir / 'vera-runtime/verad').write_bytes(b'corrupted')
            with self.assertRaises(ValueError):
                runner.restore_runtime()

    def test_pipeline_stages_normal_binaries_before_tests_and_runs_exact_cases_once(self):
        class Fake:
            def __init__(self, root):
                self.root = self.vera = root
                self.target = root / 'target'
                self.target.mkdir()
                self.env, self.calls = {}, []
                self.cache_dir = None
            def restore_runtime(self):
                return False
            def cache_runtime(self):
                pass
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
        self.assertLess(runner.calls.index(('stage', 'defradb')), first_test)
        self.assertLess(runner.calls.index(('stage', 'verad')), first_test)
        self.assertEqual(runs['compile-native'][3]['DEFRA_RUST_BINARY'], '/staged/defradb')
        self.assertEqual(runs['compile-native'][3]['DEFRA_VERA_BINARY'], '/staged/defradb')
        self.assertEqual(runs['compile-native'][3]['VERAD_BINARY'], '/staged/verad')
        tests = [c for c in runner.calls if c[0] == 'test']
        self.assertEqual(len(tests), 18)
        self.assertEqual(len(tests[0][4]), 45)
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
