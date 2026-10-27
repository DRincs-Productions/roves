//! Native class fixture for the single-document probe (task 0383): one cppgc-backed class whose
//! JavaScript wrappers carry real Rust values, registered and wrapped only through maintained
//! deno_core/rusty_v8 APIs. Glue is kept to this one fixture; it is not a binding generator and
//! makes no claim about Servo DOM bindings.
//!
//! - Registration: `#[op2] impl NativeClass` + `extension!(objects = [NativeClass])`. deno_core
//!   builds the class `FunctionTemplate` (getters on its prototype template) and keys it by
//!   `type_name::<NativeClass>()`, so `deno_core::cppgc::make_cppgc_object` instantiates
//!   wrappers with that prototype. The class function itself is only published under
//!   `Deno.core.ops`, which [`crate::new_runtime`] removes: game code reaches it only through
//!   `wrapper.constructor`, and that constructor wraps nothing (deno_core's no-op template
//!   callback), so it cannot mint native objects.
//! - Owned metadata: [`ClassMeta`] is heap-allocated Rust data behind an `Rc`; every instance
//!   holds a strong reference, so it stays valid after its creator drops its own `Rc`, and it
//!   is freed with the last instance. The class name V8 reports comes from the static op
//!   declaration and `GarbageCollected::get_name` must be `'static` (cppgc contract): the owned
//!   name is the observable [`ClassMeta::name`], read through the `kind` getter.
//! - Tracing: each instance owns a native-only cppgc child (no JS wrapper) through a traced
//!   `v8::cppgc::Member`.
//! - Identity: the `info` getter goes through `deno_core::cppgc::SameObject`, deno_core's own
//!   per-instance wrapper cache, not a map or a permanent global of ours.

use std::cell::Cell;
use std::rc::Rc;
use std::rc::Weak;

use deno_core::GarbageCollected;
use deno_core::JsRuntime;
use deno_core::cppgc::SameObject;
use deno_core::op2;
use deno_core::v8;

use crate::Log;
use crate::MemoryLoader;
use crate::err;
use crate::new_runtime_with;
use crate::script;
use crate::string_script;

/// Finalizer counts, observed from Rust. Shared by `Rc` so the test can read them after the
/// runtime that ran the finalizers is gone.
#[derive(Default)]
pub struct Drops {
    pub instances: Cell<usize>,
    pub children: Cell<usize>,
    pub infos: Cell<usize>,
}

/// Class metadata: owned Rust data, never a borrowed stack or `static` pointer.
pub struct ClassMeta {
    pub name: String,
}

/// The native object behind each JavaScript wrapper.
pub struct NativeClass {
    meta: Rc<ClassMeta>,
    value: String,
    child: v8::cppgc::Member<NativeChild>,
    info: SameObject<NativeInfo>,
    drops: Rc<Drops>,
}

// SAFETY: `trace` visits the only cppgc pointer the object holds (`child`). `info` keeps its
// wrapper through a `v8::Global` root (deno_core's `SameObject` design), not a traced edge.
unsafe impl GarbageCollected for NativeClass {
    fn trace(&self, visitor: &mut v8::cppgc::Visitor) {
        visitor.trace(&self.child);
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"NativeClass"
    }
}

impl Drop for NativeClass {
    fn drop(&mut self) {
        self.drops.instances.set(self.drops.instances.get() + 1);
    }
}

#[op2]
impl NativeClass {
    /// The owned class metadata, shared by every instance.
    #[getter]
    #[string]
    fn kind(&self) -> String {
        self.meta.name.clone()
    }

    /// This instance's own native value.
    #[getter]
    #[string]
    fn value(&self) -> String {
        self.value.clone()
    }

    /// Read through the traced `Member`: only valid while tracing keeps the child alive.
    #[getter]
    #[string]
    fn child(&self) -> String {
        // SAFETY: `child` is set at construction and visited by `trace`.
        unsafe { self.child.get() }.map(|child| child.label.clone()).unwrap_or_default()
    }

