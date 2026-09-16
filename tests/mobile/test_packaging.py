import ast
import functools
import glob
import importlib.util
import os
import plistlib
import shutil
import subprocess
import tempfile
import unittest
import uuid
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

    def _load_ios_bundle_methods(self, is_macosx_return=True, xcodegen_present=True):
        # Same isolation trick as _bundle_android above: extract just the two iOS methods so
        # this test needs neither Servo's build dependencies nor a real macOS/Xcode toolchain.
        tree = ast.parse((ROOT / "python/servo/post_build_commands.py").read_text())
        methods = [
            n for n in ast.walk(tree)
            if isinstance(n, ast.FunctionDef) and n.name in ("_bundle_ios", "_sign_and_export_ios_release")
        ]
        for method in methods:
            method.decorator_list = []
        namespace = dict(
            os=os, path=os.path, shutil=shutil, subprocess=subprocess, glob=glob,
            plistlib=plistlib, uuid=uuid, importlib=importlib,
            Optional=__import__('typing').Optional,
            is_macosx=lambda: is_macosx_return,
        )
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
        namespace['delete'] = shutil.rmtree
        exec(compile(ast.Module(body=methods, type_ignores=[]), "ios_packaging", "exec"), namespace)
        return namespace

    def test_bundle_ios_refuses_without_macos_content_dir_or_xcodegen(self):
        namespace = self._load_ios_bundle_methods(is_macosx_return=False)
        owner = SimpleNamespace(get_top_dir=lambda: "/nonexistent")
        with tempfile.TemporaryDirectory() as temp:
            output = str(Path(temp) / "out")
            self.assertEqual(namespace['_bundle_ios'](owner, None, output), 1)

        namespace = self._load_ios_bundle_methods(is_macosx_return=True)
        with tempfile.TemporaryDirectory() as temp:
            content = Path(temp) / "dist"
            content.mkdir()  # no index.html
            self.assertEqual(namespace['_bundle_ios'](owner, str(content), str(Path(temp) / "out")), 1)

        with patch.object(shutil, 'which', return_value=None):
            namespace = self._load_ios_bundle_methods(is_macosx_return=True)
            with tempfile.TemporaryDirectory() as temp:
                content = Path(temp) / "dist"
                content.mkdir()
                (content / "index.html").write_text("game")
                self.assertEqual(namespace['_bundle_ios'](owner, str(content), str(Path(temp) / "out")), 1)

    def test_bundle_ios_release_refuses_without_signing_env_vars(self):
        namespace = self._load_ios_bundle_methods(is_macosx_return=True)
        owner = SimpleNamespace(get_top_dir=lambda: "/nonexistent")
        with tempfile.TemporaryDirectory() as temp:
            content = Path(temp) / "dist"
            content.mkdir()
            (content / "index.html").write_text("game")
            with patch.dict(os.environ, {}, clear=False):
                for var in ("IOS_SIGNING_CERTIFICATE_P12_PATH", "IOS_SIGNING_PROVISIONING_PROFILE_PATH", "IOS_SIGNING_TEAM_ID"):
                    os.environ.pop(var, None)
                with patch.object(subprocess, 'check_call') as check_call:
                    result = namespace['_bundle_ios'](owner, str(content), str(Path(temp) / "out"), ios_release=True)
            self.assertEqual(result, 1)
            check_call.assert_not_called()

    def test_bundle_ios_unsigned_stages_and_invokes_xcodegen_and_simulator_build(self):
        namespace = self._load_ios_bundle_methods(is_macosx_return=True)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            shutil.copytree(ROOT / "support/ios", root / "support/ios")
            shutil.copytree(ROOT / "resources", root / "resources")
            owner = SimpleNamespace(get_top_dir=lambda: str(root))
            content, output = root / "dist", root / "output"
            content.mkdir(); output.mkdir()
            (content / "index.html").write_text("game")

            calls = []

            def build(argv, **kwargs):
                calls.append(argv)
                if argv[:2] == ["xcodegen", "generate"]:
                    return
                if argv[0] == "xcodebuild":
                    app_dir = Path("build/Build/Products/Debug-iphonesimulator/RovesGame.app")
                    app_dir.mkdir(parents=True)
                    (app_dir / "RovesGame").write_bytes(b"app-binary")

            with patch.object(subprocess, 'check_call', side_effect=build), \
                 patch.object(shutil, 'which', return_value='/usr/bin/xcodegen'):
                result = namespace['_bundle_ios'](owner, str(content), str(output), ios_app_name="Test Game")

            self.assertIsNone(result)
            self.assertTrue((output / "RovesGame.app" / "RovesGame").is_file())
            self.assertTrue(any(c[:2] == ["xcodegen", "generate"] for c in calls))
            self.assertTrue(any(c[0] == "xcodebuild" and "iphonesimulator" in c for c in calls))
            self.assertTrue(any("CODE_SIGNING_ALLOWED=NO" in c for c in calls))

    def test_bundle_ios_release_signs_and_exports_then_always_deletes_keychain(self):
        namespace = self._load_ios_bundle_methods(is_macosx_return=True)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            shutil.copytree(ROOT / "support/ios", root / "support/ios")
            shutil.copytree(ROOT / "resources", root / "resources")
            owner = SimpleNamespace(get_top_dir=lambda: str(root))
            # _bundle_ios calls self._sign_and_export_ios_release(...) for --ios-release --
            # bind the sibling extracted method onto the same fake `self` so that resolves.
            owner._sign_and_export_ios_release = functools.partial(namespace['_sign_and_export_ios_release'], owner)
            content, output = root / "dist", root / "output"
            content.mkdir(); output.mkdir()
            (content / "index.html").write_text("game")

            cert = root / "cert.p12"
            cert.write_bytes(b"fake-p12")
            profile = root / "profile.mobileprovision"
            profile.write_bytes(b"fake-profile")

            calls = []

            def check_call(argv, **kwargs):
                calls.append(argv)
                if argv[0] == "xcodebuild" and "-exportArchive" in argv:
                    export_path = argv[argv.index("-exportPath") + 1]
                    Path(export_path, "RovesGame.ipa").write_bytes(b"signed-ipa")

            def check_output(argv, **kwargs):
                if argv[:3] == ["security", "cms", "-D"]:
                    return plistlib.dumps({"UUID": "FAKE-PROFILE-UUID"})
                if argv[:3] == ["security", "list-keychains", "-d"]:
                    return "/some/existing.keychain\n"
                raise AssertionError(f"unexpected check_output call: {argv}")

            fake_home = root / "home"
            fake_home.mkdir()
            with patch.object(subprocess, 'check_call', side_effect=check_call), \
                 patch.object(subprocess, 'check_output', side_effect=check_output), \
                 patch.object(subprocess, 'call') as fake_call, \
                 patch.object(shutil, 'which', return_value='/usr/bin/xcodegen'), \
                 patch.dict(os.environ, {
                     "IOS_SIGNING_CERTIFICATE_P12_PATH": str(cert),
                     "IOS_SIGNING_CERTIFICATE_P12_PASSWORD": "pw",
                     "IOS_SIGNING_PROVISIONING_PROFILE_PATH": str(profile),
                     "IOS_SIGNING_TEAM_ID": "TEAMID1234",
                 }), \
                 patch.object(os.path, 'expanduser', return_value=str(fake_home / "Provisioning Profiles")):
                result = namespace['_bundle_ios'](owner, str(content), str(output), ios_release=True)

            self.assertIsNone(result)
            self.assertEqual((output / "RovesGame.ipa").read_bytes(), b"signed-ipa")
            self.assertTrue(any(c[:2] == ["security", "create-keychain"] for c in calls))
            self.assertTrue(any(c[:2] == ["security", "import"] and str(cert) in c for c in calls))
            self.assertTrue(any(c[0] == "xcodebuild" and "archive" in c and "DEVELOPMENT_TEAM=TEAMID1234" in c for c in calls))
            self.assertTrue(any(c[0] == "xcodebuild" and "-exportArchive" in c for c in calls))
            # The ephemeral signing keychain is always torn down, success or failure.
            self.assertTrue(fake_call.called)
            self.assertEqual(fake_call.call_args[0][0][:2], ["security", "delete-keychain"])
            self.assertTrue((fake_home / "Provisioning Profiles" / "FAKE-PROFILE-UUID.mobileprovision").is_file())


if __name__ == '__main__':
    unittest.main()
