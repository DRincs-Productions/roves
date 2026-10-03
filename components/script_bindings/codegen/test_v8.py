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
            self.assertIn("fn AvailWidth(&self) -> roves_v8::FiniteF64;", source)
            self.assertIn("fn ColorDepth(&self) -> u32;", source)
            self.assertIn("Value::Number(native.AvailWidth().get())", source)
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
        for member in ["readonly attribute float value;", "readonly attribute unsigned long long value;", "static readonly attribute boolean valid;", "[GetterThrows] readonly attribute boolean valid;", "attribute byte value;"]:
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
            self.assertIn("fn Ratio(&self) -> roves_v8::FiniteF64;", source)
            self.assertIn("fn Count(&self) -> u32;", source)
            self.assertIn('runtime.define_method(&interface, "reset"', source)
            self.assertIn("Value::Undefined", source)
            self.assertIn("Value::Bool(native.Ready())", source)
            self.assertIn("Value::Number(native.Ratio().get())", source)
            self.assertIn("Value::Number(native.Count() as f64)", source)

    def test_string_operation_returns_preserve_domstring_and_usvstring_types(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Operations { DOMString label(); DOMString? optionalLabel(); USVString usvLabel(); USVString? optionalUsvLabel(); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Label(&self) -> Vec<u16>;", source)
            self.assertIn("fn OptionalLabel(&self) -> Option<Vec<u16>>;", source)
            self.assertIn("fn UsvLabel(&self) -> String;", source)
            self.assertIn("fn OptionalUsvLabel(&self) -> Option<String>;", source)
            self.assertIn("Value::Utf16String(native.Label())", source)
            self.assertIn("map(Value::Utf16String).unwrap_or(Value::Null)", source)
            self.assertIn("Value::String(native.UsvLabel())", source)
            self.assertIn("map(Value::String).unwrap_or(Value::Null)", source)

    def test_nullable_primitive_operation_returns_use_optional_native_values(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Operations { boolean? enabled(); double? ratio(); unsigned long? count(); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Enabled(&self) -> Option<bool>;", source)
            self.assertIn("fn Ratio(&self) -> Option<roves_v8::FiniteF64>;", source)
            self.assertIn("fn Count(&self) -> Option<u32>;", source)
            self.assertIn("map(Value::Bool).unwrap_or(Value::Null)", source)
            self.assertIn("map(|value| Value::Number(value.get())).unwrap_or(Value::Null)", source)
            self.assertIn("map(|value| Value::Number(value as f64)).unwrap_or(Value::Null)", source)

    def test_integer_and_float_operation_returns_map_to_rust_numbers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Numbers.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Numbers { byte signedByte(); octet octetValue(); short shortValue(); unsigned short unsignedShortValue(); long longValue(); long long longLongValue(); unsigned long long unsignedLongLongValue(); float floatValue(); float? nullableFloat(); unrestricted float unrestrictedFloat(); unrestricted float? nullableUnrestrictedFloat(); double doubleValue(); double? nullableDouble(); unrestricted double unrestrictedDouble(); unrestricted double? nullableUnrestrictedDouble(); long long? nullableLongLong(); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            for rust_type, method in [("i8", "SignedByte"), ("u8", "OctetValue"), ("i16", "ShortValue"), ("u16", "UnsignedShortValue"), ("i32", "LongValue"), ("i64", "LongLongValue"), ("u64", "UnsignedLongLongValue"), ("roves_v8::FiniteF32", "FloatValue"), ("Option<roves_v8::FiniteF32>", "NullableFloat"), ("f32", "UnrestrictedFloat"), ("Option<f32>", "NullableUnrestrictedFloat"), ("roves_v8::FiniteF64", "DoubleValue"), ("Option<roves_v8::FiniteF64>", "NullableDouble"), ("f64", "UnrestrictedDouble"), ("Option<f64>", "NullableUnrestrictedDouble"), ("Option<i64>", "NullableLongLong")]:
                self.assertIn(f"fn {method}(&self) -> {rust_type};", source)
            self.assertIn("Value::Number(native.FloatValue().get() as f64)", source)
            self.assertIn("Value::Number(native.UnrestrictedFloat() as f64)", source)
            self.assertIn("Value::Number(native.DoubleValue().get())", source)
            self.assertIn("Value::Number(native.UnsignedLongLongValue() as f64)", source)

    def test_operations_accept_required_boolean_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Operations { boolean accepts(boolean value); undefined combine(boolean first, boolean second); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Accepts(&self, arg0: bool) -> bool;", source)
            self.assertIn("fn Combine(&self, arg0: bool, arg1: bool) -> ();", source)
            self.assertIn("args.get(0).unwrap_or(&Value::Undefined)", source)
            self.assertIn("let arg0 = match args.get(0)", source)
            self.assertIn("native.Accepts(arg0)", source)
            self.assertIn("native.Combine(arg0, arg1)", source)

    def test_operations_generate_required_numeric_argument_coercions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Operations { byte byteValue(byte value); octet octetValue(octet value); short shortValue(short value); unsigned short unsignedShortValue(unsigned short value); long longValue(long value); long long longLongValue(long long value); unsigned long unsignedLongValue(unsigned long value); unsigned long long unsignedLongLongValue(unsigned long long value); float floatValue(float value); unrestricted float unrestrictedFloat(unrestricted float value); double finite(double value); unrestricted double unrestrictedValue(unrestricted double value); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            for rust_type in ["i8", "u8", "i16", "u16", "i32", "i64", "u32", "u64", "roves_v8::FiniteF32", "f32", "roves_v8::FiniteF64", "f64"]:
                self.assertIn(f"arg0: {rust_type}", source)
            for conversion in ["Byte", "Octet", "Short", "UnsignedShort", "Long", "LongLong", "UnsignedLong", "UnsignedLongLong", "Float", "UnrestrictedFloat", "Double", "UnrestrictedDouble"]:
                self.assertIn(f"WebIdlArgumentConversion::{conversion}", source)

    def test_required_string_operation_arguments_generate_domstring_and_usvstring_coercions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "StringOperations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface StringOperations { DOMString echoDom(DOMString value); USVString echoUsv(USVString value); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn EchoDom(&self, arg0: Vec<u16>) -> Vec<u16>;", source)
            self.assertIn("fn EchoUsv(&self, arg0: String) -> String;", source)
            self.assertIn("WebIdlArgumentConversion::DomString", source)
            self.assertIn("WebIdlArgumentConversion::UsvString", source)

    def test_required_nullable_operation_arguments_generate_optional_native_contracts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "NullableOperations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface NullableOperations { boolean? flag(boolean? value); long? count(long? value); DOMString? label(DOMString? value); USVString? name(USVString? value); long? mix(long? value, long addend); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Flag(&self, arg0: Option<bool>) -> Option<bool>;", source)
            self.assertIn("fn Count(&self, arg0: Option<i32>) -> Option<i32>;", source)
            self.assertIn("fn Label(&self, arg0: Option<Vec<u16>>) -> Option<Vec<u16>>;", source)
            self.assertIn("fn Name(&self, arg0: Option<String>) -> Option<String>;", source)
            self.assertIn("Value::Null => None, Value::Bool(value) => Some(*value)", source)
            self.assertIn("define_webidl_method_with_nullable_arguments", source)
            self.assertIn("&[true]", source)
            self.assertIn("&[true, false]", source)
            self.assertIn("fn Mix(&self, arg0: Option<i32>, arg1: i32) -> Option<i32>;", source)

    def test_optional_arguments_generate_missing_state_and_nullable_presence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface Operations { long run(optional long value); long? nullable(optional long? value); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("WebIdlOptionalArgument<i32>", source)
            self.assertIn("WebIdlOptionalArgument<Option<i32>>", source)
            self.assertIn("WebIdlOptionalArgument::Missing", source)
            self.assertIn("WebIdlOptionalArgument::Present(None)", source)
            self.assertIn("define_webidl_method_with_argument_flags", source)

    def test_optional_explicit_primitive_defaults_are_emitted_without_truthiness_loss(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "DefaultOperations.webidl"
            webidl.write_text(
                "[Exposed=Window] interface DefaultOperations { boolean flag(optional boolean value = false); long count(optional long value = 0); unsigned long size(optional unsigned long value = 6); float single(optional float value = 2.5); double ratio(optional double value = 1.5); unrestricted float unrestrictedSingle(optional unrestricted float value = 2.5); unrestricted double infinity(optional unrestricted double value = Infinity); boolean? nullableFlag(optional boolean? value = null); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("Present(false)", source)
            self.assertIn("Present(0i32)", source)
            self.assertIn("Present(6u32)", source)
            self.assertIn("Present(roves_v8::FiniteF32::new(2.5f32)", source)
            self.assertIn("Present(roves_v8::FiniteF64::new(1.5f64)", source)
            self.assertIn("Present(2.5f32)", source)
            self.assertIn("Present(f64::INFINITY)", source)
            self.assertIn("Value::Missing => roves_v8::WebIdlOptionalArgument::Present(None)", source)
            self.assertIn("Value::Null => roves_v8::WebIdlOptionalArgument::Present(None)", source)

    def test_optional_explicit_string_defaults_generate_safe_native_values(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "StringDefaults.webidl"
            source_idl = (
                '[Exposed=Window] interface StringDefaults { '
                'DOMString dom(optional DOMString value = "line\\nquote"); '
                'USVString usv(optional USVString value = "rocket ' + chr(0x1F680) + '"); '
                'DOMString unicode(optional DOMString value = "rocket ' + chr(0x1F680) + '"); '
                'DOMString? nullable(optional DOMString? value = "seed"); '
                'DOMString bytes(optional ByteString value = "abc"); };'
            )
            webidl.write_text(source_idl, encoding="utf-8")
            source = generate(webidl, root / "out")
            self.assertIn("Present(vec![108u16, 105u16, 110u16, 101u16, 92u16, 110u16", source)
            self.assertIn('Present("\\u{72}\\u{6f}\\u{63}\\u{6b}\\u{65}\\u{74}\\u{20}\\u{1f680}".to_owned())', source)
            self.assertIn("Present(vec![114u16, 111u16, 99u16, 107u16, 101u16, 116u16, 32u16, 55357u16, 56960u16])", source)
            self.assertIn("Present(Some(vec![115u16, 101u16, 101u16, 100u16]))", source)
            self.assertIn("Present(vec![97u8, 98u8, 99u8])", source)

    def test_optional_explicit_defaults_variadics_and_unsupported_types_fail_closed(self):
        for signature in ["double run(double... values);", "double run(object value);"]:
            with self.subTest(signature=signature):
                self.assert_unsupported("interface Unsupported { " + signature + " };")

    def test_optional_boolean_arguments_are_supported_and_variadics_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "Operations.webidl"
            webidl.write_text("[Exposed=Window] interface Operations { boolean run(optional boolean value); };", encoding="utf-8")
            source = generate(webidl, root / "out")
            self.assertIn("WebIdlOptionalArgument<bool>", source)
            self.assertIn("WebIdlArgumentConversion::Boolean", source)
        for signature in ["boolean run(object value);", "boolean run(boolean... values);"]:
            with self.subTest(signature=signature):
                self.assert_unsupported("interface Unsupported { " + signature + " };")

    def test_bytestring_operation_arguments_use_bytes_and_keep_nullable_optional_states(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "ByteStrings.webidl"
            webidl.write_text(
                "[Exposed=Window] interface ByteStrings { DOMString echo(ByteString value); DOMString? nullable(ByteString? value); DOMString? optionalValue(optional ByteString? value); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Echo(&self, arg0: Vec<u8>) -> Vec<u16>;", source)
            self.assertIn("fn Nullable(&self, arg0: Option<Vec<u8>>) -> Option<Vec<u16>>;", source)
            self.assertIn("fn OptionalValue(&self, arg0: roves_v8::WebIdlOptionalArgument<Option<Vec<u8>>>) -> Option<Vec<u16>>;", source)
            self.assertIn("WebIdlArgumentConversion::ByteString", source)
            self.assertIn("Value::ByteString(value) => value.clone()", source)

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
            self.assertIn("fn Ratio(&self) -> Option<roves_v8::FiniteF64>;", source)
            self.assertIn("fn Count(&self) -> Option<u32>;", source)
            self.assertIn("PrimitiveConversion::NullableBoolean", source)
            self.assertIn("PrimitiveConversion::NullableDouble", source)
            self.assertIn("PrimitiveConversion::NullableUnsignedLong", source)
            self.assertIn("FiniteF64::new(*value).expect", source)
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
