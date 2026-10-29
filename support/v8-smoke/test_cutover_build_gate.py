"""A failed Cargo build must stop CI before a cached browser can be tested."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


class CutoverBuildGateTests(unittest.TestCase):
    def test_build_status_and_manifest_restoration(self):
        source = Path(__file__).resolve().parents[1] / "v8_cutover_check.py"
        spec = importlib.util.spec_from_file_location("cutover_gate", source)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        cases = [(True, 101, "file.rs:1: error: controlled failure", 101),
                 (True, 101, "dependency download failed", 101),
                 (True, 0, "", 0), (False, 101, "file.rs:1: error: controlled failure", 0)]
        for build, status, diagnostic, expected in cases:
            with self.subTest(build=build, status=status, diagnostic=diagnostic), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                manifest = b'[workspace.dependencies]\njs = { package = "mozjs", version = "0.0.0" }\n'
                lock = b'original lock contents\n'
                (root / "Cargo.toml").write_bytes(manifest)
                (root / "Cargo.lock").write_bytes(lock)

                def cargo(command, **kwargs):
                    self.assertIn("build" if build else "check", command)
                    self.assertIn(module.ROVES_JS_LINE, (root / "Cargo.toml").read_text())
                    return subprocess.CompletedProcess(command, status, "", diagnostic)

                args = ["cutover", "servoshell"] + (["--build"] if build else [])
                with patch.object(module, "ROOT", root), patch.object(sys, "argv", args), \
                        patch.object(module.subprocess, "run", side_effect=cargo), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(expected, module.main())
                self.assertEqual(manifest, (root / "Cargo.toml").read_bytes())
                self.assertEqual(lock, (root / "Cargo.lock").read_bytes())
                self.assertEqual(diagnostic, (root / "target/v8-cutover/servoshell.log").read_text())


if __name__ == "__main__":
    unittest.main()
