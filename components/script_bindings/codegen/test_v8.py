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

    def test_domstring_getters_preserve_utf16_code_units(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "StringOnly.webidl"
            webidl.write_text(
                "[Exposed=Window] interface StringOnly { attribute DOMString value; };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Value(&self) -> Vec<u16>;", source)
            self.assertIn("fn set_Value(&mut self, value: Vec<u16>);", source)
            self.assertIn("runtime.define_domstring_property(&interface, \"value\"", source)

    def test_nullable_domstring_uses_nullable_native_representation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "NullableString.webidl"
            webidl.write_text(
                "[Exposed=Window] interface NullableString { readonly attribute DOMString? initialValue; attribute DOMString? value; };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Value(&self) -> Option<Vec<u16>>;", source)
            self.assertIn("fn InitialValue(&self) -> Option<Vec<u16>>;", source)
            self.assertIn("fn set_Value(&mut self, value: Option<Vec<u16>>);", source)
            self.assertIn("unwrap_or(Value::Null)", source)
            self.assertIn('runtime.define_property(&interface, "initialValue"', source)
            self.assertIn("runtime.define_nullable_domstring_property(", source)

    def test_usvstring_generates_scalar_string_setters(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "UsvStrings.webidl"
            webidl.write_text(
                "[Exposed=Window] interface UsvStrings { attribute USVString value; attribute USVString? nullable; readonly attribute USVString? initialValue; };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Value(&self) -> String;", source)
            self.assertIn("fn set_Value(&mut self, value: String);", source)
            self.assertIn("fn Nullable(&self) -> Option<String>;", source)
            self.assertIn("fn set_Nullable(&mut self, value: Option<String>);", source)
            self.assertIn("fn InitialValue(&self) -> Option<String>;", source)
            self.assertIn("PrimitiveConversion::UsvString", source)
            self.assertIn("PrimitiveConversion::NullableUsvString", source)
            self.assertIn('runtime.define_property(&interface, "initialValue"', source)

    def test_mutable_numeric_and_boolean_attributes_generate_typed_setters(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Mutable.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Mutable { attribute boolean enabled; attribute double ratio; attribute unsigned long count; };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertEqual(source.count("runtime.define_webidl_primitive_property("), 3)
            self.assertIn("PrimitiveConversion::Boolean", source)
            self.assertIn("PrimitiveConversion::Double", source)
            self.assertIn("PrimitiveConversion::UnsignedLong", source)
            self.assertIn("fn set_Count(&mut self, value: u32);", source)

    def assert_unsupported(self, contents):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Unsupported.webidl"
            path.write_text("[Exposed=Window] " + contents, encoding="utf-8")
            with self.assertRaises(TypeError):
                generate(path, Path(directory) / "output")

    def test_unsupported_members_never_silently_disappear(self):
        for member in ["readonly attribute float value;", "readonly attribute unsigned long long value;", "static readonly attribute boolean valid;", "undefined run(long value);", "[GetterThrows] readonly attribute boolean valid;", "attribute byte value;"]:
            with self.subTest(member=member):
                self.assert_unsupported("interface Unsupported { " + member + " };")

    def test_zero_argument_operations_generate_typed_native_methods(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Operations { undefined reset(); boolean ready(); double ratio(); unsigned long count(); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Reset(&self) -> ();", source)
            self.assertIn("fn Ready(&self) -> bool;", source)
            self.assertIn("fn Ratio(&self) -> f64;", source)
            self.assertIn("fn Count(&self) -> u32;", source)
            self.assertIn('runtime.define_method(&interface, "reset"', source)
            self.assertIn("Value::Undefined", source)
            self.assertIn("Value::Bool(native.Ready())", source)
            self.assertIn("Value::Number(native.Ratio())", source)
            self.assertIn("Value::Number(native.Count() as f64)", source)

    def test_nullable_primitive_attributes_use_optional_native_values(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "NullablePrimitives.webidl"
            webidl.write_text(
                "[Exposed=Window] interface NullablePrimitives { attribute boolean? enabled; attribute double? ratio; attribute unsigned long? count; };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Enabled(&self) -> Option<bool>;", source)
            self.assertIn("fn Ratio(&self) -> Option<f64>;", source)
            self.assertIn("fn Count(&self) -> Option<u32>;", source)
            self.assertIn("PrimitiveConversion::NullableBoolean", source)
            self.assertIn("PrimitiveConversion::NullableDouble", source)
            self.assertIn("PrimitiveConversion::NullableUnsignedLong", source)
            self.assertIn("Value::Null => None", source)

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
