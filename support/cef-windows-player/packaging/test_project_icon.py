"""Contract tests of project_icon.py: project, fallback and malformed icons.

Run: python -I support/cef-windows-player/packaging/test_project_icon.py (Pillow required).
"""

import contextlib
import io
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from PIL import Image  # noqa: E402

import project_icon  # noqa: E402


def png(path, edge, height=None, color=(200, 30, 30, 255)):
    Image.new("RGBA", (edge, height or edge), color).save(path, format="PNG")
    return path


def ico(path, edge, color=(30, 30, 200, 255)):
    Image.new("RGBA", (edge, edge), color).save(path, format="ICO", sizes=[(16, 16), (edge, edge)])
    return path


class ProjectIconTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.content = os.path.join(self.root, "dist")
        os.makedirs(self.content)
        self.fallback = ico(os.path.join(self.root, "roves.ico"), 256, (20, 20, 24, 255))
        self.runs = 0

    def tearDown(self):
        self.tmp.cleanup()

    def run_contract(self, icon_png="", icon_ico="", fallback=None):
        self.runs += 1
        out = os.path.join(self.root, f"out{self.runs}")
        log = io.StringIO()
        with contextlib.redirect_stdout(log):
            code = project_icon.main(
                ["--content", self.content, "--out", out, "--icon-png", icon_png, "--icon-ico", icon_ico,
                 "--fallback-ico", self.fallback if fallback is None else fallback]
            )
        result = {}
        env = os.path.join(out, "result.env")
        if os.path.exists(env):
            with open(env, encoding="utf-8") as f:
                result = dict(line.rstrip("\n").split("=", 1) for line in f)
        return code, out, result, log.getvalue()

    def pixel(self, path, size=None):
        with Image.open(path) as im:
            if size:
                im.size = size
            return im.convert("RGBA").getpixel((0, 0))

    def test_game_icon_png_is_used_for_window_and_play_exe(self):
        png(os.path.join(self.content, "icon.png"), 512)
        code, out, result, _ = self.run_contract()
        self.assertEqual(code, 0)
        self.assertEqual((result["window_source"], result["exe_source"]), ("project", "project"))
        self.assertEqual(self.pixel(os.path.join(out, "icon.png")), (200, 30, 30, 255))
        self.assertEqual(self.pixel(os.path.join(out, "play.ico"), (256, 256)), (200, 30, 30, 255))
        self.assertEqual(project_icon.check_ico(os.path.join(out, "play.ico")), 256)

    def test_no_project_icon_falls_back_to_roves(self):
        code, out, result, _ = self.run_contract()
        self.assertEqual(code, 0)
        self.assertEqual((result["window_source"], result["exe_source"]), ("roves", "roves"))
        self.assertFalse(os.path.exists(os.path.join(out, "icon.png")))
        self.assertEqual(self.pixel(os.path.join(out, "play.ico"), (256, 256)), (20, 20, 24, 255))

    def test_old_base_without_fallback_leaves_play_exe_unchanged(self):
        code, out, result, _ = self.run_contract(fallback="")
        self.assertEqual((code, result["exe_source"]), (0, "unchanged"))
        self.assertFalse(os.path.exists(os.path.join(out, "play.ico")))

    def test_favicon_only_game_gets_its_icon_everywhere(self):
        ico(os.path.join(self.content, "favicon.ico"), 48)
        code, out, result, _ = self.run_contract()
        self.assertEqual(code, 0)
        self.assertEqual(result["exe_detail"], "game favicon.ico")
        self.assertEqual(result["window_source"], "project")
        with Image.open(os.path.join(out, "icon.png")) as im:
            self.assertEqual(im.size, (48, 48))

    def test_explicit_overrides_win_over_the_game_files(self):
        png(os.path.join(self.content, "icon.png"), 256)
        ico(os.path.join(self.content, "favicon.ico"), 32)
        own_png = png(os.path.join(self.root, "own.png"), 128, color=(10, 200, 10, 255))
        own_ico = ico(os.path.join(self.root, "own.ico"), 64, (10, 10, 10, 255))
        code, out, result, _ = self.run_contract(icon_png=own_png, icon_ico=own_ico)
        self.assertEqual(code, 0)
        self.assertEqual((result["window_detail"], result["exe_detail"]), ("icon-png 128x128", "icon-ico"))
        self.assertEqual(self.pixel(os.path.join(out, "icon.png")), (10, 200, 10, 255))
        with open(own_ico, "rb") as a, open(os.path.join(out, "play.ico"), "rb") as b:
            self.assertEqual(a.read(), b.read())

    def test_malformed_game_files_warn_and_fall_back(self):
        with open(os.path.join(self.content, "icon.png"), "wb") as f:
            f.write(b"<html>not an image</html>")
        with open(os.path.join(self.content, "favicon.ico"), "wb") as f:
            f.write(b"\x00\x00\x01\x00\x01\x00" + b"\xff" * 30)
        code, out, result, log = self.run_contract()
        self.assertEqual(code, 0)
        self.assertEqual((result["window_source"], result["exe_source"]), ("roves", "roves"))
        self.assertIn("::warning::project icon: ignoring game icon.png", log)
        self.assertIn("::warning::project icon: ignoring game favicon.ico", log)

    def test_truncated_and_out_of_bounds_game_pngs_are_rejected(self):
        good = png(os.path.join(self.root, "good.png"), 256)
        with open(good, "rb") as f:
            data = f.read()
        cases = {
            "truncated": data[: len(data) // 2],
            "non-square": None,
            "tiny": None,
            "huge": None,
        }
        for name in cases:
            path = os.path.join(self.root, f"{name}.png")
            if name == "truncated":
                with open(path, "wb") as f:
                    f.write(cases[name])
            elif name == "non-square":
                png(path, 256, 128)
            elif name == "tiny":
                png(path, 8)
            else:
                png(path, 4096)
            with self.assertRaises(project_icon.IconError, msg=name):
                project_icon.check_png(path)

    def test_malformed_explicit_icons_are_errors(self):
        bad = os.path.join(self.root, "bad.png")
        with open(bad, "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n" + b"\x00" * 40)
        code, _, result, log = self.run_contract(icon_png=bad)
        self.assertEqual((code, result), (1, {}))
        self.assertIn("::error::project icon: icon-png", log)
        code, _, _, log = self.run_contract(icon_ico=os.path.join(self.root, "missing.ico"))
        self.assertEqual(code, 1)
        self.assertIn("::error::project icon: icon-ico", log)


if __name__ == "__main__":
    unittest.main(verbosity=1)
