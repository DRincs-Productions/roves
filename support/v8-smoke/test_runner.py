"""Regression tests for the smoke protocol and the complete shell runner."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
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
            # Substitute only the process launcher; execute the real runner and validator.
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
    if scenario != 'immutable-prototype': print('ROVES-V8 immutable window prototype true')
elif page == 'csp.html':
    if scenario != 'csp': print('ROVES-V8 csp eval EvalError')
    print('ROVES-V8 csp function EvalError')
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
raise SystemExit(124 if scenario == 'timeout' or scenario == 'incomplete' else 0)
''')
            launcher.chmod(0o755)
            env = dict(os.environ, PATH=str(directory) + os.pathsep + os.environ['PATH'],
                       SMOKE_TEST_SCENARIO=scenario, SMOKE_TEST_WRAPPED=str(int(wrapped)), V8_SMOKE_MODE='normal',
                       V8_SMOKE_ALLOW_MISSING_DEVICES='0')
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
        for scenario in ('assertion', 'incomplete', 'crash', 'window', 'csp', 'navigation', 'skip'):
            with self.subTest(scenario=scenario):
                result = self.run_fixture(scenario)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                self.assertIn('::error', result.stdout)


if __name__ == '__main__':
    unittest.main()
