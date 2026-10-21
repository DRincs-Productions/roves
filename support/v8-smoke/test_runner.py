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
    def run_fixture(self, scenario, wrapped=False):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
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
else:
    assert sys.argv[2] == 'unused-browser', sys.argv
page = sys.argv[-1].rsplit('/', 1)[-1]
scenario = os.environ['SMOKE_TEST_SCENARIO']
if scenario == 'crash' and page == 'suite.html':
    raise SystemExit(139)
if page == 'test.html':
    print('ROVES-V8 dom: changed by js v8 test 1')
    print('ROVES-V8 click event click true')
    print('ROVES-V8 promise 5')
    print('ROVES-V8 timeout fired')
    if scenario != 'window': print('ROVES-V8 class 2 {"a":[1,2]} true true')
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
            return subprocess.run(['bash', str(HERE / 'run.sh'), 'unused-browser',
                                   *(['wrapper with spaces', '--option'] if wrapped else [])],
                                  cwd=directory, env=env, capture_output=True, text=True, timeout=20)

    def test_complete_and_post_completion_timeout(self):
        for wrapped in (False, True):
            for scenario in ('pass', 'timeout'):
                with self.subTest(scenario=scenario, wrapped=wrapped):
                    result = self.run_fixture(scenario, wrapped=wrapped)
                    self.assertEqual(0, result.returncode, result.stdout + result.stderr)

    def test_failure_paths(self):
        for scenario in ('assertion', 'incomplete', 'crash', 'window', 'immutable-prototype', 'window-prototype', 'cross-origin-prototype', 'cross-origin-properties', 'cross-origin-timeout', 'csp', 'trusted-script', 'trusted-function', 'trusted-function-guards', 'trusted-function-matrix', 'csp-wasm', 'navigation', 'skip'):
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

    def test_completed_suites_do_not_wait_for_the_deadline(self):
        started = time.monotonic()
        result = self.run_fixture('linger')
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        self.assertLess(time.monotonic() - started, 15)


if __name__ == '__main__':
    unittest.main()
