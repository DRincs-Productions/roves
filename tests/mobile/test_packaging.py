import ast
import glob
import importlib.util
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ios_bundle", ROOT / "support/ios/bundle.py")
ios_bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ios_bundle)


class PackagingTests(unittest.TestCase):
    def test_ios_stages_content_and_rejects_overlap_and_existing_output(self):
        with tempfile.TemporaryDirectory() as temp:
            content = Path(temp) / "dist"
            content.mkdir()
            (content / "index.html").write_text("game")
            output = Path(temp) / "ios"
            ios_bundle.stage(content, output, "Game")
            self.assertEqual((output / "www/index.html").read_text(), "game")
            self.assertTrue((output / "App.swift").is_file())
            self.assertTrue((output / "project.json").is_file())
            self.assertTrue((output / "roves-brand/servo_1024.png").is_file())
            self.assertTrue((output / "roves-brand/MetalMania-Regular.ttf").is_file())
            for invalid in (output, content, content / "build", content.parent):
                with self.assertRaises(ValueError):
                    ios_bundle.stage(content, invalid)

    def test_android_bundles_without_native_library_and_ignores_stale_apks(self):
        # Load only the packaging method so the test does not require Servo's build dependencies.
        tree = ast.parse((ROOT / "python/servo/post_build_commands.py").read_text())
        method = next(n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef) and n.name == "_bundle_android")
        method.decorator_list = []
        namespace = dict(glob=glob, os=os, path=os.path, shutil=shutil, subprocess=subprocess,
                         Optional=__import__('typing').Optional, _read_web_manifest=lambda _: {},
                         _ANDROID_ORIENTATION_MAP={}, delete=shutil.rmtree)
        from contextlib import contextmanager
        @contextmanager
        def cd(directory):
            old = os.getcwd()
            os.chdir(directory)
            try:
                yield
            finally:
                os.chdir(old)
        namespace['cd'] = cd
        exec(compile(ast.Module(body=[method], type_ignores=[]), "packaging", "exec"), namespace)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            shutil.copytree(ROOT / "support/android/apk", root / "support/android/apk")
            content, output = root / "dist", root / "output"
            content.mkdir(); output.mkdir()
            (content / "index.html").write_text("game")
            triple = "aarch64-linux-android"
            stale = root / "target" / triple / "old/servoapp.apk"
            stale.parent.mkdir(parents=True); stale.write_bytes(b"stale")
            target = SimpleNamespace(triple=lambda: triple)
            owner = SimpleNamespace(target=target, get_top_dir=lambda: str(root), config={"android": {}})
            def build(argv, env):
                self.assertNotIn("SERVO_TARGET_DIR", env)
                self.assertTrue(Path("servoapp/src/main/assets/www/index.html").is_file())
                apk = Path("servoapp/build/outputs/apk/arm64Debug/game.apk")
                apk.parent.mkdir(parents=True); apk.write_bytes(b"native-webview")
            with patch.object(subprocess, 'check_call', side_effect=build):
                namespace['_bundle_android'](owner, None, str(content), str(output))
            self.assertEqual((output / "servoapp.apk").read_bytes(), b"native-webview")


if __name__ == '__main__':
    unittest.main()
