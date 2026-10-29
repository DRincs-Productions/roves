"""Check actual DLL staging and its fail-closed CLI, without loading native libraries."""
from contextlib import redirect_stdout
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HELPER = Path(__file__).resolve().parents[2] / "python/servo/angle_runtime.py"
spec = importlib.util.spec_from_file_location("angle_runtime", HELPER)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class AngleRuntimeTests(unittest.TestCase):
    def test_stages_both_built_libraries_without_moving_them(self):
        with tempfile.TemporaryDirectory(prefix="angle runtime ") as directory:
            root = Path(directory)
            built = root / "build/angle/out"
            built.mkdir(parents=True)
            for name in ("libEGL.dll", "libGLESv2.dll"):
                (built / name).write_bytes(name.encode())
            with redirect_stdout(io.StringIO()):
                self.assertTrue(module.copy_built_angle_dlls(root))
            for name in ("libEGL.dll", "libGLESv2.dll"):
                self.assertEqual((root / name).read_bytes(), (built / name).read_bytes())
            result = subprocess.run([sys.executable, str(HELPER), str(root)], capture_output=True)
            self.assertEqual(0, result.returncode, result.stderr)

    def test_missing_either_library_fails_the_cli(self):
        for missing in ("libEGL.dll", "libGLESv2.dll"):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                built = root / "build/angle/out"
                built.mkdir(parents=True)
                for name in ("libEGL.dll", "libGLESv2.dll"):
                    if name != missing: (built / name).write_bytes(b"library")
                result = subprocess.run([sys.executable, str(HELPER), str(root)], capture_output=True, text=True)
                self.assertEqual(1, result.returncode)
                self.assertIn(missing, result.stdout)

    def test_absent_build_directory_is_not_success(self):
        with tempfile.TemporaryDirectory() as directory, redirect_stdout(io.StringIO()):
            self.assertFalse(module.copy_built_angle_dlls(directory))