    /// The same wrapper object on every read, created on first use by `SameObject`. No return
    /// attribute: deno_ops 0.289.0 parses `v8::Global<v8::$T>` as an unknown type
    /// (`op2/signature.rs:1517`) and converts it with deno_core's `ToV8 for v8::Global<T>`
    /// (`convert.rs:1322`).
    #[getter]
    fn info(&self, scope: &mut v8::PinScope) -> v8::Global<v8::Object> {
        self.info.get(scope, |_| NativeInfo { drops: self.drops.clone() })
    }
}

/// Native-only child: reachable solely through its parent's traced `Member`.
pub struct NativeChild {
    label: String,
    drops: Rc<Drops>,
}

// SAFETY: holds no cppgc pointers.
unsafe impl GarbageCollected for NativeChild {
    fn trace(&self, _visitor: &mut v8::cppgc::Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"NativeChild"
    }
}

impl Drop for NativeChild {
    fn drop(&mut self) {
        self.drops.children.set(self.drops.children.get() + 1);
    }
}

/// Object handed out by the `info` getter; unregistered, so it uses deno_core's generic
/// cppgc template.
pub struct NativeInfo {
    drops: Rc<Drops>,
}

// SAFETY: holds no cppgc pointers.
unsafe impl GarbageCollected for NativeInfo {
    fn trace(&self, _visitor: &mut v8::cppgc::Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"NativeInfo"
    }
}

impl Drop for NativeInfo {
    fn drop(&mut self) {
        self.drops.infos.set(self.drops.infos.get() + 1);
    }
}

deno_core::extension!(roves_native_class_probe, objects = [NativeClass]);

/// Allocates one instance with its native child and returns its wrapper. The child is held by
/// a `cppgc::Persistent` root until the parent is wrapped, so its liveness never depends on
/// conservative stack scanning while the wrapper allocation may trigger a GC.
fn make_instance<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    meta: &Rc<ClassMeta>,
    value: &str,
    drops: &Rc<Drops>,
) -> Result<v8::Local<'s, v8::Object>, String> {
    let (child, root) = {
        let heap = scope.get_cpp_heap().ok_or("the isolate has no CppHeap")?;
        // SAFETY: the pointer is immediately rooted by a Persistent and stored in a Member.
        let pointer = unsafe {
            v8::cppgc::make_garbage_collected(
                heap,
                NativeChild { label: format!("{value}-child"), drops: drops.clone() },
            )
        };
        (v8::cppgc::Member::new(&pointer), v8::cppgc::Persistent::new(&pointer))
    };
    let native = NativeClass {
        meta: meta.clone(),
        value: value.to_owned(),
        child,
        info: SameObject::new(),
        drops: drops.clone(),
    };
    let wrapper = deno_core::cppgc::make_cppgc_object(scope, native);
    drop(root);
    Ok(wrapper)
}

/// What `native_class_ownership` observed.
pub struct NativeClassReport {
    /// What the game script logged while both wrappers were reachable.
    pub log: Log,
    /// Strong `ClassMeta` references right after its creator dropped its own `Rc`.
    pub meta_strong_after_creator_drop: usize,
    /// `[instances, children, infos]` finalized after a full GC with both wrappers reachable.
    pub drops_while_reachable: [usize; 3],
    /// Values read by a second script after that GC.
    pub after_gc: String,
    /// `[instances, children, infos]` finalized after the roots were deleted and V8 was asked
    /// to collect, before teardown. Observational only (see README.md).
    pub drops_after_unroot: [usize; 3],
    /// `[instances, children, infos]` finalized once the runtime was dropped.
    pub drops_after_teardown: [usize; 3],
    pub meta_alive_after_teardown: bool,
}

fn counts(drops: &Drops) -> [usize; 3] {
    [drops.instances.get(), drops.children.get(), drops.infos.get()]
}

