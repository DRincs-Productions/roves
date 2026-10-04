#![cfg(feature = "webidl-pilot")]
use std::cell::Cell;
use std::rc::Rc;
use roves_v8::{Runtime, Value};
use roves_v8::webidl::screen::{ScreenBinding, ScreenNative};
use roves_v8::webidl::validity_state::{ValidityStateBinding, ValidityStateNative};

struct TestScreen;
impl ScreenNative for TestScreen {
    fn AvailWidth(&self) -> roves_v8::FiniteF64 { roves_v8::FiniteF64::new(1280.5).unwrap() }
    fn AvailHeight(&self) -> roves_v8::FiniteF64 { roves_v8::FiniteF64::new(720.25).unwrap() }
    fn Width(&self) -> roves_v8::FiniteF64 { roves_v8::FiniteF64::new(1920.0).unwrap() }
    fn Height(&self) -> roves_v8::FiniteF64 { roves_v8::FiniteF64::new(1080.0).unwrap() }
    fn ColorDepth(&self) -> u32 { 24 }
    fn PixelDepth(&self) -> u32 { 32 }
}

struct State(Rc<Cell<u16>>);
macro_rules! getter {
    ($name:ident, $bit:expr) => { fn $name(&self) -> bool { self.0.get() & (1 << $bit) != 0 } };
}
#[allow(non_snake_case)]
impl ValidityStateNative for State {
    getter!(ValueMissing, 0);
    getter!(TypeMismatch, 1);
    getter!(PatternMismatch, 2);
    getter!(TooLong, 3);
    getter!(TooShort, 4);
    getter!(RangeUnderflow, 5);
    getter!(RangeOverflow, 6);
    getter!(StepMismatch, 7);
    getter!(BadInput, 8);
    getter!(CustomError, 9);
    fn Valid(&self) -> bool { self.0.get() == 0 }
}

#[test]
fn generated_real_webidl_reads_every_attribute_live_and_per_instance() {
    let mut runtime = Runtime::new();
    let binding = ValidityStateBinding::<State>::install(&mut runtime).unwrap();
    // Interface exposure must not depend on creating the first native wrapper.
    assert_eq!(runtime.eval("typeof ValidityState").unwrap(), "function");
    let flags = Rc::new(Cell::new(0));
    let a = binding.create(&mut runtime, State(flags.clone()));
    let b = binding.create(&mut runtime, State(Rc::new(Cell::new(0))));
    runtime.set_global_property("a", &a).unwrap();
    runtime.set_global_property("b", &b).unwrap();
    assert_eq!(runtime.eval_value("a.valid && b.valid").unwrap(), Value::Bool(true));
    let attributes = ["valueMissing", "typeMismatch", "patternMismatch", "tooLong", "tooShort", "rangeUnderflow", "rangeOverflow", "stepMismatch", "badInput", "customError"];
    for (bit, name) in attributes.iter().enumerate() {
        flags.set(1 << bit);
        for (other, other_name) in attributes.iter().enumerate() {
            assert_eq!(runtime.eval_value(&format!("a.{other_name}")).unwrap(), Value::Bool(bit == other), "{name} -> {other_name}");
        }
        assert_eq!(runtime.eval_value("a.valid").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("b.valid").unwrap(), Value::Bool(true));
    }
    assert_eq!(runtime.eval("a instanceof ValidityState").unwrap(), "true");
}

#[test]
fn generated_binding_preserves_readonly_descriptors_and_rejects_spoofing() {
    let mut runtime = Runtime::new();
    let binding = ValidityStateBinding::<State>::install(&mut runtime).unwrap();
    let state = binding.create(&mut runtime, State(Rc::new(Cell::new(0))));
    runtime.set_global_property("state", &state).unwrap();
    assert_eq!(runtime.eval("Object.hasOwn(state, 'valid')").unwrap(), "false");
    assert_eq!(runtime.eval("const d = Object.getOwnPropertyDescriptor(ValidityState.prototype, 'valid'); [d.get.name, d.get.length, d.set === undefined, d.enumerable, d.configurable].join(',')").unwrap(), "get valid,0,true,true,true");
    assert_eq!(runtime.eval("Object.prototype.toString.call(state)").unwrap(), "[object ValidityState]");
    assert_eq!(runtime.eval("const tag = Object.getOwnPropertyDescriptor(ValidityState.prototype, Symbol.toStringTag); [tag.value, tag.writable, tag.enumerable, tag.configurable].join(',')").unwrap(), "ValidityState,false,false,true");
    assert_eq!(runtime.eval("const ctor = Object.getOwnPropertyDescriptor(globalThis, 'ValidityState'); [ctor.writable, ctor.enumerable, ctor.configurable].join(',')").unwrap(), "true,false,true");
    assert_eq!(runtime.eval("const proto = Object.getOwnPropertyDescriptor(ValidityState, 'prototype'); [proto.writable, proto.enumerable, proto.configurable].join(',')").unwrap(), "false,false,false");
    for source in ["ValidityState()", "new ValidityState()", "d.get.call({})", "d.get.call(Object.create(ValidityState.prototype))", "'use strict'; state.valid = false"] {
        assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
    }
    assert_eq!(runtime.eval_value("state.valid").unwrap(), Value::Bool(true));
}

