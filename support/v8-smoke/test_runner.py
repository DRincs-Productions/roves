"""Regression tests for the smoke protocol and the complete shell runner."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

from check_results import validate

HERE = Path(__file__).resolve().parent


class ResultsTests(unittest.TestCase):
    def test_complete(self):
        self.assertEqual([], validate('suite.html', 'noise\nSUITE PASS example\nSUITE TOTAL 1/1\n'))

    def test_rejects_false_green(self):
        for output in (
            '',
            'SUITE PASS example\n',
            'SUITE FAIL example :: false\nSUITE TOTAL 0/1\n',
            'SUITE FAIL example :: false\nSUITE TOTAL 1/1\n',
            'SUITE TOTAL 0/0\n',
            'SUITE PASS a\nSUITE TOTAL 2/2\n',
            'SUITE PASS a\nSUITE TOTAL 1/1\nSUITE TOTAL 1/1\n',
            'SUITE PASS a\nSUITE TOTAL 1/1\nSUITE PASS b\n',
            'SUITE PASS a\nSUITE PASS a\nSUITE TOTAL 2/2\n',
            'SUITE PASS a\nSUITE TOTAL broken\n',
            'SUITE BOGUS a\nSUITE TOTAL 0/0\n',
        ):
            with self.subTest(output=output):
                self.assertTrue(validate('suite.html', output))

    def test_skip_policy(self):
        output = 'SUITE PASS base\nSUITE SKIP WebAssembly.instantiate (async compile)\nSUITE TOTAL 1/1 (1 skipped)\n'
        self.assertTrue(validate('suite2.html', output))
        self.assertEqual([], validate('suite2.html', output, 'jitless'))
        self.assertTrue(validate('other.html', output, 'jitless'))
        output = 'SUITE PASS base\nSUITE SKIP WebGPU adapter request (when available)\nSUITE TOTAL 1/1 (1 skipped)\n'
        self.assertTrue(validate('graphics.html', output))
        self.assertEqual([], validate('graphics.html', output, allow_missing_devices=True))
        self.assertTrue(validate('saves.html', 'SUITE SKIP saves: protocol is available\nSUITE TOTAL 0/0 (1 skipped)'))

    def test_jitless_cannot_silently_enable_wasm(self):
        output = 'SUITE PASS WebAssembly.instantiate (async compile)\nSUITE TOTAL 1/1'
        self.assertTrue(validate('suite2.html', output, 'jitless'))
        self.assertEqual([], validate('suite2.html', output))


@unittest.skipUnless(os.name == 'posix' and shutil.which('bash'), 'shell integration needs POSIX bash')
class RunnerTests(unittest.TestCase):
    def run_fixture(self, scenario, wrapped=False, pref_flag=None, gc_flag=None):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            runner = HERE / 'run.sh'
            if pref_flag is not None or gc_flag is not None:
                # Simulate a runner regression that drops or changes a preference flag.
                copy = directory / 'smoke'
                copy.mkdir()
                shutil.copy(HERE / 'check_results.py', copy)
                source = runner.read_text()
                for original, replacement in ((" --pref js_disable_jit=true\n", pref_flag),
                                              (" --pref dom_testutils_enabled=true\n", gc_flag)):
                    self.assertEqual(1, source.count(original))
                    if replacement is not None:
                        source = source.replace(original, replacement + "\n")
                (copy / 'run.sh').write_text(source)
                runner = copy / 'run.sh'
            python = shutil.which('python3') or sys.executable
            python_stub = directory / 'python3'
            python_stub.write_text(f'''#!{sys.executable}
import os, sys, time
args = sys.argv[1:]
if args[:2] == ['-m', 'http.server']:
    time.sleep(120)
elif args[:1] == ['-c']:
    raise SystemExit(0)
else:
    os.execv(os.environ['SMOKE_TEST_REAL_PYTHON'], [os.environ['SMOKE_TEST_REAL_PYTHON'], *args])
''')
            python_stub.chmod(0o755)
            # Stub browser launches and HTTP setup; execute the real runner and validator.
            launcher = directory / 'timeout'
            launcher.write_text('''#!/usr/bin/env python3
import os, sys
if os.environ['SMOKE_TEST_WRAPPED'] == '1':
    assert sys.argv[2:4] == ['wrapper with spaces', '--option'], sys.argv
    browser = 4
else:
    browser = 2
assert sys.argv[browser] == 'unused-browser', sys.argv
assert sys.argv[-2] == '-z', sys.argv
options = sys.argv[browser + 1:-2]
page = sys.argv[-1].rsplit('/', 1)[-1]
scenario = os.environ['SMOKE_TEST_SCENARIO']
if scenario == 'crash' and page == 'suite.html':
    raise SystemExit(139)
if page not in ('pref-jitless.html', 'gc-lifecycle.html'):
    # Only the dedicated fresh processes may change a preference.
    assert options == [], sys.argv
prefs = {}
if page in ('pref-jitless.html', 'gc-lifecycle.html'):
    with open(('pref' if page == 'pref-jitless.html' else 'gc') + '-args.txt', 'w') as received:
        received.write('\\n'.join(sys.argv[2:browser + 1] + options))
    # Like servoshell: each `--pref name[=value]` must name a real preference.
    for flag, value in zip(options[::2], options[1::2]):
        assert flag == '--pref', sys.argv
        name, _, setting = value.partition('=')
        if name not in ('js_disable_jit', 'dom_testutils_enabled'):
            print(f'panicked: Unknown preference: "{name}"', file=sys.stderr)
            raise SystemExit(101)
        prefs[name] = setting in ('', 'true')
if page == 'gc-lifecycle.html':
    if scenario == 'gc-crash':
        raise SystemExit(139)
    # Without the preference the TestUtils namespace is not exposed.
    api = prefs.get('dom_testutils_enabled', False) and scenario != 'gc-api'
    print('ROVES-V8 gc api ' + str(api).lower())
    if not api:
        print('ROVES-V8 gc failure TestUtils.gc unavailable')
        print('ROVES-V8 gc result fail TestUtils.gc unavailable')
    elif scenario == 'gc-retention':
        print('ROVES-V8 gc unreachable dom collected false 12/240 alive after 8 collections')
        print('ROVES-V8 gc result fail unreachable dom collected')
    elif scenario != 'gc-result':
        for line in ('unreachable dom collected true', 'unreachable cycles collected true',
                     'retained nodes true true true', 'importer compiled true',
                     'dynamic import after collection 42', 'dropped importer collected true',
                     'module owners left right', 'module owners after collection left right',
                     'eval module owners left right', 'eval module owners after collection left right',
                     'retained nodes final true true true', 'result pass'):
            if scenario == 'gc-import' and line.startswith('dynamic import'):
                line = 'dynamic import after collection undefined'
            if scenario == 'gc-import' and line == 'result pass':
                line = 'result fail dynamic import'
            # A module import resolved through the other frame's module owner.
            if scenario == 'gc-owner' and line.startswith('module owners'):
                line = line.replace('left right', 'right right')
            if scenario == 'gc-owner' and line == 'result pass':
                line = 'result fail module owners; module owners after collection'
            # A page that claims success without reporting the owner check.
            if scenario == 'gc-owner-missing' and line.startswith('module owners'):
                continue
            # Eval code whose import() took the other frame's module owner; the page claims success.
            if scenario == 'gc-eval-owner' and line.startswith('eval module owners'):
                line = line.replace('left right', 'left left')
            if scenario == 'gc-eval-owner-missing' and line.startswith('eval module owners'):
                continue
            print('ROVES-V8 gc ' + line)
elif page == 'pref-jitless.html':
    if scenario == 'pref-cli' and '--pref' in options:
        print('error: --pref is not expected in this context', file=sys.stderr)
        raise SystemExit(1)
    jitless = prefs.get('js_disable_jit', False)
    if scenario == 'pref-crash':
        raise SystemExit(139)
    if scenario == 'pref-wasm':
        jitless = False
    if scenario != 'pref-result':
        print('ROVES-V8 pref js_disable_jit js true wasm ' + ('unavailable' if jitless else 'available'))
elif page == 'test.html':
    print('ROVES-V8 dom: changed by js v8 test 1')
    print('ROVES-V8 click event click true')
    print('ROVES-V8 promise 5')
    print('ROVES-V8 timeout fired')
    if scenario != 'window': print('ROVES-V8 class 2 {"a":[1,2]} true true')
    print('ROVES-V8 testutils default ' + ('object' if scenario == 'testutils-default' else 'undefined'))
    if scenario != 'immutable-prototype': print('ROVES-V8 ordinary prototype mutation true')
    if scenario != 'window-prototype': print('ROVES-V8 immutable Window prototype true true true true')
    if scenario != 'cross-origin-prototype': print('ROVES-V8 cross-origin WindowProxy prototype true true')
    if scenario != 'cross-origin-properties': print('ROVES-V8 cross-origin WindowProxy properties true true')
    if scenario == 'cross-origin-timeout': print('ROVES-V8 cross-origin iframe timeout true')
elif page == 'csp.html':
    if scenario != 'csp': print('ROVES-V8 csp eval EvalError')
    print('ROVES-V8 csp function EvalError')
    if scenario != 'trusted-script': print('ROVES-V8 csp trusted eval 42')
    if scenario != 'trusted-function': print('ROVES-V8 csp trusted function 42')
    if scenario != 'trusted-function-guards': print('ROVES-V8 csp trusted function guards true')
    if scenario != 'trusted-function-matrix': print('ROVES-V8 csp trusted function matrix true true true')
    if scenario != 'constructor-graph': print('ROVES-V8 csp constructor graph true true true')
    print('ROVES-V8 csp ordinary eval unchanged true')
elif page == 'wasm-csp.html':
    if scenario != 'csp-wasm': print('ROVES-V8 csp wasm blocked CompileError')
elif page == 'nav1.html':
    print('ROVES-V8 nav2 loaded nav2.html true second page')
    if scenario != 'navigation': print('ROVES-V8 nav1 pageshow persisted=true')
else:
    if page == 'suite.html' and scenario == 'assertion':
        print('SUITE FAIL example :: intentionally false')
        print('SUITE TOTAL 0/1')
    elif page == 'suite.html' and scenario == 'incomplete':
        print('SUITE PASS example')
    elif page == 'suite.html' and scenario == 'skip':
        print('SUITE SKIP unexpected capability')
        print('SUITE TOTAL 0/0 (1 skipped)')
    else:
        print('SUITE PASS example')
        print('SUITE TOTAL 1/1')
# Browsers may stay open after reporting all results; that timeout is acceptable.
if scenario == 'linger' and page.endswith('.html') and page not in ('test.html', 'csp.html', 'wasm-csp.html', 'nav1.html'):
    sys.stdout.flush()
    import time
    time.sleep(120)
raise SystemExit(124 if scenario == 'timeout' or scenario == 'incomplete' else 0)
''')
            launcher.chmod(0o755)
            env = dict(os.environ, PATH=str(directory) + os.pathsep + os.environ['PATH'],
                       SMOKE_TEST_SCENARIO=scenario, SMOKE_TEST_WRAPPED=str(int(wrapped)), V8_SMOKE_MODE='normal',
                       V8_SMOKE_ALLOW_MISSING_DEVICES='0', PYTHON3=str(python_stub),
                       SMOKE_TEST_REAL_PYTHON=python)
            env.pop('GITHUB_STEP_SUMMARY', None)
            result = subprocess.run(['bash', str(runner), 'unused-browser',
                                     *(['wrapper with spaces', '--option'] if wrapped else [])],
                                    cwd=directory, env=env, capture_output=True, text=True, timeout=20)
            received = directory / 'pref-args.txt'
            result.pref_args = received.read_text().split('\n') if received.exists() else None
            received = directory / 'gc-args.txt'
            result.gc_args = received.read_text().split('\n') if received.exists() else None
            return result

    def test_complete_and_post_completion_timeout(self):
        for wrapped in (False, True):
            for scenario in ('pass', 'timeout'):
                with self.subTest(scenario=scenario, wrapped=wrapped):
                    result = self.run_fixture(scenario, wrapped=wrapped)
                    self.assertEqual(0, result.returncode, result.stdout + result.stderr)
                    wrapper = ['wrapper with spaces', '--option'] if wrapped else []
                    self.assertEqual(wrapper + ['unused-browser', '--pref', 'js_disable_jit=true'], result.pref_args)
                    self.assertIn('::notice::pref-jitless.html: --pref js_disable_jit=true started V8 without WebAssembly', result.stdout)
                    self.assertEqual(wrapper + ['unused-browser', '--pref', 'dom_testutils_enabled=true'], result.gc_args)
                    self.assertIn('::notice::gc-lifecycle.html: ROVES-V8 gc result pass', result.stdout)
                    self.assertIn('::notice::test.html: TestUtils hidden by default', result.stdout)

    def test_gc_lifecycle_failures(self):
        cases = (
            ('pass', '', "missing 'ROVES-V8 gc api true' (ROVES-V8 gc api false"),
            ('pass', ' --pref dom_testutils_enabled=false', "missing 'ROVES-V8 gc api true' (ROVES-V8 gc api false"),
            ('pass', ' --pref dom_testutils=true', 'gc-lifecycle.html: servoshell exited with 101'),
            ('gc-api', None, "missing 'ROVES-V8 gc api true'"),
            ('gc-retention', None, "missing 'ROVES-V8 gc unreachable dom collected true' (ROVES-V8 gc api true ROVES-V8 gc unreachable dom collected false 12/240"),
            ('gc-import', None, "missing 'ROVES-V8 gc dynamic import after collection 42'"),
            ('gc-owner', None, "missing 'ROVES-V8 gc module owners left right' (ROVES-V8 gc api true"),
            ('gc-owner-missing', None, "missing 'ROVES-V8 gc module owners after collection left right'"),
            ('gc-eval-owner', None, "missing 'ROVES-V8 gc eval module owners left right' (ROVES-V8 gc api true"),
            ('gc-eval-owner-missing', None, "missing 'ROVES-V8 gc eval module owners after collection left right'"),
            ('gc-result', None, "missing 'ROVES-V8 gc result pass'"),
            ('gc-crash', None, 'gc-lifecycle.html: servoshell exited with 139'),
        )
        for scenario, gc_flag, message in cases:
            with self.subTest(scenario=scenario, gc_flag=gc_flag):
                result = self.run_fixture(scenario, gc_flag=gc_flag)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                if scenario in ('gc-owner-missing', 'gc-eval-owner', 'gc-eval-owner-missing'):
                    # A reported pass cannot replace a missing or wrong required marker.
                    self.assertIn('::notice::gc-lifecycle.html: ROVES-V8 gc result pass', result.stdout)
                else:
                    self.assertIn("::error::gc-lifecycle.html: missing 'ROVES-V8 gc result pass'", result.stdout)
                self.assertIn(message, result.stdout)
                self.assertIn('V8 smoke diagnostics: gc-lifecycle.log', result.stdout)
                # The other preference process keeps its exact flag.
                self.assertEqual(['unused-browser', '--pref', 'js_disable_jit=true'], result.pref_args)

    def test_preference_probe_failures(self):
        cases = (
            ('pass', '', 'wasm available'),
            ('pass', ' --pref js_disable_jit=false', 'wasm available'),
            ('pref-wasm', None, 'wasm available'),
            ('pref-result', None, 'not confirmed ()'),
            ('pref-crash', None, 'pref-jitless.html: servoshell exited with 139'),
            ('pref-cli', None, 'pref-jitless.html: servoshell exited with 1'),
        )
        for scenario, pref_flag, message in cases:
            with self.subTest(scenario=scenario, pref_flag=pref_flag):
                result = self.run_fixture(scenario, pref_flag=pref_flag)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                self.assertIn('::error::pref-jitless.html: --pref js_disable_jit=true not confirmed', result.stdout)
                self.assertIn(message, result.stdout)
                self.assertIn('V8 smoke diagnostics: pref-jitless.log', result.stdout)

    def test_failure_paths(self):
        for scenario in ('assertion', 'incomplete', 'crash', 'window', 'immutable-prototype', 'window-prototype', 'cross-origin-prototype', 'cross-origin-properties', 'cross-origin-timeout', 'csp', 'trusted-script', 'trusted-function', 'trusted-function-guards', 'trusted-function-matrix', 'constructor-graph', 'csp-wasm', 'navigation', 'skip', 'testutils-default'):
            with self.subTest(scenario=scenario):
                result = self.run_fixture(scenario)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                self.assertIn('::error', result.stdout)
                if scenario == 'immutable-prototype':
                    self.assertIn('ordinary prototype mutation failed', result.stdout)
                if scenario == 'window-prototype':
                    self.assertIn('immutable Window prototype not confirmed', result.stdout)
                if scenario == 'cross-origin-prototype':
                    self.assertIn('cross-origin WindowProxy prototype not confirmed', result.stdout)
                if scenario == 'cross-origin-properties':
                    self.assertIn('cross-origin WindowProxy properties not confirmed', result.stdout)
                if scenario == 'constructor-graph':
                    self.assertIn("missing 'ROVES-V8 csp constructor graph true true true'", result.stdout)
                if scenario == 'testutils-default':
                    self.assertIn('TestUtils exposed without dom_testutils_enabled (ROVES-V8 testutils default object', result.stdout)

    def test_completed_suites_do_not_wait_for_the_deadline(self):
        started = time.monotonic()
        result = self.run_fixture('linger')
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        self.assertLess(time.monotonic() - started, 15)


if __name__ == '__main__':
    unittest.main()
