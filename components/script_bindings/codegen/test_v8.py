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
            self.assertIn("pub fn create_with_identity(", source)
            self.assertIn("pub fn wrap_traced(&self, runtime: &mut Runtime, native: &roves_v8::GcRoot<T>) -> Handle", source)
            self.assertIn("runtime.create_traced_instance(&self.interface, native)", source)
            self.assertIn("runtime.create_instance_with_identity(&self.interface, native_identity, create)", source)
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
            self.assertIn("fn set_Value(&self, value: Vec<u16>);", source)
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
            self.assertIn("fn set_Value(&self, value: Option<Vec<u16>>);", source)
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
            self.assertIn("fn set_Value(&self, value: String);", source)
            self.assertIn("fn Nullable(&self) -> Option<String>;", source)
            self.assertIn("fn set_Nullable(&self, value: Option<String>);", source)
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
            self.assertIn("fn set_Count(&self, value: u32);", source)

    def assert_unsupported(self, contents):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Unsupported.webidl"
            path.write_text("[Exposed=Window] " + contents, encoding="utf-8")
            with self.assertRaises(TypeError):
                generate(path, Path(directory) / "output")

    def test_unsupported_members_never_silently_disappear(self):
        for member in ["readonly attribute (long or DOMString) value;", "readonly attribute Promise<any> value;", "static readonly attribute boolean valid;", "[Unscopable] readonly attribute boolean valid;", "attribute Unsupported? owner;"]:
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
            self.assertIn("define_webidl_method_with_argument_flags_and_enums", source)
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
            self.assertIn("define_webidl_method_with_argument_flags_and_enums", source)

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

    def test_optional_nullable_string_null_defaults_preserve_missing_null_and_values(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "NullableDefaults.webidl"
            webidl.write_text(
                "[Exposed=Window] interface NullableDefaults { "
                "long dom(optional DOMString? value = null); "
                "long usv(optional USVString? value = null); "
                "long bytes(optional ByteString? value = null); };",
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("Present(None)", source)
            self.assertIn("Value::Null => roves_v8::WebIdlOptionalArgument::Present(None)", source)
            self.assertIn("WebIdlArgumentConversion::DomString", source)
            self.assertIn("WebIdlArgumentConversion::UsvString", source)
            self.assertIn("WebIdlArgumentConversion::ByteString", source)

    def test_enum_arguments_validate_declared_values_and_support_optional_defaults(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            webidl = root / "EnumOperations.webidl"
            webidl.write_text(
                'enum Direction { "left", "right" }; '
                '[Exposed=Window] interface EnumOperations { '
                'USVString echo(Direction value); '
                'USVString optionalValue(optional Direction value = "right"); };',
                encoding="utf-8",
            )
            source = generate(webidl, root / "out")
            self.assertIn("fn Echo(&self, arg0: String) -> String;", source)
            self.assertIn('Present("\\u{72}\\u{69}\\u{67}\\u{68}\\u{74}".to_owned())', source)
            self.assertIn("WebIdlArgumentConversion::Enumeration", source)
            self.assertIn('Some(&["\\u{6c}\\u{65}\\u{66}\\u{74}", "\\u{72}\\u{69}\\u{67}\\u{68}\\u{74}"])', source)

    def test_optional_explicit_defaults_variadics_and_unsupported_types_fail_closed(self):
        for signature in ["double run(Promise<any> value);", "double run(record<DOMString, long> value);"]:
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
        for signature in ["boolean run(record<DOMString, long> value);", "boolean run(Promise<any> value);"]:
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
        self.assert_unsupported("interface Unsupported { [Pref=\"dom_x\"] constructor(); };")
        self.assert_unsupported("namespace Unsupported { undefined run(); };")

    def test_non_window_exposure_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "WorkerOnly.webidl"
            path.write_text("[Exposed=Worker] interface WorkerOnly {};", encoding="utf-8")
            with self.assertRaisesRegex(TypeError, "exposed to Window"):
                generate(path, Path(directory) / "output")

    def test_multi_global_exposure_including_window_is_supported(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Shared.webidl"
            path.write_text(
                "[Exposed=(Window,Worker)] interface Shared { readonly attribute boolean ok; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            self.assertIn("fn Ok(&self) -> bool;", source)

    def test_interface_shape_rejections_name_the_shape(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Shapes.webidl"
            path.write_text("[Exposed=Window] namespace Shapes { undefined run(); };", encoding="utf-8")
            with self.assertRaises(TypeError):
                generate(path, Path(directory) / "output")

    def test_constructor_generates_native_constructor_and_constructible_interface(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Point.webidl"
            path.write_text(
                "[Exposed=Window] interface Point { constructor(double x, optional DOMString label = \"p\"); readonly attribute double x; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            self.assertIn("fn Constructor(arg0: roves_v8::FiniteF64, arg1: roves_v8::WebIdlOptionalArgument<Vec<u16>>) -> Self where Self: Sized;", source)
            self.assertIn("runtime.define_constructible_interface(", source)
            self.assertIn("Ok(roves_v8::TracedNative::new(<T as PointNative>::Constructor(arg0, arg1)))", source)
            self.assertIn("&[roves_v8::WebIdlArgumentConversion::Double, roves_v8::WebIdlArgumentConversion::DomString]", source)
            self.assertIn("&[false, true]", source)

    def test_count_distinguishable_overloads_share_one_dispatcher(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Over.webidl"
            path.write_text(
                "[Exposed=Window] interface Over { boolean fill(); boolean fill(DOMString rule, optional boolean even = false); "
                "undefined at(unsigned long x, unsigned long y, unsigned long z); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "fn Fill(&self) -> bool;",
                "fn Fill_(&self, arg0: Vec<u16>, arg1: roves_v8::WebIdlOptionalArgument<bool>) -> bool;",
                'runtime.define_overloaded_webidl_method(&interface, "fill", &[',
                "roves_v8::WebIdlOverload {",
                "Ok(Value::Bool(native.Fill()))",
                "Ok(Value::Bool(native.Fill_(arg0, arg1)))",
                'runtime.define_webidl_method_with_argument_flags_and_enums(&interface, "at"',
            ]:
                self.assertIn(expected, source)
            self.assertEqual(source.count("roves_v8::WebIdlOverload {"), 2)

    def test_ce_reactions_members_are_wrapped_in_the_native_hook(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Reactive.webidl"
            path.write_text(
                "[Exposed=Window] interface Reactive { [CEReactions] attribute DOMString title; "
                "[CEReactions, Throws] undefined run(); readonly attribute boolean plain; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            self.assertIn("pub trait ReactiveNative: 'static + roves_v8::CeReactions {", source)
            self.assertIn("<T as roves_v8::CeReactions>::with_ce_reactions(move || -> Value {", source)
            self.assertIn("<T as roves_v8::CeReactions>::with_ce_reactions(move || -> () {", source)
            self.assertIn("<T as roves_v8::CeReactions>::with_ce_reactions(move || -> Result<Value, roves_v8::WebIdlError> {", source)
            # Getter + setter of `title` and the `run` operation; `plain` is not wrapped.
            self.assertEqual(source.count("with_ce_reactions("), 3)

    def test_abstract_interfaces_generate_like_any_other(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Shape.webidl"
            path.write_text("[Abstract, Exposed=Window] interface Shape { readonly attribute double area; };", encoding="utf-8")
            source = generate(path, Path(directory) / "output")
            self.assertIn("fn Area(&self) -> roves_v8::FiniteF64;", source)
            self.assertIn('runtime.define_interface("Shape", None)', source)

    def test_any_object_and_callback_members_receive_a_script_context(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Engine.webidl"
            path.write_text(
                "callback Visitor = boolean (any value); [Exposed=Window] interface Engine { "
                "any echo(any value); boolean visit(Visitor visitor, object target); undefined plain(boolean flag); "
                "attribute boolean state; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "fn Echo(&self, cx: &mut roves_v8::ScriptContext, arg0: roves_v8::Handle) -> Result<roves_v8::Handle, roves_v8::WebIdlError>;",
                "fn Visit(&self, cx: &mut roves_v8::ScriptContext, arg0: roves_v8::Handle, arg1: roves_v8::Handle) -> Result<bool, roves_v8::WebIdlError>;",
                "fn Plain(&self, arg0: bool) -> ();",
                # Setters take &self: natives may re-enter JS, so mutation is interior.
                "fn set_State(&self, value: bool);",
                'runtime.define_contextual_webidl_method(&interface, "echo", |cx, native, args| {',
                "let result = native.Echo(cx, arg0)?;",
                "roves_v8::WebIdlArgumentConversion::Callback, roves_v8::WebIdlArgumentConversion::Object",
            ]:
                self.assertIn(expected, source)
            self.assertNotIn("downcast_mut", source)

    def test_event_handler_and_any_attributes_are_contextual(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Host.webidl"
            path.write_text(
                "[LegacyTreatNonObjectAsNull] callback Handler = any (any event); typedef Handler? EventHandler; "
                "callback Strict = undefined (); "
                "[Exposed=Window] interface Host { attribute EventHandler onping; attribute Strict? strict; "
                "attribute any data; readonly attribute object? shape; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "fn Onping(&self, cx: &mut roves_v8::ScriptContext) -> Result<Option<roves_v8::Handle>, roves_v8::WebIdlError>;",
                "fn set_Onping(&self, cx: &mut roves_v8::ScriptContext, value: Option<roves_v8::Handle>) -> Result<(), roves_v8::WebIdlError>;",
                "fn Data(&self, cx: &mut roves_v8::ScriptContext) -> Result<roves_v8::Handle, roves_v8::WebIdlError>;",
                "fn Shape(&self, cx: &mut roves_v8::ScriptContext) -> Result<Option<roves_v8::Handle>, roves_v8::WebIdlError>;",
                "roves_v8::WebIdlArgumentConversion::LegacyCallback, true)?;",
                "roves_v8::WebIdlArgumentConversion::Callback, true)?;",
                "roves_v8::WebIdlArgumentConversion::Any, false)?;",
            ]:
                self.assertIn(expected, source)
            self.assertNotIn("fn set_Shape", source)

    def test_sequences_use_structured_webidl_types(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Seq.webidl"
            path.write_text(
                "[Exposed=Window] interface Seq { sequence<long?> holes(sequence<sequence<long>> grid, boolean flag); "
                "unsigned long count(optional sequence<boolean> flags = []); sequence<any> echo(sequence<any> values); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "fn Holes(&self, arg0: Vec<Vec<i32>>, arg1: bool) -> Vec<Option<i32>>;",
                "fn Count(&self, arg0: roves_v8::WebIdlOptionalArgument<Vec<bool>>) -> u32;",
                "fn Echo(&self, cx: &mut roves_v8::ScriptContext, arg0: Vec<roves_v8::Handle>) -> Result<Vec<roves_v8::Handle>, roves_v8::WebIdlError>;",
                "roves_v8::WebIdlType::Sequence(Box::new(roves_v8::WebIdlType::Sequence(Box::new(roves_v8::WebIdlType::Primitive(roves_v8::WebIdlArgumentConversion::Long)))))",
                # A flat argument mixed into a structured operation keeps its conversion.
                "roves_v8::WebIdlArgument { ty: roves_v8::WebIdlType::Primitive(roves_v8::WebIdlArgumentConversion::Boolean), optional: false, variadic: false }",
                "Value::Missing => roves_v8::WebIdlOptionalArgument::Present(Vec::new())",
                "roves_v8::WebIdlNativeOperation::Contextual(",
                "match item { Some(item) => Value::Number(item as f64), None => Value::Null }",
            ]:
                self.assertIn(expected, source)

    def test_dictionaries_generate_structs_in_webidl_member_order(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Dict.webidl"
            path.write_text(
                "dictionary Base { boolean zeta = false; boolean alpha = true; }; "
                "dictionary Derived : Base { required DOMString type; long count; }; "
                "[Exposed=Window] interface Dict { constructor(optional Base init = {}); Derived echo(Derived value); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "pub struct Base {\n    pub alpha: bool,\n    pub zeta: bool,\n}",
                # Inherited members first, each level sorted; a missing optional member is None.
                "pub struct Derived {\n    pub alpha: bool,\n    pub zeta: bool,\n    pub count: Option<i32>,\n    pub r#type: Vec<u16>,\n}",
                'roves_v8::WebIdlDictionaryMember { name: "type".to_owned(), ty: roves_v8::WebIdlType::Primitive(roves_v8::WebIdlArgumentConversion::DomString), required: true, default: None }',
                'name: "alpha".to_owned(), ty: roves_v8::WebIdlType::Primitive(roves_v8::WebIdlArgumentConversion::Boolean), required: false, default: Some(Value::Bool(true))',
                "fn Constructor(arg0: Base) -> Self where Self: Sized;",
                "runtime.define_typed_constructible_interface(",
                # The optional dictionary argument is never missing: undefined converts to defaults.
                "optional: false",
                "fn Echo(&self, arg0: Derived) -> Derived;",
                "native.Echo(arg0).into_value()",
            ]:
                self.assertIn(expected, source)

    def test_unions_and_callback_interfaces_use_structured_types(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Target.webidl"
            path.write_text(
                "[Exposed=Window] callback interface Listener { undefined handleEvent(any event); }; "
                "dictionary Options { boolean once = false; }; "
                "[Exposed=Window] interface Target { constructor(); "
                "undefined listen(Listener? listener, optional (Options or boolean) options = {}); "
                "DOMString kind((long or DOMString) value); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "pub enum OptionsOrBoolean {\n    Options(Options),\n    Boolean(bool),\n}",
                "pub enum LongOrString {\n    Long(i32),\n    String(Vec<u16>),\n}",
                "fn Listen(&self, cx: &mut roves_v8::ScriptContext, arg0: Option<roves_v8::Handle>, arg1: OptionsOrBoolean) -> Result<(), roves_v8::WebIdlError>;",
                "roves_v8::WebIdlType::Nullable(Box::new(roves_v8::WebIdlType::CallbackInterface))",
                "roves_v8::WebIdlType::Union(vec![",
                # A union defaulting to `{}` is never missing: undefined selects the dictionary.
                "optional: false",
                # Constructible interfaces create traced platform objects.
                "pub trait TargetNative: 'static + roves_v8::Trace {",
                "Ok(roves_v8::TracedNative::new(<T as TargetNative>::Constructor()))",
            ]:
                self.assertIn(expected, source)

    def test_throwing_numeric_and_forwarding_attributes_use_contextual_accessors(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "Tokens.webidl"
            target.write_text("[Exposed=Window] interface Tokens { attribute DOMString value; };", encoding="utf-8")
            path = Path(directory) / "Owner.webidl"
            path.write_text(
                "[Exposed=Window] interface Owner { [SetterThrows] attribute DOMString? label; "
                "[GetterThrows] readonly attribute unsigned long checked; attribute byte small; "
                "[PutForwards=value] readonly attribute Tokens tokens; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output", (target,))
            for expected in [
                "fn Label(&self) -> Option<Vec<u16>>;",
                "fn set_Label(&self, value: Option<Vec<u16>>) -> Result<(), roves_v8::WebIdlError>;",
                "fn Checked(&self) -> Result<u32, roves_v8::WebIdlError>;",
                "fn set_Small(&self, value: i8);",
                "fn Tokens(&self) -> roves_v8::NativeRef;",
                "roves_v8::WebIdlArgumentConversion::DomString, true)?;",
                "roves_v8::WebIdlArgumentConversion::Byte, false)?;",
                'cx.set_property(&target, "value", value)',
            ]:
                self.assertIn(expected, source)
            self.assertNotIn("fn set_Tokens", source)

    def test_variadic_arguments_collect_into_a_vec(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Var.webidl"
            path.write_text("[Exposed=Window] interface Var { DOMString join(DOMString separator, DOMString... parts); };", encoding="utf-8")
            source = generate(path, Path(directory) / "output")
            self.assertIn("fn Join(&self, arg0: Vec<u16>, arg1: Vec<Vec<u16>>) -> Vec<u16>;", source)
            self.assertIn("ty: roves_v8::WebIdlType::Primitive(roves_v8::WebIdlArgumentConversion::DomString), optional: false, variadic: true }", source)

    def test_overloads_needing_type_distinction_fail_closed(self):
        self.assert_unsupported("interface Unsupported { undefined f(DOMString a); undefined f(boolean a); };")
        self.assert_unsupported("interface Unsupported { undefined f(DOMString a); undefined f(boolean a, optional boolean b); };")

    def test_unsupported_and_overloaded_constructors_fail_closed(self):
        for constructor, message in [
            ("[Pref=\"dom_x\"] constructor();", "unsupported constructor attributes"),
            ("constructor(); constructor(boolean flag);", "single-signature constructors"),
        ]:
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "Ctor.webidl"
                path.write_text(f"[Exposed=Window] interface Ctor {{ {constructor} }};", encoding="utf-8")
                with self.assertRaisesRegex(TypeError, message):
                    generate(path, Path(directory) / "output")

    def test_throws_generates_fallible_native_signatures(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Risky.webidl"
            path.write_text(
                "[Exposed=Window] interface Risky { [Throws] constructor(boolean ok); "
                "[Throws] unsigned long parse(DOMString text); [Throws] undefined reset(); boolean plain(); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            self.assertIn("fn Constructor(arg0: bool) -> Result<Self, roves_v8::WebIdlError> where Self: Sized;", source)
            self.assertIn(".map(roves_v8::TracedNative::new)", source)
            self.assertIn("fn Parse(&self, arg0: Vec<u16>) -> Result<u32, roves_v8::WebIdlError>;", source)
            self.assertIn("fn Reset(&self) -> Result<(), roves_v8::WebIdlError>;", source)
            self.assertIn("fn Plain(&self) -> bool;", source)
            self.assertIn('runtime.define_fallible_webidl_method(&interface, "parse"', source)
            self.assertIn("let result = native.Parse(arg0)?;", source)
            self.assertIn("Ok(Value::Number(result as f64))", source)
            self.assertIn('runtime.define_fallible_webidl_method(&interface, "reset"', source)
            self.assertIn('runtime.define_method(&interface, "plain"', source)

    def write_hierarchy(self, directory, derived_body="readonly attribute boolean derived;"):
        base = Path(directory) / "Base.webidl"
        base.write_text("[Exposed=Window] interface Base { readonly attribute boolean base; };", encoding="utf-8")
        derived = Path(directory) / "HTMLDerived.webidl"
        derived.write_text(f"[Exposed=Window] interface HTMLDerived : Base {{ {derived_body} }};", encoding="utf-8")
        return base, derived

    def test_inherited_interface_extends_parent_native_trait_and_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            base, derived = self.write_hierarchy(directory)
            output = Path(directory) / "output"
            source = generate(derived, output, (base,))
            self.assertEqual(source, generate(derived, output, (base,)))
            self.assertIn("pub trait HTMLDerivedNative: super::base::BaseNative {", source)
            self.assertIn("parent: &super::base::BaseBinding<T>", source)
            self.assertIn('runtime.define_interface("HTMLDerived", Some(parent.interface()))', source)
            self.assertIn("fn Derived(&self) -> bool;", source)
            self.assertNotIn("fn Base(&self)", source)
            base_source = generate(base, output)
            self.assertIn("pub trait BaseNative: 'static {", base_source)
            self.assertIn('runtime.define_interface("Base", None)', base_source)
            self.assertIn("pub fn interface(&self) -> &Interface", base_source)

    def test_redeclaring_an_inherited_member_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            base, derived = self.write_hierarchy(directory, "readonly attribute boolean base;")
            with self.assertRaisesRegex(TypeError, "shadowing an inherited member"):
                generate(derived, Path(directory) / "output", (base,))

    def test_unforgeable_attributes_install_once_on_the_declaring_interface(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory) / "Base.webidl"
            base.write_text(
                "[Exposed=Window] interface Base { [LegacyUnforgeable] readonly attribute boolean trusted; };",
                encoding="utf-8",
            )
            child = Path(directory) / "Child.webidl"
            child.write_text("[Exposed=Window] interface Child : Base { readonly attribute boolean own; };", encoding="utf-8")
            output = Path(directory) / "output"
            base_source = generate(base, output)
            self.assertIn('runtime.define_unforgeable_property(&interface, "trusted"', base_source)
            child_source = generate(child, output, (base,))
            # The parser copies the unforgeable member into Child; it must not be redeclared.
            self.assertNotIn("trusted", child_source)
            self.assertNotIn("Trusted", child_source)
            self.assertIn("fn Own(&self) -> bool;", child_source)

    def test_mutable_unforgeable_attributes_fail_closed(self):
        self.assert_unsupported("interface Unsupported { [LegacyUnforgeable] attribute boolean flag; };")

    def test_readonly_numeric_attributes_map_to_rust_numbers(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Numbers.webidl"
            path.write_text(
                "[Exposed=Window] interface Numbers { readonly attribute short a; readonly attribute long long b; "
                "readonly attribute float c; readonly attribute unrestricted double d; readonly attribute unsigned short? e; "
                "const unsigned short ONE = 1; const unrestricted double INF = Infinity; };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "fn A(&self) -> i16;", "fn B(&self) -> i64;", "fn C(&self) -> roves_v8::FiniteF32;",
                "fn D(&self) -> f64;", "fn E(&self) -> Option<u16>;",
                "Value::Number(native.C().get() as f64)", "Value::Number(native.D())",
                "native.E().map(|value| Value::Number(value as f64)).unwrap_or(Value::Null)",
                'runtime.define_constant(&interface, "ONE", &Value::Number(1.0))?;',
                'runtime.define_constant(&interface, "INF", &Value::Number(f64::INFINITY))?;',
            ]:
                self.assertIn(expected, source)

    def test_interface_typed_members_use_native_refs(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Tree.webidl"
            path.write_text(
                "[Exposed=Window] interface Tree { readonly attribute Tree? parent; readonly attribute Tree root; "
                "Tree? childAt(unsigned long index); boolean contains(Tree? other); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "fn Parent(&self) -> Option<roves_v8::NativeRef>;",
                "fn Root(&self) -> roves_v8::NativeRef;",
                "fn ChildAt(&self, arg0: u32) -> Option<roves_v8::NativeRef>;",
                "fn Contains(&self, arg0: Option<roves_v8::NativeRef>) -> bool;",
                "native.Parent().map(Value::Native).unwrap_or(Value::Null)",
                "Value::Native(native.Root())",
                "roves_v8::WebIdlArgumentConversion::Interface",
                '&[Some(&["Tree"])]',
            ]:
                self.assertIn(expected, source)

    def test_callback_interfaces_and_promises_are_not_dom_interface_values(self):
        self.assert_unsupported("interface Uses { undefined add(Promise<any> pending); };")

    def test_exposure_conditions_gate_interface_objects_and_members(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Gated.webidl"
            path.write_text(
                '[Exposed=Window, Pref="dom_gated"] interface Gated { readonly attribute boolean a; '
                '[Pref="dom_extra", SecureContext] readonly attribute boolean b; [SecureContext] undefined c(); };',
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for expected in [
                "Self::install_with(runtime, &roves_v8::ExposeAll)",
                "exposure: &dyn roves_v8::Exposure,",
                'if exposure.pref_enabled("dom_extra") && exposure.is_secure_context() {',
                "if exposure.is_secure_context() {",
                'if !(exposure.pref_enabled("dom_gated")) {',
                "runtime.hide_interface_object(&interface);",
            ]:
                self.assertIn(expected, source)
            hidden = Path(directory) / "Hidden.webidl"
            hidden.write_text("[Exposed=Window, LegacyNoInterfaceObject] interface Hidden { readonly attribute boolean a; };", encoding="utf-8")
            hidden_source = generate(hidden, Path(directory) / "output")
            self.assertIn("        runtime.hide_interface_object(&interface);\n        runtime.expose_interface(&interface)?;", hidden_source)

    def test_jit_hint_attributes_are_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Hints.webidl"
            path.write_text(
                "[Exposed=Window] interface Hints { [Pure] readonly attribute boolean a; "
                "[Constant] readonly attribute boolean b; [Pure] boolean c(); };",
                encoding="utf-8",
            )
            source = generate(path, Path(directory) / "output")
            for method in ["fn A(&self) -> bool;", "fn B(&self) -> bool;", "fn C(&self) -> bool;"]:
                self.assertIn(method, source)

    def test_binding_module_names_are_snake_case(self):
        from codegen import v8_module_name

        for interface, module in [("ValidityState", "validity_state"), ("HTMLElement", "html_element"),
                                  ("WebGL2RenderingContext", "web_gl2_rendering_context"), ("Node", "node")]:
            self.assertEqual(v8_module_name(interface), module)

    def test_coverage_report_counts_real_webidl(self):
        from v8_coverage import measure

        report = measure()
        self.assertGreater(report["total"], 400)
        self.assertIn("ValidityState", report["supported"])
        self.assertIn("Screen", report["supported"])
        self.assertEqual(len(report["supported"]) + len(report["rejected"]), report["total"])
        # Every rejection must be a deliberate fail-closed TypeError from the backend,
        # never a crash inside the generator itself.
        for name, message in report["rejected"].items():
            self.assertRegex(message, r"^V8 (backend|pilot) ", name)


if __name__ == "__main__":
    unittest.main()