#[test]
fn generated_screen_binding_converts_real_webidl_numeric_types() {
    let mut runtime = Runtime::new();
    let binding = ScreenBinding::<TestScreen>::install(&mut runtime).unwrap();
    let screen = binding.create(&mut runtime, TestScreen);
    runtime.set_global_property("screen", &screen).unwrap();
    assert_eq!(runtime.eval_value("screen.availWidth").unwrap(), Value::Number(1280.5));
    assert_eq!(runtime.eval_value("screen.availHeight").unwrap(), Value::Number(720.25));
    assert_eq!(runtime.eval_value("screen.width").unwrap(), Value::Number(1920.0));
    assert_eq!(runtime.eval_value("screen.height").unwrap(), Value::Number(1080.0));
    assert_eq!(runtime.eval_value("screen.colorDepth").unwrap(), Value::Number(24.0));
    assert_eq!(runtime.eval_value("screen.pixelDepth").unwrap(), Value::Number(32.0));
    assert_eq!(runtime.eval("screen instanceof Screen").unwrap(), "true");
}

#[test]
fn generated_binding_uses_native_identity_to_reuse_dom_wrappers() {
    let mut runtime = Runtime::new();
    let binding = ValidityStateBinding::<State>::install(&mut runtime).unwrap();
    let flags = Rc::new(Cell::new(0));
    let first = binding.create_with_identity(&mut runtime, 41, || State(flags.clone()));
    // This models two native references to the same DOM object. The second native payload must
    // not replace the first wrapper's native value or create a second JS-visible identity.
    let same_native = binding.create_with_identity(&mut runtime, 41, || {
        panic!("a live native wrapper must be reused without invoking the factory")
    });
    runtime.set_global_property("first", &first).unwrap();
    runtime
        .set_global_property("sameNative", &same_native)
        .unwrap();
    assert_eq!(runtime.eval("first === sameNative").unwrap(), "true");
    assert_eq!(
        runtime.eval("first instanceof ValidityState").unwrap(),
        "true"
    );
}


#[test]
fn generated_validity_state_binding_matches_the_real_servo_webidl_contract() {
    let mut runtime = Runtime::new();
    let binding = ValidityStateBinding::<State>::install(&mut runtime).unwrap();
    let flags = Rc::new(Cell::new(0));
    let state = binding.create(&mut runtime, State(flags.clone()));
    runtime.set_global_property("state", &state).unwrap();

    for name in [
        "valueMissing", "typeMismatch", "patternMismatch", "tooLong", "tooShort",
        "rangeUnderflow", "rangeOverflow", "stepMismatch", "badInput", "customError", "valid",
    ] {
        assert_eq!(runtime.eval_value(&format!("state.{name}")).unwrap(), Value::Bool(name == "valid"));
        let descriptor = format!(
            "(() => {{ const d = Object.getOwnPropertyDescriptor(ValidityState.prototype, '{name}'); \
             return [typeof d.get, d.set === undefined, d.enumerable, d.configurable].join(','); }})()"
        );
        assert_eq!(runtime.eval(&descriptor).unwrap(), "function,true,true,true", "{name}");
    }

    for bit in 0..10 {
        flags.set(1 << bit);
        assert_eq!(runtime.eval_value("state.valid").unwrap(), Value::Bool(false));
    }
    flags.set(0);
    assert_eq!(runtime.eval_value("state.valid").unwrap(), Value::Bool(true));
    assert_eq!(runtime.eval("state instanceof ValidityState").unwrap(), "true");
}