/// V8's public `LowMemoryNotification`: full GCs of the JS and cppgc heaps, with no test flag,
/// no `--expose-gc` and nothing visible to scripts. Called with no open `HandleScope`, so no
/// `Local` of ours roots a wrapper. cppgc may still scan the native stack conservatively.
fn collect(runtime: &mut JsRuntime) {
    runtime.v8_isolate().low_memory_notification();
}

// An IIFE: top-level `const`s of a classic script would live on in the global lexical
// environment and keep the wrappers reachable after `unroot.js`.
const GAME_CHECKS: &str = r#"(() => {
  const log = [];
  const a = globalThis.nativeA, b = globalThis.nativeB;
  const proto = Object.getPrototypeOf(a);
  log.push("same-proto:" + (proto === Object.getPrototypeOf(b)));
  log.push("constructor:" + proto.constructor.name);
  log.push("instanceof:" + (a instanceof proto.constructor && b instanceof proto.constructor));
  log.push("own-props:" + Object.getOwnPropertyNames(a).length);
  log.push("accessors:" + ["kind", "value", "child", "info"].map(k => typeof Object.getOwnPropertyDescriptor(proto, k)?.get).join());
  log.push("kind:" + a.kind + ":" + b.kind);
  log.push("value:" + a.value + ":" + b.value);
  log.push("child:" + a.child + ":" + b.child);
  log.push("info-stable:" + (a.info === a.info && b.info === b.info));
  log.push("info-distinct:" + (a.info !== b.info));
  log.push("distinct:" + (a !== b));
  for (const [label, run] of [
    ["foreign-receiver", () => Object.getOwnPropertyDescriptor(proto, "value").get.call({})],
    ["constructed-receiver", () => new proto.constructor().value],
  ]) {
    try { run(); log.push(label + ":returned"); } catch (e) { log.push(label + ":threw"); }
  }
  return JSON.stringify(log);
})()"#;

/// Two instances of one native class share owned metadata, keep their own values and native
/// children, have stable wrapper identity and a shared prototype; metadata outlives its Rust
/// creator while wrappers are reachable; everything is finalized at teardown.
pub fn native_class_ownership() -> Result<NativeClassReport, String> {
    crate::block_on(async {
        let drops = Rc::new(Drops::default());
        let meta_weak: Weak<ClassMeta>;
        let mut runtime = new_runtime_with(Rc::new(MemoryLoader::new(&[])), vec![roves_native_class_probe::init()])?;

        let meta_strong_after_creator_drop = {
            // Built at runtime: heap data owned by the creator, not a literal or a static.
            let creator = Rc::new(ClassMeta { name: ["Native", "Class"].concat() });
            meta_weak = Rc::downgrade(&creator);
            deno_core::scope!(scope, runtime);
            let context = scope.get_current_context();
            let global = context.global(scope);
            for (name, value) in [("nativeA", "alpha"), ("nativeB", "beta")] {
                let wrapper = make_instance(scope, &creator, value, &drops)?;
                let key = v8::String::new(scope, name).ok_or("cannot allocate a V8 string")?;
                global.set(scope, key.into(), wrapper.into()).ok_or("setting a global threw")?;
            }
            drop(creator);
            meta_weak.strong_count()
        };

        let json = string_script(&mut runtime, "native-class.js", GAME_CHECKS)?;
        let log: Log = deno_core::serde_json::from_str(&json).map_err(err)?;

        collect(&mut runtime);
        let drops_while_reachable = counts(&drops);
        let after_gc = string_script(
            &mut runtime,
            "after-gc.js",
            "[nativeA, nativeB].map(o => o.kind + '/' + o.value + '/' + o.child).join()",
        )?;

        script(&mut runtime, "unroot.js", "delete globalThis.nativeA; delete globalThis.nativeB;")?;
        collect(&mut runtime);
        let drops_after_unroot = counts(&drops);

        drop(runtime);
        Ok(NativeClassReport {
            log,
            meta_strong_after_creator_drop,
            drops_while_reachable,
            after_gc,
            drops_after_unroot,
            drops_after_teardown: counts(&drops),
            meta_alive_after_teardown: meta_weak.upgrade().is_some(),
        })
    })
}
