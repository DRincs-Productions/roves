"""V8 generator contracts: real AST input, deterministic output, fail-closed scope."""
from pathlib import Path
import tempfile
import unittest

from run_v8 import ROOT, generate


class V8GeneratorTests(unittest.TestCase):
    def test_real_webidl_generation_is_deterministic_and_has_every_attribute(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            source = generate(ROOT / "webidls/ValidityState.webidl", output)
            self.assertEqual(source, generate(ROOT / "webidls/ValidityState.webidl", output))
            self.assertEqual(source.count("runtime.define_property("), 11)
            self.assertIn("fn ValueMissing(&self) -> bool;", source)
            self.assertIn("fn Valid(&self) -> bool;", source)
            self.assertIn("runtime.expose_interface(&interface)?;", source)
            self.assertNotIn("js::", source)
            self.assertNotIn("v8::", source.replace("roves_v8::", ""))

    def test_real_screen_webidl_generates_supported_numeric_types(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            source = generate(ROOT / "webidls/Screen.webidl", output)
            self.assertEqual(source, generate(ROOT / "webidls/Screen.webidl", output))
            self.assertEqual(source.count("runtime.define_property("), 6)
            self.assertIn("fn AvailWidth(&self) -> f64;", source)
            self.assertIn("fn ColorDepth(&self) -> u32;", source)
            self.assertIn("Value::Number(native.AvailWidth())", source)
            self.assertIn("Value::Number(native.ColorDepth() as f64)", source)

    def assert_unsupported(self, contents):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Unsupported.webidl"
            path.write_text("[Exposed=Window] " + contents, encoding="utf-8")
            with self.assertRaises(TypeError):
                generate(path, Path(directory) / "output")

    def test_unsupported_members_never_silently_disappear(self):
        for member in ["attribute boolean valid;", "readonly attribute float value;", "readonly attribute unsigned long long value;", "readonly attribute DOMString value;", "readonly attribute boolean? value;", "undefined run();", "static readonly attribute boolean valid;", "[GetterThrows] readonly attribute boolean valid;"]:
            with self.subTest(member=member):
                self.assert_unsupported("interface Unsupported { " + member + " };")

    def test_unsupported_interface_shapes_fail(self):
        self.assert_unsupported("interface Unsupported { constructor(); };")
        self.assert_unsupported("namespace Unsupported { undefined run(); };")
        self.assert_unsupported("interface Base {}; [Exposed=Window] interface Unsupported : Base {};")

    def test_non_window_exposure_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "WorkerOnly.webidl"
            path.write_text("[Exposed=Worker] interface WorkerOnly {};", encoding="utf-8")
            with self.assertRaisesRegex(TypeError, "exposed to Window"):
                generate(path, Path(directory) / "output")


if __name__ == "__main__":
    unittest.main()
