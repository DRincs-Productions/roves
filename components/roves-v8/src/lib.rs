//! Minimal, contained V8 runtime integration — Phases 1-2 of `../../docs/V8_MIGRATION.md`.
//!
//! This crate owns V8 platform/isolate/context lifecycle and exposes a small, engine-neutral
//! surface: no `v8::*` type appears in this crate's public API, matching the migration plan's
//! hard architectural rule that V8 types must not cross the scripting boundary. It is not wired
//! into Roves/Servo's production script engine yet; see the plan document for the full phase
//! sequence and this phase's own checkpoint.
//!
//! Scope note for anyone extending this file: this version of the `v8` crate uses `!Unpin`,
//! pinned scopes (`v8::scope!`/`v8::tc_scope!` macros, not `HandleScope::new`/`TryCatch::new`
//! called directly — see `docs/V8_MIGRATION.md`'s Phase 1 status note for why), and only one
//! isolate may be entered per OS thread at a time. Exception-reading and value-conversion logic
//! below is duplicated inline at each call site rather than factored into shared generic
//! functions: earlier attempts at that ran into the scope macros' anonymous, lifetime-heavy
//! generated types being awkward to name in a function signature. A handful of duplicated lines
//! is a fine trade against fighting that.

extern crate self as roves_v8;

#[cfg(feature = "webidl-pilot")]
pub mod webidl {
    pub mod screen {
        include!(concat!(env!("OUT_DIR"), "/ScreenV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod utf16_string_state {
        include!(concat!(env!("OUT_DIR"), "/Utf16StringStateV8Binding.rs"));
    }
    pub mod validity_state {
        include!(concat!(env!("OUT_DIR"), "/ValidityStateV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod mutable_primitives {
        include!(concat!(env!("OUT_DIR"), "/MutablePrimitivesV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod nullable_domstring {
        include!(concat!(env!("OUT_DIR"), "/NullableDomStringV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod usv_strings {
        include!(concat!(env!("OUT_DIR"), "/UsvStringsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod string_operations {
        include!(concat!(env!("OUT_DIR"), "/StringOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod nullable_operations {
        include!(concat!(env!("OUT_DIR"), "/NullableOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_operations {
        include!(concat!(env!("OUT_DIR"), "/OptionalOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod bytestring_operations {
        include!(concat!(env!("OUT_DIR"), "/ByteStringOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_defaults {
        include!(concat!(env!("OUT_DIR"), "/OptionalDefaultsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_string_defaults {
        include!(concat!(env!("OUT_DIR"), "/OptionalStringDefaultsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_nullable_string_defaults {
        include!(concat!(env!("OUT_DIR"), "/OptionalNullableStringDefaultsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod enum_operations {
        include!(concat!(env!("OUT_DIR"), "/EnumOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod inheritance_base {
        include!(concat!(env!("OUT_DIR"), "/InheritanceBaseV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod inheritance_derived {
        include!(concat!(env!("OUT_DIR"), "/InheritanceDerivedV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod constructible_counter {
        include!(concat!(env!("OUT_DIR"), "/ConstructibleCounterV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod constructible_child {
        include!(concat!(env!("OUT_DIR"), "/ConstructibleChildV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod throwing_operations {
        include!(concat!(env!("OUT_DIR"), "/ThrowingOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod unforgeable_base {
        include!(concat!(env!("OUT_DIR"), "/UnforgeableBaseV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod unforgeable_child {
        include!(concat!(env!("OUT_DIR"), "/UnforgeableChildV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod web_idl_constants {
        include!(concat!(env!("OUT_DIR"), "/WebIdlConstantsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod linked_node {
        include!(concat!(env!("OUT_DIR"), "/LinkedNodeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod linked_leaf {
        include!(concat!(env!("OUT_DIR"), "/LinkedLeafV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod exposure_gated {
        include!(concat!(env!("OUT_DIR"), "/ExposureGatedV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod hidden_interface {
        include!(concat!(env!("OUT_DIR"), "/HiddenInterfaceV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod overloaded_operations {
        include!(concat!(env!("OUT_DIR"), "/OverloadedOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod ce_reactive {
        include!(concat!(env!("OUT_DIR"), "/CeReactiveV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod callback_operations {
        include!(concat!(env!("OUT_DIR"), "/CallbackOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod handler_host {
        include!(concat!(env!("OUT_DIR"), "/HandlerHostV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod sequence_operations {
        include!(concat!(env!("OUT_DIR"), "/SequenceOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod dictionary_probe {
        include!(concat!(env!("OUT_DIR"), "/DictionaryProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod listener_target {
        include!(concat!(env!("OUT_DIR"), "/ListenerTargetV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod token_probe {
        include!(concat!(env!("OUT_DIR"), "/TokenProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod attribute_probe {
        include!(concat!(env!("OUT_DIR"), "/AttributeProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod variadic_operations {
        include!(concat!(env!("OUT_DIR"), "/VariadicOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod json_base {
        include!(concat!(env!("OUT_DIR"), "/JsonBaseV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod json_child {
        include!(concat!(env!("OUT_DIR"), "/JsonChildV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod namespace_probe {
        include!(concat!(env!("OUT_DIR"), "/NamespaceProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod overloaded_constructor {
        include!(concat!(env!("OUT_DIR"), "/OverloadedConstructorV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod factory_probe {
        include!(concat!(env!("OUT_DIR"), "/FactoryProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod holder {
        include!(concat!(env!("OUT_DIR"), "/HolderV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod gate {
        include!(concat!(env!("OUT_DIR"), "/GateV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod pair_list {
        include!(concat!(env!("OUT_DIR"), "/PairListV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod record_probe {
        include!(concat!(env!("OUT_DIR"), "/RecordProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod failure_probe {
        include!(concat!(env!("OUT_DIR"), "/FailureProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod filter_probe {
        include!(concat!(env!("OUT_DIR"), "/FilterProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod token_set {
        include!(concat!(env!("OUT_DIR"), "/TokenSetV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod score_map {
        include!(concat!(env!("OUT_DIR"), "/ScoreMapV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod storage_probe {
        include!(concat!(env!("OUT_DIR"), "/StorageProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod data_map_probe {
        include!(concat!(env!("OUT_DIR"), "/DataMapProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod location_probe {
        include!(concat!(env!("OUT_DIR"), "/LocationProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod global_base {
        include!(concat!(env!("OUT_DIR"), "/GlobalBaseV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod global_scope_probe {
        include!(concat!(env!("OUT_DIR"), "/GlobalScopeProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod window_probe {
        include!(concat!(env!("OUT_DIR"), "/WindowProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod promise_operations {
        include!(concat!(env!("OUT_DIR"), "/PromiseOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod defaults_probe {
        include!(concat!(env!("OUT_DIR"), "/DefaultsProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod buffer_operations {
        include!(concat!(env!("OUT_DIR"), "/BufferOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod path_probe {
        include!(concat!(env!("OUT_DIR"), "/PathProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod draw_probe {
        include!(concat!(env!("OUT_DIR"), "/DrawProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod gradient_probe {
        include!(concat!(env!("OUT_DIR"), "/GradientProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod style_probe {
        include!(concat!(env!("OUT_DIR"), "/StyleProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod item_list {
        include!(concat!(env!("OUT_DIR"), "/ItemListV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod item_list_child {
        include!(concat!(env!("OUT_DIR"), "/ItemListChildV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod named_collection {
        include!(concat!(env!("OUT_DIR"), "/NamedCollectionV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod lenient_probe {
        include!(concat!(env!("OUT_DIR"), "/LenientProbeV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod shadow_base {
        include!(concat!(env!("OUT_DIR"), "/ShadowBaseV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod shadow_child {
        include!(concat!(env!("OUT_DIR"), "/ShadowChildV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod type_gaps_probe {
        include!(concat!(env!("OUT_DIR"), "/TypeGapsProbeV8Binding.rs"));
    }
}

use std::sync::Once;

static V8_PLATFORM_INIT: Once = Once::new();

/// Initializes the V8 platform exactly once per process. Safe to call repeatedly; only the
/// first call has an effect. Must run before any [`Runtime`] is created.
/// Initializes the process-wide V8 platform once, for every crate that creates isolates
/// (`roves-js`, the mozjs API on V8, shares it). Idempotent.
#[doc(hidden)]
pub fn initialize_engine() {
    ensure_platform_initialized();
}

/// Like [`initialize_engine`], also applying `flags` (for example `--expose-gc` in another
/// crate's tests) if the platform is not initialized yet; later calls cannot change flags.
#[doc(hidden)]
pub fn initialize_engine_with_flags(flags: &str) {
    V8_PLATFORM_INIT.call_once(|| {
        v8::V8::set_flags_from_string(flags);
        initialize_platform();
    });
}

fn ensure_platform_initialized() {
    V8_PLATFORM_INIT.call_once(initialize_platform);
}

fn initialize_platform() {
    {
        // roves-js scans its root stack when cppgc traces its root set, like SpiderMonkey
        // scans rooted values at GC time. Without incremental/concurrent marking that scan
        // happens in one atomic pause and sees every root (correctness first; revisit with
        // write barriers on root updates).
        v8::V8::set_flags_from_string("--no-incremental-marking --no-concurrent-marking --no-parallel-marking");
        // See ../../docs/V8_MIGRATION.md's "Console-oriented requirement: JIT-less" — a
        // first-class supported configuration from the start. This must run before
        // `initialize_platform`/`initialize` below, not after.
        #[cfg(feature = "jitless")]
        v8::V8::set_flags_from_string("--jitless");

        // `Isolate::request_garbage_collection_for_testing` (used by this crate's own GC
        // stress tests, see docs/V8_MIGRATION.md's Phase 3 status note) only works if
        // `--expose-gc` was set here first — it "has a strong negative impact on garbage
        // collection performance" per its own doc comment, so this only applies to `cargo test`
        // builds, never a real embedding.
        #[cfg(test)]
        v8::V8::set_flags_from_string("--expose-gc");

        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    }
}

/// An engine-neutral snapshot of a JS value — see this module's own doc comment on why no
/// `v8::*` type crosses this boundary. `Object` is a one-way read-only marker: Phase 2 doesn't
/// need structural property access yet (that's Phase 4's WebIDL-bindings job), so a value that
/// isn't one of the primitives below just reports "it's some kind of object", and converting an
/// `Object` back into a JS value (e.g. via [`Runtime::store`]) produces `undefined` — there is
/// no data here to reconstruct the original object from.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Internal WebIDL argument state for an omitted optional argument (including explicit
    /// `undefined`). This is never a JavaScript value and cannot be stored back into V8.
    Missing,
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// WebIDL ByteString bytes; conversion to JavaScript preserves each byte as a code unit.
    ByteString(Vec<u8>),
    /// A lossless UTF-16 string for JavaScript strings containing unpaired surrogates, which
    /// Rust's UTF-8 `String` cannot represent. Ordinary scalar-valid strings use `String`.
    Utf16String(Vec<u16>),
    /// Raw bytes — round-trips through a JS `Uint8Array` (see [`Runtime::eval_value`] and
    /// [`Runtime::store`]). Distinct from `Object` because this phase's own bindgen-primitives
    /// scope explicitly needs ArrayBuffer/TypedArray, not just "some object".
    Bytes(Vec<u8>),
    Object,
    /// An arbitrary JS value kept as-is: WebIDL `any`, `object` and callback-function values.
    Js(Handle),
    /// A WebIDL sequence; converting it to JS creates an array.
    Sequence(Vec<Value>),
    /// A WebIDL union value: the index of the selected member type and the converted value.
    Union(usize, Box<Value>),
    /// A WebIDL dictionary: `(member name, value)` in member order, with [`Value::Missing`] for
    /// members that are absent. Converting it to JS creates a plain object of the present ones.
    Dictionary(Vec<(String, Value)>),
    /// A WebIDL `record<K, V>`: `(key, value)` pairs in the source object's property order, with
    /// keys converted to the (string) key type. Converting it to JS creates a plain object.
    Record(Vec<(Value, Value)>),
    /// A traced native DOM object (an interface-typed WebIDL value). Converting it to JS yields
    /// the native's one wrapper, created on first use as an instance of its concrete interface.
    Native(NativeRef),
}

/// Engine-neutral representation of the WebIDL restricted `float` type.
/// Construction rejects NaN and infinities, keeping the IDL invariant across the V8 boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteF32(f32);

impl FiniteF32 {
    pub fn new(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

/// Engine-neutral representation of the WebIDL restricted `double` type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteF64(f64);

impl FiniteF64 {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// A persistent reference to a JS value, outliving any single [`Runtime::eval`]/`eval_value`
/// call. Backed by a `v8::Global`, which is isolate-scoped rather than context-scoped, so it can
/// be read back (via [`Runtime::load`]) from a different fresh context than the one it was
/// created in — the migration plan's Phase 3 (GC/DOM ownership) will build the real reflector
/// identity/lifetime model on top of this same primitive; this phase only needs the handle
/// itself to work.
#[derive(Clone)]
pub struct Handle(v8::Global<v8::Value>);

impl PartialEq for Handle {
    /// Identity of the referenced JS value (the same object, or the same primitive handle).
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Handle")
    }
}

/// A named JS interface (constructor + prototype chain), created via
/// [`Runtime::define_interface`] — see that method's own doc comment. Cloning yields another
/// handle to the same interface (the runtime keeps one to re-expose it in a new global realm).
#[derive(Clone)]
pub struct Interface(std::rc::Rc<InterfaceData>);

impl std::ops::Deref for Interface {
    type Target = InterfaceData;

    fn deref(&self) -> &InterfaceData {
        &self.0
    }
}

/// The shared state behind an [`Interface`] handle.
pub struct InterfaceData {
    template: v8::Global<v8::FunctionTemplate>,
    /// The parent interface's template, so exposure can make the parent interface object
    /// this interface object's `[[Prototype]]` (see [`Runtime::expose_interface`]).
    parent_template: Option<v8::Global<v8::FunctionTemplate>>,
    /// `[LegacyUnforgeable]` accessors (own and inherited), by name with their getter
    /// template. `FunctionTemplate::inherit` does not carry instance-template accessor pairs
    /// into descendants, so each descendant reinstalls these on its own instance template.
    unforgeable_accessors: std::rc::Rc<std::cell::RefCell<Vec<(String, v8::Global<v8::FunctionTemplate>)>>>,
    /// Set once a descendant has been defined: it copied the unforgeable list at that point,
    /// so later unforgeable attributes on this interface would be missing from it.
    has_descendants: std::rc::Rc<std::cell::Cell<bool>>,
    name: String,
    /// Set once the constructor has actually been exposed on the global object (lazily, at the
    /// first [`Runtime::create_instance`] call — see that method's own doc comment on why this
    /// can't happen eagerly in [`Runtime::define_interface`]).
    constructor_exposed: std::cell::Cell<bool>,
    /// Whether exposure defines the interface object on the global (false for
    /// `[LegacyNoInterfaceObject]` and for interfaces whose `[Pref]`/`[SecureContext]` condition
    /// does not hold). Instances, prototypes and inheritance work either way.
    interface_object_on_global: std::cell::Cell<bool>,
    /// `[LegacyWindowAlias]` names: extra global properties for the same interface object.
    aliases: std::cell::RefCell<Vec<String>>,
    /// A WebIDL value iterable: the prototype gets `Array.prototype`'s iteration methods.
    value_iterable: std::cell::Cell<bool>,
    /// A WebIDL namespace (`console`, `CSS`): exposed as a plain object holding its operations.
    namespace: std::cell::Cell<bool>,
    /// A WebIDL pair iterable (`iterable<K, V>`): its config, which owns the iterator prototype.
    pair_iterable: std::cell::Cell<Option<*const PairIterableConfig>>,
    /// Readonly attributes with `[LegacyLenientSetter]`: assignments are silently ignored.
    lenient_setters: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Promise-returning operations: argument conversion errors reject instead of throwing.
    promise_operations: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Readonly attributes with `[Replaceable]`: assignments define an own data property.
    replaceable: std::cell::RefCell<std::collections::HashSet<String>>,
    /// `[ExceptionClass]` (`DOMException`): the prototype object inherits `Error.prototype`.
    exception_class: std::cell::Cell<bool>,
    /// Interface-level `[LegacyUnforgeable]` (`Location`): every regular attribute and operation
    /// is a non-configurable own property of each instance instead of a prototype member.
    unforgeable_members: std::cell::Cell<bool>,
    /// `[Global]` (`Window`, worker global scopes): regular attributes and operations are own
    /// properties of the global object (configurable, unlike `[LegacyUnforgeable]`).
    global: std::cell::Cell<bool>,
    /// `[LegacyFactoryFunction]`s (`Image`, `Audio`, `Option`): name and function template.
    legacy_factories: std::cell::RefCell<Vec<(String, v8::Global<v8::FunctionTemplate>)>>,
    // Materializing a child also freezes all ancestor templates. Shared flags track
    // that separately from whether each constructor has been exposed globally.
    materialized: std::rc::Rc<std::cell::Cell<bool>>,
    ancestors: Vec<std::rc::Rc<std::cell::Cell<bool>>>,
}

/// A single, isolated V8 execution environment: one [`v8::OwnedIsolate`] plus one persistent
/// JS realm (`context`), shared by every [`Runtime`] method — a global installed by
/// [`Runtime::define_native_function`] is only visible to a later [`Runtime::eval`] call because
/// both enter the *same* context. An earlier version of this crate created a brand-new,
/// throwaway context inside every method, which is why that sharing silently didn't work — each
/// call had its own global object, exactly like two different pages/realms in a real browser.
pub struct Runtime {
    isolate: v8::OwnedIsolate,
    context: v8::Global<v8::Context>,
    wrapper_identities: WrapperIdentityMap,
    next_wrapper_token: u64,
    /// Keeps callbacks armed until they have actually completed. An empty weak handle
    /// alone is insufficient: first-pass GC may clear it before second-pass finalization.
    /// Shared with constructor callbacks, which create wrappers from inside V8.
    wrapped_finalizers: std::rc::Rc<std::cell::RefCell<Vec<WrappedFinalizer>>>,
    /// Per-method callback data stays alive as long as this isolate can invoke the callbacks.
    method_configs: Vec<Box<WebIdlMethodConfig>>,
    /// Per-constructor callback data, kept alive for the same reason as `method_configs`.
    constructor_configs: Vec<Box<WebIdlConstructorConfig>>,
    /// Per-overloaded-method callback data, kept alive for the same reason.
    overload_configs: Vec<Box<WebIdlOverloadConfig>>,
    typed_overload_configs: Vec<Box<TypedOverloadConfig>>,
    /// Per-contextual-attribute callback data, kept alive for the same reason.
    attribute_configs: Vec<Box<ContextualAttributeConfig>>,
    /// Per-static-method and default-toJSON callback data, kept alive for the same reason.
    static_configs: Vec<Box<StaticMethodConfig>>,
    to_json_configs: Vec<Box<DefaultToJsonConfig>>,
    static_overload_configs: Vec<Box<StaticOverloadConfig>>,
    legacy_factory_configs: Vec<Box<LegacyFactoryConfig>>,
    pair_iterable_configs: Vec<Box<PairIterableConfig>>,
    like_configs: Vec<Box<LikeConfig>>,
    named_configs: Vec<Box<NamedConfig>>,
    indexed_configs: Vec<Box<IndexedConfig>>,
    /// Every interface defined, so [`Runtime::install_global`] can expose them in the new realm.
    interfaces: Vec<Interface>,
    /// The native of the `[Global]` object installed by [`Runtime::install_global`].
    global_native: Option<v8::cppgc::Persistent<GcBox>>,
}

struct WrappedFinalizer {
    _weak: v8::Weak<v8::Value>,
    completed: std::rc::Rc<std::cell::Cell<bool>>,
}

type WrapperIdentityMap =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<u64, CachedWrapper>>>;

struct CachedWrapper {
    token: u64,
    weak: v8::Weak<v8::Value>,
}

/// Tag passed to `set_aligned_pointer_in_internal_field`/`get_aligned_pointer_from_internal_field`
/// — required by the API. V8's sandboxed external-pointer table only accepts a small range of
/// tag values (found by hitting "Fatal error in ToExternalPointerTag: the provided tag is
/// outside the allowed range" with an arbitrary `0xC0DE` on the first attempt); `0` is what the
/// `v8` crate's own `tests/test_api.rs` uses for its single-tag internal-field examples, and this
/// crate likewise only has one kind of tagged pointer so far, so there's no need for a distinct
/// value per type yet.
const WRAPPED_POINTER_TAG: u16 = 0;

// ---------------------------------------------------------------------------------------------
// Traced native objects (Phase 3 production ownership model, prototype).
//
// Servo's DOM objects are owned by the JS engine's GC: SpiderMonkey traces them through
// `JSTraceable`, and DOM-to-DOM references are `Dom<T>` fields that the trace hook visits. The V8
// equivalent is the unified heap: native objects live on V8's cppgc heap, are traced together with
// JS objects in one marking pass, and are associated with their JS wrapper through
// `Object::wrap`. That lets cycles through native and JS objects (a DOM node whose event
// listener closes over the node's own wrapper) be collected, which the earlier
// `create_instance` model (a strong `Box` released by a weak-handle finalizer) cannot see.
//
// None of the cppgc types escape: the scripting layer sees `Trace`, `Tracer`, `GcMember`,
// `GcRoot` and `JsRef`, which mirror `JSTraceable`, `JSTracer`, `Dom<T>`, a rooted `DomRoot<T>`
// and `Heap<JSVal>` respectively.

/// A native object that may live on the traced heap. `trace` must report every [`GcMember`] and
/// [`JsRef`] the object holds — the same contract as Servo's `JSTraceable`. A missed reference
/// lets its target be collected while still referenced.
///
/// `#[derive(Trace)]` implements it by tracing every field, with the same `#[no_trace]` and
/// `#[custom_trace]` field attributes as `#[derive(JSTraceable)]`. Containers and references
/// implement it below; types that hold no traced reference implement it as a no-op.
pub trait Trace: 'static {
    fn trace(&self, tracer: &mut Tracer);
}

pub use roves_v8_derive::Trace;

/// Tracing for foreign types that cannot implement [`Trace`] (the orphan rule), selected with
/// `#[custom_trace]` on a field — the counterpart of Servo's `CustomTraceable`.
pub trait CustomTrace {
    fn trace(&self, tracer: &mut Tracer);
}

impl<T: Trace> Trace for GcMember<T> {
    fn trace(&self, tracer: &mut Tracer) {
        tracer.member(self);
    }
}

impl Trace for JsRef {
    fn trace(&self, tracer: &mut Tracer) {
        tracer.js(self);
    }
}

impl<T: Trace> Trace for Option<T> {
    fn trace(&self, tracer: &mut Tracer) {
        if let Some(value) = self {
            value.trace(tracer);
        }
    }
}

impl<T: Trace> Trace for Box<T> {
    fn trace(&self, tracer: &mut Tracer) {
        (**self).trace(tracer);
    }
}

impl<T: Trace> Trace for std::rc::Rc<T> {
    fn trace(&self, tracer: &mut Tracer) {
        (**self).trace(tracer);
    }
}

impl<T: Trace> Trace for Vec<T> {
    fn trace(&self, tracer: &mut Tracer) {
        for item in self {
            item.trace(tracer);
        }
    }
}

impl<T: Trace> Trace for std::collections::VecDeque<T> {
    fn trace(&self, tracer: &mut Tracer) {
        for item in self {
            item.trace(tracer);
        }
    }
}

impl<T: Trace, const N: usize> Trace for [T; N] {
    fn trace(&self, tracer: &mut Tracer) {
        for item in self {
            item.trace(tracer);
        }
    }
}

impl<K: 'static, V: Trace, S: 'static> Trace for std::collections::HashMap<K, V, S> {
    fn trace(&self, tracer: &mut Tracer) {
        for value in self.values() {
            value.trace(tracer);
        }
    }
}

impl<K: 'static, V: Trace> Trace for std::collections::BTreeMap<K, V> {
    fn trace(&self, tracer: &mut Tracer) {
        for value in self.values() {
            value.trace(tracer);
        }
    }
}

impl<T: Trace> Trace for std::cell::RefCell<T> {
    fn trace(&self, tracer: &mut Tracer) {
        // SAFETY: as with Servo's `DomRefCell::borrow_for_gc_trace`, tracing only reads the
        // value and runs on the mutator thread inside a GC; a `RefMut` held across the code that
        // triggered the GC does not write while the tracer reads.
        unsafe { &*self.as_ptr() }.trace(tracer);
    }
}

impl<A: Trace, B: Trace> Trace for (A, B) {
    fn trace(&self, tracer: &mut Tracer) {
        self.0.trace(tracer);
        self.1.trace(tracer);
    }
}

/// `Trace` for types that hold no traced reference.
macro_rules! no_trace {
    ($($ty:ty),* $(,)?) => {
        $(impl Trace for $ty {
            #[inline]
            fn trace(&self, _tracer: &mut Tracer) {}
        })*
    };
}

no_trace!(
    (), bool, char, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64,
    String, &'static str, std::time::Duration, std::time::Instant,
);

impl<T: Copy + 'static> Trace for std::cell::Cell<T> {
    #[inline]
    fn trace(&self, _tracer: &mut Tracer) {}
}

/// Passed to [`Trace::trace`] to report the references a traced object holds.
pub struct Tracer<'a> {
    visitor: &'a mut v8::cppgc::Visitor,
}

impl Tracer<'_> {
    /// Reports a strong reference to another traced native object.
    pub fn member<T: Trace>(&mut self, member: &GcMember<T>) {
        self.visitor.trace(&member.inner);
    }

    /// Reports a reference from this native object to a JS value.
    pub fn js(&mut self, reference: &JsRef) {
        self.visitor.trace(&reference.0);
    }
}

/// The single cppgc type behind every traced native: the concrete native is type-erased so the
/// existing `&dyn Any` callbacks (getters, setters, methods) work unchanged on traced objects.
struct GcBox {
    /// Pointed to by wrapper internal field 0, exactly like a `create_instance` native. Callbacks
    /// only take shared references to it; natives mutate through interior mutability.
    native: std::cell::UnsafeCell<Box<dyn std::any::Any>>,
    trace: fn(&dyn std::any::Any, &mut Tracer),
    /// The native's one JS wrapper, once created. Traced, like Blink's ScriptWrappable wrapper
    /// reference: the wrapper (and its expando properties) lives as long as the native, and
    /// the native/wrapper cycle is collected together once neither is reachable.
    wrapper: std::cell::UnsafeCell<Option<v8::TracedReference<v8::Object>>>,
    /// The interface the wrapper was created as; interface-typed arguments check it (and its
    /// ancestors) instead of the spoofable prototype chain.
    wrapper_interface: std::cell::UnsafeCell<Option<String>>,
}

// SAFETY: `trace` forwards to the native's `Trace` impl, which must visit every reference it
// holds (the documented contract of `Trace`).
unsafe impl v8::cppgc::GarbageCollected for GcBox {
    fn trace(&self, visitor: &mut v8::cppgc::Visitor) {
        // SAFETY: tracing runs on the mutator thread while no callback holds `&mut native`.
        let native = unsafe { &*self.native.get() };
        (self.trace)(native.as_ref(), &mut Tracer { visitor });
        // SAFETY: the wrapper slot is only written by create_traced_instance, never during GC.
        if let Some(wrapper) = unsafe { &*self.wrapper.get() } {
            visitor.trace(wrapper);
        }
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesTracedNative"
    }
}

fn trace_native<T: Trace>(native: &dyn std::any::Any, tracer: &mut Tracer) {
    native
        .downcast_ref::<T>()
        .expect("GcBox trace function matches its native type")
        .trace(tracer);
}

/// A strong reference from one traced native object to another, like Servo's `Dom<T>`. Valid only
/// as a field of a traced native whose [`Trace`] impl reports it.
pub struct GcMember<T: Trace> {
    inner: v8::cppgc::Member<GcBox>,
    native: std::marker::PhantomData<fn() -> T>,
}

impl<T: Trace> GcMember<T> {
    pub fn empty() -> Self {
        GcMember { inner: v8::cppgc::Member::empty(), native: std::marker::PhantomData }
    }

    /// A type-erased reference to the member's target as an instance of `interface`.
    ///
    /// # Safety
    /// Same contract as [`GcMember::get`].
    pub unsafe fn native_ref(&self, interface: &str) -> Option<NativeRef> {
        // SAFETY: forwarded to the caller's contract.
        unsafe { self.inner.get() }?;
        Some(NativeRef { gc: v8::cppgc::Persistent::new(&self.inner), interface: interface.to_owned() })
    }

    /// Points this member at `root`'s object (with the GC write barrier).
    pub fn set(&mut self, root: &GcRoot<T>) {
        self.inner.set(&root.inner);
    }

    /// The referenced native object.
    ///
    /// # Safety
    /// The native that owns this member must be reachable and must report the member from its
    /// [`Trace`] impl, so the target cannot have been collected.
    pub unsafe fn get(&self) -> Option<&T> {
        // SAFETY: forwarded to the caller's contract.
        unsafe { self.inner.get() }.map(|gc_box| {
            // SAFETY: only setter callbacks mutate the native, never while a getter borrows it.
            unsafe { &*gc_box.native.get() }
                .downcast_ref::<T>()
                .expect("GcMember type matches its native")
        })
    }
}

/// An off-heap strong root keeping a traced native alive, like a rooted `DomRoot<T>`. It must be
/// dropped before the [`Runtime`] that allocated it.
pub struct GcRoot<T: Trace> {
    inner: v8::cppgc::Persistent<GcBox>,
    native: std::marker::PhantomData<fn() -> T>,
}

impl<T: Trace> GcRoot<T> {
    pub fn get(&self) -> &T {
        let gc_box = self.inner.get().expect("a GcRoot always points at a live native");
        // SAFETY: only setter callbacks mutate the native, never while a root borrows it.
        unsafe { &*gc_box.native.get() }
            .downcast_ref::<T>()
            .expect("GcRoot type matches its native")
    }

    /// A member pointing at the same native, for storing in another traced native.
    pub fn member(&self) -> GcMember<T> {
        GcMember { inner: v8::cppgc::Member::new(&self.inner), native: std::marker::PhantomData }
    }
}

/// A reference from a traced native object to a JS value (for example a listener callback or
/// another object's wrapper), like Servo's `Heap<JSVal>`. Valid only as a field of a traced native
/// that reports it from [`Trace::trace`]; the native keeps the JS value alive, and a cycle through
/// both is collected when nothing else reaches it.
pub struct JsRef(v8::TracedReference<v8::Value>);

/// Tag for `Object::wrap` of traced natives (distinct from the internal-field pointer tag).
const TRACED_NATIVE_TAG: u16 = 1;

/// A type-erased root for a traced native, tagged with its concrete interface name: what an
/// interface-typed WebIDL attribute, return value or argument carries ([`Value::Native`]).
pub struct NativeRef {
    gc: v8::cppgc::Persistent<GcBox>,
    interface: String,
}

impl NativeRef {
    /// The concrete interface a wrapper is created as, if the native has none yet.
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// The native, if it is a `T`.
    pub fn get<T: Trace>(&self) -> Option<&T> {
        let gc_box = self.gc.get()?;
        // SAFETY: shared read; setters never run while a native reference is borrowed.
        unsafe { &*gc_box.native.get() }.downcast_ref::<T>()
    }

    /// Whether both refer to the same native object.
    pub fn same_native(&self, other: &NativeRef) -> bool {
        use v8::cppgc::GetRustObj;
        self.gc.get_rust_obj() == other.gc.get_rust_obj()
    }
}

impl Clone for NativeRef {
    fn clone(&self) -> Self {
        NativeRef { gc: v8::cppgc::Persistent::new(&self.gc), interface: self.interface.clone() }
    }
}

impl PartialEq for NativeRef {
    fn eq(&self, other: &Self) -> bool {
        self.same_native(other)
    }
}

impl std::fmt::Debug for NativeRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.debug_struct("NativeRef").field("interface", &self.interface).finish()
    }
}

impl<T: Trace> GcRoot<T> {
    /// A type-erased reference to this native as an instance of `interface` (its concrete,
    /// most-derived interface), for returning it from an interface-typed member.
    pub fn native_ref(&self, interface: &str) -> NativeRef {
        NativeRef { gc: v8::cppgc::Persistent::new(&self.inner), interface: interface.to_owned() }
    }
}

/// Every defined interface by name with its parent, shared through an isolate slot so callbacks
/// can create wrappers for returned natives and check interface-typed arguments.
#[derive(Default)]
struct InterfaceRegistry {
    templates: std::collections::HashMap<String, v8::Global<v8::FunctionTemplate>>,
    parents: std::collections::HashMap<String, Option<String>>,
}

type SharedInterfaceRegistry = std::rc::Rc<std::cell::RefCell<InterfaceRegistry>>;

impl InterfaceRegistry {
    fn inherits(&self, interface: &str, ancestor: &str) -> bool {
        let mut current = Some(interface.to_owned());
        while let Some(name) = current {
            if name == ancestor {
                return true;
            }
            current = self.parents.get(&name).cloned().flatten();
        }
        false
    }
}

/// Returns `gc_box`'s wrapper, creating it from `template` (as `interface`) on first use.
fn traced_wrapper<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<'s, v8::FunctionTemplate>,
    interface: &str,
    gc: &v8::cppgc::Persistent<GcBox>,
) -> v8::Local<'s, v8::Object> {
    let gc_box = gc.get().expect("a traced root always points at a live native");
    // SAFETY: shared read; the slot is only written below, on this thread.
    if let Some(existing) = unsafe { &*gc_box.wrapper.get() }.as_ref().and_then(|wrapper| wrapper.get(scope)) {
        return existing;
    }
    let object = template
        .instance_template(scope)
        .new_instance(scope)
        .expect("a freshly created ObjectTemplate instance should never fail");
    attach_traced_wrapper(scope, object, interface, gc);
    object
}

/// The setter of a readonly `[LegacyLenientSetter]` attribute: assignments are ignored (it
/// exists so that `obj.attr = x` does not throw in strict mode), or `None` for other attributes.
fn lenient_setter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    interface: &Interface,
    name: &str,
) -> Option<v8::Local<'s, v8::FunctionTemplate>> {
    if interface.replaceable.borrow().contains(name) {
        // [Replaceable]: the assignment shadows the attribute with an own data property.
        let key = v8::String::new(scope, name)?;
        let setter = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue| {
                if args.length() == 0 {
                    throw_type_error(scope, "Not enough arguments");
                    return;
                }
                let Ok(name) = v8::Local::<v8::Name>::try_from(args.data()) else { return };
                args.this().create_data_property(scope, name, args.get(0));
            },
        )
        .data(key.into())
        .length(1)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        setter.set_class_name(v8::String::new(scope, &format!("set {name}"))?);
        return Some(setter);
    }
    if !interface.lenient_setters.borrow().contains(name) {
        return None;
    }
    let setter = v8::FunctionTemplate::builder(
        |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue| {
            if args.length() == 0 {
                throw_type_error(scope, "Not enough arguments");
            }
        },
    )
    .length(1)
    .constructor_behavior(v8::ConstructorBehavior::Throw)
    .build(scope);
    setter.set_class_name(v8::String::new(scope, &format!("set {name}"))?);
    Some(setter)
}

/// Where a regular member of `interface` is defined: the prototype, or for an interface-level
/// `[LegacyUnforgeable]` each instance, as a non-configurable (and, for operations, read-only)
/// own property.
fn member_target<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    interface: &Interface,
    template: v8::Local<'s, v8::FunctionTemplate>,
    operation: bool,
) -> (v8::Local<'s, v8::ObjectTemplate>, v8::PropertyAttribute) {
    if !interface.unforgeable_members.get() {
        if interface.global.get() {
            // [Global]: on the global object itself, with the usual (configurable) attributes.
            return (template.instance_template(scope), v8::PropertyAttribute::NONE);
        }
        return (template.prototype_template(scope), v8::PropertyAttribute::NONE);
    }
    let attributes = if operation {
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE
    } else {
        v8::PropertyAttribute::DONT_DELETE
    };
    (template.instance_template(scope), attributes)
}

/// Runs a promise-returning operation's pre-native steps (overload selection, argument
/// conversion). WebIDL: when one throws, the operation returns a promise rejected with the
/// exception instead of throwing. Other operations just run `step`.
fn reject_on_failure<T>(
    scope: &mut v8::PinScope,
    promise: bool,
    retval: &mut v8::ReturnValue,
    step: impl FnOnce(&mut v8::PinScope) -> Option<T>,
) -> Option<T> {
    if !promise {
        return step(scope);
    }
    v8::tc_scope!(let try_catch, scope);
    let result = step(try_catch);
    if result.is_none() {
        let exception = try_catch.exception().unwrap_or_else(|| v8::undefined(try_catch).into());
        try_catch.reset();
        if let Some(resolver) = v8::PromiseResolver::new(try_catch) {
            resolver.reject(try_catch, exception);
            retval.set(resolver.get_promise(try_catch).into());
        }
    }
    result
}

/// Moves a newly constructed native onto the traced heap with `object` as its one wrapper.
fn attach_new_traced_native(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, interface: &str, native: TracedNative) {
    let gc_box = GcBox {
        native: std::cell::UnsafeCell::new(native.native),
        trace: native.trace,
        wrapper: std::cell::UnsafeCell::new(None),
        wrapper_interface: std::cell::UnsafeCell::new(None),
    };
    let heap = scope.get_cpp_heap().expect("V8 isolates carry a cppgc heap");
    // SAFETY: moved into a Persistent before anything else can run a GC.
    let pointer = unsafe { v8::cppgc::make_garbage_collected(heap, gc_box) };
    let root = v8::cppgc::Persistent::new(&pointer);
    // The wrapper now keeps the native alive; the temporary root then goes away.
    attach_traced_wrapper(scope, object, interface, &root);
}

/// Makes `object` the wrapper of `gc`'s native, created as `interface`.
fn attach_traced_wrapper(
    scope: &mut v8::PinScope,
    object: v8::Local<v8::Object>,
    interface: &str,
    gc: &v8::cppgc::Persistent<GcBox>,
) {
    let gc_box = gc.get().expect("a traced root always points at a live native");
    object.set_aligned_pointer_in_internal_field(
        0,
        gc_box.native.get() as *const std::ffi::c_void,
        WRAPPED_POINTER_TAG,
    );
    // SAFETY: TRACED_NATIVE_TAG is used only for GcBox wrappers in this runtime.
    unsafe { v8::Object::wrap::<TRACED_NATIVE_TAG, GcBox>(scope, object, gc) };
    // SAFETY: no other reference to these slots is live; TracedReference creation performs
    // the GC write barrier.
    unsafe {
        *gc_box.wrapper.get() = Some(v8::TracedReference::new(scope, object));
        *gc_box.wrapper_interface.get() = Some(interface.to_owned());
    }
}

/// Converts a callback result to JS, wrapping [`Value::Native`] results.
fn v8_result<'s>(scope: &mut v8::PinScope<'s, '_>, value: &Value) -> v8::Local<'s, v8::Value> {
    if let Value::Union(_, value) = value {
        return v8_result(scope, value);
    }
    if let Value::Dictionary(entries) = value {
        // Members may be natives, which need wrapping.
        let object = v8::Object::new(scope);
        for (name, member) in entries {
            if *member == Value::Missing {
                continue;
            }
            let key = v8::String::new(scope, name).unwrap();
            let member = v8_result(scope, member);
            object.create_data_property(scope, key.into(), member);
        }
        return object.into();
    }
    if let Value::Record(entries) = value {
        // Values may be natives, which need wrapping.
        let object = v8::Object::new(scope);
        for (key, item) in entries {
            let key = v8_value(scope, key);
            let item = v8_result(scope, item);
            object.create_data_property(scope, key.try_into().unwrap(), item);
        }
        return object.into();
    }
    if let Value::Sequence(elements) = value {
        // Elements may be natives, which need wrapping.
        let mut converted = Vec::with_capacity(elements.len());
        for element in elements {
            converted.push(v8_result(scope, element));
        }
        return v8::Array::new_with_elements(scope, &converted).into();
    }
    let Value::Native(native) = value else {
        return v8_value(scope, value);
    };
    let registry = scope
        .get_slot::<SharedInterfaceRegistry>()
        .expect("runtime installs the interface registry")
        .clone();
    let template = {
        let registry = registry.borrow();
        let Some(template) = registry.templates.get(&native.interface) else {
            throw_type_error(scope, "native object of an undefined interface");
            return v8::undefined(scope).into();
        };
        v8::Local::new(scope, template)
    };
    traced_wrapper(scope, template, &native.interface, &native.gc).into()
}

/// The traced native behind `value` if it implements `expected` (or a descendant); never throws.
fn implementing_native(scope: &mut v8::PinScope, value: v8::Local<v8::Value>, expected: &str) -> Option<NativeRef> {
    let object = v8::Local::<v8::Object>::try_from(value).ok()?;
    if !object.is_api_wrapper() {
        return None;
    }
    // SAFETY: TRACED_NATIVE_TAG wrappers always hold a GcBox; the pointer moves straight into
    // a Persistent.
    let pointer = unsafe { v8::Object::unwrap::<TRACED_NATIVE_TAG, GcBox>(scope, object) }?;
    // SAFETY: the value keeps the wrapper, and so its GcBox, alive.
    let gc_box = unsafe { pointer.as_ref() };
    // SAFETY: shared read of a slot written only when the wrapper was created.
    let interface = unsafe { &*gc_box.wrapper_interface.get() }.clone()?;
    let registry = scope.get_slot::<SharedInterfaceRegistry>()?.clone();
    if !registry.borrow().inherits(&interface, expected) {
        return None;
    }
    Some(NativeRef { gc: v8::cppgc::Persistent::new(&pointer), interface })
}

/// Converts an interface-typed argument: a traced wrapper whose interface is `expected` or
/// descends from it. `None` means a TypeError is pending.
fn native_argument(
    scope: &mut v8::PinScope,
    argument: v8::Local<v8::Value>,
    expected: &str,
) -> Option<Value> {
    let not_an_instance = |scope: &mut v8::PinScope| {
        throw_type_error(scope, &format!("argument is not an object implementing {expected}"));
        None
    };
    let Ok(object) = v8::Local::<v8::Object>::try_from(argument) else {
        return not_an_instance(scope);
    };
    // `Object::unwrap` is only defined for API wrappers: on an ordinary object it reads an
    // arbitrary field instead of returning null.
    if !object.is_api_wrapper() {
        return not_an_instance(scope);
    }
    // SAFETY: TRACED_NATIVE_TAG wrappers always hold a GcBox; the pointer moves straight into
    // a Persistent.
    let Some(pointer) = (unsafe { v8::Object::unwrap::<TRACED_NATIVE_TAG, GcBox>(scope, object) }) else {
        return not_an_instance(scope);
    };
    // SAFETY: the argument keeps the wrapper, and so its GcBox, alive.
    let gc_box = unsafe { pointer.as_ref() };
    // SAFETY: shared read of a slot written only when the wrapper was created.
    let Some(interface) = unsafe { &*gc_box.wrapper_interface.get() }.clone() else {
        return not_an_instance(scope);
    };
    let registry = scope.get_slot::<SharedInterfaceRegistry>().expect("runtime installs the interface registry").clone();
    if !registry.borrow().inherits(&interface, expected) {
        return not_an_instance(scope);
    }
    Some(Value::Native(NativeRef { gc: v8::cppgc::Persistent::new(&pointer), interface }))
}

/// The settling side of a promise created by [`ScriptContext::new_promise`]. It keeps the
/// promise alive until settled; settling twice has no effect.
#[derive(Clone)]
pub struct PromiseResolver(v8::Global<v8::PromiseResolver>);

impl std::fmt::Debug for PromiseResolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("PromiseResolver")
    }
}

/// What a native WebIDL member that needs the JS engine receives, like the `cx` Servo passes to
/// its DOM methods: it can call JS functions, read JS values and create references to them,
/// without exposing engine types. It exists only for the duration of one native call.
pub struct ScriptContext<'a, 's, 'i> {
    scope: &'a mut v8::PinScope<'s, 'i>,
}

impl ScriptContext<'_, '_, '_> {
    /// Calls `function` with `this` and `arguments`. An exception the function throws comes back
    /// as [`WebIdlError::Js`], ready to rethrow by returning it from the native member.
    pub fn call(&mut self, function: &Handle, this: &Value, arguments: &[Value]) -> Result<Value, WebIdlError> {
        let scope = &mut *self.scope;
        let function = v8::Local::new(scope, &function.0);
        let Ok(function) = v8::Local::<v8::Function>::try_from(function) else {
            return Err(WebIdlError::TypeError("value is not callable".into()));
        };
        let this = v8_result(scope, this);
        let mut converted = Vec::with_capacity(arguments.len());
        for argument in arguments {
            converted.push(v8_result(scope, argument));
        }
        v8::tc_scope!(let try_catch, scope);
        match function.call(try_catch, this, &converted) {
            Some(result) => Ok(Value::Js(Handle(v8::Global::new(try_catch, result)))),
            None => {
                let exception = try_catch
                    .exception()
                    .unwrap_or_else(|| v8::undefined(try_catch).into());
                Err(WebIdlError::Js(Handle(v8::Global::new(try_catch, exception))))
            },
        }
    }

    /// WebIDL "call a user object's operation" for a callback interface value such as an
    /// `EventListener`: a callable object is called directly (with `this` undefined); otherwise
    /// its `operation` method is called with the object as `this`. A missing method is a
    /// TypeError; exceptions come back as [`WebIdlError::Js`].
    pub fn call_user_object_operation(
        &mut self,
        object: &Handle,
        operation: &str,
        arguments: &[Value],
    ) -> Result<Value, WebIdlError> {
        let local = v8::Local::new(self.scope, &object.0);
        if local.is_function() {
            return self.call(object, &Value::Undefined, arguments);
        }
        let Ok(target) = v8::Local::<v8::Object>::try_from(local) else {
            return Err(WebIdlError::TypeError("callback interface value is not an object".into()));
        };
        let key = v8::String::new(self.scope, operation).unwrap();
        let method = {
            v8::tc_scope!(let try_catch, self.scope);
            match target.get(try_catch, key.into()) {
                Some(method) => Ok(v8::Global::new(try_catch, method)),
                None => Err(try_catch.exception().map(|exception| v8::Global::new(try_catch, exception))),
            }
        };
        let method = match method {
            Ok(method) => Handle(method),
            Err(Some(exception)) => return Err(WebIdlError::Js(Handle(exception))),
            Err(None) => return Err(WebIdlError::TypeError("callback operation lookup failed".into())),
        };
        if !v8::Local::new(self.scope, &method.0).is_function() {
            return Err(WebIdlError::TypeError(format!("callback interface object has no callable {operation}")));
        }
        self.call(&method, &Value::Js(object.clone()), arguments)
    }

    /// `object[name] = value` (an ordinary `[[Set]]`, so setters run). Exceptions come back as
    /// [`WebIdlError::Js`]; a non-object target is a TypeError. Used by `[PutForwards]`.
    pub fn set_property(&mut self, object: &Handle, name: &str, value: &Value) -> Result<(), WebIdlError> {
        let local = v8::Local::new(self.scope, &object.0);
        let Ok(target) = v8::Local::<v8::Object>::try_from(local) else {
            return Err(WebIdlError::TypeError("cannot set a property on a non-object".into()));
        };
        let value = v8_result(self.scope, value);
        let key = v8::String::new(self.scope, name).unwrap();
        v8::tc_scope!(let try_catch, self.scope);
        match target.set(try_catch, key.into(), value) {
            Some(_) => Ok(()),
            None => {
                let exception = try_catch
                    .exception()
                    .unwrap_or_else(|| v8::undefined(try_catch).into());
                Err(WebIdlError::Js(Handle(v8::Global::new(try_catch, exception))))
            },
        }
    }

    /// Runs `access` on the bytes of a buffer source in place (a view's own range, or a whole
    /// `ArrayBuffer`); `None` if `buffer` is not one. A detached buffer is empty. The bytes are
    /// the live JS memory: writes are visible to JS, and must not be kept past `access`.
    pub fn with_buffer_bytes<R>(&mut self, buffer: &Handle, access: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        let local = v8::Local::new(self.scope, &buffer.0);
        let (data, length) = if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(local) {
            (view.data() as *mut u8, view.byte_length())
        } else if let Ok(array_buffer) = v8::Local::<v8::ArrayBuffer>::try_from(local) {
            let data = array_buffer.data().map_or(std::ptr::null_mut(), |data| data.as_ptr() as *mut u8);
            (data, array_buffer.byte_length())
        } else {
            return None;
        };
        if data.is_null() || length == 0 {
            return Some(access(&mut []));
        }
        // SAFETY: V8 keeps the backing store alive while `local` (and the scope) lives, and no
        // JS runs during `access`, so nothing can detach or resize the buffer meanwhile.
        Some(access(unsafe { std::slice::from_raw_parts_mut(data, length) }))
    }

    /// A new `ArrayBuffer` owning `bytes`.
    pub fn new_array_buffer(&mut self, bytes: Vec<u8>) -> Handle {
        let store = v8::ArrayBuffer::new_backing_store_from_vec(bytes).make_shared();
        let array_buffer: v8::Local<v8::Value> = v8::ArrayBuffer::with_backing_store(self.scope, &store).into();
        Handle(v8::Global::new(self.scope, array_buffer))
    }

    /// A new typed array of `kind` over a fresh buffer holding `bytes` (whose length must be a
    /// multiple of the element size). `None` for `ArrayBufferView`/`DataView`, which have no
    /// single element type.
    pub fn new_typed_array(&mut self, kind: BufferKind, bytes: Vec<u8>) -> Option<Handle> {
        let length = bytes.len();
        let store = v8::ArrayBuffer::new_backing_store_from_vec(bytes).make_shared();
        let buffer = v8::ArrayBuffer::with_backing_store(self.scope, &store);
        let scope = &*self.scope;
        let array: v8::Local<v8::Value> = match kind {
            BufferKind::ArrayBuffer => buffer.into(),
            BufferKind::Int8Array => v8::Int8Array::new(scope, buffer, 0, length)?.into(),
            BufferKind::Uint8Array => v8::Uint8Array::new(scope, buffer, 0, length)?.into(),
            BufferKind::Uint8ClampedArray => v8::Uint8ClampedArray::new(scope, buffer, 0, length)?.into(),
            BufferKind::Int16Array => v8::Int16Array::new(scope, buffer, 0, length / 2)?.into(),
            BufferKind::Uint16Array => v8::Uint16Array::new(scope, buffer, 0, length / 2)?.into(),
            BufferKind::Int32Array => v8::Int32Array::new(scope, buffer, 0, length / 4)?.into(),
            BufferKind::Uint32Array => v8::Uint32Array::new(scope, buffer, 0, length / 4)?.into(),
            BufferKind::Float32Array => v8::Float32Array::new(scope, buffer, 0, length / 4)?.into(),
            BufferKind::Float64Array => v8::Float64Array::new(scope, buffer, 0, length / 8)?.into(),
            BufferKind::BigInt64Array => v8::BigInt64Array::new(scope, buffer, 0, length / 8)?.into(),
            BufferKind::BigUint64Array => v8::BigUint64Array::new(scope, buffer, 0, length / 8)?.into(),
            BufferKind::ArrayBufferView | BufferKind::DataView => return None,
        };
        Some(Handle(v8::Global::new(self.scope, array)))
    }

    /// Creates a pending promise. Returns the promise (to hand to JS) and its resolver (to settle
    /// it later with [`ScriptContext::resolve_promise`]/[`ScriptContext::reject_promise`] or,
    /// outside a native call, [`Runtime::settle_promise`]).
    pub fn new_promise(&mut self) -> (Handle, PromiseResolver) {
        let resolver = v8::PromiseResolver::new(self.scope).expect("creating a promise resolver");
        let promise: v8::Local<v8::Value> = resolver.get_promise(self.scope).into();
        (Handle(v8::Global::new(self.scope, promise)), PromiseResolver(v8::Global::new(self.scope, resolver)))
    }

    /// Fulfils the promise of `resolver` with `value`.
    pub fn resolve_promise(&mut self, resolver: &PromiseResolver, value: &Value) {
        settle_promise(self.scope, resolver, Ok(value));
    }

    /// Rejects the promise of `resolver` with the JS exception of `error`.
    pub fn reject_promise(&mut self, resolver: &PromiseResolver, error: &WebIdlError) {
        settle_promise(self.scope, resolver, Err(error));
    }

    /// A promise already rejected with `error`: what a promise-returning operation produces
    /// instead of throwing.
    pub fn rejected_promise(&mut self, error: &WebIdlError) -> Handle {
        let (promise, resolver) = self.new_promise();
        self.reject_promise(&resolver, error);
        promise
    }

    /// The engine-neutral value of a JS value (primitives convert; objects stay [`Value::Js`]).
    pub fn value(&mut self, value: &Handle) -> Value {
        let local = v8::Local::new(self.scope, &value.0);
        if local.is_object() && !local.is_uint8_array() {
            return Value::Js(value.clone());
        }
        native_value(self.scope, local)
    }

    /// A JS value for `value` (natives become their wrapper).
    pub fn handle(&mut self, value: &Value) -> Handle {
        let local = v8_result(self.scope, value);
        Handle(v8::Global::new(self.scope, local))
    }

    /// A traced reference to `value`, for storing in a traced native (see [`JsRef`]).
    pub fn js_ref(&mut self, value: &Handle) -> JsRef {
        let local = v8::Local::new(self.scope, &value.0);
        JsRef(v8::TracedReference::new(self.scope, local))
    }

    /// The value a [`JsRef`] points at.
    pub fn js_ref_value(&mut self, reference: &JsRef) -> Option<Handle> {
        let local = reference.0.get(self.scope)?;
        Some(Handle(v8::Global::new(self.scope, local)))
    }
}

/// A static WebIDL operation (on the interface object), registered via
/// [`Runtime::define_static_webidl_method`]. It has no receiver native.
pub type StaticNativeMethod =
    for<'a, 's, 'i> fn(&mut ScriptContext<'a, 's, 'i>, &[Value]) -> Result<Value, WebIdlError>;

struct StaticMethodConfig {
    method: StaticNativeMethod,
    arguments: WebIdlArguments,
    promise: bool,
}

struct DefaultToJsonConfig {
    attributes: Vec<String>,
}

/// The getter of an attribute whose native needs the JS engine, registered via
/// [`Runtime::define_contextual_attribute`].
pub type ContextualPropertyGetter =
    for<'a, 's, 'i> fn(&mut ScriptContext<'a, 's, 'i>, &dyn std::any::Any) -> Result<Value, WebIdlError>;

/// The setter of a contextual attribute; it receives the already converted value.
pub type ContextualPropertySetter =
    for<'a, 's, 'i> fn(&mut ScriptContext<'a, 's, 'i>, &dyn std::any::Any, &Value) -> Result<(), WebIdlError>;

struct ContextualAttributeConfig {
    getter: ContextualPropertyGetter,
    setter: Option<ContextualPropertySetter>,
    conversion: WebIdlArguments,
    /// `[LegacyLenientThis]`: an invalid receiver reads `undefined` and ignores writes instead
    /// of throwing (so the accessors carry no V8 signature).
    lenient_this: bool,
}

/// The receiver's native for an attribute accessor: throws "Illegal invocation" for an invalid
/// receiver, unless the attribute has `[LegacyLenientThis]`, which returns `None` silently.
fn attribute_receiver<'n>(
    scope: &mut v8::PinScope,
    this: v8::Local<v8::Object>,
    lenient_this: bool,
) -> Option<&'n dyn std::any::Any> {
    if !lenient_this {
        return receiver_native(scope, this);
    }
    if this.internal_field_count() < 1 {
        return None;
    }
    let raw = unsafe { this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) }
        as *mut Box<dyn std::any::Any>;
    // SAFETY: as for receiver_native.
    (!raw.is_null()).then(|| unsafe { (&*raw).as_ref() })
}

/// Reads the native of a wrapper receiving a member call, or throws "Illegal invocation".
fn receiver_native<'n>(scope: &mut v8::PinScope, this: v8::Local<v8::Object>) -> Option<&'n dyn std::any::Any> {
    if this.internal_field_count() < 1 {
        throw_type_error(scope, "Illegal invocation");
        return None;
    }
    let raw = unsafe { this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) }
        as *mut Box<dyn std::any::Any>;
    if raw.is_null() {
        throw_type_error(scope, "Illegal invocation");
        return None;
    }
    // SAFETY: the receiver owns this Box (or its GcBox does) and is alive for the callback.
    Some(unsafe { (&*raw).as_ref() })
}

/// A WebIDL operation that needs the JS engine (`any`, `object` or callback values), registered
/// via [`Runtime::define_contextual_webidl_method`].
pub type ContextualNativeMethod =
    for<'a, 's, 'i> fn(&mut ScriptContext<'a, 's, 'i>, &dyn std::any::Any, &[Value]) -> Result<Value, WebIdlError>;

/// The result of running a script: either its final expression's string representation, or the
/// message from an uncaught exception (syntax error or a thrown value). Deliberately a plain
/// `Result<String, String>` — see this module's own doc comment on why no `v8::*` type escapes.
pub type ScriptResult = Result<String, String>;

/// A native function callable from JS, registered via [`Runtime::define_native_function`].
/// A plain function pointer, not an arbitrary closure: this `v8` crate version's `Function::new`
/// requires its callback to be convertible to a bare `extern "C"` function pointer
/// (`impl MapFnTo<FunctionCallback>`), which only non-capturing closures/fn items satisfy.
/// Registering a callback that needs captured state (e.g. a reference to Roves-side data)
/// requires V8's `External`-data mechanism, deliberately not attempted in this phase — see
/// `docs/V8_MIGRATION.md`'s Phase 2 status note for why that's a distinct, harder follow-up.
pub type NativeFunction = fn(&[Value]) -> Value;

/// A read-only accessor's getter, registered via [`Runtime::define_property`]: reads whatever
/// Rust value an instance of the interface it's defined on actually wraps (already
/// downcast-checked the same way [`Runtime::get_wrapped`] is — see this crate's top doc comment
/// on why a raw pointer cast isn't used instead) and converts it to an engine-neutral [`Value`]
/// to return to JS. A plain function pointer, not an arbitrary closure, for the same reason
/// [`NativeFunction`] is: V8's accessor callback machinery requires a non-capturing closure/fn
/// under the hood.
pub type PropertyGetter = fn(&dyn std::any::Any) -> Value;

/// A settable accessor's setter, registered via [`Runtime::define_settable_property`] alongside
/// a [`PropertyGetter`] — mutates whatever Rust value the instance actually wraps, the same way
/// [`PropertyGetter`] reads it (downcast-checked, not a blind cast). Same plain-function-pointer
/// restriction as [`PropertyGetter`]/[`NativeFunction`], for the same reason.
pub type PropertySetter = fn(&dyn std::any::Any, &Value);

/// A WebIDL DOMString setter. JavaScript string conversion runs in the runtime and preserves
/// the resulting UTF-16 code units, including lone surrogates.
pub type DomStringSetter = fn(&dyn std::any::Any, Vec<u16>);

/// A nullable WebIDL DOMString setter. `None` represents the IDL `null` value.
pub type NullableDomStringSetter = fn(&dyn std::any::Any, Option<Vec<u16>>);

/// A setter for primitive WebIDL attributes after JavaScript coercion.
pub type WebIdlPrimitiveSetter = fn(&dyn std::any::Any, &Value);

/// JavaScript-to-WebIDL coercion requested by a generated primitive attribute setter.
#[derive(Clone, Copy)]
pub enum PrimitiveConversion {
    Boolean,
    Double,
    UnsignedLong,
    NullableBoolean,
    NullableDouble,
    NullableUnsignedLong,
    /// JavaScript ToString followed by USVString scalar-value conversion.
    UsvString,
    /// Nullable USVString: JavaScript null maps to IDL null; other values use ToString.
    NullableUsvString,
}

/// Required WebIDL operation argument coercions supported by the generated V8 pilot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WebIdlArgumentConversion {
    Boolean,
    Byte,
    Octet,
    Short,
    UnsignedShort,
    Long,
    LongLong,
    UnsignedLongLong,
    Float,
    UnrestrictedFloat,
    Double,
    UnrestrictedDouble,
    UnsignedLong,
    DomString,
    ByteString,
    UsvString,
    Enumeration,
    /// A traced native implementing an interface. The interface name is passed in the
    /// argument's `enumeration_values` slot (`Some(&["Node"])`).
    Interface,
    /// WebIDL `any`: the value unchanged, as [`Value::Js`].
    Any,
    /// WebIDL `object`: any JS object, as [`Value::Js`]; primitives are a TypeError.
    Object,
    /// A WebIDL callback function: any callable, as [`Value::Js`]; else a TypeError.
    Callback,
    /// A `[LegacyTreatNonObjectAsNull]` callback (event handlers): any object, as
    /// [`Value::Js`]; every non-object becomes [`Value::Null`] instead of throwing.
    LegacyCallback,
}

/// A structured WebIDL type, for values the flat [`WebIdlArgumentConversion`]s cannot describe
/// (sequences, and nullable or interface types nested inside them).
#[derive(Clone, Debug, PartialEq)]
pub enum WebIdlType {
    Primitive(WebIdlArgumentConversion),
    Enumeration(Vec<String>),
    /// A traced native implementing the named interface (or a descendant).
    Interface(String),
    Nullable(Box<WebIdlType>),
    /// `sequence<T>`: any JS iterable, converted element by element ([`Value::Sequence`]).
    Sequence(Box<WebIdlType>),
    /// A dictionary: members in WebIDL order (inherited first, each level sorted by name),
    /// converted into a [`Value::Dictionary`].
    Dictionary(Vec<WebIdlDictionaryMember>),
    /// A callback interface (e.g. `EventListener`): any object, as [`Value::Js`]; call it with
    /// [`ScriptContext::call_user_object_operation`].
    CallbackInterface,
    /// A union of (non-nullable) member types, selected by the WebIDL union conversion
    /// algorithm into a [`Value::Union`]. A nullable union is `Nullable(Union(..))`.
    Union(Vec<WebIdlType>),
    /// `Promise<T>`: any value, converted like `Promise.resolve(value)` ([`Value::Js`]).
    Promise,
    /// An integer type annotated `[Clamp]` or `[EnforceRange]`: `conversion` names the integer
    /// type (`Byte` .. `UnsignedLongLong`).
    Integer(WebIdlArgumentConversion, IntegerMode),
    /// `undefined` as a union member (`(USVString or undefined)`): accepts only `undefined`.
    Undefined,
    /// `record<K, V>` (`K` a string type): an object's own enumerable properties, converted
    /// into a [`Value::Record`].
    Record(Box<WebIdlType>, Box<WebIdlType>),
    /// A buffer source type (`ArrayBuffer`, `ArrayBufferView`, a typed array, `DataView`):
    /// type-checked and passed as [`Value::Js`] without copying; natives access the bytes in
    /// place with [`ScriptContext::with_buffer_bytes`].
    Buffer(BufferKind),
}

/// The WebIDL conversion mode of an annotated integer type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerMode {
    /// `[Clamp]`: NaN is 0; otherwise clamp to the range and round half to even.
    Clamp,
    /// `[EnforceRange]`: non-finite or out-of-range (after truncation) values are TypeErrors.
    EnforceRange,
}

/// The value range of a WebIDL integer type (64-bit types use the exactly representable
/// ±(2^53 − 1), as WebIDL specifies).
fn integer_range(conversion: WebIdlArgumentConversion) -> Option<(f64, f64)> {
    let safe = 9007199254740991.0;
    Some(match conversion {
        WebIdlArgumentConversion::Byte => (-128.0, 127.0),
        WebIdlArgumentConversion::Octet => (0.0, 255.0),
        WebIdlArgumentConversion::Short => (-32768.0, 32767.0),
        WebIdlArgumentConversion::UnsignedShort => (0.0, 65535.0),
        WebIdlArgumentConversion::Long => (-2147483648.0, 2147483647.0),
        WebIdlArgumentConversion::UnsignedLong => (0.0, 4294967295.0),
        WebIdlArgumentConversion::LongLong => (-safe, safe),
        WebIdlArgumentConversion::UnsignedLongLong => (0.0, safe),
        _ => return None,
    })
}

/// Applies `[Clamp]`/`[EnforceRange]` to an already ToNumber-converted value.
fn convert_annotated_integer(number: f64, conversion: WebIdlArgumentConversion, mode: IntegerMode) -> Result<f64, String> {
    let (min, max) = integer_range(conversion).ok_or("not an integer type")?;
    match mode {
        IntegerMode::EnforceRange => {
            if !number.is_finite() {
                return Err("value is not a finite number".into());
            }
            let truncated = number.trunc();
            if truncated < min || truncated > max {
                return Err(format!("value {truncated} is outside the range {min}..={max}"));
            }
            Ok(truncated + 0.0)
        },
        IntegerMode::Clamp => {
            if number.is_nan() {
                return Ok(0.0);
            }
            let clamped = number.clamp(min, max);
            // Round half to even.
            let floor = clamped.floor();
            let rounded = if clamped - floor == 0.5 {
                if floor % 2.0 == 0.0 { floor } else { floor + 1.0 }
            } else {
                clamped.round()
            };
            Ok(rounded + 0.0)
        },
    }
}

/// The WebIDL buffer source types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    ArrayBuffer,
    ArrayBufferView,
    DataView,
    Int8Array,
    Uint8Array,
    Uint8ClampedArray,
    Int16Array,
    Uint16Array,
    Int32Array,
    Uint32Array,
    Float32Array,
    Float64Array,
    BigInt64Array,
    BigUint64Array,
}

impl BufferKind {
    fn matches(self, value: v8::Local<v8::Value>) -> bool {
        match self {
            BufferKind::ArrayBuffer => value.is_array_buffer(),
            BufferKind::ArrayBufferView => value.is_array_buffer_view(),
            BufferKind::DataView => value.is_data_view(),
            BufferKind::Int8Array => value.is_int8_array(),
            BufferKind::Uint8Array => value.is_uint8_array(),
            BufferKind::Uint8ClampedArray => value.is_uint8_clamped_array(),
            BufferKind::Int16Array => value.is_int16_array(),
            BufferKind::Uint16Array => value.is_uint16_array(),
            BufferKind::Int32Array => value.is_int32_array(),
            BufferKind::Uint32Array => value.is_uint32_array(),
            BufferKind::Float32Array => value.is_float32_array(),
            BufferKind::Float64Array => value.is_float64_array(),
            BufferKind::BigInt64Array => value.is_big_int64_array(),
            BufferKind::BigUint64Array => value.is_big_uint64_array(),
        }
    }
}

/// One member of a [`WebIdlType::Dictionary`].
#[derive(Clone, Debug, PartialEq)]
pub struct WebIdlDictionaryMember {
    pub name: String,
    pub ty: WebIdlType,
    /// A missing required member is a TypeError.
    pub required: bool,
    /// The value used when the member is missing, converted through `ty` like a passed value.
    /// `Value::Undefined` therefore turns a nested dictionary's `= {}` into that dictionary's
    /// own defaults.
    pub default: Option<Value>,
}

/// One argument of a [`Runtime::define_typed_webidl_method`] operation.
#[derive(Clone, Debug)]
pub struct WebIdlArgument {
    pub ty: WebIdlType,
    pub optional: bool,
    /// A trailing variadic argument (`T... values`): every remaining JS argument converts by
    /// `ty` into one [`Value::Sequence`] (empty when none are passed).
    pub variadic: bool,
    /// For an optional argument, the WebIDL default used when it is missing or `undefined`,
    /// converted through `ty` like a passed value.
    pub default: Option<Value>,
}

/// WebIDL optional-argument state. `Missing` differs from a present nullable `None`.
#[derive(Clone, Debug, PartialEq)]
pub enum WebIdlOptionalArgument<T> {
    Missing,
    Present(T),
}

fn convert_webidl_integer(number: f64, bits: u32, signed: bool) -> f64 {
    if !number.is_finite() || number == 0.0 {
        return 0.0;
    }
    let truncated = number.trunc();
    // Every f64 this large is a multiple of 2^64 (its ulp exceeds it), so the modulo is 0.
    if truncated.abs() >= 2.0_f64.powi(127) {
        return 0.0;
    }
    // Exact integer arithmetic: in f64, `-5 mod 2^64` would round 2^64 - 5 up to 2^64 and
    // turn every negative `long long` into 0.
    let modulus: i128 = 1 << bits;
    let mut value = (truncated as i128).rem_euclid(modulus);
    if signed && value >= modulus / 2 {
        value -= modulus;
    }
    value as f64
}

struct WebIdlMethodConfig {
    method: NativeMethodKind,
    arguments: WebIdlArguments,
    /// A promise-returning operation: conversion errors reject instead of throwing.
    promise: bool,
}

/// How a WebIDL operation's or constructor's JS arguments convert to [`Value`]s.
struct WebIdlArguments {
    conversions: Vec<WebIdlArgumentConversion>,
    nullable_arguments: Vec<bool>,
    optional_arguments: Vec<bool>,
    enumeration_values: Vec<Option<Vec<Vec<u16>>>>,
    /// Structured types, which take precedence over the flat conversion of the same index.
    types: Vec<Option<WebIdlType>>,
    /// Whether the last argument is variadic.
    variadic_last: bool,
    /// Per-argument defaults (structured arguments only).
    defaults: Vec<Option<Value>>,
}

impl WebIdlArguments {
    fn new(
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
        enumeration_values: &[Option<&[&str]>],
    ) -> Self {
        Self {
            conversions: conversions.to_vec(),
            nullable_arguments: (0..conversions.len())
                .map(|index| nullable_arguments.get(index).copied().unwrap_or(false))
                .collect(),
            optional_arguments: (0..conversions.len())
                .map(|index| optional_arguments.get(index).copied().unwrap_or(false))
                .collect(),
            enumeration_values: (0..conversions.len())
                .map(|index| enumeration_values.get(index).and_then(|values| *values).map(|values| {
                    values.iter().map(|value| value.encode_utf16().collect()).collect()
                }))
                .collect(),
            types: Vec::new(),
            variadic_last: false,
            defaults: Vec::new(),
        }
    }

    fn typed(arguments: &[WebIdlArgument]) -> Self {
        Self {
            // Placeholders: every argument has a structured type.
            conversions: vec![WebIdlArgumentConversion::Any; arguments.len()],
            nullable_arguments: vec![false; arguments.len()],
            optional_arguments: arguments.iter().map(|argument| argument.optional).collect(),
            enumeration_values: vec![None; arguments.len()],
            types: arguments.iter().map(|argument| Some(argument.ty.clone())).collect(),
            variadic_last: arguments.last().is_some_and(|argument| argument.variadic),
            defaults: arguments.iter().map(|argument| argument.default.clone()).collect(),
        }
    }
}

/// A WebIDL constructor, registered via [`Runtime::define_constructible_interface`]: receives
/// the JS arguments already converted to [`Value`] and returns the new native object, which the
/// runtime attaches to the `this` that `new` created (same ownership and finalization as
/// [`Runtime::create_instance`]). Same plain-function-pointer restriction as the other callbacks.
pub type NativeConstructor = fn(&[Value]) -> Result<TracedNative, WebIdlError>;

/// A newly constructed native for the traced heap, as returned by a [`NativeConstructor`]: the
/// object `new` creates owns it exactly like a [`Runtime::create_traced_instance`] wrapper, so a
/// JS-constructed object is a full platform object (accepted by interface-typed arguments).
pub struct TracedNative {
    native: Box<dyn std::any::Any>,
    trace: fn(&dyn std::any::Any, &mut Tracer),
}

impl TracedNative {
    pub fn new<T: Trace>(native: T) -> Self {
        TracedNative { native: Box::new(native), trace: trace_native::<T> }
    }
}

/// Answers the exposure conditions generated bindings check while installing: Servo
/// preferences (`[Pref]`) and whether the realm is a secure context (`[SecureContext]`).
pub trait Exposure {
    fn pref_enabled(&self, name: &str) -> bool;
    fn is_secure_context(&self) -> bool;

    /// Servo's `[Func="path"]` exposure predicates, by path.
    fn func_enabled(&self, _function: &str) -> bool {
        true
    }

    /// The WebIDL name of the realm's global (`Window`, `DedicatedWorker`, `PaintWorklet`...).
    fn global_name(&self) -> &str {
        "Window"
    }

    /// Whether an `[Exposed=(..)]` list includes this realm's global: `*` matches every global,
    /// and `Worker` matches every worker global (`DedicatedWorker`, `ServiceWorker`...).
    fn exposed_in(&self, globals: &[&str]) -> bool {
        let global = self.global_name();
        globals.iter().any(|exposed| {
            *exposed == "*" || *exposed == global || (*exposed == "Worker" && global.ends_with("Worker"))
        })
    }
}

/// The custom element reaction hook generated bindings call around `[CEReactions]` members:
/// Servo pushes an element queue before the member runs and pops (running the queued reactions)
/// after it returns, whether it succeeded or threw. Implemented by the binding's native type.
pub trait CeReactions {
    fn with_ce_reactions<R>(run: impl FnOnce() -> R) -> R;
}

/// Exposes everything: every pref enabled, secure context.
pub struct ExposeAll;

impl Exposure for ExposeAll {
    fn pref_enabled(&self, _name: &str) -> bool {
        true
    }

    fn is_secure_context(&self) -> bool {
        true
    }
}

/// An exception a WebIDL `[Throws]` member raises, without exposing engine types. The runtime
/// turns it into the matching JS exception when the native callback returns it.
#[derive(Clone, Debug, PartialEq)]
pub enum WebIdlError {
    TypeError(String),
    RangeError(String),
    /// A `DOMException` with the given `name` (e.g. `"InvalidStateError"`). Thrown as
    /// `new DOMException(message, name)` when the realm defines `DOMException`; until the
    /// runtime installs that interface itself, it falls back to an `Error` whose `name` is set.
    DomException { name: String, message: String },
    /// Rethrow this JS value unchanged (an exception a callback threw, per WebIDL's
    /// "rethrow" semantics for callback functions invoked by an operation).
    Js(Handle),
}

/// A WebIDL operation that may throw (`[Throws]`), registered via
/// [`Runtime::define_fallible_webidl_method`].
pub type FallibleNativeMethod =
    fn(&dyn std::any::Any, &[Value]) -> Result<Value, WebIdlError>;

/// One signature of an overloaded WebIDL operation, registered with
/// [`Runtime::define_overloaded_webidl_method`]. The argument slices mean the same as for
/// [`Runtime::define_fallible_webidl_method`].
pub struct WebIdlOverload<'a> {
    pub method: FallibleNativeMethod,
    pub conversions: &'a [WebIdlArgumentConversion],
    pub nullable_arguments: &'a [bool],
    pub optional_arguments: &'a [bool],
    pub enumeration_values: &'a [Option<&'a [&'a str]>],
}

struct WebIdlOverloadConfig {
    overloads: Vec<(FallibleNativeMethod, WebIdlArguments)>,
}

/// One overload of an operation registered with
/// [`Runtime::define_typed_overloaded_webidl_method`].
pub struct WebIdlTypedOverload<'a> {
    pub method: WebIdlNativeOperation,
    pub arguments: &'a [WebIdlArgument],
}

struct StaticOverloadConfig {
    overloads: Vec<(StaticNativeMethod, WebIdlArguments, Vec<WebIdlArgument>)>,
    promise: bool,
}

struct TypedOverloadConfig {
    overloads: Vec<(NativeMethodKind, WebIdlArguments, Vec<WebIdlArgument>)>,
    promise: bool,
}

/// WebIDL overload resolution over structured overloads: the argument count (capped at the
/// longest overload) narrows the candidates; if several remain, the first argument position
/// where their types differ is the distinguishing argument, and its value selects the overload
/// (an optional argument for `undefined`, a nullable/dictionary type for `null`/`undefined`,
/// otherwise the union selection steps). `Err` means an exception is pending.
fn select_overload(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    overloads: &[&[WebIdlArgument]],
) -> Result<usize, ()> {
    let longest = overloads
        .iter()
        .map(|arguments| if arguments.last().is_some_and(|argument| argument.variadic) { usize::MAX } else { arguments.len() })
        .max()
        .unwrap_or(0);
    let count = (args.length().max(0) as usize).min(longest);
    let candidates: Vec<usize> = overloads
        .iter()
        .enumerate()
        .filter(|(_, arguments)| {
            let required = arguments.iter().filter(|argument| !argument.optional && !argument.variadic && argument.default.is_none()).count();
            let variadic = arguments.last().is_some_and(|argument| argument.variadic);
            required <= count && (count <= arguments.len() || variadic)
        })
        .map(|(index, _)| index)
        .collect();
    match candidates.len() {
        0 => {
            throw_type_error(scope, "no overload accepts this number of arguments");
            return Err(());
        },
        1 => return Ok(candidates[0]),
        _ => {},
    }
    let type_at = |overload: usize, position: usize| -> Option<WebIdlType> {
        let arguments = overloads[overload];
        arguments
            .get(position)
            .or_else(|| arguments.last().filter(|argument| argument.variadic))
            .map(|argument| argument.ty.clone())
    };
    let Some(position) = (0..count.max(1)).find(|position| {
        let first = type_at(candidates[0], *position);
        candidates.iter().any(|candidate| type_at(*candidate, *position) != first)
    }) else {
        // Indistinguishable within the given arguments: the first candidate wins.
        return Ok(candidates[0]);
    };
    let value = args.get(position as i32);
    if value.is_undefined() {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            overloads[**candidate].get(position).is_some_and(|argument| argument.optional || argument.default.is_some())
        }) {
            return Ok(*candidate);
        }
    }
    if value.is_null_or_undefined() {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            matches!(type_at(**candidate, position), Some(WebIdlType::Nullable(_) | WebIdlType::Dictionary(_)))
        }) {
            return Ok(*candidate);
        }
    }
    let types: Vec<WebIdlType> = candidates
        .iter()
        .map(|candidate| match type_at(*candidate, position) {
            Some(WebIdlType::Nullable(inner)) => *inner,
            Some(ty) => ty,
            None => WebIdlType::Primitive(WebIdlArgumentConversion::Any),
        })
        .collect();
    let index = select_union_member(scope, value, &types)?;
    Ok(candidates[index])
}

/// The native behind a [`Runtime::define_typed_webidl_method`] operation.
pub enum WebIdlNativeOperation {
    Plain(NativeMethod),
    Fallible(FallibleNativeMethod),
    Contextual(ContextualNativeMethod),
}

enum NativeMethodKind {
    Infallible(NativeMethod),
    Fallible(FallibleNativeMethod),
    Contextual(ContextualNativeMethod),
}

/// A pair iterable's length (`iterable<K, V>`), read before every step as WebIDL requires.
pub type PairIterableLength = fn(&dyn std::any::Any) -> u32;
/// A pair iterable's key or value at an index (below the current length).
pub type PairIterableItem = fn(&dyn std::any::Any, u32) -> Value;

struct PairIterableConfig {
    length: PairIterableLength,
    key: PairIterableItem,
    value: PairIterableItem,
    /// %<Interface>IteratorPrototype%, created when the interface is exposed.
    iterator_prototype: std::cell::RefCell<Option<v8::Global<v8::Object>>>,
}

/// What a default iterator object yields (WebIDL "kind").
#[derive(Clone, Copy)]
enum PairIteratorKind {
    Key = 0,
    Value = 1,
    Entry = 2,
}

fn pair_iterator_private<'s>(scope: &mut v8::PinScope<'s, '_>, name: &str) -> v8::Local<'s, v8::Private> {
    let name = v8::String::new(scope, name).expect("private names are short ASCII");
    v8::Private::for_api(scope, Some(name))
}

/// `entries()`/`keys()`/`values()` (and `@@iterator`): a new default iterator object over the
/// receiver.
fn create_pair_iterator(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    retval: &mut v8::ReturnValue,
    kind: PairIteratorKind,
) {
    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    // SAFETY: the External points to this interface's boxed pair iterable config.
    let config = unsafe { &*(external.value() as *const PairIterableConfig) };
    let this = args.this();
    if receiver_native(scope, this).is_none() {
        return;
    }
    let Some(prototype) = config.iterator_prototype.borrow().as_ref().map(|prototype| v8::Local::new(scope, prototype)) else {
        throw_type_error(scope, "interface is not exposed");
        return;
    };
    let iterator = v8::Object::with_prototype_and_properties(scope, prototype.into(), &[], &[]);
    let owner = pair_iterator_private(scope, "roves#iterator.owner");
    let target = pair_iterator_private(scope, "roves#iterator.target");
    let index = pair_iterator_private(scope, "roves#iterator.index");
    let kind_key = pair_iterator_private(scope, "roves#iterator.kind");
    iterator.set_private(scope, owner, external.into());
    iterator.set_private(scope, target, this.into());
    let zero = v8::Integer::new_from_unsigned(scope, 0);
    iterator.set_private(scope, index, zero.into());
    let kind = v8::Integer::new(scope, kind as i32);
    iterator.set_private(scope, kind_key, kind.into());
    retval.set(iterator.into());
}

/// `%<Interface>IteratorPrototype%.next()`.
fn pair_iterator_next(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue) {
    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    // SAFETY: the External points to this interface's boxed pair iterable config.
    let config = unsafe { &*(external.value() as *const PairIterableConfig) };
    let this = args.this();
    let owner = pair_iterator_private(scope, "roves#iterator.owner");
    // Only default iterator objects of this very interface are valid receivers.
    let owned = this
        .get_private(scope, owner)
        .and_then(|owner| v8::Local::<v8::External>::try_from(owner).ok())
        .is_some_and(|owner| owner.value() == external.value());
    if !owned {
        throw_type_error(scope, "next() called on an object that is not this interface's iterator");
        return;
    }
    let target_key = pair_iterator_private(scope, "roves#iterator.target");
    let index_key = pair_iterator_private(scope, "roves#iterator.index");
    let kind_key = pair_iterator_private(scope, "roves#iterator.kind");
    let Some(target) = this.get_private(scope, target_key).and_then(|target| v8::Local::<v8::Object>::try_from(target).ok()) else {
        return;
    };
    let index = this.get_private(scope, index_key).and_then(|index| index.uint32_value(scope)).unwrap_or(0);
    let kind = this.get_private(scope, kind_key).and_then(|kind| kind.int32_value(scope)).unwrap_or(0);
    let Some(native) = receiver_native(scope, target) else { return };
    let result = v8::Object::new(scope);
    let value_key = v8::String::new(scope, "value").unwrap();
    let done_key = v8::String::new(scope, "done").unwrap();
    if index >= (config.length)(native) {
        let undefined = v8::undefined(scope);
        result.create_data_property(scope, value_key.into(), undefined.into());
        let done = v8::Boolean::new(scope, true);
        result.create_data_property(scope, done_key.into(), done.into());
        retval.set(result.into());
        return;
    }
    let value = match kind {
        0 => v8_result(scope, &(config.key)(native, index)),
        1 => v8_result(scope, &(config.value)(native, index)),
        _ => {
            let key = v8_result(scope, &(config.key)(native, index));
            let value = v8_result(scope, &(config.value)(native, index));
            v8::Array::new_with_elements(scope, &[key, value]).into()
        },
    };
    let next = v8::Integer::new_from_unsigned(scope, index + 1);
    this.set_private(scope, index_key, next.into());
    result.create_data_property(scope, value_key.into(), value);
    let done = v8::Boolean::new(scope, false);
    result.create_data_property(scope, done_key.into(), done.into());
    retval.set(result.into());
}

/// The natives behind a WebIDL `maplike<K, V>` or `setlike<K>`, modelled on Servo's
/// `Maplike`/`Setlike` traits: entries are read by index (in insertion order). Mutators are
/// `None` for a readonly declaration, or when the interface defines that operation itself.
pub struct WebIdlLikeNatives {
    pub size: fn(&dyn std::any::Any) -> u32,
    /// The key at an index below `size` (for a setlike, the element).
    pub key_at: PairIterableItem,
    /// The value at an index below `size` (for a setlike, the element again).
    pub value_at: PairIterableItem,
    pub has: fn(&dyn std::any::Any, &Value) -> bool,
    /// Maplike `get`: the value, or [`Value::Undefined`] for a missing key.
    pub get: Option<fn(&dyn std::any::Any, &Value) -> Value>,
    /// Maplike `set(key, value)` or setlike `add(key)` (the value is [`Value::Undefined`]).
    pub set: Option<fn(&dyn std::any::Any, &Value, &Value)>,
    pub delete: Option<fn(&dyn std::any::Any, &Value) -> bool>,
    pub clear: Option<fn(&dyn std::any::Any)>,
}

struct LikeConfig {
    natives: WebIdlLikeNatives,
    key: WebIdlArguments,
    key_value: WebIdlArguments,
}

fn like_config<'c>(args: &v8::FunctionCallbackArguments) -> Option<&'c LikeConfig> {
    let external = v8::Local::<v8::External>::try_from(args.data()).ok()?;
    // SAFETY: the External points to this interface's boxed maplike/setlike config.
    Some(unsafe { &*(external.value() as *const LikeConfig) })
}

struct LegacyFactoryConfig {
    constructor: NativeConstructor,
    arguments: WebIdlArguments,
    /// The interface whose instances the factory creates.
    interface: String,
    template: v8::Global<v8::FunctionTemplate>,
}

struct WebIdlConstructorConfig {
    constructor: NativeConstructor,
    arguments: WebIdlArguments,
    /// For an overloaded constructor: every overload (chosen with WebIDL overload resolution);
    /// `constructor`/`arguments` are then unused.
    overloads: Vec<(NativeConstructor, WebIdlArguments, Vec<WebIdlArgument>)>,
    /// The interface `new` constructs: recorded on the native for interface-typed checks.
    interface: String,
}

/// A callable method, registered via [`Runtime::define_method`]: receives the wrapped Rust value
/// of whichever instance it was called on (`node.someMethod()`) plus its JS arguments already
/// converted to [`Value`], and returns a [`Value`]. Same plain-function-pointer restriction as
/// this crate's other callback types, for the same reason.
pub type NativeMethod = fn(&dyn std::any::Any, &[Value]) -> Value;

/// A WebIDL indexed setter: the index and the value converted to the setter's value type.
pub type IndexedPropertySetter = fn(&dyn std::any::Any, u32, &Value) -> Result<(), WebIdlError>;

struct IndexedConfig {
    getter: IndexedPropertyGetter,
    setter: IndexedPropertySetter,
    value_type: WebIdlType,
}

fn indexed_receiver<'c>(args: &v8::PropertyCallbackArguments) -> Option<(&'c IndexedConfig, &'c dyn std::any::Any)> {
    let external = v8::Local::<v8::External>::try_from(args.data()).ok()?;
    // SAFETY: the External points to this interface's boxed indexed-properties config.
    let config = unsafe { &*(external.value() as *const IndexedConfig) };
    let holder = args.holder();
    if holder.internal_field_count() < 1 {
        return None;
    }
    let raw = unsafe { holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) } as *mut Box<dyn std::any::Any>;
    if raw.is_null() {
        return None;
    }
    // SAFETY: the holder owns this Box (or its GcBox) and is alive for the interceptor call.
    Some((config, unsafe { &*raw }.as_ref()))
}

/// The supported property names of a named-properties object, in order.
pub type NamedPropertyNames = fn(&dyn std::any::Any) -> Vec<String>;
/// A WebIDL named setter: the name and the value converted to the setter's value type.
pub type NamedPropertySetter = fn(&dyn std::any::Any, &str, &Value) -> Result<(), WebIdlError>;
/// A WebIDL named deleter.
pub type NamedPropertyDeleter = fn(&dyn std::any::Any, &str) -> Result<(), WebIdlError>;

/// A legacy platform object's named properties, for [`Runtime::define_named_properties`].
pub struct NamedProperties {
    pub getter: NamedPropertyGetter,
    pub names: Option<NamedPropertyNames>,
    /// The named setter and the WebIDL type assigned values convert to.
    pub setter: Option<(NamedPropertySetter, WebIdlType)>,
    pub deleter: Option<NamedPropertyDeleter>,
    /// `[LegacyOverrideBuiltIns]`: named properties shadow prototype members.
    pub override_builtins: bool,
    /// False for `[LegacyUnenumerableNamedProperties]`.
    pub enumerable: bool,
}

struct NamedConfig {
    properties: NamedProperties,
    setter_arguments: Option<WebIdlArguments>,
}

/// The config and holder native of a named-property interceptor call.
fn named_receiver<'c>(args: &v8::PropertyCallbackArguments) -> Option<(&'c NamedConfig, &'c dyn std::any::Any)> {
    let external = v8::Local::<v8::External>::try_from(args.data()).ok()?;
    // SAFETY: the External points to this interface's boxed named-properties config.
    let config = unsafe { &*(external.value() as *const NamedConfig) };
    let holder = args.holder();
    if holder.internal_field_count() < 1 {
        return None;
    }
    let raw = unsafe { holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) } as *mut Box<dyn std::any::Any>;
    if raw.is_null() {
        return None;
    }
    // SAFETY: the holder owns this Box (or its GcBox) and is alive for the interceptor call.
    Some((config, unsafe { &*raw }.as_ref()))
}

/// WebIDL named property visibility, minus "is a supported name": hidden by an own property,
/// or (without `[LegacyOverrideBuiltIns]`) by a property on the prototype chain.
fn named_property_hidden(scope: &mut v8::PinScope, config: &NamedConfig, holder: v8::Local<v8::Object>, key: v8::Local<v8::Name>) -> bool {
    if config.properties.override_builtins {
        holder.has_real_named_property(scope, key).unwrap_or(false)
    } else {
        named_property_shadowed(scope, holder, key)
    }
}

/// The value of a visible named property, or `None`.
fn visible_named_property(
    scope: &mut v8::PinScope,
    config: &NamedConfig,
    holder: v8::Local<v8::Object>,
    native: &dyn std::any::Any,
    key: v8::Local<v8::Name>,
) -> Option<Value> {
    if named_property_hidden(scope, config, holder, key) {
        return None;
    }
    let name = key.to_rust_string_lossy(scope);
    (config.properties.getter)(native, &name)
}

/// An indexed-property read interceptor. `Some(value)` handles the index (including
/// `Some(Value::Undefined)`); `None` lets normal JS own/prototype lookup continue.
/// This is a primitive for future WebIDL collection bindings, not a complete implementation
/// of their query, enumeration, descriptor, assignment or deletion semantics.
pub type IndexedPropertyGetter = fn(&dyn std::any::Any, u32) -> Option<Value>;

/// A named-property read interceptor (WebIDL named getter). `Some(value)` handles the name;
/// `None` means it is not a supported property name.
pub type NamedPropertyGetter = fn(&dyn std::any::Any, &str) -> Option<Value>;

impl Runtime {
    /// Creates a new isolate, initializing the V8 platform first if this is the first
    /// [`Runtime`] in the process.
    pub fn new() -> Self {
        ensure_platform_initialized();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        isolate.set_slot(SharedInterfaceRegistry::default());
        let context = {
            v8::scope!(let scope, &mut isolate);
            let context = v8::Context::new(scope, Default::default());
            v8::Global::new(scope, context)
        };
        Runtime {
            isolate,
            context,
            wrapper_identities: WrapperIdentityMap::default(),
            next_wrapper_token: 0,
            wrapped_finalizers: Default::default(),
            constructor_configs: Vec::new(),
            overload_configs: Vec::new(),
            typed_overload_configs: Vec::new(),
            attribute_configs: Vec::new(),
            static_configs: Vec::new(),
            to_json_configs: Vec::new(),
            static_overload_configs: Vec::new(),
            legacy_factory_configs: Vec::new(),
            pair_iterable_configs: Vec::new(),
            like_configs: Vec::new(),
            named_configs: Vec::new(),
            indexed_configs: Vec::new(),
            interfaces: Vec::new(),
            global_native: None,
            method_configs: Vec::new(),
        }
    }

    /// Compiles and runs `source` as a classic (non-module) script in this [`Runtime`]'s one
    /// persistent context, and returns its final expression's value as a string, or the
    /// uncaught exception's message (covers both a syntax error, which fails at compile, and a
    /// thrown value, which fails at run).
    pub fn eval(&mut self, source: &str) -> ScriptResult {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let try_catch, scope);

        let Some(code) = v8::String::new(try_catch, source) else {
            return Err("source contained invalid UTF-16/UTF-8".to_string());
        };

        let Some(script) = v8::Script::compile(try_catch, code, None) else {
            let message = match try_catch.exception() {
                Some(exception) => exception.to_rust_string_lossy(try_catch),
                None => "unknown script error (no exception object captured)".to_string(),
            };
            return Err(message);
        };

        match script.run(try_catch) {
            Some(value) => Ok(value.to_rust_string_lossy(try_catch)),
            None => {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown script error (no exception object captured)".to_string(),
                };
                Err(message)
            },
        }
    }

    /// Same as [`Runtime::eval`], but returns an engine-neutral [`Value`] instead of a string —
    /// Phase 2's primitive conversion layer.
    pub fn eval_value(&mut self, source: &str) -> Result<Value, String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let try_catch, scope);

        let Some(code) = v8::String::new(try_catch, source) else {
            return Err("source contained invalid UTF-16/UTF-8".to_string());
        };

        let Some(script) = v8::Script::compile(try_catch, code, None) else {
            let message = match try_catch.exception() {
                Some(exception) => exception.to_rust_string_lossy(try_catch),
                None => "unknown script error (no exception object captured)".to_string(),
            };
            return Err(message);
        };

        match script.run(try_catch) {
            Some(value) => Ok(native_value(try_catch, value)),
            None => {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown script error (no exception object captured)".to_string(),
                };
                Err(message)
            },
        }
    }

    /// Runs `source` like [`Runtime::eval_value`] but returns the result as a [`Handle`], so
    /// objects and functions keep their identity instead of being converted.
    pub fn eval_handle(&mut self, source: &str) -> Result<Handle, String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let try_catch, scope);
        let code = v8::String::new(try_catch, source).ok_or("source contained invalid UTF-16/UTF-8")?;
        let result = v8::Script::compile(try_catch, code, None).and_then(|script| script.run(try_catch));
        match result {
            Some(value) => Ok(Handle(v8::Global::new(try_catch, value))),
            None => Err(match try_catch.exception() {
                Some(exception) => exception.to_rust_string_lossy(try_catch),
                None => "unknown script error (no exception object captured)".to_string(),
            }),
        }
    }

    /// Same as [`Runtime::eval_value`], but if the result is a `Promise`, drives V8's microtask
    /// queue (`Isolate::perform_microtask_checkpoint`) until it settles and returns the resolved
    /// value, or the rejection's message. A non-`Promise` result returns immediately, same as
    /// `eval_value`. Bounded (see `MAX_MICROTASK_CHECKPOINTS`) so a promise this crate has no
    /// event loop to ever settle (e.g. one waiting on a timer) fails loudly instead of hanging.
    ///
    /// Microtask pumping happens *outside* any handle/context scope: `perform_microtask_checkpoint`
    /// needs `&mut self.isolate` directly, which a live scope already borrows. Each loop
    /// iteration opens and closes its own short-lived scope purely to read the promise's current
    /// state — see this module's own top doc comment on why this file favors small, repeated
    /// scope blocks over trying to share one across an operation like this.
    pub fn eval_resolved(&mut self, source: &str) -> Result<Value, String> {
        enum Outcome {
            Value(Value),
            Promise(v8::Global<v8::Promise>),
        }

        let outcome = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let try_catch, scope);

            let Some(code) = v8::String::new(try_catch, source) else {
                return Err("source contained invalid UTF-16/UTF-8".to_string());
            };
            let Some(script) = v8::Script::compile(try_catch, code, None) else {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown script error (no exception object captured)".to_string(),
                };
                return Err(message);
            };
            match script.run(try_catch) {
                Some(value) => match v8::Local::<v8::Promise>::try_from(value) {
                    Ok(promise) => Outcome::Promise(v8::Global::new(try_catch, promise)),
                    Err(_) => Outcome::Value(native_value(try_catch, value)),
                },
                None => {
                    let message = match try_catch.exception() {
                        Some(exception) => exception.to_rust_string_lossy(try_catch),
                        None => {
                            "unknown script error (no exception object captured)".to_string()
                        },
                    };
                    return Err(message);
                },
            }
        };

        let promise_global = match outcome {
            Outcome::Value(value) => return Ok(value),
            Outcome::Promise(promise) => promise,
        };

        const MAX_MICROTASK_CHECKPOINTS: u32 = 10_000;
        for _ in 0..MAX_MICROTASK_CHECKPOINTS {
            self.isolate.perform_microtask_checkpoint();

            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            let promise = v8::Local::new(scope, &promise_global);
            match promise.state() {
                v8::PromiseState::Pending => continue,
                v8::PromiseState::Fulfilled => {
                    let result = promise.result(scope);
                    return Ok(native_value(scope, result));
                },
                v8::PromiseState::Rejected => {
                    let result = promise.result(scope);
                    return Err(result.to_rust_string_lossy(scope));
                },
            }
        }
        Err("promise did not settle within the microtask pump budget".to_string())
    }

    /// Compiles, instantiates and evaluates `source` as an ES module (as opposed to [`eval`]/
    /// [`eval_value`]'s classic script), returning its completion value or an error.
    ///
    /// Only supports a **self-contained module with no imports** — `import`/dynamic `import()`
    /// resolution is deliberately out of scope for this phase (see
    /// `unreachable_resolve_module_callback`'s own doc comment). This still exercises the real
    /// primitives a later phase's module loader needs: compiling module source distinctly from
    /// a classic script, instantiation (which is where import resolution would normally happen),
    /// and evaluation. Module evaluation always produces a `Promise` under the hood (per the
    /// spec's top-level-await semantics) even when nothing in the module actually awaits
    /// anything, so this pumps microtasks exactly like [`Runtime::eval_resolved`] to get at the
    /// real completion value or propagate a rejection.
    ///
    /// [`eval`]: Runtime::eval
    /// [`eval_value`]: Runtime::eval_value
    pub fn eval_module(&mut self, source: &str) -> Result<Value, String> {
        let promise_value = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let try_catch, scope);

            let Some(code) = v8::String::new(try_catch, source) else {
                return Err("source contained invalid UTF-16/UTF-8".to_string());
            };
            // A module's ScriptOrigin must have `is_module = true` -- unlike a classic script
            // (eval/eval_value/eval_resolved above, where `None` is fine), V8 asserts this at
            // compile time and fatally aborts the whole process (not a catchable exception) if
            // it's missing, found by actually hitting that abort while getting this to work.
            let resource_name = v8::String::new(try_catch, "roves-v8-module")
                .map(Into::into)
                .unwrap_or_else(|| v8::undefined(try_catch).into());
            let origin = v8::ScriptOrigin::new(
                try_catch,
                resource_name,
                0,
                0,
                false,
                0,
                None,
                false,
                false,
                true,
                None,
            );
            let mut compiler_source = v8::script_compiler::Source::new(code, Some(&origin));
            let Some(module) =
                v8::script_compiler::compile_module(try_catch, &mut compiler_source)
            else {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown module compile error".to_string(),
                };
                return Err(message);
            };

            match module.instantiate_module(try_catch, unreachable_resolve_module_callback) {
                Some(true) => {},
                _ => {
                    let message = if module.get_status() == v8::ModuleStatus::Errored {
                        module.get_exception().to_rust_string_lossy(try_catch)
                    } else {
                        "module instantiation failed (does it have an unsupported import?)"
                            .to_string()
                    };
                    return Err(message);
                },
            }

            match module.evaluate(try_catch) {
                Some(value) => v8::Global::new(try_catch, value),
                None => {
                    let message = if module.get_status() == v8::ModuleStatus::Errored {
                        module.get_exception().to_rust_string_lossy(try_catch)
                    } else {
                        let message = match try_catch.exception() {
                            Some(exception) => exception.to_rust_string_lossy(try_catch),
                            None => "unknown module evaluation error".to_string(),
                        };
                        message
                    };
                    return Err(message);
                },
            }
        };

        const MAX_MICROTASK_CHECKPOINTS: u32 = 10_000;
        for _ in 0..MAX_MICROTASK_CHECKPOINTS {
            self.isolate.perform_microtask_checkpoint();

            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            let value = v8::Local::new(scope, &promise_value);

            let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) else {
                // Not a Promise at all -- some V8 configurations may complete module
                // evaluation synchronously without wrapping it. Return the value directly.
                return Ok(native_value(scope, value));
            };
            match promise.state() {
                v8::PromiseState::Pending => continue,
                v8::PromiseState::Fulfilled => {
                    let result = promise.result(scope);
                    return Ok(native_value(scope, result));
                },
                v8::PromiseState::Rejected => {
                    let result = promise.result(scope);
                    return Err(result.to_rust_string_lossy(scope));
                },
            }
        }
        Err("module evaluation did not settle within the microtask pump budget".to_string())
    }

    /// Reflects `value` into a fresh JS object, transferring ownership of it to that object's
    /// JS lifetime: `value` is dropped exactly once, when V8 collects the wrapper — never
    /// before, never after — via a *guaranteed* finalizer (`v8::Weak::with_guaranteed_finalizer`,
    /// which the crate itself documents as "guaranteed to be called before the isolate is
    /// destroyed," unlike a regular weak-handle finalizer that only fires "on a best effort
    /// basis"). This is Phase 3's ownership primitive per `docs/V8_MIGRATION.md`: a later
    /// WebIDL-generated DOM reflector (Phase 4) builds its property/method access layer on top
    /// of this same lifecycle; this phase only needs the lifecycle itself to be correct, which
    /// is what its own stress tests check.
    ///
    /// Returns a strong [`Handle`] keeping the object (and so `value`) alive. Drop every
    /// `Handle` (and any other JS-side reference — none exist yet since nothing stores this
    /// object anywhere JS code can reach) to make it eligible for collection.
    pub fn create_wrapped<T: 'static>(&mut self, value: T) -> Handle {
        // Boxed as `Box<dyn Any>` (double-boxed: the inner `Box<dyn Any>` is a fat pointer, and
        // an internal field can only hold a thin one, so `Box::into_raw` is taken of the *outer*
        // box instead — a plain, thin pointer to heap memory holding that fat pointer struct).
        // This is what lets `get_wrapped` check the requested type against the actual one at
        // read-back instead of trusting the caller — see that method's own doc comment.
        let boxed_any: Box<dyn std::any::Any> = Box::new(value);

        let (global_value, raw) = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);

            let template = v8::ObjectTemplate::new(scope);
            template.set_internal_field_count(1);
            let object = template
                .new_instance(scope)
                .expect("a freshly created ObjectTemplate instance should never fail");

            let raw = Box::into_raw(Box::new(boxed_any));
            object.set_aligned_pointer_in_internal_field(
                0,
                raw as *const std::ffi::c_void,
                WRAPPED_POINTER_TAG,
            );
            let object_value: v8::Local<v8::Value> = object.into();
            (v8::Global::new(scope, object_value), raw)
        };

        self.install_guaranteed_finalizer(global_value, raw, None)
    }

    /// Shared by [`create_wrapped`][Self::create_wrapped] and
    /// [`create_instance`][Self::create_instance]: installs the guaranteed finalizer that drops
    /// `raw` (a `Box<Box<dyn Any>>`, produced identically by both callers) exactly once, when V8
    /// collects `global_value`, and returns the strong [`Handle`] keeping it alive until then.
    fn install_guaranteed_finalizer(
        &mut self,
        global_value: v8::Global<v8::Value>,
        raw: *mut Box<dyn std::any::Any>,
        identity: Option<u64>,
    ) -> Handle {
        let identity_token = identity.map(|_| {
            let token = self.next_wrapper_token;
            self.next_wrapper_token = self
                .next_wrapper_token
                .checked_add(1)
                .expect("V8 wrapper identity token space exhausted");
            token
        });
        let identity_registry = self.wrapper_identities.clone();
        let weak = arm_native_finalizer(
            &mut self.isolate,
            &self.wrapped_finalizers,
            &global_value,
            raw,
            Box::new(move || {
                if let (Some(identity), Some(token)) = (identity, identity_token) {
                    let mut registry = identity_registry.borrow_mut();
                    if registry
                        .get(&identity)
                        .is_some_and(|cached| cached.token == token)
                    {
                        registry.remove(&identity);
                    }
                }
            }),
        );
        if let (Some(identity), Some(token)) = (identity, identity_token) {
            self.wrapper_identities
                .borrow_mut()
                .insert(identity, CachedWrapper { token, weak });
        }

        Handle(global_value)
    }

    /// Allocates `native` on V8's traced heap and returns a root keeping it alive. See the
    /// "Traced native objects" section for the ownership model.
    pub fn allocate_traced<T: Trace>(&mut self, native: T) -> GcRoot<T> {
        let gc_box = GcBox {
            native: std::cell::UnsafeCell::new(Box::new(native)),
            trace: trace_native::<T>,
            wrapper: std::cell::UnsafeCell::new(None),
            wrapper_interface: std::cell::UnsafeCell::new(None),
        };
        let heap = self.isolate.get_cpp_heap().expect("V8 isolates carry a cppgc heap");
        // SAFETY: the pointer is moved into a Persistent before anything else can run a GC.
        let pointer = unsafe { v8::cppgc::make_garbage_collected(heap, gc_box) };
        GcRoot { inner: v8::cppgc::Persistent::new(&pointer), native: std::marker::PhantomData }
    }

    /// Returns the JS wrapper of `interface` for a traced native, creating it on first use.
    /// Unlike `create_instance`, the wrapper does not own the native through a finalizer: V8
    /// traces wrapper -> native through `Object::wrap`, and native -> wrapper and native -> JS
    /// through the native's traced references, so the pair lives while either side is reachable
    /// and cycles between them are collected. A native has exactly one wrapper: later calls
    /// return the same object (with its expando properties) for as long as the native lives.
    /// The interface's existing getters, setters and methods work unchanged.
    pub fn create_traced_instance<T: Trace>(&mut self, interface: &Interface, native: &GcRoot<T>) -> Handle {
        self.expose_interface(interface).expect("failed to expose native interface");
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let object: v8::Local<v8::Value> = traced_wrapper(scope, template, &interface.name, &native.inner).into();
        Handle(v8::Global::new(scope, object))
    }

    /// Settles a promise created by [`ScriptContext::new_promise`] from outside a native call
    /// (for example when asynchronous work completes), then runs the microtask checkpoint so
    /// its reactions run.
    pub fn settle_promise(&mut self, resolver: &PromiseResolver, outcome: Result<&Value, &WebIdlError>) {
        {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            settle_promise(scope, resolver, outcome);
        }
        self.isolate.perform_microtask_checkpoint();
    }

    /// Recovers the traced native behind a wrapper made by `create_traced_instance`, or `None`
    /// for any other value (including `create_instance` wrappers).
    pub fn traced_native<T: Trace>(&mut self, wrapper: &Handle) -> Option<GcRoot<T>> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let value = v8::Local::new(scope, &wrapper.0);
        let object = v8::Local::<v8::Object>::try_from(value).ok()?;
        if !object.is_api_wrapper() {
            return None;
        }
        // SAFETY: TRACED_NATIVE_TAG wrappers always hold a GcBox; the pointer is moved straight
        // into a Persistent.
        let pointer = unsafe { v8::Object::unwrap::<TRACED_NATIVE_TAG, GcBox>(scope, object) }?;
        // SAFETY: the wrapper is alive (we hold its handle), so its GcBox is too.
        let gc_box = unsafe { pointer.as_ref() };
        // SAFETY: shared read of the native; no setter callback is running.
        unsafe { &*gc_box.native.get() }.downcast_ref::<T>()?;
        Some(GcRoot { inner: v8::cppgc::Persistent::new(&pointer), native: std::marker::PhantomData })
    }

    /// A reference to `value` for storing in a traced native (see [`JsRef`]).
    pub fn js_ref(&mut self, value: &Handle) -> JsRef {
        v8::scope!(let scope, &mut self.isolate);
        let local = v8::Local::new(scope, &value.0);
        JsRef(v8::TracedReference::new(scope, local))
    }

    /// The JS value a [`JsRef`] points at, as a strong [`Handle`].
    pub fn js_ref_value(&mut self, reference: &JsRef) -> Option<Handle> {
        v8::scope!(let scope, &mut self.isolate);
        let local = reference.0.get(scope)?;
        Some(Handle(v8::Global::new(scope, local)))
    }

    /// Defines a named JS interface — a constructor function exposed on the global object (e.g.
    /// `window.Node`), backed by a `v8::FunctionTemplate`, optionally inheriting from `parent`'s
    /// prototype chain via `FunctionTemplate::inherit` ("the function's prototype.__proto__ is
    /// set to the parent function's prototype", per that method's own doc comment — exactly the
    /// WebIDL interface-inheritance shape, e.g. `Element` inheriting from `Node`). Instances are
    /// created with [`Runtime::create_instance`], not by calling the constructor from JS (no
    /// constructor body is wired up in this prototype).
    ///
    /// This is the foundational primitive real DOM interfaces are built on that neither
    /// [`create_wrapped`][Self::create_wrapped] nor Phase 3's other primitives provide: JS-visible
    /// interface identity via the prototype chain (`instanceof`), not just Rust-side type
    /// checking via [`Runtime::get_wrapped`]. Prototyped here, inside `roves-v8`, before touching
    /// `components/script_bindings` for real — see `docs/V8_MIGRATION.md`'s Phase 4 status note
    /// on why: the production support modules a real interface needs
    /// (`interface.rs`/`proxyhandler.rs`/`finalize.rs`) are mutually interdependent around
    /// SpiderMonkey's `JSClass`-based object model, so no single one of them can be swapped for a
    /// V8 equivalent in isolation — this prototype is where that V8-side foundation gets designed
    /// and validated first.
    pub fn define_interface(&mut self, name: &str, parent: Option<&Interface>) -> Interface {
        self.define_interface_inner(name, parent, None)
    }

    /// Defines a WebIDL interface with a `constructor(...)`: `new Name(...)` converts the
    /// arguments like an operation's, calls `constructor` and attaches the returned native
    /// object to the new instance, with the same ownership and guaranteed finalization as
    /// [`Runtime::create_instance`]. Calling it without `new` throws a TypeError, and the
    /// interface object's `length` is the number of required arguments. Native code can still
    /// create instances with `create_instance`. JS-constructed objects are not entered in the
    /// native-identity wrapper cache.
    pub fn define_constructible_interface(
        &mut self,
        name: &str,
        parent: Option<&Interface>,
        constructor: NativeConstructor,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
        enumeration_values: &[Option<&[&str]>],
    ) -> Interface {
        let config = Box::new(WebIdlConstructorConfig {
            constructor,
            arguments: WebIdlArguments::new(
                conversions,
                nullable_arguments,
                optional_arguments,
                enumeration_values,
            ),
            overloads: Vec::new(),
            interface: name.to_owned(),
        });
        let config_pointer = (&*config) as *const WebIdlConstructorConfig;
        self.constructor_configs.push(config);
        self.define_interface_inner(name, parent, Some(config_pointer))
    }

    /// Like [`Runtime::define_constructible_interface`] with constructor arguments described by
    /// structured [`WebIdlType`]s (dictionaries, sequences).
    pub fn define_typed_constructible_interface(
        &mut self,
        name: &str,
        parent: Option<&Interface>,
        constructor: NativeConstructor,
        arguments: &[WebIdlArgument],
    ) -> Interface {
        let config = Box::new(WebIdlConstructorConfig {
            constructor,
            arguments: WebIdlArguments::typed(arguments),
            overloads: Vec::new(),
            interface: name.to_owned(),
        });
        let config_pointer = (&*config) as *const WebIdlConstructorConfig;
        self.constructor_configs.push(config);
        self.define_interface_inner(name, parent, Some(config_pointer))
    }

    /// Like [`Runtime::define_typed_constructible_interface`] for an overloaded constructor
    /// (`Path2D`, `ImageData`): `new` picks the overload with WebIDL's overload resolution
    /// algorithm, and the interface object's `length` is the shortest overload's.
    pub fn define_overloaded_constructible_interface(
        &mut self,
        name: &str,
        parent: Option<&Interface>,
        overloads: &[(NativeConstructor, &[WebIdlArgument])],
    ) -> Interface {
        let (first, _) = overloads.first().expect("an overloaded constructor has overloads");
        let config = Box::new(WebIdlConstructorConfig {
            constructor: *first,
            arguments: WebIdlArguments::typed(&[]),
            overloads: overloads
                .iter()
                .map(|(constructor, arguments)| (*constructor, WebIdlArguments::typed(arguments), arguments.to_vec()))
                .collect(),
            interface: name.to_owned(),
        });
        let config_pointer = (&*config) as *const WebIdlConstructorConfig;
        self.constructor_configs.push(config);
        self.define_interface_inner(name, parent, Some(config_pointer))
    }

    fn define_interface_inner(
        &mut self,
        name: &str,
        parent: Option<&Interface>,
        constructor: Option<*const WebIdlConstructorConfig>,
    ) -> Interface {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let template = match constructor {
            // Nonconstructible WebIDL interfaces throw on both calls and construction.
            // Native instances are created by create_instance without invoking this function.
            None => v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 _args: v8::FunctionCallbackArguments,
                 _retval: v8::ReturnValue| {
                    throw_type_error(scope, "Illegal constructor");
                },
            )
            .build(scope),
            Some(config_pointer) => {
                // SAFETY: the config is boxed in `constructor_configs`, which outlives the isolate.
                let config = unsafe { &*config_pointer };
                let required = if config.overloads.is_empty() {
                    config.arguments.optional_arguments.iter().take_while(|optional| !**optional).count()
                } else {
                    config
                        .overloads
                        .iter()
                        .map(|(_, _, arguments)| {
                            arguments.iter().filter(|argument| !argument.optional && !argument.variadic && argument.default.is_none()).count()
                        })
                        .min()
                        .unwrap_or(0)
                };
                let external_data =
                    v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
                v8::FunctionTemplate::builder(
                    |scope: &mut v8::PinScope,
                     args: v8::FunctionCallbackArguments,
                     _retval: v8::ReturnValue| {
                        let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else {
                            return;
                        };
                        // SAFETY: the External points to this interface's boxed constructor config.
                        let config =
                            unsafe { &*(external.value() as *const WebIdlConstructorConfig) };
                        if args.new_target().is_undefined() {
                            throw_type_error(scope, "Constructor requires 'new'");
                            return;
                        }
                        let this = args.this();
                        if this.internal_field_count() < 1 {
                            throw_type_error(scope, "Illegal constructor");
                            return;
                        }
                        let (constructor, arguments) = if config.overloads.is_empty() {
                            (config.constructor, &config.arguments)
                        } else {
                            let candidates: Vec<&[WebIdlArgument]> =
                                config.overloads.iter().map(|(_, _, arguments)| arguments.as_slice()).collect();
                            let Ok(selected) = select_overload(scope, &args, &candidates) else { return };
                            let (constructor, arguments, _) = &config.overloads[selected];
                            (*constructor, arguments)
                        };
                        let Some(arguments) = convert_webidl_arguments(scope, &args, arguments) else {
                            return;
                        };
                        let native = match constructor(&arguments) {
                            Ok(native) => native,
                            Err(error) => {
                                throw_webidl_error(scope, &error);
                                return;
                            },
                        };
                        // `this` becomes the native's one wrapper (`new` returns it because the
                        // return value is left unset).
                        attach_new_traced_native(scope, this, &config.interface, native);
                    },
                )
                .data(external_data.into())
                .length(required as i32)
                .build(scope)
            },
        };
        template.instance_template(scope).set_internal_field_count(1);

        let mut inherited_unforgeable = Vec::new();
        if let Some(parent) = parent {
            let parent_template = v8::Local::new(scope, &parent.template);
            template.inherit(parent_template);
            parent.has_descendants.set(true);
            let instance_template = template.instance_template(scope);
            for (accessor_name, getter) in parent.unforgeable_accessors.borrow().iter() {
                let key = v8::String::new(scope, accessor_name).unwrap();
                let getter = v8::Local::new(scope, getter);
                instance_template.set_accessor_property(
                    key.into(),
                    Some(getter),
                    None,
                    v8::PropertyAttribute::DONT_DELETE,
                );
                inherited_unforgeable.push((accessor_name.clone(), v8::Global::new(scope, getter)));
            }
        }

        if let Some(class_name) = v8::String::new(scope, name) {
            template.set_class_name(class_name);
            let tag = v8::Symbol::get_to_string_tag(scope);
            template.prototype_template(scope).set_with_attr(
                tag.into(), class_name.into(),
                v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
            );
        }
        template.read_only_prototype();

        // Deliberately NOT calling `template.get_function(scope)` here to expose the constructor
        // eagerly -- found by hitting a real bug: `get_function` materializes the actual
        // prototype JS object from `prototype_template`'s *current* contents immediately, and
        // later mutations to `prototype_template` (e.g. a `define_method` call after this
        // `define_interface` call returns) do NOT retroactively update that already-materialized
        // object. Every `define_method` test failed with "is not a function" until this was
        // deferred to `create_instance`, by which point real usage has already finished defining
        // the interface's members. `instance_template`-based accessors don't have this problem
        // (nothing forces an early snapshot of the instance template), which is why the earlier
        // `define_property` checkpoint never hit it.
        let _ = context;

        let registry = scope
            .get_slot::<SharedInterfaceRegistry>()
            .expect("runtime installs the interface registry")
            .clone();
        registry.borrow_mut().templates.insert(name.to_owned(), v8::Global::new(scope, template));
        registry
            .borrow_mut()
            .parents
            .insert(name.to_owned(), parent.map(|parent| parent.name.clone()));

        let interface = Interface(std::rc::Rc::new(InterfaceData {
            template: v8::Global::new(scope, template),
            parent_template: parent.map(|parent| parent.template.clone()),
            unforgeable_accessors: std::rc::Rc::new(std::cell::RefCell::new(inherited_unforgeable)),
            has_descendants: std::rc::Rc::new(std::cell::Cell::new(false)),
            name: name.to_string(),
            constructor_exposed: std::cell::Cell::new(false),
            interface_object_on_global: std::cell::Cell::new(true),
            aliases: std::cell::RefCell::new(Vec::new()),
            value_iterable: std::cell::Cell::new(false),
            namespace: std::cell::Cell::new(false),
            legacy_factories: std::cell::RefCell::new(Vec::new()),
            lenient_setters: std::cell::RefCell::new(std::collections::HashSet::new()),
            promise_operations: std::cell::RefCell::new(std::collections::HashSet::new()),
            replaceable: std::cell::RefCell::new(std::collections::HashSet::new()),
            exception_class: std::cell::Cell::new(false),
            unforgeable_members: std::cell::Cell::new(false),
            global: std::cell::Cell::new(false),
            pair_iterable: std::cell::Cell::new(None),
            materialized: std::rc::Rc::new(std::cell::Cell::new(false)),
            ancestors: parent.map_or_else(Vec::new, |parent| {
                let mut ancestors = parent.ancestors.clone();
                ancestors.push(parent.materialized.clone());
                ancestors
            }),
        }));
        self.interfaces.push(interface.clone());
        interface
    }

    /// Defines a WebIDL namespace: register its operations with
    /// [`Runtime::define_static_webidl_method`]; exposure then defines a plain object of that
    /// name holding them (and no constructor or prototype).
    pub fn define_namespace(&mut self, name: &str) -> Interface {
        let namespace = self.define_interface_inner(name, None, None);
        namespace.namespace.set(true);
        namespace
    }

    /// Makes `interface` a WebIDL value iterable (`iterable<V>`): its prototype gets
    /// `Array.prototype`'s `entries`/`keys`/`values`/`forEach` and `@@iterator`, which iterate
    /// the object through its indexed getter and `length`. Call before exposure.
    pub fn define_value_iterable(&mut self, interface: &Interface) {
        interface.value_iterable.set(true);
    }

    /// Makes `interface` a WebIDL pair iterable (`iterable<K, V>`, e.g. `FormData`, `Headers`):
    /// its prototype gets `entries`, `keys`, `values`, `forEach` and `@@iterator` (the same
    /// function as `entries`). The iterators are default iterator objects whose prototype,
    /// `%<Interface>IteratorPrototype%`, inherits `%IteratorPrototype%`. `length`, `key` and
    /// `value` are read live on every step.
    pub fn define_pair_iterable(
        &mut self,
        interface: &Interface,
        length: PairIterableLength,
        key: PairIterableItem,
        value: PairIterableItem,
    ) -> Result<(), String> {
        self.define_iteration(interface, length, key, value, false)
    }

    /// Makes `interface` a WebIDL `maplike<K, V>` (`value` is `Some`) or `setlike<K>`
    /// (`CSSFontFeatureValuesMap`, `CustomStateSet`, `FontFaceSet`): `size`, `has`, `get`
    /// (maplike), the iteration methods, `forEach`, and the mutators `natives` provides
    /// (`set`/`add`, `delete`, `clear`). Keys and values convert through `key`/`value`; `set` and
    /// `add` return the receiver.
    pub fn define_maplike(
        &mut self,
        interface: &Interface,
        key: WebIdlType,
        value: Option<WebIdlType>,
        natives: WebIdlLikeNatives,
    ) -> Result<(), String> {
        let set_like = value.is_none();
        self.define_iteration(interface, natives.size, natives.key_at, natives.value_at, set_like)?;
        let key_argument = WebIdlArgument { ty: key.clone(), optional: false, variadic: false, default: None };
        let value_argument = WebIdlArgument {
            ty: value.unwrap_or(WebIdlType::Primitive(WebIdlArgumentConversion::Any)),
            optional: false,
            variadic: false,
            default: None,
        };
        let has_get = natives.get.is_some();
        let has_set = natives.set.is_some();
        let has_delete = natives.delete.is_some();
        let has_clear = natives.clear.is_some();
        let config = Box::new(LikeConfig {
            natives,
            key: WebIdlArguments::typed(std::slice::from_ref(&key_argument)),
            key_value: WebIdlArguments::typed(&[key_argument.clone(), value_argument]),
        });
        let config_pointer = (&*config) as *const LikeConfig;
        self.like_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let prototype = template.prototype_template(scope);

        let size = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                let Some(config) = like_config(&args) else { return };
                let Some(native) = receiver_native(scope, args.this()) else { return };
                retval.set_uint32((config.natives.size)(native));
            },
        )
        .data(data.into())
        .signature(signature)
        .length(0)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        size.set_class_name(v8::String::new(scope, "get size").unwrap());
        let size_key = v8::String::new(scope, "size").unwrap();
        prototype.set_accessor_property(size_key.into(), Some(size), None, v8::PropertyAttribute::NONE);

        let mut methods: Vec<(&str, v8::Local<v8::FunctionTemplate>)> = Vec::new();
        let has = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                let Some(config) = like_config(&args) else { return };
                let Some(native) = receiver_native(scope, args.this()) else { return };
                let Some(key) = convert_webidl_arguments(scope, &args, &config.key) else { return };
                retval.set_bool((config.natives.has)(native, &key[0]));
            },
        )
        .data(data.into())
        .signature(signature)
        .length(1)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        methods.push(("has", has));
        if has_get {
            let get = v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                    let Some(config) = like_config(&args) else { return };
                    let Some(native) = receiver_native(scope, args.this()) else { return };
                    let Some(key) = convert_webidl_arguments(scope, &args, &config.key) else { return };
                    let value = (config.natives.get.expect("get exists"))(native, &key[0]);
                    let value = v8_result(scope, &value);
                    retval.set(value);
                },
            )
            .data(data.into())
            .signature(signature)
            .length(1)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope);
            methods.push(("get", get));
        }
        if has_set {
            let set = if set_like {
                v8::FunctionTemplate::builder(
                    |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                        let Some(config) = like_config(&args) else { return };
                        let Some(native) = receiver_native(scope, args.this()) else { return };
                        let Some(key) = convert_webidl_arguments(scope, &args, &config.key) else { return };
                        (config.natives.set.expect("add exists"))(native, &key[0], &Value::Undefined);
                        retval.set(args.this().into());
                    },
                )
            } else {
                v8::FunctionTemplate::builder(
                    |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                        let Some(config) = like_config(&args) else { return };
                        let Some(native) = receiver_native(scope, args.this()) else { return };
                        let Some(entry) = convert_webidl_arguments(scope, &args, &config.key_value) else { return };
                        (config.natives.set.expect("set exists"))(native, &entry[0], &entry[1]);
                        retval.set(args.this().into());
                    },
                )
            }
            .data(data.into())
            .signature(signature)
            .length(if set_like { 1 } else { 2 })
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope);
            methods.push((if set_like { "add" } else { "set" }, set));
        }
        if has_delete {
            let delete = v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                    let Some(config) = like_config(&args) else { return };
                    let Some(native) = receiver_native(scope, args.this()) else { return };
                    let Some(key) = convert_webidl_arguments(scope, &args, &config.key) else { return };
                    retval.set_bool((config.natives.delete.expect("delete exists"))(native, &key[0]));
                },
            )
            .data(data.into())
            .signature(signature)
            .length(1)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope);
            methods.push(("delete", delete));
        }
        if has_clear {
            let clear = v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue| {
                    let Some(config) = like_config(&args) else { return };
                    let Some(native) = receiver_native(scope, args.this()) else { return };
                    (config.natives.clear.expect("clear exists"))(native);
                },
            )
            .data(data.into())
            .signature(signature)
            .length(0)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope);
            methods.push(("clear", clear));
        }
        for (name, function) in methods {
            let key = v8::String::new(scope, name).ok_or("invalid method name")?;
            prototype.set(key.into(), function.into());
        }
        Ok(())
    }

    /// The iteration methods shared by pair iterables, maplike and setlike. For a setlike,
    /// `keys` and `@@iterator` are the `values` function (WebIDL), otherwise `@@iterator` is
    /// `entries`.
    fn define_iteration(
        &mut self,
        interface: &Interface,
        length: PairIterableLength,
        key: PairIterableItem,
        value: PairIterableItem,
        set_like: bool,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let config = Box::new(PairIterableConfig { length, key, value, iterator_prototype: std::cell::RefCell::new(None) });
        let config_pointer = (&*config) as *const PairIterableConfig;
        self.pair_iterable_configs.push(config);
        interface.pair_iterable.set(Some(config_pointer));
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let prototype = template.prototype_template(scope);
        macro_rules! iterator_method {
            ($kind:expr) => {
                v8::FunctionTemplate::builder(
                    |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                        create_pair_iterator(scope, &args, &mut retval, $kind)
                    },
                )
            };
        }
        let mut functions = Vec::new();
        for builder in [
            iterator_method!(PairIteratorKind::Entry),
            iterator_method!(PairIteratorKind::Key),
            iterator_method!(PairIteratorKind::Value),
        ] {
            functions.push(
                builder
                    .data(data.into())
                    .signature(signature)
                    .length(0)
                    .constructor_behavior(v8::ConstructorBehavior::Throw)
                    .build(scope),
            );
        }
        let (entries, mut keys, values) = (functions[0], functions[1], functions[2]);
        if set_like {
            // A set's keys are its values: `keys` is the `values` function object itself.
            keys = values;
        }
        for (name, function) in [("entries", entries), ("keys", keys), ("values", values)] {
            let key = v8::String::new(scope, name).ok_or("invalid method name")?;
            prototype.set(key.into(), function.into());
        }
        // WebIDL: @@iterator is the same function object as `entries` (`values` for a setlike).
        let iterator = v8::Symbol::get_iterator(scope);
        let default_iterator = if set_like { values } else { entries };
        prototype.set_with_attr(iterator.into(), default_iterator.into(), v8::PropertyAttribute::DONT_ENUM);
        let for_each = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue| {
                let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else { return };
                // SAFETY: the External points to this interface's boxed pair iterable config.
                let config = unsafe { &*(external.value() as *const PairIterableConfig) };
                let this = args.this();
                let Ok(callback) = v8::Local::<v8::Function>::try_from(args.get(0)) else {
                    throw_type_error(scope, "forEach callback is not a function");
                    return;
                };
                let this_arg = args.get(1);
                let mut index = 0;
                loop {
                    // The length and items are read again after every callback, which may mutate.
                    let Some(native) = receiver_native(scope, this) else { return };
                    if index >= (config.length)(native) {
                        return;
                    }
                    let value = v8_result(scope, &(config.value)(native, index));
                    let key = v8_result(scope, &(config.key)(native, index));
                    if callback.call(scope, this_arg, &[value, key, this.into()]).is_none() {
                        return;
                    }
                    index += 1;
                }
            },
        )
        .data(data.into())
        .signature(signature)
        .length(1)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        let key = v8::String::new(scope, "forEach").ok_or("invalid method name")?;
        prototype.set(key.into(), for_each.into());
        Ok(())
    }

    /// Also exposes the interface object under `alias` (`[LegacyWindowAlias]`, e.g.
    /// `webkitURL` for `URL`). Call before exposure.
    pub fn add_interface_alias(&mut self, interface: &Interface, alias: &str) {
        interface.aliases.borrow_mut().push(alias.to_owned());
    }

    /// Keeps `interface`'s interface object off the global object when it is exposed, for
    /// `[LegacyNoInterfaceObject]` or an unmet exposure condition. Call before exposure.
    pub fn hide_interface_object(&mut self, interface: &Interface) {
        interface.interface_object_on_global.set(false);
    }

    /// Finalizes registration and exposes a nonconstructible interface constructor even
    /// before any native instance exists. All members must be registered first.
    pub fn expose_interface(&mut self, interface: &Interface) -> Result<(), String> {
        if interface.constructor_exposed.get() { return Ok(()); }
        interface.materialized.set(true);
        for ancestor in &interface.ancestors { ancestor.set(true); }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let function = template.get_function(scope).ok_or("failed to materialize interface")?;
        // WebIDL: an interface object's [[Prototype]] is its parent's interface object
        // (`Object.getPrototypeOf(Element) === Node`). `FunctionTemplate::inherit` only links
        // the two `prototype` objects, so set the constructor's own prototype here.
        if interface.value_iterable.get() {
            // WebIDL value iterators: @@iterator, entries, keys, values and forEach are
            // %Array.prototype%'s own functions (the interface also has an indexed getter and
            // `length`, so they iterate the platform object like an array).
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            let prototype = function
                .get(scope, prototype_key.into())
                .and_then(|prototype| v8::Local::<v8::Object>::try_from(prototype).ok())
                .ok_or("interface object has no prototype")?;
            let array_key = v8::String::new(scope, "Array").unwrap();
            let array_prototype = context
                .global(scope)
                .get(scope, array_key.into())
                .and_then(|array| v8::Local::<v8::Object>::try_from(array).ok())
                .and_then(|array| array.get(scope, prototype_key.into()))
                .and_then(|array_prototype| v8::Local::<v8::Object>::try_from(array_prototype).ok())
                .ok_or("realm has no Array.prototype")?;
            for name in ["entries", "keys", "values", "forEach"] {
                let key = v8::String::new(scope, name).unwrap();
                let method = array_prototype.get(scope, key.into()).ok_or("missing Array.prototype method")?;
                prototype.define_own_property(scope, key.into(), method, v8::PropertyAttribute::NONE);
            }
            let values_key = v8::String::new(scope, "values").unwrap();
            let values = array_prototype.get(scope, values_key.into()).ok_or("missing Array.prototype.values")?;
            let iterator = v8::Symbol::get_iterator(scope);
            prototype.define_own_property(scope, iterator.into(), values, v8::PropertyAttribute::DONT_ENUM);
        }
        if let Some(parent_template) = &interface.parent_template {
            let parent_template = v8::Local::new(scope, parent_template);
            let parent_function = parent_template
                .get_function(scope)
                .ok_or("failed to materialize parent interface")?;
            if function.set_prototype(scope, parent_function.into()) != Some(true) {
                return Err("failed to link interface object to its parent".into());
            }
        }
        let key = v8::String::new(scope, &interface.name).ok_or("invalid interface name")?;
        if interface.namespace.get() {
            // A namespace object is an ordinary object (prototype %Object.prototype%) holding the
            // operations defined with define_static_webidl_method, plus @@toStringTag.
            let namespace = v8::Object::new(scope);
            let names = function
                .get_own_property_names(scope, v8::GetPropertyNamesArgs::default())
                .ok_or("failed to read namespace members")?;
            for index in 0..names.length() {
                let Some(name) = names.get_index(scope, index) else { continue };
                let skip = name.to_rust_string_lossy(scope);
                if matches!(skip.as_str(), "length" | "name" | "prototype") {
                    continue;
                }
                let Ok(name) = v8::Local::<v8::Name>::try_from(name) else { continue };
                // Copy accessors (namespace attributes) as accessors, not their current value.
                let Some(descriptor) = function.get_own_property_descriptor(scope, name) else { continue };
                let Ok(descriptor) = v8::Local::<v8::Object>::try_from(descriptor) else { continue };
                macro_rules! field {
                    ($scope:expr, $name:literal) => {
                        v8::String::new($scope, $name).and_then(|key| descriptor.get($scope, key.into()))
                    };
                }
                let enumerable = field!(scope, "enumerable").is_some_and(|value| value.is_true());
                let configurable = field!(scope, "configurable").is_some_and(|value| value.is_true());
                let mut copy = if let Some(getter) = field!(scope, "get").filter(|getter| getter.is_function()) {
                    let undefined = v8::undefined(scope).into();
                    v8::PropertyDescriptor::new_from_get_set(getter, undefined)
                } else {
                    let Some(value) = field!(scope, "value") else { continue };
                    let writable = field!(scope, "writable").is_some_and(|value| value.is_true());
                    v8::PropertyDescriptor::new_from_value_writable(value, writable)
                };
                copy.set_enumerable(enumerable);
                copy.set_configurable(configurable);
                namespace.define_property(scope, name, &copy);
            }
            let tag = v8::Symbol::get_to_string_tag(scope);
            namespace.define_own_property(scope, tag.into(), key.into(), v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM);
            if interface.interface_object_on_global.get()
                && context.global(scope).define_own_property(scope, key.into(), namespace.into(), v8::PropertyAttribute::DONT_ENUM)
                    != Some(true)
            {
                return Err("failed to expose namespace".into());
            }
            interface.constructor_exposed.set(true);
            return Ok(());
        }
        if interface.exception_class.get() {
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            let error_key = v8::String::new(scope, "Error").unwrap();
            let error_prototype = context
                .global(scope)
                .get(scope, error_key.into())
                .and_then(|error| v8::Local::<v8::Object>::try_from(error).ok())
                .and_then(|error| error.get(scope, prototype_key.into()))
                .ok_or("failed to find Error.prototype")?;
            function
                .get(scope, prototype_key.into())
                .and_then(|prototype| v8::Local::<v8::Object>::try_from(prototype).ok())
                .ok_or("interface object has no prototype")?
                .set_prototype(scope, error_prototype);
        }
        if let Some(config_pointer) = interface.pair_iterable.get() {
            // SAFETY: the config is boxed in `pair_iterable_configs`, which outlives the isolate.
            let config = unsafe { &*config_pointer };
            // %IteratorPrototype% is the prototype of %ArrayIteratorPrototype%.
            let array = v8::Array::new(scope, 0);
            let iterator_symbol = v8::Symbol::get_iterator(scope);
            let array_iterator = array
                .get(scope, iterator_symbol.into())
                .and_then(|function| v8::Local::<v8::Function>::try_from(function).ok())
                .and_then(|function| function.call(scope, array.into(), &[]))
                .and_then(|iterator| v8::Local::<v8::Object>::try_from(iterator).ok())
                .ok_or("failed to create an array iterator")?;
            let iterator_prototype = array_iterator
                .get_prototype(scope)
                .and_then(|prototype| v8::Local::<v8::Object>::try_from(prototype).ok())
                .and_then(|prototype| prototype.get_prototype(scope))
                .ok_or("failed to find %IteratorPrototype%")?;
            let prototype = v8::Object::with_prototype_and_properties(scope, iterator_prototype, &[], &[]);
            let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
            let next = v8::Function::builder(pair_iterator_next)
                .data(data.into())
                .length(0)
                .constructor_behavior(v8::ConstructorBehavior::Throw)
                .build(scope)
                .ok_or("failed to create next()")?;
            let next_key = v8::String::new(scope, "next").unwrap();
            next.set_name(next_key);
            prototype.create_data_property(scope, next_key.into(), next.into());
            let tag = v8::Symbol::get_to_string_tag(scope);
            let tag_value = v8::String::new(scope, &format!("{} Iterator", interface.name)).ok_or("invalid interface name")?;
            prototype.define_own_property(scope, tag.into(), tag_value.into(), v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM);
            *config.iterator_prototype.borrow_mut() = Some(v8::Global::new(scope, prototype));
        }
        if interface.interface_object_on_global.get() {
            let global = context.global(scope);
            let mut names = vec![key];
            for alias in interface.aliases.borrow().iter() {
                names.push(v8::String::new(scope, alias).ok_or("invalid interface alias")?);
            }
            for name in names {
                if global.define_own_property(scope, name.into(), function.into(), v8::PropertyAttribute::DONT_ENUM)
                    != Some(true)
                {
                    return Err("failed to expose interface".into());
                }
            }
            let prototype_key = v8::String::new(scope, "prototype").unwrap();
            let prototype = function.get(scope, prototype_key.into()).ok_or("interface object has no prototype")?;
            for (name, factory) in interface.legacy_factories.borrow().iter() {
                let factory = v8::Local::new(scope, factory);
                let factory = factory.get_function(scope).ok_or("failed to materialize legacy factory function")?;
                // WebIDL: the factory's `prototype` is the interface prototype object.
                let mut descriptor = v8::PropertyDescriptor::new_from_value_writable(prototype, false);
                descriptor.set_enumerable(false);
                descriptor.set_configurable(false);
                factory.define_property(scope, prototype_key.into(), &descriptor);
                let name = v8::String::new(scope, name).ok_or("invalid factory name")?;
                if global.define_own_property(scope, name.into(), factory.into(), v8::PropertyAttribute::DONT_ENUM)
                    != Some(true)
                {
                    return Err("failed to expose legacy factory function".into());
                }
            }
        }
        interface.constructor_exposed.set(true);
        Ok(())
    }

    /// Creates an instance of `interface`, reflecting `value` into it exactly like
    /// [`Runtime::create_wrapped`] (same ownership/finalization lifecycle, same
    /// [`Runtime::get_wrapped`] read-back) — but the instance's `[[Prototype]]` is
    /// `interface`'s prototype object, so `instanceof` and the prototype chain work from JS,
    /// which a plain `create_wrapped` object doesn't have.
    pub fn create_instance<T: 'static>(&mut self, interface: &Interface, value: T) -> Handle {
        self.create_instance_with_identity_inner(interface, value, None)
    }

    /// Creates or reuses a wrapper for a stable native identity. The identity must be unique
    /// within this runtime and must always refer to the same native object type and interface.
    /// The factory runs only when no live wrapper exists. The cache is weak: it preserves wrapper
    /// identity while JavaScript still reaches the wrapper, but does not keep an otherwise-dead
    /// DOM object alive.
    pub fn create_instance_with_identity<T: 'static>(
        &mut self,
        interface: &Interface,
        identity: u64,
        create: impl FnOnce() -> T,
    ) -> Handle {
        if let Some(handle) = self.find_wrapper(identity) {
            return handle;
        }
        self.create_instance_with_identity_inner(interface, create(), Some(identity))
    }

    fn create_instance_with_identity_inner<T: 'static>(
        &mut self,
        interface: &Interface,
        value: T,
        identity: Option<u64>,
    ) -> Handle {
        self.expose_interface(interface).expect("failed to expose native interface");
        let boxed_any: Box<dyn std::any::Any> = Box::new(value);

        let (global_value, raw) = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);

            let template = v8::Local::new(scope, &interface.template);

            let instance_template = template.instance_template(scope);
            let object = instance_template
                .new_instance(scope)
                .expect("a freshly created ObjectTemplate instance should never fail");

            let raw = Box::into_raw(Box::new(boxed_any));
            object.set_aligned_pointer_in_internal_field(
                0,
                raw as *const std::ffi::c_void,
                WRAPPED_POINTER_TAG,
            );
            let object_value: v8::Local<v8::Value> = object.into();
            (v8::Global::new(scope, object_value), raw)
        };

        self.install_guaranteed_finalizer(global_value, raw, identity)
    }

    fn find_wrapper(&mut self, identity: u64) -> Option<Handle> {
        let (token, weak) = self
            .wrapper_identities
            .borrow()
            .get(&identity)
            .map(|cached| (cached.token, cached.weak.clone()))?;
        v8::scope!(let scope, &mut self.isolate);
        if let Some(value) = weak.to_local(scope) {
            Some(Handle(v8::Global::new(scope, value)))
        } else {
            let mut registry = self.wrapper_identities.borrow_mut();
            if registry
                .get(&identity)
                .is_some_and(|cached| cached.token == token)
            {
                registry.remove(&identity);
            }
            None
        }
    }

    /// Defines a read-only WebIDL-style attribute on the interface prototype.
    /// Function-template accessors receive the actual `this` and enforce the interface
    /// signature, unlike PropertyCallbackArguments-based instance accessors.
    pub fn define_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, None, None, false)
    }

    /// Defines a prototype attribute whose native getter (and optional setter) needs the JS
    /// engine: both receive a [`ScriptContext`], and the setter gets the assigned value already
    /// converted by `conversion` (with `nullable` mapping null/undefined to [`Value::Null`]).
    /// Errors the natives return are thrown.
    pub fn define_contextual_attribute(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: ContextualPropertyGetter,
        setter: Option<ContextualPropertySetter>,
        conversion: WebIdlArgumentConversion,
        nullable: bool,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let arguments = WebIdlArguments::new(&[conversion], &[nullable], &[false], &[None]);
        self.define_attribute_with_conversion(interface, name, getter, setter, arguments, false)
    }

    /// [`Runtime::define_contextual_attribute`] for a `[LegacyLenientThis]` attribute (most
    /// `GlobalEventHandlers`): with an invalid receiver the getter returns `undefined` and the
    /// setter does nothing, instead of throwing.
    pub fn define_lenient_contextual_attribute(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: ContextualPropertyGetter,
        setter: Option<ContextualPropertySetter>,
        conversion: WebIdlArgumentConversion,
        nullable: bool,
    ) -> Result<(), String> {
        let arguments = WebIdlArguments::new(&[conversion], &[nullable], &[false], &[None]);
        self.define_attribute_with_conversion(interface, name, getter, setter, arguments, true)
    }

    /// Like [`Runtime::define_contextual_attribute`] with the assigned value converted by a
    /// structured [`WebIdlType`] (unions such as `fillStyle`'s, buffers, handles).
    pub fn define_typed_attribute(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: ContextualPropertyGetter,
        setter: Option<ContextualPropertySetter>,
        ty: WebIdlType,
    ) -> Result<(), String> {
        let arguments = WebIdlArguments::typed(&[WebIdlArgument { ty, optional: false, variadic: false, default: None }]);
        self.define_attribute_with_conversion(interface, name, getter, setter, arguments, false)
    }

    fn define_attribute_with_conversion(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: ContextualPropertyGetter,
        setter: Option<ContextualPropertySetter>,
        conversion: WebIdlArguments,
        lenient_this: bool,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let config = Box::new(ContextualAttributeConfig { getter, setter, conversion, lenient_this });
        let config_pointer = (&*config) as *const ContextualAttributeConfig;
        self.attribute_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let key = v8::String::new(scope, name).ok_or("invalid attribute name")?;
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let getter_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this attribute's boxed config.
                let config = unsafe { &*(external.value() as *const ContextualAttributeConfig) };
                let Some(native) = attribute_receiver(scope, args.this(), config.lenient_this) else { return };
                let result = (config.getter)(&mut ScriptContext { scope }, native);
                match result {
                    Ok(value) => {
                        let value = v8_result(scope, &value);
                        retval.set(value);
                    },
                    Err(error) => throw_webidl_error(scope, &error),
                }
            },
        )
        .data(data.into());
        let getter_template = if lenient_this { getter_template } else { getter_template.signature(signature) }
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope);
        let getter_name = v8::String::new(scope, &format!("get {name}")).unwrap();
        getter_template.set_class_name(getter_name);
        let setter_template = if setter.is_some() {
            let setter_template = v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 args: v8::FunctionCallbackArguments,
                 _retval: v8::ReturnValue| {
                    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                    // SAFETY: the External points to this attribute's boxed config.
                    let config = unsafe { &*(external.value() as *const ContextualAttributeConfig) };
                    let Some(native) = attribute_receiver(scope, args.this(), config.lenient_this) else { return };
                    let Some(value) = convert_webidl_arguments(scope, &args, &config.conversion) else {
                        return;
                    };
                    let setter = config.setter.expect("setter template exists only with a setter");
                    if let Err(error) = setter(&mut ScriptContext { scope }, native, &value[0]) {
                        throw_webidl_error(scope, &error);
                    }
                },
            )
            .data(data.into());
            let setter_template = if lenient_this { setter_template } else { setter_template.signature(signature) }
                .length(1)
                .constructor_behavior(v8::ConstructorBehavior::Throw)
                .build(scope);
            let setter_name = v8::String::new(scope, &format!("set {name}")).unwrap();
            setter_template.set_class_name(setter_name);
            Some(setter_template)
        } else {
            None
        };
        let setter_template = setter_template.or_else(|| lenient_setter(scope, interface, name));
        let (target, attributes) = member_target(scope, interface, template, false);
        target.set_accessor_property(key.into(), Some(getter_template), setter_template, attributes);
        Ok(())
    }

    /// Defines a static operation on the interface object (`Interface.name(...)`), with
    /// structured arguments. The native receives a [`ScriptContext`] and no receiver; errors
    /// it returns are thrown.
    pub fn define_static_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: StaticNativeMethod,
        arguments: &[WebIdlArgument],
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let promise = interface.promise_operations.borrow().contains(name);
        let config = Box::new(StaticMethodConfig { method, arguments: WebIdlArguments::typed(arguments), promise });
        let required = arguments.iter().take_while(|argument| !argument.optional && !argument.variadic).count();
        let config_pointer = (&*config) as *const StaticMethodConfig;
        self.static_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let key = v8::String::new(scope, name).ok_or("invalid method name")?;
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this method's boxed config.
                let config = unsafe { &*(external.value() as *const StaticMethodConfig) };
                let Some(arguments) = reject_on_failure(scope, config.promise, &mut retval, |scope| {
                    convert_webidl_arguments(scope, &args, &config.arguments)
                }) else {
                    return;
                };
                let result = (config.method)(&mut ScriptContext { scope }, &arguments);
                match result {
                    Ok(value) => {
                        let value = v8_result(scope, &value);
                        retval.set(value);
                    },
                    Err(error) => throw_webidl_error(scope, &error),
                }
            },
        )
        .data(data.into())
        .length(required as i32)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        template.set(key.into(), function_template.into());
        Ok(())
    }

    /// Like [`Runtime::define_static_webidl_method`] for an overloaded static operation (or
    /// namespace operation, e.g. `CSS.supports`): the overload is chosen with WebIDL's overload
    /// resolution algorithm, as for [`Runtime::define_typed_overloaded_webidl_method`].
    pub fn define_static_overloaded_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        overloads: &[(StaticNativeMethod, &[WebIdlArgument])],
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let length = overloads
            .iter()
            .map(|(_, arguments)| arguments.iter().filter(|argument| !argument.optional && !argument.variadic && argument.default.is_none()).count())
            .min()
            .unwrap_or(0);
        let config = Box::new(StaticOverloadConfig {
            overloads: overloads
                .iter()
                .map(|(method, arguments)| (*method, WebIdlArguments::typed(arguments), arguments.to_vec()))
                .collect(),
            promise: interface.promise_operations.borrow().contains(name),
        });
        let config_pointer = (&*config) as *const StaticOverloadConfig;
        self.static_overload_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let key = v8::String::new(scope, name).ok_or("invalid method name")?;
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this method's boxed overload config.
                let config = unsafe { &*(external.value() as *const StaticOverloadConfig) };
                let candidates: Vec<&[WebIdlArgument]> = config.overloads.iter().map(|(_, _, arguments)| arguments.as_slice()).collect();
                let Some((selected, converted)) = reject_on_failure(scope, config.promise, &mut retval, |scope| {
                    let selected = select_overload(scope, &args, &candidates).ok()?;
                    let converted = convert_webidl_arguments(scope, &args, &config.overloads[selected].1)?;
                    Some((selected, converted))
                }) else {
                    return;
                };
                let (method, _, _) = &config.overloads[selected];
                match method(&mut ScriptContext { scope }, &converted) {
                    Ok(value) => {
                        let value = v8_result(scope, &value);
                        retval.set(value);
                    },
                    Err(error) => throw_webidl_error(scope, &error),
                }
            },
        )
        .data(data.into())
        .length(length as i32)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        template.set(key.into(), function_template.into());
        Ok(())
    }

    /// Defines a `[LegacyFactoryFunction=name(arguments)]` (`new Image(w, h)`): a constructor
    /// function, exposed on the global with `interface`, whose `prototype` is the interface
    /// prototype object and which creates traced instances of `interface`.
    pub fn define_legacy_factory_function(
        &mut self,
        interface: &Interface,
        name: &str,
        constructor: NativeConstructor,
        arguments: &[WebIdlArgument],
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let required = arguments
            .iter()
            .take_while(|argument| !argument.optional && !argument.variadic && argument.default.is_none())
            .count();
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let config = Box::new(LegacyFactoryConfig {
            constructor,
            arguments: WebIdlArguments::typed(arguments),
            interface: interface.name.clone(),
            template: interface.template.clone(),
        });
        let config_pointer = (&*config) as *const LegacyFactoryConfig;
        self.legacy_factory_configs.push(config);
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let factory = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this factory's boxed config.
                let config = unsafe { &*(external.value() as *const LegacyFactoryConfig) };
                if args.new_target().is_undefined() {
                    throw_type_error(scope, "Constructor requires 'new'");
                    return;
                }
                let Some(arguments) = convert_webidl_arguments(scope, &args, &config.arguments) else { return };
                let native = match (config.constructor)(&arguments) {
                    Ok(native) => native,
                    Err(error) => {
                        throw_webidl_error(scope, &error);
                        return;
                    },
                };
                // The new object is an instance of the interface, not of the factory function.
                let template = v8::Local::new(scope, &config.template);
                let Some(object) = template.instance_template(scope).new_instance(scope) else { return };
                attach_new_traced_native(scope, object, &config.interface, native);
                retval.set(object.into());
            },
        )
        .data(data.into())
        .length(required as i32)
        .build(scope);
        factory.set_class_name(v8::String::new(scope, name).ok_or("invalid factory name")?);
        interface.legacy_factories.borrow_mut().push((name.to_owned(), v8::Global::new(scope, factory)));
        Ok(())
    }

    /// Marks the readonly attribute `name` as `[Replaceable]` (`self.origin`, `window.length`):
    /// assigning it defines an own data property of that name on the receiver, shadowing the
    /// accessor. Call before defining the attribute.
    pub fn mark_replaceable(&mut self, interface: &Interface, name: &str) {
        interface.replaceable.borrow_mut().insert(name.to_owned());
    }

    /// Makes `interface` an `[ExceptionClass]` (`DOMException`): its prototype object inherits
    /// `Error.prototype`, so its instances (and descendants') are `instanceof Error`.
    pub fn make_exception_class(&mut self, interface: &Interface) {
        interface.exception_class.set(true);
    }

    /// Defines the interface object of a callback interface with constants (`NodeFilter`): a
    /// function that throws when called and holds the constants, without a `prototype`.
    pub fn define_callback_interface(&mut self, name: &str) -> Interface {
        let interface = self.define_interface_inner(name, None, None);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::Local::new(scope, &interface.template).remove_prototype();
        interface
    }

    /// Makes `interface` a `[Global]` interface (`Window`, `DedicatedWorkerGlobalScope`): its
    /// regular attributes and operations defined after this are own properties of the global
    /// object. Create the realm with [`Runtime::install_global`].
    pub fn make_global(&mut self, interface: &Interface) {
        interface.global.set(true);
    }

    /// Replaces this runtime's realm with a new one whose global object is an instance of the
    /// `[Global]` interface `interface`, owning `native` (traced like a constructed instance):
    /// `globalThis` is the `Window`, its prototype chain is `Window.prototype` → ancestors, and
    /// its members are own properties. Every interface already defined is exposed again in the
    /// new realm.
    ///
    /// Call this after installing the bindings and before running scripts or creating
    /// instances: values, wrappers and global properties of the previous realm stay there.
    pub fn install_global(&mut self, interface: &Interface, native: TracedNative) -> Result<(), String> {
        if !interface.global.get() {
            return Err(format!("{} is not a [Global] interface", interface.name));
        }
        let context = {
            v8::scope!(let scope, &mut self.isolate);
            let template = v8::Local::new(scope, &interface.template);
            let global_template = template.instance_template(scope);
            let context = v8::Context::new(
                scope,
                v8::ContextOptions { global_template: Some(global_template), global_object: None, microtask_queue: None },
            );
            let scope = &mut v8::ContextScope::new(scope, context);
            // `context.global()` is the global proxy, the only global object scripts ever see
            // (receivers, `globalThis`); V8 gives it the template's internal fields.
            let proxy = context.global(scope);
            if proxy.internal_field_count() < 1 {
                return Err("the global proxy has no internal field".into());
            }
            let gc_box = GcBox {
                native: std::cell::UnsafeCell::new(native.native),
                trace: native.trace,
                wrapper: std::cell::UnsafeCell::new(None),
                wrapper_interface: std::cell::UnsafeCell::new(None),
            };
            let heap = scope.get_cpp_heap().expect("V8 isolates carry a cppgc heap");
            // SAFETY: moved into a Persistent before anything else can run a GC.
            let pointer = unsafe { v8::cppgc::make_garbage_collected(heap, gc_box) };
            let root = v8::cppgc::Persistent::new(&pointer);
            let gc_box = root.get().expect("a fresh root points at its native");
            proxy.set_aligned_pointer_in_internal_field(0, gc_box.native.get() as *const std::ffi::c_void, WRAPPED_POINTER_TAG);
            // The proxy is the native's one wrapper, so a native returning itself
            // (`window.self`) yields `globalThis`.
            // SAFETY: no other reference to these slots is live.
            unsafe {
                *gc_box.wrapper.get() = Some(v8::TracedReference::new(scope, proxy));
                *gc_box.wrapper_interface.get() = Some(interface.name.clone());
            }
            // The global lives as long as the realm: the runtime roots its native.
            self.global_native = Some(root);
            v8::Global::new(scope, context)
        };
        self.context = context;
        for defined in self.interfaces.clone() {
            if defined.constructor_exposed.get() {
                defined.constructor_exposed.set(false);
                self.expose_interface(&defined)?;
            }
        }
        Ok(())
    }

    /// The native of the global object installed by [`Runtime::install_global`], as an
    /// interface-typed value (for example what `window.self` returns).
    pub fn global_ref(&self) -> Option<NativeRef> {
        let root = self.global_native.as_ref()?;
        let gc_box = root.get()?;
        // SAFETY: shared read of the interface name, set once at installation.
        let interface = unsafe { &*gc_box.wrapper_interface.get() }.clone()?;
        Some(NativeRef { gc: v8::cppgc::Persistent::new(root), interface })
    }

    /// Makes `interface` `[LegacyUnforgeable]` as a whole (`Location`): regular attributes and
    /// operations defined after this become non-configurable own properties of each instance,
    /// and instances get WebIDL's unforgeable `valueOf` (`%Object.prototype.valueOf%`).
    pub fn make_unforgeable(&mut self, interface: &Interface) {
        interface.unforgeable_members.set(true);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let key = v8::String::new(scope, "valueOf").unwrap();
        template.instance_template(scope).set_intrinsic_data_property(
            key.into(),
            v8::Intrinsic::ObjProtoValueOf,
            v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM | v8::PropertyAttribute::DONT_DELETE,
        );
    }

    /// Marks the operation `name` as promise-returning: when its arguments fail to convert, it
    /// returns a rejected promise instead of throwing (WebIDL). Call before defining it; the
    /// native's own errors are already returned as rejected promises by generated code.
    pub fn mark_promise_operation(&mut self, interface: &Interface, name: &str) {
        interface.promise_operations.borrow_mut().insert(name.to_owned());
    }

    /// Marks the readonly attribute `name` as `[LegacyLenientSetter]`: it gets a setter that
    /// ignores assignments. Call before defining the attribute.
    pub fn mark_lenient_setter(&mut self, interface: &Interface, name: &str) {
        interface.lenient_setters.borrow_mut().insert(name.to_owned());
    }

    /// Defines a readonly static attribute (or a namespace attribute, e.g. `CSS.paintWorklet`):
    /// an accessor on the interface object whose getter calls `getter` with no arguments.
    pub fn define_static_attribute(&mut self, interface: &Interface, name: &str, getter: StaticNativeMethod) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let config = Box::new(StaticMethodConfig { method: getter, arguments: WebIdlArguments::typed(&[]), promise: false });
        let config_pointer = (&*config) as *const StaticMethodConfig;
        self.static_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let key = v8::String::new(scope, name).ok_or("invalid attribute name")?;
        let getter_name = v8::String::new(scope, &format!("get {name}")).ok_or("invalid attribute name")?;
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let getter_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this attribute's boxed config.
                let config = unsafe { &*(external.value() as *const StaticMethodConfig) };
                match (config.method)(&mut ScriptContext { scope }, &[]) {
                    Ok(value) => {
                        let value = v8_result(scope, &value);
                        retval.set(value);
                    },
                    Err(error) => throw_webidl_error(scope, &error),
                }
            },
        )
        .data(data.into())
        .length(0)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        getter_template.set_class_name(getter_name);
        template.set_accessor_property(key.into(), Some(getter_template), None, v8::PropertyAttribute::NONE);
        Ok(())
    }

    /// Installs `@@unscopables` on the interface prototype: an object whose `names` are `true`,
    /// so `with` statements do not see those members. `names` must include the unscopable
    /// members of ancestor interfaces too (the generator flattens them).
    pub fn define_unscopables(&mut self, interface: &Interface, names: &[&str]) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let unscopables = v8::ObjectTemplate::new(scope);
        for name in names {
            let key = v8::String::new(scope, name).ok_or("invalid unscopable name")?;
            unscopables.set(key.into(), v8::Boolean::new(scope, true).into());
        }
        let symbol = v8::Symbol::get_unscopables(scope);
        template.prototype_template(scope).set_with_attr(
            symbol.into(),
            unscopables.into(),
            v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
        );
        Ok(())
    }

    /// Defines WebIDL's default `toJSON` operation: a new plain object holding the current
    /// value of each listed attribute (read through the normal getters, so the receiver check
    /// and conversions apply). The generator lists the JSON-typed attributes of the interface
    /// and of every ancestor that also has a default `toJSON`, ancestors first.
    pub fn define_default_to_json(&mut self, interface: &Interface, attributes: &[&str]) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let config = Box::new(DefaultToJsonConfig {
            attributes: attributes.iter().map(|name| name.to_string()).collect(),
        });
        let config_pointer = (&*config) as *const DefaultToJsonConfig;
        self.to_json_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let key = v8::String::new(scope, "toJSON").unwrap();
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this operation's boxed config.
                let config = unsafe { &*(external.value() as *const DefaultToJsonConfig) };
                let this = args.this();
                let result = v8::Object::new(scope);
                for name in &config.attributes {
                    let key = v8::String::new(scope, name).unwrap();
                    // A getter that throws leaves its exception pending; stop there.
                    let Some(value) = this.get(scope, key.into()) else { return };
                    result.create_data_property(scope, key.into(), value);
                }
                retval.set(result.into());
            },
        )
        .data(data.into())
        .signature(signature)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        template.prototype_template(scope).set(key.into(), function_template.into());
        Ok(())
    }

    /// Defines a WebIDL constant: a `{ writable: false, enumerable: true, configurable: false }`
    /// data property on both the interface object and its prototype.
    pub fn define_constant(&mut self, interface: &Interface, name: &str, value: &Value) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let key = v8::String::new(scope, name).ok_or("invalid constant name")?;
        let constant: v8::Local<v8::Data> = match value {
            Value::Number(number) => v8::Number::new(scope, *number).into(),
            Value::Bool(boolean) => v8::Boolean::new(scope, *boolean).into(),
            _ => return Err(format!("unsupported WebIDL constant value for {name}")),
        };
        let attributes = || v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE;
        template.set_with_attr(key.into(), constant, attributes());
        template.prototype_template(scope).set_with_attr(key.into(), constant, attributes());
        Ok(())
    }

    /// Defines a read-only `[LegacyUnforgeable]` attribute: a non-configurable accessor that is
    /// an own property of every instance of the interface and of its descendants, instead of a
    /// prototype property.
    pub fn define_unforgeable_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, None, None, true)
    }

    /// Defines a prototype attribute with both a getter and a setter. Callback functions
    /// each carry their own pointer data; no shared tuple allocation is needed.
    pub fn define_settable_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: PropertySetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, Some(setter), None, None, None, false)
    }

    /// Defines a WebIDL DOMString attribute with JavaScript ToString conversion.
    pub fn define_domstring_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: DomStringSetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, Some(setter), None, None, false)
    }

    /// Defines a nullable WebIDL DOMString attribute. JavaScript `null` maps to IDL null;
    /// other values use JavaScript ToString and preserve UTF-16 code units.
    pub fn define_nullable_domstring_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: NullableDomStringSetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, Some(setter), None, false)
    }

    /// Defines a settable primitive attribute whose conversion follows WebIDL's boolean or
    /// numeric conversion rules before native state is mutably borrowed.
    pub fn define_webidl_primitive_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: WebIdlPrimitiveSetter,
        conversion: PrimitiveConversion,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, None, Some((setter, conversion)), false)
    }

    fn define_attribute(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: Option<PropertySetter>,
        domstring_setter: Option<DomStringSetter>,
        nullable_domstring_setter: Option<NullableDomStringSetter>,
        primitive_setter: Option<(WebIdlPrimitiveSetter, PrimitiveConversion)>,
        unforgeable: bool,
    ) -> Result<(), String> {
        if unforgeable && interface.has_descendants.get() {
            return Err("unforgeable attributes must be defined before descendant interfaces".into());
        }
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let key = v8::String::new(scope, name).ok_or("invalid attribute name")?;
        let getter_data = v8::External::new(scope, getter as *mut std::ffi::c_void);
        let getter_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: data is the PropertyGetter function pointer installed above.
                let getter: PropertyGetter = unsafe { std::mem::transmute(external.value()) };
                let this = args.this();
                // The signature rejects foreign receivers before this callback runs.
                // A valid receiver still needs initialized native data (JS constructors
                // for nonconstructible interfaces are rejected separately).
                if this.internal_field_count() < 1 {
                    throw_type_error(scope, "Illegal invocation");
                    return;
                }
                let raw = unsafe {
                    this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    throw_type_error(scope, "Illegal invocation");
                    return;
                }
                // SAFETY: our instance owns this Box; its live local handle prevents GC.
                let value = getter(unsafe { (&*raw).as_ref() });
                let value = v8_result(scope, &value);
                retval.set(value);
            },
        )
        .data(getter_data.into())
        .signature(signature)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        let getter_name = v8::String::new(scope, &format!("get {name}")).unwrap();
        getter_template.set_class_name(getter_name);
        let setter_template = if let Some(setter) = setter {
            let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
            Some(v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 args: v8::FunctionCallbackArguments,
                 _retval: v8::ReturnValue| {
                    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                    let setter: PropertySetter = unsafe { std::mem::transmute(external.value()) };
                    let this = args.this();
                    if this.internal_field_count() < 1 {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let raw = unsafe {
                        this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                    } as *mut Box<dyn std::any::Any>;
                    if raw.is_null() {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let value = native_value(scope, args.get(0));
                    // SAFETY: the instance owns this Box and its live local handle prevents GC.
                    // Setters receive a shared reference: natives may re-enter JavaScript
                    // (ScriptContext::call), so mutation goes through interior mutability, as
                    // in Servo's DOM.
                    setter(unsafe { (&*raw).as_ref() }, &value);
                },
            )
            .data(setter_data.into())
            .signature(signature)
            .length(1)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope))
        } else if let Some((setter, conversion)) = primitive_setter {
            let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
            macro_rules! primitive_setter_template {
                ($conversion:expr) => {
                    v8::FunctionTemplate::builder(
                        |scope: &mut v8::PinScope,
                         args: v8::FunctionCallbackArguments,
                         _retval: v8::ReturnValue| {
                            let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                            let setter: WebIdlPrimitiveSetter = unsafe { std::mem::transmute(external.value()) };
                            let this = args.this();
                            if this.internal_field_count() < 1 {
                                throw_type_error(scope, "Illegal invocation");
                                return;
                            }
                            let raw = unsafe { this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) }
                                as *mut Box<dyn std::any::Any>;
                            if raw.is_null() {
                                throw_type_error(scope, "Illegal invocation");
                                return;
                            }
                            let value = args.get(0);
                            let converted = match $conversion {
                                PrimitiveConversion::Boolean => Value::Bool(value.boolean_value(scope)),
                                PrimitiveConversion::Double => {
                                    let Some(number) = value.number_value(scope) else { return; };
                                    if !number.is_finite() {
                                        throw_type_error(scope, "double must be finite");
                                        return;
                                    }
                                    Value::Number(number)
                                }
                                PrimitiveConversion::UnsignedLong => {
                                    let Some(number) = value.number_value(scope) else { return; };
                                    if !number.is_finite() || number == 0.0 {
                                        Value::Number(0.0)
                                    } else {
                                        Value::Number(number.trunc().rem_euclid(4_294_967_296.0))
                                    }
                                }
                                PrimitiveConversion::NullableBoolean => {
                                    if value.is_null() { Value::Null } else { Value::Bool(value.boolean_value(scope)) }
                                }
                                PrimitiveConversion::NullableDouble => {
                                    if value.is_null() {
                                        Value::Null
                                    } else {
                                        let Some(number) = value.number_value(scope) else { return; };
                                        if !number.is_finite() {
                                            throw_type_error(scope, "double must be finite");
                                            return;
                                        }
                                        Value::Number(number)
                                    }
                                }
                                PrimitiveConversion::NullableUnsignedLong => {
                                    if value.is_null() {
                                        Value::Null
                                    } else {
                                        let Some(number) = value.number_value(scope) else { return; };
                                        if !number.is_finite() || number == 0.0 {
                                            Value::Number(0.0)
                                        } else {
                                            Value::Number(number.trunc().rem_euclid(4_294_967_296.0))
                                        }
                                    }
                                }
                                PrimitiveConversion::UsvString => {
                                    let Some(string) = value.to_string(scope) else { return; };
                                    Value::String(string.to_rust_string_lossy(scope))
                                }
                                PrimitiveConversion::NullableUsvString => {
                                    if value.is_null() {
                                        Value::Null
                                    } else {
                                        let Some(string) = value.to_string(scope) else { return; };
                                        Value::String(string.to_rust_string_lossy(scope))
                                    }
                                }
                            };
                            // Coercion may run user JavaScript; borrow native state only after it completes.
                            setter(unsafe { (&*raw).as_ref() }, &converted);
                        },
                    )
                    .data(setter_data.into())
                    .signature(signature)
                    .length(1)
                    .constructor_behavior(v8::ConstructorBehavior::Throw)
                    .build(scope)
                };
            }
            Some(match conversion {
                PrimitiveConversion::Boolean => primitive_setter_template!(PrimitiveConversion::Boolean),
                PrimitiveConversion::Double => primitive_setter_template!(PrimitiveConversion::Double),
                PrimitiveConversion::UnsignedLong => primitive_setter_template!(PrimitiveConversion::UnsignedLong),
                PrimitiveConversion::NullableBoolean => primitive_setter_template!(PrimitiveConversion::NullableBoolean),
                PrimitiveConversion::NullableDouble => primitive_setter_template!(PrimitiveConversion::NullableDouble),
                PrimitiveConversion::NullableUnsignedLong => primitive_setter_template!(PrimitiveConversion::NullableUnsignedLong),
                PrimitiveConversion::UsvString => primitive_setter_template!(PrimitiveConversion::UsvString),
                PrimitiveConversion::NullableUsvString => primitive_setter_template!(PrimitiveConversion::NullableUsvString),
            })
        } else if let Some(setter) = nullable_domstring_setter {
            let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
            Some(v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 args: v8::FunctionCallbackArguments,
                 _retval: v8::ReturnValue| {
                    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                    let setter: NullableDomStringSetter = unsafe { std::mem::transmute(external.value()) };
                    let this = args.this();
                    if this.internal_field_count() < 1 {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let raw = unsafe { this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) }
                        as *mut Box<dyn std::any::Any>;
                    if raw.is_null() {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let value = args.get(0);
                    let converted = if value.is_null() {
                        None
                    } else {
                        let Some(string) = value.to_string(scope) else { return; };
                        let mut units = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
                        Some(units)
                    };
                    // ToString can run user code or throw; borrow native state only afterwards.
                    setter(unsafe { (&*raw).as_ref() }, converted);
                },
            )
            .data(setter_data.into())
            .signature(signature)
            .length(1)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope))
        } else {
            domstring_setter.map(|setter| {
                let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
                v8::FunctionTemplate::builder(
                    |scope: &mut v8::PinScope,
                     args: v8::FunctionCallbackArguments,
                     _retval: v8::ReturnValue| {
                        let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                        let setter: DomStringSetter =
                            unsafe { std::mem::transmute(external.value()) };
                        let this = args.this();
                        if this.internal_field_count() < 1 {
                            throw_type_error(scope, "Illegal invocation");
                            return;
                        }
                        let raw = unsafe {
                            this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                        } as *mut Box<dyn std::any::Any>;
                        if raw.is_null() {
                            throw_type_error(scope, "Illegal invocation");
                            return;
                        }
                        let Some(string) = args.get(0).to_string(scope) else { return; };
                        let mut units = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
                        // The ToString operation above may execute user code, so take the
                        // mutable native borrow only after conversion has completed.
                        setter(unsafe { (&*raw).as_ref() }, units);
                    },
                )
                .data(setter_data.into())
                .signature(signature)
                .length(1)
                .constructor_behavior(v8::ConstructorBehavior::Throw)
                .build(scope)
            })
        };
        let setter_template = setter_template.map(|function| {
            let setter_name = v8::String::new(scope, &format!("set {name}")).unwrap();
            function.set_class_name(setter_name);
            function
        });
        // [LegacyUnforgeable] attributes are non-configurable own properties of every
        // instance; FunctionTemplate::inherit carries instance-template accessors into
        // descendants, matching the WebIDL parser copying them into each descendant.
        let (target, attributes) = if unforgeable {
            interface
                .unforgeable_accessors
                .borrow_mut()
                .push((name.to_string(), v8::Global::new(scope, getter_template)));
            (template.instance_template(scope), v8::PropertyAttribute::DONT_DELETE)
        } else {
            member_target(scope, interface, template, false)
        };
        let setter_template = setter_template.or_else(|| lenient_setter(scope, interface, name));
        target.set_accessor_property(key.into(), Some(getter_template), setter_template, attributes);
        Ok(())
    }

    /// Defines a nonconstructible method on the interface prototype, with a signature
    /// enforcing valid receivers (including interface inheritance).
    pub fn define_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
    ) -> Result<(), String> {
        self.define_webidl_method(interface, name, method, &[])
    }

    /// Defines a method whose arguments are converted using the listed required WebIDL types.
    /// Extra JavaScript arguments are ignored by the generated binding; omitted arguments are
    /// converted from `undefined` according to the declared type.
    pub fn define_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
    ) -> Result<(), String> {
        self.define_webidl_method_with_nullable_arguments(interface, name, method, conversions, &[])
    }

    /// Defines a method whose required arguments use the listed WebIDL conversions and
    /// nullable flags. For nullable types both `null` and `undefined` map to IDL null before
    /// applying the inner type's conversion.
    pub fn define_webidl_method_with_nullable_arguments(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
    ) -> Result<(), String> {
        self.define_webidl_method_with_argument_flags(
            interface, name, method, conversions, nullable_arguments, &[],
        )
    }

    /// Defines a method with per-argument nullable and optional WebIDL semantics. An optional
    /// argument whose value is omitted or `undefined` reaches the native callback as
    /// [`Value::Missing`], before nullable conversion; `null` for a nullable argument remains
    /// [`Value::Null`]. Explicit default values are handled by generated bindings separately.
    pub fn define_webidl_method_with_argument_flags(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
    ) -> Result<(), String> {
        self.define_webidl_method_with_argument_flags_and_enums(
            interface, name, method, conversions, nullable_arguments, optional_arguments, &[],
        )
    }

    /// Defines a method with nullable, optional and enum-value constraints for each argument.
    /// Enum values are validated after JavaScript ToString and before the native callback.
    pub fn define_webidl_method_with_argument_flags_and_enums(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
        enumeration_values: &[Option<&[&str]>],
    ) -> Result<(), String> {
        self.define_method_inner(
            interface,
            name,
            NativeMethodKind::Infallible(method),
            conversions,
            nullable_arguments,
            optional_arguments,
            enumeration_values,
        )
    }

    /// Defines an overloaded WebIDL operation whose overloads accept disjoint argument-count
    /// ranges. As in the WebIDL overload resolution algorithm, the argument count (capped at the
    /// longest overload) selects the overload, whose own conversions then apply; a count no
    /// overload accepts throws a TypeError. Every overload is fallible (`[Throws]` or not).
    pub fn define_overloaded_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        overloads: &[WebIdlOverload],
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let config = Box::new(WebIdlOverloadConfig {
            overloads: overloads
                .iter()
                .map(|overload| {
                    (
                        overload.method,
                        WebIdlArguments::new(
                            overload.conversions,
                            overload.nullable_arguments,
                            overload.optional_arguments,
                            overload.enumeration_values,
                        ),
                    )
                })
                .collect(),
        });
        let config_pointer = (&*config) as *const WebIdlOverloadConfig;
        self.overload_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let key = v8::String::new(scope, name).ok_or("invalid method name")?;
        // `length` of an overloaded operation is the shortest overload's required count.
        let length = overloads
            .iter()
            .map(|overload| overload.optional_arguments.iter().filter(|optional| !**optional).count())
            .min()
            .unwrap_or(0);
        let external_data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else {
                    return;
                };
                // SAFETY: the External points to this method's boxed overload config.
                let config = unsafe { &*(external.value() as *const WebIdlOverloadConfig) };
                let this = args.this();
                if this.internal_field_count() < 1 {
                    throw_type_error(scope, "Illegal invocation");
                    return;
                }
                let raw = unsafe {
                    this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    throw_type_error(scope, "Illegal invocation");
                    return;
                }
                let longest = config
                    .overloads
                    .iter()
                    .map(|(_, arguments)| arguments.conversions.len())
                    .max()
                    .unwrap_or(0);
                let count = (args.length().max(0) as usize).min(longest);
                let selected = config.overloads.iter().find(|(_, arguments)| {
                    let required = arguments.optional_arguments.iter().filter(|optional| !**optional).count();
                    required <= count && count <= arguments.conversions.len()
                });
                let Some((method, arguments)) = selected else {
                    throw_type_error(scope, "no overload accepts this number of arguments");
                    return;
                };
                let Some(converted) = convert_webidl_arguments(scope, &args, arguments) else {
                    return;
                };
                // SAFETY: as for the single-signature method callback.
                let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };
                match method(boxed_any.as_ref(), &converted) {
                    Ok(result) => {
                        let result = v8_result(scope, &result);
                        retval.set(result);
                    },
                    Err(error) => throw_webidl_error(scope, &error),
                }
            },
        )
        .data(external_data.into())
        .signature(signature)
        .length(length as i32)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        let function_value: v8::Local<v8::Data> = function_template.into();
        let (target, attributes) = member_target(scope, interface, template, true);
        target.set_with_attr(key.into(), function_value, attributes);
        Ok(())
    }

    /// Defines an overloaded operation resolved by the full WebIDL overload resolution
    /// algorithm (argument count, then the distinguishing argument's type). Each overload may
    /// be plain, fallible or contextual.
    pub fn define_typed_overloaded_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        overloads: Vec<WebIdlTypedOverload>,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let length = overloads
            .iter()
            .map(|overload| {
                overload.arguments.iter().filter(|argument| !argument.optional && !argument.variadic && argument.default.is_none()).count()
            })
            .min()
            .unwrap_or(0);
        let config = Box::new(TypedOverloadConfig {
            overloads: overloads
                .into_iter()
                .map(|overload| {
                    let kind = match overload.method {
                        WebIdlNativeOperation::Plain(method) => NativeMethodKind::Infallible(method),
                        WebIdlNativeOperation::Fallible(method) => NativeMethodKind::Fallible(method),
                        WebIdlNativeOperation::Contextual(method) => NativeMethodKind::Contextual(method),
                    };
                    (kind, WebIdlArguments::typed(overload.arguments), overload.arguments.to_vec())
                })
                .collect(),
            promise: interface.promise_operations.borrow().contains(name),
        });
        let config_pointer = (&*config) as *const TypedOverloadConfig;
        self.typed_overload_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let key = v8::String::new(scope, name).ok_or("invalid method name")?;
        let data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: the External points to this method's boxed overload config.
                let config = unsafe { &*(external.value() as *const TypedOverloadConfig) };
                let Some(native) = receiver_native(scope, args.this()) else { return };
                let candidates: Vec<&[WebIdlArgument]> = config.overloads.iter().map(|(_, _, arguments)| arguments.as_slice()).collect();
                let Some((selected, converted)) = reject_on_failure(scope, config.promise, &mut retval, |scope| {
                    let selected = select_overload(scope, &args, &candidates).ok()?;
                    let converted = convert_webidl_arguments(scope, &args, &config.overloads[selected].1)?;
                    Some((selected, converted))
                }) else {
                    return;
                };
                let (method, _, _) = &config.overloads[selected];
                let result = match method {
                    NativeMethodKind::Infallible(method) => Ok(method(native, &converted)),
                    NativeMethodKind::Fallible(method) => method(native, &converted),
                    NativeMethodKind::Contextual(method) => method(&mut ScriptContext { scope }, native, &converted),
                };
                match result {
                    Ok(result) => {
                        let result = v8_result(scope, &result);
                        retval.set(result);
                    },
                    Err(error) => throw_webidl_error(scope, &error),
                }
            },
        )
        .data(data.into())
        .signature(signature)
        .length(length as i32)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        let function_value: v8::Local<v8::Data> = function_template.into();
        let (target, attributes) = member_target(scope, interface, template, true);
        target.set_with_attr(key.into(), function_value, attributes);
        Ok(())
    }

    /// Like [`Runtime::define_webidl_method_with_argument_flags_and_enums`] for an operation
    /// that may throw: an `Err` from `method` is thrown to JS as the matching exception.
    pub fn define_fallible_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: FallibleNativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
        enumeration_values: &[Option<&[&str]>],
    ) -> Result<(), String> {
        self.define_method_inner(
            interface,
            name,
            NativeMethodKind::Fallible(method),
            conversions,
            nullable_arguments,
            optional_arguments,
            enumeration_values,
        )
    }

    /// Like [`Runtime::define_fallible_webidl_method`] for an operation whose native needs the JS
    /// engine: it receives a [`ScriptContext`].
    pub fn define_contextual_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: ContextualNativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
        enumeration_values: &[Option<&[&str]>],
    ) -> Result<(), String> {
        self.define_method_inner(
            interface,
            name,
            NativeMethodKind::Contextual(method),
            conversions,
            nullable_arguments,
            optional_arguments,
            enumeration_values,
        )
    }

    /// Defines an operation whose arguments are described by structured [`WebIdlType`]s
    /// (sequences and nested nullable/interface types). `method` is plain, fallible or
    /// contextual, as with the other `define_*_method` functions.
    pub fn define_typed_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: WebIdlNativeOperation,
        arguments: &[WebIdlArgument],
    ) -> Result<(), String> {
        let kind = match method {
            WebIdlNativeOperation::Plain(method) => NativeMethodKind::Infallible(method),
            WebIdlNativeOperation::Fallible(method) => NativeMethodKind::Fallible(method),
            WebIdlNativeOperation::Contextual(method) => NativeMethodKind::Contextual(method),
        };
        self.define_method_with_arguments(interface, name, kind, WebIdlArguments::typed(arguments))
    }

    fn define_method_inner(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethodKind,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
        enumeration_values: &[Option<&[&str]>],
    ) -> Result<(), String> {
        let arguments = WebIdlArguments::new(conversions, nullable_arguments, optional_arguments, enumeration_values);
        self.define_method_with_arguments(interface, name, method, arguments)
    }

    fn define_method_with_arguments(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethodKind,
        arguments: WebIdlArguments,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let (member_template, member_attributes) = member_target(scope, interface, template, true);

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a method name string"));
        };
        // Keep the immutable config alive for the isolate's lifetime. Box preserves its address
        // while the owning vector grows, and the isolate drops before these entries.
        let promise = interface.promise_operations.borrow().contains(name);
        let config = Box::new(WebIdlMethodConfig { method, arguments, promise });
        let config_pointer = (&*config) as *const WebIdlMethodConfig as *mut WebIdlMethodConfig;
        self.method_configs.push(config);
        let external_data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);

        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: the External points to the config allocated for this method template.
                let config = unsafe { &*(external.value() as *const WebIdlMethodConfig) };

                let this = args.this();
                if this.internal_field_count() < 1 {
                    return;
                }
                // SAFETY: same reasoning as `get_wrapped`/`define_property` -- this field is
                // only ever set by `create_wrapped`/`create_instance`, always via
                // `set_aligned_pointer_in_internal_field` with this exact `WRAPPED_POINTER_TAG`.
                let raw = unsafe {
                    this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    return;
                }
                let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };

                // WebIDL's ToNumber/ToString conversions can run user code and throw. Preserve
                // that exact exception across the native callback boundary instead of returning
                // undefined when rusty_v8 reports the failed conversion as `None`.
                let Some(arguments) = reject_on_failure(scope, config.promise, &mut retval, |scope| {
                    convert_webidl_arguments(scope, &args, &config.arguments)
                }) else {
                    return;
                };
                let result = match config.method {
                    NativeMethodKind::Infallible(method) => method(boxed_any.as_ref(), &arguments),
                    NativeMethodKind::Fallible(method) => {
                        match method(boxed_any.as_ref(), &arguments) {
                            Ok(result) => result,
                            Err(error) => {
                                throw_webidl_error(scope, &error);
                                return;
                            },
                        }
                    },
                    NativeMethodKind::Contextual(method) => {
                        let result = method(&mut ScriptContext { scope }, boxed_any.as_ref(), &arguments);
                        match result {
                            Ok(result) => result,
                            Err(error) => {
                                throw_webidl_error(scope, &error);
                                return;
                            },
                        }
                    },
                };
                let result = v8_result(scope, &result);
                retval.set(result);
            },
        )
        .data(external_data.into())
        .signature(signature)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);

        let function_value: v8::Local<v8::Data> = function_template.into();
        member_template.set_with_attr(key.into(), function_value, member_attributes);
        Ok(())
    }

    /// Installs a WebIDL named getter for `[LegacyUnenumerableNamedProperties]` interfaces
    /// (`HTMLCollection`, `Window`): `object.name` reads `getter(native, name)` when no real
    /// property of that name exists on the object or its prototype chain (the WebIDL named
    /// property visibility rule), and assigning a supported name is rejected as for a legacy
    /// platform object without a named setter. Only string
    /// keys are intercepted; array indices go to the indexed getter. Like indexed getters, it
    /// must be registered on each derived interface too.
    pub fn define_named_property_getter(&mut self, interface: &Interface, getter: NamedPropertyGetter) -> Result<(), String> {
        self.define_named_properties(
            interface,
            NamedProperties { getter, names: None, setter: None, deleter: None, override_builtins: false, enumerable: false },
        )
    }

    /// Gives `interface` WebIDL named properties (a legacy platform object's named getter, and
    /// optionally its supported names, named setter and named deleter), following WebIDL's
    /// named property visibility: a supported name is visible unless the object has an own
    /// property of that name or, without `[LegacyOverrideBuiltIns]`, its prototype chain does.
    /// - `names` lists the supported property names: they are own keys (enumerable unless
    ///   `enumerable` is false, for `[LegacyUnenumerableNamedProperties]`).
    /// - With a `setter`, `[[Set]]` of any string key on the object itself calls it with the
    ///   value converted to the setter's type (`storage.foo = 1`); without one, assigning a
    ///   visible name is rejected (a TypeError in strict mode).
    /// - `delete` of a visible name calls the `deleter`, or fails without one.
    /// Not inherited through interface inheritance: register on each interface that has them.
    pub fn define_named_properties(&mut self, interface: &Interface, properties: NamedProperties) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("named properties must be defined before creating instances or descendants".to_string());
        }
        let setter_arguments = properties.setter.as_ref().map(|(_, ty)| {
            WebIdlArguments::typed(&[WebIdlArgument { ty: ty.clone(), optional: false, variadic: false, default: None }])
        });
        let config = Box::new(NamedConfig { properties, setter_arguments });
        let config_pointer = (&*config) as *const NamedConfig;
        self.named_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let external_data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let configuration = v8::NamedPropertyHandlerConfiguration::new()
            .getter(
                |scope: &mut v8::PinScope,
                 key: v8::Local<v8::Name>,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Value>| {
                    let Some((config, native)) = named_receiver(&args) else { return v8::Intercepted::kNo };
                    match visible_named_property(scope, config, args.holder(), native, key) {
                        Some(value) => {
                            let value = v8_result(scope, &value);
                            retval.set(value);
                            v8::Intercepted::kYes
                        },
                        None => v8::Intercepted::kNo,
                    }
                },
            )
            .setter(
                |scope: &mut v8::PinScope,
                 key: v8::Local<v8::Name>,
                 value: v8::Local<v8::Value>,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Boolean>| {
                    let Some((config, native)) = named_receiver(&args) else { return v8::Intercepted::kNo };
                    let holder = args.holder();
                    if let (Some((setter, _)), Some(arguments)) = (&config.properties.setter, &config.setter_arguments) {
                        // WebIDL [[Set]]: a string key always goes to the named setter (even names
                        // of prototype members, like `setItem`). V8 does not expose the receiver
                        // to interceptors, so assignments through an inheriting object also land
                        // here (WebIDL would define an own property on that object instead).
                        let Some(arguments_type) = arguments.types.first().and_then(|ty| ty.as_ref()) else {
                            return v8::Intercepted::kNo;
                        };
                        let Some(converted) = convert_typed_value(scope, value, arguments_type) else {
                            return v8::Intercepted::kYes;
                        };
                        let name = key.to_rust_string_lossy(scope);
                        if let Err(error) = setter(native, &name, &converted) {
                            throw_webidl_error(scope, &error);
                        }
                        retval.set_bool(true);
                        return v8::Intercepted::kYes;
                    }
                    // WebIDL legacy platform objects without a named setter reject defining a
                    // supported property name: silently in sloppy mode, TypeError in strict mode.
                    if holder.has_real_named_property(scope, key).unwrap_or(false) {
                        return v8::Intercepted::kNo;
                    }
                    let name = key.to_rust_string_lossy(scope);
                    if (config.properties.getter)(native, &name).is_none() {
                        return v8::Intercepted::kNo;
                    }
                    // `false` reports the failed [[Set]]: V8 throws the TypeError in strict mode.
                    retval.set_bool(false);
                    v8::Intercepted::kYes
                },
            )
            .query(
                |scope: &mut v8::PinScope,
                 key: v8::Local<v8::Name>,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Integer>| {
                    let Some((config, native)) = named_receiver(&args) else { return v8::Intercepted::kNo };
                    if visible_named_property(scope, config, args.holder(), native, key).is_none() {
                        return v8::Intercepted::kNo;
                    }
                    let mut attributes = v8::PropertyAttribute::NONE;
                    if !config.properties.enumerable {
                        attributes = attributes | v8::PropertyAttribute::DONT_ENUM;
                    }
                    if config.properties.setter.is_none() {
                        attributes = attributes | v8::PropertyAttribute::READ_ONLY;
                    }
                    retval.set_int32(attributes.as_u32() as i32);
                    v8::Intercepted::kYes
                },
            )
            .deleter(
                |scope: &mut v8::PinScope,
                 key: v8::Local<v8::Name>,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Boolean>| {
                    let Some((config, native)) = named_receiver(&args) else { return v8::Intercepted::kNo };
                    if visible_named_property(scope, config, args.holder(), native, key).is_none() {
                        return v8::Intercepted::kNo;
                    }
                    match config.properties.deleter {
                        Some(deleter) => {
                            let name = key.to_rust_string_lossy(scope);
                            if let Err(error) = deleter(native, &name) {
                                throw_webidl_error(scope, &error);
                            }
                            retval.set_bool(true);
                        },
                        // Without a deleter a visible named property cannot be deleted.
                        None => retval.set_bool(false),
                    }
                    v8::Intercepted::kYes
                },
            )
            .enumerator(
                |scope: &mut v8::PinScope, args: v8::PropertyCallbackArguments, mut retval: v8::ReturnValue<v8::Array>| {
                    let Some((config, native)) = named_receiver(&args) else { return };
                    let Some(names) = config.properties.names else { return };
                    let holder = args.holder();
                    let mut visible = Vec::new();
                    for name in names(native) {
                        let Some(key) = v8::String::new(scope, &name) else { continue };
                        if named_property_hidden(scope, config, holder, key.into()) {
                            continue;
                        }
                        visible.push(key.into());
                    }
                    let array = v8::Array::new_with_elements(scope, &visible);
                    retval.set(array);
                },
            )
            .flags(v8::PropertyHandlerFlags::ONLY_INTERCEPT_STRINGS)
            .data(external_data.into());
        template.instance_template(scope).set_named_property_handler(configuration);
        Ok(())
    }

    /// Installs an indexed read interceptor on the interface's instance template.
    /// Register before creating instances of this interface or any descendant; materialized
    /// V8 templates cannot be mutated. The guard also detects descendant instantiation.
    /// V8 supplies canonical array indices as `u32`; named properties such as `"01"`,
    /// negative numbers and `2**32 - 1` remain normal JS properties. The holder is the
    /// wrapped instance even when lookup starts on an object inheriting from it.
    /// Unlike accessors, V8 does not propagate this handler through FunctionTemplate::inherit;
    /// register it explicitly on each derived interface that needs indexed reads.
    /// Only reads are intercepted: this intentionally does not promise full WebIDL
    /// legacy-platform-object semantics or return object wrappers through `Value::Object`.
    pub fn define_indexed_property_getter(
        &mut self,
        interface: &Interface,
        getter: IndexedPropertyGetter,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("indexed getter must be defined before creating instances or descendants".to_string());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let external_data = v8::External::new(scope, getter as *mut std::ffi::c_void);
        let configuration = v8::IndexedPropertyHandlerConfiguration::new()
            .getter(
                |scope: &mut v8::PinScope,
                 index: u32,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Value>| {
                    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else {
                        return v8::Intercepted::kNo;
                    };
                    // SAFETY: this External contains exactly the function pointer supplied
                    // above, which remains valid for the lifetime of the template.
                    let getter: IndexedPropertyGetter =
                        unsafe { std::mem::transmute(external.value()) };
                    let holder = args.holder();
                    if holder.internal_field_count() < 1 {
                        return v8::Intercepted::kNo;
                    }
                    // SAFETY: only create_instance installs this field, using this tag and
                    // a Box<Box<dyn Any>>. The local holder keeps its finalizer from running.
                    let raw = unsafe {
                        holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                    } as *mut Box<dyn std::any::Any>;
                    if raw.is_null() {
                        return v8::Intercepted::kNo;
                    }
                    let boxed_any = unsafe { &*raw };
                    match getter(boxed_any.as_ref(), index) {
                        Some(value) => {
                            let value = v8_result(scope, &value);
                            retval.set(value);
                            v8::Intercepted::kYes
                        },
                        None => v8::Intercepted::kNo,
                    }
                },
            )
            .data(external_data.into());
        template
            .instance_template(scope)
            .set_indexed_property_handler(configuration);
        Ok(())
    }

    /// Like [`Runtime::define_indexed_property_getter`] with a WebIDL indexed setter
    /// (`HTMLOptionsCollection`, `HTMLSelectElement`): `[[Set]]` of an array index converts the
    /// value to `value_type` and calls `setter` (which may throw).
    pub fn define_indexed_properties(
        &mut self,
        interface: &Interface,
        getter: IndexedPropertyGetter,
        setter: IndexedPropertySetter,
        value_type: WebIdlType,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("indexed properties must be defined before creating instances or descendants".to_string());
        }
        let config = Box::new(IndexedConfig { getter, setter, value_type });
        let config_pointer = (&*config) as *const IndexedConfig;
        self.indexed_configs.push(config);
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let external_data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);
        let configuration = v8::IndexedPropertyHandlerConfiguration::new()
            .getter(
                |scope: &mut v8::PinScope, index: u32, args: v8::PropertyCallbackArguments, mut retval: v8::ReturnValue<v8::Value>| {
                    let Some((config, native)) = indexed_receiver(&args) else { return v8::Intercepted::kNo };
                    match (config.getter)(native, index) {
                        Some(value) => {
                            let value = v8_result(scope, &value);
                            retval.set(value);
                            v8::Intercepted::kYes
                        },
                        None => v8::Intercepted::kNo,
                    }
                },
            )
            .setter(
                |scope: &mut v8::PinScope,
                 index: u32,
                 value: v8::Local<v8::Value>,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Boolean>| {
                    let Some((config, native)) = indexed_receiver(&args) else { return v8::Intercepted::kNo };
                    let Some(converted) = convert_typed_value(scope, value, &config.value_type) else {
                        return v8::Intercepted::kYes;
                    };
                    if let Err(error) = (config.setter)(native, index, &converted) {
                        throw_webidl_error(scope, &error);
                    }
                    retval.set_bool(true);
                    v8::Intercepted::kYes
                },
            )
            .data(external_data.into());
        template.instance_template(scope).set_indexed_property_handler(configuration);
        Ok(())
    }

    /// Reads back the Rust value a live [`Handle`] from [`Runtime::create_wrapped`] points at,
    /// checking that it's actually a `T` (not just trusting the caller) via `Any::downcast_ref`
    /// on the same `Box<dyn Any>` `create_wrapped` stored — returns `None` on a type mismatch
    /// instead of the memory-unsafe behavior a naive raw-pointer cast would have. Also `None` if
    /// `handle` doesn't point at a `create_wrapped`-created object at all (e.g. a plain `Value`
    /// from [`Runtime::store`]).
    ///
    /// This is only the JS-to-Rust half of wrapper identity — nothing here yet ensures wrapping
    /// the *same* conceptual value twice reuses the first wrapper instead of creating a second,
    /// independent one; see `docs/V8_MIGRATION.md`'s Phase 3 status note.
    ///
    /// # Safety requirement this relies on
    ///
    /// The returned reference borrows from both the runtime and `handle` — passing a `handle`
    /// that isn't actually keeping the object alive (there is no such way to construct one
    /// outside this crate) would be unsound; a live `Handle` argument is what guarantees the
    /// finalizer in `create_wrapped` hasn't run yet. Borrowing the runtime also prevents JS
    /// setters from mutating the data, or isolate disposal from freeing it, while it is read.
    ///
    /// ```compile_fail
    /// use roves_v8::Runtime;
    /// let mut runtime = Runtime::new();
    /// let handle = runtime.create_wrapped(String::from("native"));
    /// let reference = runtime.get_wrapped::<String>(&handle).unwrap();
    /// runtime.eval("1").unwrap(); // Cannot run JS while native data is borrowed.
    /// println!("{reference}");
    /// ```
    ///
    /// ```compile_fail
    /// use roves_v8::Runtime;
    /// let mut runtime = Runtime::new();
    /// let handle = runtime.create_wrapped(String::from("native"));
    /// let reference = runtime.get_wrapped::<String>(&handle).unwrap();
    /// drop(runtime); // Cannot dispose the isolate while native data is borrowed.
    /// println!("{reference}");
    /// ```
    pub fn get_wrapped<'h, T: 'static>(&'h mut self, handle: &'h Handle) -> Option<&'h T> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let value = v8::Local::new(scope, &handle.0);
        let object = v8::Local::<v8::Object>::try_from(value).ok()?;
        if object.internal_field_count() < 1 {
            return None;
        }
        // SAFETY: this field is only ever set by `create_wrapped`, always via
        // `set_aligned_pointer_in_internal_field` with this exact `WRAPPED_POINTER_TAG`, so a
        // non-null result always points at a live `Box<Box<dyn Any>>` this same crate allocated.
        let raw = unsafe {
            object.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
        } as *mut Box<dyn std::any::Any>;
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is non-null and was produced by `create_wrapped` as described above;
        // `handle` being alive (a live `Global`, per this method's own doc comment) guarantees
        // the guaranteed finalizer that would free it hasn't run.
        let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };
        // The signature ties this read to both the live handle and an exclusive runtime
        // borrow. No JS evaluation/setter or isolate disposal can overlap the reference.
        boxed_any.downcast_ref::<T>()
    }

    /// Sets `property` on the JS object `on` points at to the JS value `other` points at.
    /// Exists to build real reference graphs between wrapped (or plain) JS values — in
    /// particular, this crate's own cycle stress test uses it to make two `create_wrapped`
    /// objects reference each other, the same shape a real DOM has everywhere (a parent
    /// referencing a child that references its parent back, an event listener closing over a
    /// node, ...). Deliberately a plain JS property, not an extra `v8::Global` cross-reference
    /// on the Rust side: V8's own tracing GC already collects cycles among ordinary JS object
    /// graphs correctly (that's the entire point of tracing over reference counting) — the only
    /// way this phase's ownership model could still leak a cycle is by adding a *Rust-side*
    /// strong reference on top, which this method doesn't.
    pub fn link(&mut self, on: &Handle, property: &str, other: &Handle) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let on_value = v8::Local::new(scope, &on.0);
        let Ok(object) = v8::Local::<v8::Object>::try_from(on_value) else {
            return Err("`on` does not point at a JS object".to_string());
        };
        let Some(key) = v8::String::new(scope, property) else {
            return Err(format!("{property:?} is not valid as a property name string"));
        };
        let other_value = v8::Local::new(scope, &other.0);
        if object.set(scope, key.into(), other_value) != Some(true) {
            return Err(format!("failed to set property {property:?}"));
        }
        Ok(())
    }

    /// Exposes `handle`'s value as a named property of this [`Runtime`]'s global object —
    /// e.g. `set_global_property("window", handle)` makes `handle`'s value reachable from
    /// script as `globalThis.window`/bare `window`. This is Phase 4's first validation
    /// milestone per `docs/V8_MIGRATION.md` ("global/window exposure"), prototyped here as a
    /// temporary experiment entirely within `roves-v8` — the plan document explicitly allows
    /// "manually implementing DOM APIs one by one... as temporary experiments used to validate
    /// the runtime" — rather than by touching `components/script_bindings/codegen.py` or any
    /// other production Servo file, which stays completely untouched by this and everything else
    /// in this crate so far.
    pub fn set_global_property(&mut self, name: &str, handle: &Handle) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let global = context.global(scope);
        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a global property name string"));
        };
        let value = v8::Local::new(scope, &handle.0);
        if global.set(scope, key.into(), value) != Some(true) {
            return Err(format!("failed to set global property {name:?}"));
        }
        Ok(())
    }

    /// Forces a full garbage collection cycle — test-only (see `ensure_platform_initialized`'s
    /// own comment on why `--expose-gc` isn't set outside `#[cfg(test)]`). Exists so this
    /// crate's own GC stress tests don't have to rely on GC happening to run on its own schedule
    /// within a test's short lifetime.
    #[cfg(test)]
    fn force_full_gc_for_testing(&mut self) {
        self.isolate
            .request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
        // The isolate-level request scans the native stack conservatively, so a stale pointer
        // left in a test's stack frame can keep a traced native alive. Also run a precise
        // unified-heap collection, which is what an idle production GC does.
        if let Some(heap) = self.isolate.get_cpp_heap() {
            // SAFETY: no traced-heap pointer is held only on the stack across this call: test
            // code keeps natives in roots/members, never in raw locals.
            unsafe {
                heap.collect_garbage_for_testing(v8::cppgc::EmbedderStackState::NoHeapPointers)
            };
        }
    }

    /// Stores `value` as a JS value in this isolate and returns a [`Handle`] that outlives this
    /// call — see [`Handle`]'s own doc comment.
    pub fn store(&mut self, value: &Value) -> Handle {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let local = v8_value(scope, value);
        Handle(v8::Global::new(scope, local))
    }

    /// Reads a [`Handle`] back into an engine-neutral [`Value`]. A `Handle` is backed by a
    /// `v8::Global`, which is isolate- not context-scoped, so this works even though it enters
    /// this [`Runtime`]'s one persistent context rather than whatever context was active when
    /// [`Runtime::store`] created it — see [`Handle`]'s own doc comment.
    pub fn load(&mut self, handle: &Handle) -> Value {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let local = v8::Local::new(scope, &handle.0);
        native_value(scope, local)
    }

    /// Registers `f` as a global function named `name`, callable from a later [`Runtime::eval`]/
    /// `eval_value` call. Demonstrates this phase's "callbacks" primitive: arguments and the
    /// return value both round-trip through [`Value`], not raw `v8::*` types. See
    /// [`NativeFunction`]'s own doc comment for why `f` must be a plain function pointer, not an
    /// arbitrary closure.
    pub fn define_native_function(&mut self, name: &str, f: NativeFunction) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        // `f` (a fn pointer) can't be captured by a `v8::Function::new` closure argument
        // directly and still satisfy `MapFnTo` (which requires the closure itself to carry no
        // captured state) unless it's threaded through as V8 "callback data" instead — a
        // `v8::External` wrapping the raw pointer, read back inside the (capture-free) closure.
        let external_data = v8::External::new(scope, f as *mut std::ffi::c_void);
        let Some(function) = v8::Function::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: `external`'s value is exactly the `NativeFunction` pointer this same
                // `define_native_function` call stored a few lines above, cast back to its
                // original type. Nothing else ever constructs this External.
                let f: NativeFunction =
                    unsafe { std::mem::transmute(external.value()) };

                let mut arguments = Vec::with_capacity(args.length() as usize);
                for i in 0..args.length() {
                    arguments.push(native_value(scope, args.get(i)));
                }
                let result = f(&arguments);
                retval.set(v8_value(scope, &result));
            },
        )
        .data(external_data.into())
        .build(scope) else {
            return Err(format!("failed to create native function {name:?}"));
        };

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a JS identifier string"));
        };
        let global = context.global(scope);
        if global.set(scope, key.into(), function.into()) != Some(true) {
            return Err(format!("failed to install {name:?} on the global object"));
        }
        Ok(())
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// Passed to `Module::instantiate_module` by [`Runtime::eval_module`], which only supports a
/// self-contained module with no imports (see that method's own doc comment on why) — so this
/// should never actually run. Written as an ordinary Rust fn with the "logical" signature the
/// `v8` crate's own doc comment on `ResolveModuleCallback` describes; despite that type's raw
/// form being an `unsafe extern "C" fn` with a platform-specific ABI (an extra leading out-param
/// on Windows), the crate's `MapFnTo`/`MapFnFrom` machinery generates that wrapper automatically
/// from a plain closure/fn like this one — writing the raw ABI by hand isn't necessary and (as
/// found while getting this to compile) isn't even accepted where a `MapFnTo` bound is expected.
fn unreachable_resolve_module_callback<'s>(
    _context: v8::Local<'s, v8::Context>,
    _specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    unreachable!(
        "roves-v8's eval_module only supports import-free modules in this phase; \
         see docs/V8_MIGRATION.md's Phase 2 status note"
    )
}

/// Converts a WebIDL operation's or constructor's JS arguments. WebIDL's ToNumber/ToString
/// conversions can run user code and throw: `None` means a conversion threw and that exact
/// exception is pending, so the caller must return without touching native state.
fn convert_webidl_arguments<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
    config: &WebIdlArguments,
) -> Option<Vec<Value>> {
    let argument_count = if config.conversions.is_empty() {
        args.length() as usize
    } else {
        config.conversions.len()
    };
    let mut arguments = Vec::with_capacity(argument_count);
    for i in 0..argument_count {
        if config.variadic_last && i + 1 == argument_count {
            let ty = config.types[i].as_ref().expect("variadic arguments are structured");
            let mut rest = Vec::new();
            for index in i..args.length().max(0) as usize {
                rest.push(convert_typed_value(scope, args.get(index as i32), ty)?);
            }
            arguments.push(Value::Sequence(rest));
            break;
        }
        let argument = args.get(i as i32);
        let default = config.defaults.get(i).and_then(Option::as_ref);
        let converted = if let (Some(default), true) = (default, argument.is_undefined()) {
            let default = v8_value(scope, default);
            convert_typed_value(scope, default, config.types[i].as_ref().expect("defaults are structured"))?
        } else if config.optional_arguments.get(i).copied().unwrap_or(false)
            && argument.is_undefined()
        {
            Value::Missing
        } else if config.nullable_arguments.get(i).copied().unwrap_or(false)
            && (argument.is_null() || argument.is_undefined())
        {
            Value::Null
        } else {
            match config.types.get(i).and_then(Option::as_ref) {
                Some(ty) => convert_typed_value(scope, argument, ty)?,
                None => convert_webidl_value(
                    scope,
                    argument,
                    config.conversions.get(i),
                    config.enumeration_values.get(i).and_then(Option::as_ref),
                )?,
            }
        };
        arguments.push(converted);
    }
    Some(arguments)
}

/// Converts one JS value by a flat WebIDL conversion (`None`: no conversion). `None` means a
/// conversion threw and the exception is pending. `enumeration_values` lists an enumeration's
/// values, or names the interface of an `Interface` conversion.
fn convert_webidl_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    argument: v8::Local<'_, v8::Value>,
    conversion: Option<&WebIdlArgumentConversion>,
    enumeration_values: Option<&Vec<Vec<u16>>>,
) -> Option<Value> {
    Some(match conversion {
        None => native_value(scope, argument),
        Some(WebIdlArgumentConversion::Boolean) => Value::Bool(argument.boolean_value(scope)),
        Some(WebIdlArgumentConversion::Byte) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 8, true))
        }
        Some(WebIdlArgumentConversion::Octet) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 8, false))
        }
        Some(WebIdlArgumentConversion::Short) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 16, true))
        }
        Some(WebIdlArgumentConversion::UnsignedShort) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 16, false))
        }
        Some(WebIdlArgumentConversion::Long) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 32, true))
        }
        Some(WebIdlArgumentConversion::LongLong) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 64, true))
        }
        Some(WebIdlArgumentConversion::UnsignedLongLong) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 64, false))
        }
        Some(WebIdlArgumentConversion::Float) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            let value = number as f32;
            if !value.is_finite() {
                throw_type_error(scope, "float argument must be finite");
                return None;
            }
            Value::Number(value as f64)
        }
        Some(WebIdlArgumentConversion::UnrestrictedFloat) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(number as f32 as f64)
        }
        Some(WebIdlArgumentConversion::Double) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            if !number.is_finite() {
                throw_type_error(scope, "double argument must be finite");
                return None;
            }
            Value::Number(number)
        }
        Some(WebIdlArgumentConversion::UnrestrictedDouble) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(number)
        }
        Some(WebIdlArgumentConversion::UnsignedLong) => {
            let Some(number) = argument.number_value(scope) else { return None; };
            Value::Number(convert_webidl_integer(number, 32, false))
        }
        Some(WebIdlArgumentConversion::DomString) => {
            if argument.is_symbol() {
                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                return None;
            }
            let result = {
                v8::tc_scope!(let tc_scope, scope);
                let scope = tc_scope;
                match argument.to_string(scope) {
                    Some(string) => {
                        let mut utf16 = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                        Ok(Value::Utf16String(utf16))
                    }
                    None => Err(scope.exception()),
                }
            };
            match result {
                Ok(value) => value,
                Err(Some(exception)) => { scope.throw_exception(exception); return None; }
                Err(None) => return None,
            }
        }
        Some(WebIdlArgumentConversion::UsvString) => {
            if argument.is_symbol() {
                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                return None;
            }
            let result = {
                v8::tc_scope!(let tc_scope, scope);
                let scope = tc_scope;
                match argument.to_string(scope) {
                    Some(string) => {
                        let mut utf16 = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                        Ok(Value::String(String::from_utf16_lossy(&utf16)))
                    }
                    None => Err(scope.exception()),
                }
            };
            match result {
                Ok(value) => value,
                Err(Some(exception)) => { scope.throw_exception(exception); return None; }
                Err(None) => return None,
            }
        }
        Some(WebIdlArgumentConversion::ByteString) => {
            if argument.is_symbol() {
                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                return None;
            }
            let result = {
                v8::tc_scope!(let tc_scope, scope);
                let scope = tc_scope;
                match argument.to_string(scope) {
                    Some(string) => {
                        let mut utf16 = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                        if utf16.iter().any(|unit| *unit > 0xFF) {
                            let message = v8::String::new(scope, "ByteString contains a code unit greater than 255").unwrap();
                            Err(Some(v8::Exception::type_error(scope, message).into()))
                        } else {
                            Ok(Value::ByteString(utf16.into_iter().map(|unit| unit as u8).collect()))
                        }
                    }
                    None => Err(scope.exception()),
                }
            };
            match result {
                Ok(value) => value,
                Err(Some(exception)) => { scope.throw_exception(exception); return None; }
                Err(None) => return None,
            }
        }
        Some(WebIdlArgumentConversion::Any) => {
            Value::Js(Handle(v8::Global::new(scope, argument)))
        }
        Some(WebIdlArgumentConversion::Object) => {
            if !argument.is_object() {
                throw_type_error(scope, "argument is not an object");
                return None;
            }
            Value::Js(Handle(v8::Global::new(scope, argument)))
        }
        Some(WebIdlArgumentConversion::LegacyCallback) => {
            if argument.is_object() {
                Value::Js(Handle(v8::Global::new(scope, argument)))
            } else {
                Value::Null
            }
        }
        Some(WebIdlArgumentConversion::Callback) => {
            if !argument.is_function() {
                throw_type_error(scope, "argument is not callable");
                return None;
            }
            Value::Js(Handle(v8::Global::new(scope, argument)))
        }
        Some(WebIdlArgumentConversion::Interface) => {
            let expected = enumeration_values
                .and_then(|names| names.first())
                .map(|name| String::from_utf16_lossy(name))
                .expect("interface-typed arguments name their interface");
            native_argument(scope, argument, &expected)?
        }
        Some(WebIdlArgumentConversion::Enumeration) => {
            if argument.is_symbol() {
                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                return None;
            }
            let result = {
                v8::tc_scope!(let tc_scope, scope);
                let scope = tc_scope;
                match argument.to_string(scope) {
                    Some(string) => {
                        let mut utf16 = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                        let valid = enumeration_values
                            .is_some_and(|values| values.iter().any(|value| value == &utf16));
                        if valid {
                            Ok(Value::String(String::from_utf16_lossy(&utf16)))
                        } else {
                            let message = v8::String::new(scope, "Value is not a valid WebIDL enum value").unwrap();
                            Err(Some(v8::Exception::type_error(scope, message).into()))
                        }
                    }
                    None => Err(scope.exception()),
                }
            };
            match result {
                Ok(value) => value,
                Err(Some(exception)) => { scope.throw_exception(exception); return None; }
                Err(None) => return None,
            }
        }
    })
}

/// Converts one JS value by a structured WebIDL type (see [`WebIdlType`]).
fn convert_typed_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'_, v8::Value>,
    ty: &WebIdlType,
) -> Option<Value> {
    match ty {
        WebIdlType::Primitive(conversion) => convert_webidl_value(scope, value, Some(conversion), None),
        WebIdlType::Enumeration(values) => {
            let values: Vec<Vec<u16>> = values.iter().map(|value| value.encode_utf16().collect()).collect();
            convert_webidl_value(scope, value, Some(&WebIdlArgumentConversion::Enumeration), Some(&values))
        },
        WebIdlType::Interface(name) => native_argument(scope, value, name),
        WebIdlType::Nullable(inner) => {
            if value.is_null() || value.is_undefined() {
                Some(Value::Null)
            } else {
                convert_typed_value(scope, value, inner)
            }
        },
        WebIdlType::CallbackInterface => {
            if !value.is_object() {
                throw_type_error(scope, "value is not a callback interface object");
                return None;
            }
            Some(Value::Js(Handle(v8::Global::new(scope, value))))
        },
        WebIdlType::Union(members) => convert_union(scope, value, members),
        WebIdlType::Integer(conversion, mode) => {
            let number = value.number_value(scope)?;
            match convert_annotated_integer(number, *conversion, *mode) {
                Ok(number) => Some(Value::Number(number)),
                Err(message) => {
                    throw_type_error(scope, &message);
                    None
                },
            }
        },
        WebIdlType::Buffer(kind) => {
            if !kind.matches(value) {
                throw_type_error(scope, &format!("value is not {kind:?}"));
                return None;
            }
            Some(Value::Js(Handle(v8::Global::new(scope, value))))
        },
        WebIdlType::Promise => {
            let resolver = v8::PromiseResolver::new(scope)?;
            resolver.resolve(scope, value)?;
            let promise: v8::Local<v8::Value> = resolver.get_promise(scope).into();
            Some(Value::Js(Handle(v8::Global::new(scope, promise))))
        },
        WebIdlType::Dictionary(members) => {
            // WebIDL dictionary conversion: undefined and null are an empty dictionary.
            let object = if value.is_null_or_undefined() {
                None
            } else if let Ok(object) = v8::Local::<v8::Object>::try_from(value) {
                Some(object)
            } else {
                throw_type_error(scope, "value is not a dictionary object");
                return None;
            };
            let mut entries = Vec::with_capacity(members.len());
            for member in members {
                let raw: v8::Local<v8::Value> = match object {
                    Some(object) => {
                        let key = v8::String::new(scope, &member.name).unwrap();
                        object.get(scope, key.into())?
                    },
                    None => v8::undefined(scope).into(),
                };
                let converted = if raw.is_undefined() {
                    match &member.default {
                        // Defaults convert through the member type, so union, enumeration and
                        // `any` defaults take the same shape as a passed value.
                        Some(default) => {
                            let default = v8_value(scope, default);
                            convert_typed_value(scope, default, &member.ty)?
                        },
                        None if member.required => {
                            throw_type_error(scope, &format!("required dictionary member '{}' is missing", member.name));
                            return None;
                        },
                        None => Value::Missing,
                    }
                } else {
                    convert_typed_value(scope, raw, &member.ty)?
                };
                entries.push((member.name.clone(), converted));
            }
            Some(Value::Dictionary(entries))
        },
        WebIdlType::Undefined => {
            if !value.is_undefined() {
                throw_type_error(scope, "value is not undefined");
                return None;
            }
            Some(Value::Undefined)
        },
        WebIdlType::Record(key_type, value_type) => {
            // WebIDL ES-to-record: the own enumerable properties, in [[OwnPropertyKeys]] order.
            let Ok(object) = v8::Local::<v8::Object>::try_from(value) else {
                throw_type_error(scope, "value is not a record object");
                return None;
            };
            let keys = object.get_own_property_names(
                scope,
                v8::GetPropertyNamesArgs {
                    mode: v8::KeyCollectionMode::OwnOnly,
                    property_filter: v8::PropertyFilter::ALL_PROPERTIES,
                    index_filter: v8::IndexFilter::IncludeIndices,
                    key_conversion: v8::KeyConversionMode::ConvertToString,
                },
            )?;
            let mut entries: Vec<(Value, Value)> = Vec::new();
            for index in 0..keys.length() {
                let key = keys.get_index(scope, index)?;
                let Ok(name) = v8::Local::<v8::Name>::try_from(key) else { continue };
                // [[GetOwnProperty]] may run proxy traps; only enumerable properties count.
                let Some(descriptor) = object.get_own_property_descriptor(scope, name) else { return None };
                if descriptor.is_undefined() {
                    continue;
                }
                let Ok(descriptor) = v8::Local::<v8::Object>::try_from(descriptor) else { continue };
                let enumerable_key = v8::String::new(scope, "enumerable").unwrap();
                if !descriptor.get(scope, enumerable_key.into())?.boolean_value(scope) {
                    continue;
                }
                let typed_key = convert_typed_value(scope, key, key_type)?;
                let item = object.get(scope, key)?;
                let typed_value = convert_typed_value(scope, item, value_type)?;
                // Keys that convert to the same IDL value (e.g. USVString replacement) keep the
                // first position and the last value.
                match entries.iter_mut().find(|(existing, _)| *existing == typed_key) {
                    Some(entry) => entry.1 = typed_value,
                    None => entries.push((typed_key, typed_value)),
                }
            }
            Some(Value::Record(entries))
        },
        WebIdlType::Sequence(element_type) => {
            // WebIDL "create a sequence from an iterable": the @@iterator protocol, so any
            // iterable (not only arrays) converts, and user iterators run.
            let Ok(object) = v8::Local::<v8::Object>::try_from(value) else {
                throw_type_error(scope, "value is not an iterable object");
                return None;
            };
            let iterator_symbol = v8::Symbol::get_iterator(scope);
            let method = object.get(scope, iterator_symbol.into())?;
            let Ok(method) = v8::Local::<v8::Function>::try_from(method) else {
                throw_type_error(scope, "value is not iterable");
                return None;
            };
            let iterator = method.call(scope, object.into(), &[])?;
            let Ok(iterator) = v8::Local::<v8::Object>::try_from(iterator) else {
                throw_type_error(scope, "iterator is not an object");
                return None;
            };
            let next_key = v8::String::new(scope, "next").unwrap();
            let next = iterator.get(scope, next_key.into())?;
            let Ok(next) = v8::Local::<v8::Function>::try_from(next) else {
                throw_type_error(scope, "iterator has no next method");
                return None;
            };
            let done_key = v8::String::new(scope, "done").unwrap();
            let value_key = v8::String::new(scope, "value").unwrap();
            let mut elements = Vec::new();
            loop {
                let result = next.call(scope, iterator.into(), &[])?;
                let Ok(result) = v8::Local::<v8::Object>::try_from(result) else {
                    throw_type_error(scope, "iterator result is not an object");
                    return None;
                };
                let done = result.get(scope, done_key.into())?;
                if done.boolean_value(scope) {
                    break;
                }
                let element = result.get(scope, value_key.into())?;
                elements.push(convert_typed_value(scope, element, element_type)?);
            }
            Some(Value::Sequence(elements))
        },
    }
}

/// The WebIDL ES-to-union conversion over the member types this runtime supports: selects the
/// member with [`select_union_member`], then converts the value by it.
fn convert_union<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'_, v8::Value>,
    members: &[WebIdlType],
) -> Option<Value> {
    let index = select_union_member(scope, value, members).ok()?;
    let converted = convert_typed_value(scope, value, &members[index])?;
    Some(Value::Union(index, Box::new(converted)))
}

/// The member type a value selects by the WebIDL union conversion algorithm (also the type
/// step of overload resolution). Steps follow the specification's order: nullish values to a
/// dictionary, platform objects to an interface, buffers, callables to a callback, objects to
/// a sequence/dictionary/callback interface/object, then boolean and number values, then the
/// string, numeric and boolean fallbacks. `Err` means a TypeError (or a getter's exception) is
/// pending.
fn select_union_member<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'_, v8::Value>,
    members: &[WebIdlType],
) -> Result<usize, ()> {
    let find = |predicate: &dyn Fn(&WebIdlType) -> bool| members.iter().position(|member| predicate(member));
    let is_numeric = |member: &WebIdlType| {
        matches!(member, WebIdlType::Primitive(conversion) if !matches!(conversion,
            WebIdlArgumentConversion::Boolean | WebIdlArgumentConversion::DomString |
            WebIdlArgumentConversion::UsvString | WebIdlArgumentConversion::ByteString |
            WebIdlArgumentConversion::Any | WebIdlArgumentConversion::Object |
            WebIdlArgumentConversion::Callback | WebIdlArgumentConversion::LegacyCallback |
            WebIdlArgumentConversion::Interface | WebIdlArgumentConversion::Enumeration))
    };
    let is_string = |member: &WebIdlType| {
        matches!(member, WebIdlType::Enumeration(_) | WebIdlType::Primitive(
            WebIdlArgumentConversion::DomString | WebIdlArgumentConversion::UsvString |
            WebIdlArgumentConversion::ByteString))
    };
    let is_boolean = |member: &WebIdlType| matches!(member, WebIdlType::Primitive(WebIdlArgumentConversion::Boolean));
    let is_object = |member: &WebIdlType| matches!(member, WebIdlType::Primitive(WebIdlArgumentConversion::Object));

    if value.is_undefined() {
        if let Some(index) = find(&|member| matches!(member, WebIdlType::Undefined)) {
            return Ok(index);
        }
    }
    if value.is_null_or_undefined() {
        if let Some(index) = find(&|member| matches!(member, WebIdlType::Dictionary(_))) {
            return Ok(index);
        }
    }
    if value.is_object() {
        for (index, member) in members.iter().enumerate() {
            if let WebIdlType::Interface(name) = member {
                if implementing_native(scope, value, name).is_some() {
                    return Ok(index);
                }
            }
        }
        for (index, member) in members.iter().enumerate() {
            if let WebIdlType::Buffer(kind) = member {
                if kind.matches(value) {
                    return Ok(index);
                }
            }
        }
        if value.is_function() {
            if let Some(index) = find(&|member| matches!(member, WebIdlType::Primitive(WebIdlArgumentConversion::Callback))) {
                return Ok(index);
            }
        }
        if let Some(index) = find(&|member| matches!(member, WebIdlType::Sequence(_))) {
            let object = v8::Local::<v8::Object>::try_from(value).unwrap();
            let iterator_symbol = v8::Symbol::get_iterator(scope);
            let method = object.get(scope, iterator_symbol.into()).ok_or(())?;
            if !method.is_undefined() {
                return Ok(index);
            }
        }
        if let Some(index) = find(&|member| matches!(member, WebIdlType::Dictionary(_) | WebIdlType::Record(..))) {
            return Ok(index);
        }
        if let Some(index) = find(&|member| matches!(member, WebIdlType::CallbackInterface)) {
            return Ok(index);
        }
        if let Some(index) = find(&is_object) {
            return Ok(index);
        }
    }
    if value.is_boolean() {
        if let Some(index) = find(&is_boolean) {
            return Ok(index);
        }
    }
    if value.is_number() {
        if let Some(index) = find(&is_numeric) {
            return Ok(index);
        }
    }
    for predicate in [&is_string as &dyn Fn(&WebIdlType) -> bool, &is_numeric, &is_boolean] {
        if let Some(index) = find(predicate) {
            return Ok(index);
        }
    }
    throw_type_error(scope, "value does not match any member of the union");
    Err(())
}

fn settle_promise(scope: &mut v8::PinScope, resolver: &PromiseResolver, outcome: Result<&Value, &WebIdlError>) {
    let resolver = v8::Local::new(scope, &resolver.0);
    match outcome {
        Ok(value) => {
            let value = v8_result(scope, value);
            resolver.resolve(scope, value);
        },
        Err(error) => {
            let reason = webidl_error_value(scope, error);
            resolver.reject(scope, reason);
        },
    }
}

/// Whether a real (non-interceptor) property `key` exists on `object` or its prototype chain,
/// which hides a named property of the same name.
fn named_property_shadowed(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, key: v8::Local<v8::Name>) -> bool {
    let mut current = Some(object);
    while let Some(object) = current {
        if object.has_real_named_property(scope, key).unwrap_or(false) {
            return true;
        }
        current = object
            .get_prototype(scope)
            .and_then(|prototype| v8::Local::<v8::Object>::try_from(prototype).ok());
    }
    false
}

/// Arms the guaranteed finalizer that drops `raw` (a `Box<Box<dyn Any>>` attached to `wrapper`'s
/// internal field) exactly once, when V8 collects `wrapper`, then runs `after_drop`. The returned
/// weak handle observes the wrapper; `finalizers` keeps the callback armed until it completes.
fn arm_native_finalizer(
    isolate: &mut v8::Isolate,
    finalizers: &std::cell::RefCell<Vec<WrappedFinalizer>>,
    wrapper: &v8::Global<v8::Value>,
    raw: *mut Box<dyn std::any::Any>,
    after_drop: Box<dyn FnOnce()>,
) -> v8::Weak<v8::Value> {
    // Reclaim only completed finalizers, never callbacks waiting for GC's second pass.
    finalizers.borrow_mut().retain(|entry| !entry.completed.get());
    let completed = std::rc::Rc::new(std::cell::Cell::new(false));
    let completion = completed.clone();
    let weak = v8::Weak::with_guaranteed_finalizer(
        isolate,
        wrapper,
        Box::new(move || {
            // SAFETY: `raw` came from `Box::into_raw` in the caller and is reachable from exactly
            // one place afterward (this closure) -- V8 guarantees this finalizer runs at most
            // once, and only after nothing JS-reachable points at the wrapper anymore, so
            // nothing else can read `raw` concurrently or afterward.
            drop(unsafe { Box::from_raw(raw) });
            after_drop();
            completion.set(true);
        }),
    );
    // Only the original `Weak` owns the finalizer (a clone is a plain observer), so the
    // original must stay in `finalizers` and the caller gets the clone.
    let observer = weak.clone();
    finalizers
        .borrow_mut()
        .push(WrappedFinalizer { _weak: weak, completed });
    observer
}

/// Throws the JS exception matching a native [`WebIdlError`].
fn throw_webidl_error(scope: &mut v8::PinScope, error: &WebIdlError) {
    let exception = webidl_error_value(scope, error);
    scope.throw_exception(exception);
}

/// The JS exception value of a native [`WebIdlError`] (thrown, or used to reject a promise).
fn webidl_error_value<'s>(scope: &mut v8::PinScope<'s, '_>, error: &WebIdlError) -> v8::Local<'s, v8::Value> {
    match error {
        WebIdlError::Js(value) => v8::Local::new(scope, &value.0),
        WebIdlError::TypeError(message) => {
            let message = v8::String::new(scope, message).unwrap();
            v8::Exception::type_error(scope, message)
        },
        WebIdlError::RangeError(message) => {
            let message = v8::String::new(scope, message).unwrap();
            v8::Exception::range_error(scope, message)
        },
        WebIdlError::DomException { name, message } => {
            let message = v8::String::new(scope, message).unwrap();
            let name = v8::String::new(scope, name).unwrap();
            let context = scope.get_current_context();
            let global = context.global(scope);
            let key = v8::String::new(scope, "DOMException").unwrap();
            let constructor = global
                .get(scope, key.into())
                .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok());
            match constructor
                .and_then(|constructor| constructor.new_instance(scope, &[message.into(), name.into()]))
            {
                Some(exception) => exception.into(),
                None => {
                    let exception = v8::Exception::error(scope, message);
                    if let Ok(object) = v8::Local::<v8::Object>::try_from(exception) {
                        let name_key = v8::String::new(scope, "name").unwrap();
                        object.set(scope, name_key.into(), name.into());
                    }
                    exception
                },
            }
        },
    }
}

fn throw_type_error(scope: &mut v8::PinScope, message: &str) {
    let message = v8::String::new(scope, message).unwrap();
    let exception = v8::Exception::type_error(scope, message);
    scope.throw_exception(exception);
}

/// Converts a `v8::Local<Value>` into this crate's engine-neutral [`Value`] — see [`Value`]'s
/// own doc comment on the `Object`/`Bytes` variants' scope.
fn native_value<'s>(scope: &v8::PinScope<'s, '_>, value: v8::Local<'s, v8::Value>) -> Value {
    if value.is_undefined() {
        Value::Undefined
    } else if value.is_null() {
        Value::Null
    } else if value.is_boolean() {
        Value::Bool(value.boolean_value(scope))
    } else if value.is_number() {
        Value::Number(value.number_value(scope).unwrap_or(f64::NAN))
    } else if value.is_string() {
        let string = v8::Local::<v8::String>::try_from(value).unwrap();
        let mut utf16 = vec![0; string.length()];
        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
        match std::string::String::from_utf16(&utf16) {
            Ok(string) => Value::String(string),
            Err(_) => Value::Utf16String(utf16),
        }
    } else if value.is_uint8_array() {
        let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) else {
            return Value::Object;
        };
        let len = view.byte_length();
        let mut bytes = vec![0u8; len];
        let copied = view.copy_contents(&mut bytes);
        bytes.truncate(copied);
        Value::Bytes(bytes)
    } else {
        Value::Object
    }
}

/// Converts this crate's engine-neutral [`Value`] into a `v8::Local<Value>` — the inverse of
/// [`native_value`]. `Value::Object` has no data to reconstruct an object from and becomes
/// `undefined`; see [`Value`]'s own doc comment.
fn v8_value<'s>(scope: &v8::PinScope<'s, '_>, value: &Value) -> v8::Local<'s, v8::Value> {
    match value {
        Value::Missing | Value::Undefined | Value::Object => v8::undefined(scope).into(),
        // Wrapping a native needs mutable isolate access; callback results go through
        // `v8_result`, which does that. Other conversions have no interface context.
        Value::Native(_) => v8::undefined(scope).into(),
        Value::Js(handle) => v8::Local::new(scope, &handle.0),
        Value::Sequence(elements) => {
            let elements: Vec<_> = elements.iter().map(|element| v8_value(scope, element)).collect();
            v8::Array::new_with_elements(scope, &elements).into()
        },
        Value::Union(_, value) => v8_value(scope, value),
        Value::Record(entries) => {
            let object = v8::Object::new(scope);
            for (key, item) in entries {
                let key = v8_value(scope, key);
                let item = v8_value(scope, item);
                if let Ok(key) = v8::Local::<v8::Name>::try_from(key) {
                    object.create_data_property(scope, key, item);
                }
            }
            object.into()
        },
        Value::Dictionary(entries) => {
            let object = v8::Object::new(scope);
            for (name, value) in entries {
                if *value == Value::Missing {
                    continue;
                }
                let key = v8::String::new(scope, name).unwrap();
                let value = v8_value(scope, value);
                object.create_data_property(scope, key.into(), value);
            }
            object.into()
        },
        Value::Null => v8::null(scope).into(),
        Value::Bool(b) => v8::Boolean::new(scope, *b).into(),
        Value::Number(n) => v8::Number::new(scope, *n).into(),
        Value::String(s) => v8::String::new(scope, s)
            .map(Into::into)
            .unwrap_or_else(|| v8::undefined(scope).into()),
        Value::ByteString(bytes) => {
            let units = bytes.iter().map(|byte| u16::from(*byte)).collect::<Vec<_>>();
            v8::String::new_from_two_byte(scope, &units, v8::NewStringType::Normal)
                .map(Into::into)
                .unwrap_or_else(|| v8::undefined(scope).into())
        }
        Value::Utf16String(units) => v8::String::new_from_two_byte(
            scope,
            units,
            v8::NewStringType::Normal,
        )
        .map(Into::into)
        .unwrap_or_else(|| v8::undefined(scope).into()),
        Value::Bytes(bytes) => {
            let buffer = v8::ArrayBuffer::new(scope, bytes.len());
            // SAFETY: `buffer` was just created above with exactly `bytes.len()` bytes backing
            // it, and nothing else holds a reference to it yet.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    buffer.data().unwrap().as_ptr() as *mut u8,
                    bytes.len(),
                );
            }
            match v8::Uint8Array::new(scope, buffer, 0, bytes.len()) {
                Some(array) => array.into(),
                None => v8::undefined(scope).into(),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Handle, NativeFunction, Runtime, Value};

    /// Increments a shared counter when dropped — used by the `create_wrapped` GC stress tests
    /// below to observe exactly how many wrapped values actually got dropped, and how many
    /// times each (a double-drop would show up as a count too high, not just "ran").
    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn evaluates_a_basic_expression() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.eval("1 + 2").unwrap(), "3");
    }

    #[test]
    fn evaluates_string_concatenation() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.eval("'hello' + ' ' + 'world'").unwrap(),
            "hello world"
        );
    }

    #[test]
    fn reports_a_thrown_exception() {
        let mut runtime = Runtime::new();
        let err = runtime
            .eval("throw new Error('roves-v8 smoke test')")
            .unwrap_err();
        assert!(
            err.contains("roves-v8 smoke test"),
            "expected the thrown message in the error, got: {err}"
        );
    }

    #[test]
    fn reports_a_syntax_error() {
        let mut runtime = Runtime::new();
        assert!(runtime.eval("this is not valid javascript (((").is_err());
    }

    #[test]
    fn a_second_runtime_after_the_first_is_dropped_still_works() {
        // Exercises `ensure_platform_initialized`'s Once guard: a second Runtime later in the
        // same process must not re-initialize the platform, and must still work correctly.
        //
        // Deliberately sequential, not concurrent: V8 only allows one isolate "entered" on a
        // given thread at a time (see this crate's own `isolate.rs` doc comment on
        // `OwnedIsolate` — the Locker/Unlocker API is required to hold more than one alive
        // across threads, which this crate doesn't attempt in Phase 1). Two `Runtime`s alive at
        // once on one thread is not a real Roves usage pattern anyway: production Roves runs
        // exactly one game/isolate per process.
        let mut a = Runtime::new();
        assert_eq!(a.eval("21 * 2").unwrap(), "42");
        drop(a);

        let mut b = Runtime::new();
        assert_eq!(b.eval("'still' + 'works'").unwrap(), "stillworks");
    }

    #[test]
    fn eval_value_converts_primitives() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.eval_value("undefined").unwrap(), Value::Undefined);
        assert_eq!(runtime.eval_value("null").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("true").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("1 + 2").unwrap(), Value::Number(3.0));
        assert_eq!(
            runtime.eval_value("'hi'").unwrap(),
            Value::String("hi".to_string())
        );
        assert_eq!(runtime.eval_value("({})").unwrap(), Value::Object);
    }

    #[test]
    fn unpaired_surrogates_round_trip_without_loss() {
        let mut runtime = Runtime::new();
        let units = vec![0xD800, b'A' as u16, 0xDC00];
        let handle = runtime.store(&Value::Utf16String(units.clone()));
        runtime.set_global_property("wide", &handle).unwrap();
        assert_eq!(runtime.eval_value("wide.length").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_value("wide.charCodeAt(0)").unwrap(), Value::Number(0xD800 as f64));
        assert_eq!(runtime.eval_value("wide.charCodeAt(1)").unwrap(), Value::Number(65.0));
        assert_eq!(runtime.eval_value("wide.charCodeAt(2)").unwrap(), Value::Number(0xDC00 as f64));
        assert_eq!(runtime.eval_value("wide").unwrap(), Value::Utf16String(units.clone()));
        assert_eq!(runtime.load(&handle), Value::Utf16String(units));
    }

    #[test]
    #[cfg(feature = "webidl-pilot")]
    fn generated_domstring_binding_preserves_unpaired_surrogates() {
        use crate::webidl::utf16_string_state::{Utf16StringStateBinding, Utf16StringStateNative};

        struct NativeString(std::cell::RefCell<Vec<u16>>);
        impl Utf16StringStateNative for NativeString {
            fn Value(&self) -> Vec<u16> {
                self.0.borrow().clone()
            }
            fn set_Value(&self, value: Vec<u16>) {
                *self.0.borrow_mut() = value;
            }
        }

        let mut runtime = Runtime::new();
        let binding = Utf16StringStateBinding::<NativeString>::install(&mut runtime).unwrap();
        let units = vec![0xD800, b'A' as u16, 0xDC00];
        let state = binding.create(&mut runtime, NativeString(std::cell::RefCell::new(units.clone())));
        runtime.set_global_property("state", &state).unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::Utf16String(units));
        assert_eq!(
            runtime.eval_value("state.value.charCodeAt(0)").unwrap(),
            Value::Number(0xD800 as f64)
        );
        assert_eq!(
            runtime.eval_value("state.value.charCodeAt(1)").unwrap(),
            Value::Number(65.0)
        );
        assert_eq!(
            runtime.eval_value("state.value.charCodeAt(2)").unwrap(),
            Value::Number(0xDC00 as f64)
        );
        runtime.eval("state.value = 42").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("42".into()));
        runtime.eval("state.value = '\\ud800x\\udc00'").unwrap();
        assert_eq!(
            runtime.eval_value("state.value").unwrap(),
            Value::Utf16String(vec![0xD800, b'x' as u16, 0xDC00])
        );
        assert!(runtime
            .eval("state.value = Symbol('no string conversion')")
            .unwrap_err()
            .contains("TypeError"));
    }

    #[test]
    fn eval_value_converts_uint8_array_to_bytes() {
        let mut runtime = Runtime::new();
        let value = runtime
            .eval_value("new Uint8Array([1, 2, 3, 255])")
            .unwrap();
        assert_eq!(value, Value::Bytes(vec![1, 2, 3, 255]));
    }

    #[test]
    fn handle_round_trips_a_value_across_calls() {
        let mut runtime = Runtime::new();
        let handle = runtime.store(&Value::Number(42.0));
        // A fresh, unrelated eval call happens between store and load, proving the handle
        // doesn't depend on the context it was created in.
        assert_eq!(runtime.eval("1 + 1").unwrap(), "2");
        assert_eq!(runtime.load(&handle), Value::Number(42.0));
    }

    #[test]
    fn handle_round_trips_bytes() {
        let mut runtime = Runtime::new();
        let handle = runtime.store(&Value::Bytes(vec![9, 8, 7]));
        assert_eq!(runtime.load(&handle), Value::Bytes(vec![9, 8, 7]));
    }

    #[test]
    fn native_function_is_callable_from_js() {
        let double: NativeFunction = |args| match args.first() {
            Some(Value::Number(n)) => Value::Number(n * 2.0),
            _ => Value::Undefined,
        };

        let mut runtime = Runtime::new();
        runtime.define_native_function("double", double).unwrap();
        assert_eq!(runtime.eval("double(21)").unwrap(), "42");
    }

    #[test]
    fn eval_resolved_passes_through_a_non_promise_value() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.eval_resolved("1 + 2").unwrap(),
            Value::Number(3.0)
        );
    }

    #[test]
    fn eval_resolved_returns_an_already_resolved_promise() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.eval_resolved("Promise.resolve(42)").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn eval_resolved_pumps_microtasks_for_a_chained_then() {
        // Unlike Promise.resolve(42) (already fulfilled the instant it's created), this
        // promise only settles once its .then callback actually runs as a microtask -- this
        // is the case that needs the perform_microtask_checkpoint loop, not just a state read.
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime
                .eval_resolved("Promise.resolve(1).then(v => v + 1)")
                .unwrap(),
            Value::Number(2.0)
        );
    }

    #[test]
    fn eval_resolved_reports_a_rejected_promise() {
        let mut runtime = Runtime::new();
        let err = runtime
            .eval_resolved("Promise.reject(new Error('roves-v8 promise rejection'))")
            .unwrap_err();
        assert!(
            err.contains("roves-v8 promise rejection"),
            "expected the rejection message in the error, got: {err}"
        );
    }

    #[test]
    fn eval_module_runs_a_self_contained_module() {
        let mut runtime = Runtime::new();
        runtime
            .eval_module("globalThis.moduleRan = 42;")
            .unwrap();
        assert_eq!(
            runtime.eval_value("globalThis.moduleRan").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn eval_module_reports_a_thrown_exception() {
        let mut runtime = Runtime::new();
        let err = runtime
            .eval_module("throw new Error('roves-v8 module error')")
            .unwrap_err();
        assert!(
            err.contains("roves-v8 module error"),
            "expected the thrown message in the error, got: {err}"
        );
    }

    #[test]
    fn eval_module_reports_a_syntax_error() {
        let mut runtime = Runtime::new();
        assert!(
            runtime
                .eval_module("this is not valid javascript (((")
                .is_err()
        );
    }

    #[test]
    fn create_wrapped_keeps_the_value_alive_while_the_handle_lives() {
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handle = runtime.create_wrapped(DropCounter(counter.clone()));
        // A GC right now must not collect it -- the returned Handle is still a live, strong
        // reference.
        runtime.force_full_gc_for_testing();
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        drop(handle);
    }

    #[test]
    fn create_wrapped_drops_the_value_exactly_once_when_collected() {
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handle = runtime.create_wrapped(DropCounter(counter.clone()));
        drop(handle); // the only strong reference -- now eligible for collection

        let mut collected = false;
        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) > 0 {
                collected = true;
                break;
            }
        }
        assert!(collected, "value was never collected within 20 full GC cycles");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "value must be dropped exactly once, not zero or more than once"
        );
    }

    #[test]
    fn clamp_and_enforce_range_follow_webidl() {
        use super::{convert_annotated_integer, IntegerMode, WebIdlArgumentConversion as C};
        let clamp = |number, conversion| convert_annotated_integer(number, conversion, IntegerMode::Clamp).unwrap();
        assert_eq!(clamp(300.0, C::Octet), 255.0);
        assert_eq!(clamp(-5.0, C::Octet), 0.0);
        assert_eq!(clamp(f64::NAN, C::Long), 0.0);
        assert_eq!(clamp(2.5, C::Octet), 2.0);
        assert_eq!(clamp(3.5, C::Octet), 4.0);
        assert_eq!(clamp(-0.4, C::Long), 0.0);
        assert!(clamp(-0.4, C::Long).is_sign_positive());
        assert_eq!(clamp(f64::INFINITY, C::UnsignedLongLong), 9007199254740991.0);
        let enforce = |number, conversion| convert_annotated_integer(number, conversion, IntegerMode::EnforceRange);
        assert_eq!(enforce(255.9, C::Octet), Ok(255.0));
        assert!(enforce(256.0, C::Octet).is_err());
        assert!(enforce(-1.0, C::UnsignedLong).is_err());
        assert!(enforce(f64::INFINITY, C::Long).is_err());
        assert!(enforce(f64::NAN, C::Long).is_err());
    }

    #[test]
    fn webidl_integer_conversion_is_exact_for_64_bit_types() {
        assert_eq!(super::convert_webidl_integer(-5.0, 64, true), -5.0);
        assert_eq!(super::convert_webidl_integer(-5.0, 64, false), 18446744073709551611.0);
        assert_eq!(super::convert_webidl_integer(-1.0, 32, false), 4294967295.0);
        assert_eq!(super::convert_webidl_integer(300.0, 8, true), 44.0);
        assert_eq!(super::convert_webidl_integer(-129.5, 8, true), 127.0);
        assert_eq!(super::convert_webidl_integer(2.0_f64.powi(63), 64, true), -(2.0_f64.powi(63)));
        assert_eq!(super::convert_webidl_integer(1e300, 64, true), 0.0);
        assert_eq!(super::convert_webidl_integer(f64::NAN, 64, true), 0.0);
    }

    mod traced {
        use super::super::*;
        use std::cell::{Cell, RefCell};

        thread_local! {
            static DROPPED: Cell<usize> = const { Cell::new(0) };
        }

        fn dropped() -> usize {
            DROPPED.with(Cell::get)
        }

        /// A DOM-like node: a strong native reference to another node, and a JS reference (a
        /// listener or its own wrapper), both reported to the tracer.
        struct Node {
            id: u32,
            next: GcMember<Node>,
            listener: RefCell<Option<JsRef>>,
        }

        impl Trace for Node {
            fn trace(&self, tracer: &mut Tracer) {
                tracer.member(&self.next);
                if let Some(listener) = &*self.listener.borrow() {
                    tracer.js(listener);
                }
            }
        }

        impl Drop for Node {
            fn drop(&mut self) {
                DROPPED.with(|dropped| dropped.set(dropped.get() + 1));
            }
        }

        fn node(id: u32) -> Node {
            Node { id, next: GcMember::empty(), listener: RefCell::new(None) }
        }

        fn node_interface(runtime: &mut Runtime) -> Interface {
            let interface = runtime.define_interface("Node", None);
            runtime
                .define_property(&interface, "id", |native| {
                    Value::Number(native.downcast_ref::<Node>().expect("Node wrapper").id as f64)
                })
                .unwrap();
            runtime
                .define_property(&interface, "nextId", |native| {
                    let node = native.downcast_ref::<Node>().expect("Node wrapper");
                    // SAFETY: `node` is reachable (its wrapper is the receiver) and traces `next`.
                    match unsafe { node.next.get() } {
                        Some(next) => Value::Number(next.id as f64),
                        None => Value::Null,
                    }
                })
                .unwrap();
            interface
        }

        fn collect(runtime: &mut Runtime, expected: usize) {
            for _ in 0..20 {
                runtime.force_full_gc_for_testing();
                if dropped() >= expected {
                    break;
                }
            }
        }

        thread_local! {
            static TREES_DROPPED: Cell<usize> = const { Cell::new(0) };
        }

        /// A foreign-looking type traced through `CustomTrace` (`#[custom_trace]`).
        struct Extra(GcMember<Tree>);

        impl CustomTrace for Extra {
            fn trace(&self, tracer: &mut Tracer) {
                tracer.member(&self.0);
            }
        }

        /// Traced entirely by `#[derive(Trace)]`: containers, an enum, a generic pair, a
        /// `#[custom_trace]` field and `#[no_trace]` fields.
        #[derive(Trace)]
        struct Tree {
            #[no_trace]
            id: u32,
            children: RefCell<Vec<GcMember<Tree>>>,
            slot: RefCell<Slot>,
            pair: Pair<Option<GcMember<Tree>>>,
            #[custom_trace]
            extra: Extra,
            #[no_trace = "a plain string holds no traced reference"]
            name: String,
        }

        #[derive(Trace)]
        enum Slot {
            Empty,
            One(GcMember<Tree>),
            Named { tree: GcMember<Tree>, #[no_trace] label: u8 },
        }

        #[derive(Trace)]
        struct Pair<T> {
            first: T,
            second: T,
        }

        impl Drop for Tree {
            fn drop(&mut self) {
                TREES_DROPPED.with(|dropped| dropped.set(dropped.get() + 1));
            }
        }

        fn tree(id: u32) -> Tree {
            Tree {
                id,
                children: RefCell::new(Vec::new()),
                slot: RefCell::new(Slot::Empty),
                pair: Pair { first: None, second: None },
                extra: Extra(GcMember::empty()),
                name: format!("tree {id}"),
            }
        }

        #[test]
        fn derived_trace_keeps_every_referenced_native_alive() {
            let trees_dropped = || TREES_DROPPED.with(Cell::get);
            let mut runtime = Runtime::new();
            // Six natives, each reachable from the root only through a different derived path.
            let leaves: Vec<GcRoot<Tree>> = (2..=7).map(|id| runtime.allocate_traced(tree(id))).collect();
            let mut root_native = tree(1);
            root_native.children.borrow_mut().push(leaves[0].member());
            root_native.children.borrow_mut().push(leaves[1].member());
            *root_native.slot.borrow_mut() = Slot::Named { tree: leaves[2].member(), label: 0 };
            root_native.pair = Pair { first: Some(leaves[3].member()), second: Some(leaves[4].member()) };
            root_native.extra = Extra(leaves[5].member());
            let root = runtime.allocate_traced(root_native);
            drop(leaves);
            for _ in 0..5 {
                runtime.force_full_gc_for_testing();
            }
            // A field missed by the derive would let its target be collected here.
            assert_eq!(trees_dropped(), 0);
            assert_eq!(root.get().name, "tree 1");
            let ids: Vec<u32> = root
                .get()
                .children
                .borrow()
                .iter()
                // SAFETY: the root is reachable and traces its children.
                .map(|child| unsafe { child.get() }.unwrap().id)
                .collect();
            assert_eq!(ids, [2, 3]);
            // Replacing a slot drops the only reference to its tree.
            *root.get().slot.borrow_mut() = Slot::One(GcMember::empty());
            for _ in 0..20 {
                runtime.force_full_gc_for_testing();
                if trees_dropped() >= 1 {
                    break;
                }
            }
            assert_eq!(trees_dropped(), 1);
            drop(root);
            for _ in 0..20 {
                runtime.force_full_gc_for_testing();
                if trees_dropped() >= 7 {
                    break;
                }
            }
            assert_eq!(trees_dropped(), 7);
        }

        #[test]
        fn traced_natives_serve_getters_and_follow_native_references() {
            let mut runtime = Runtime::new();
            let interface = node_interface(&mut runtime);
            let second = runtime.allocate_traced(node(2));
            let mut first_native = node(1);
            first_native.next = second.member();
            let first = runtime.allocate_traced(first_native);
            let wrapper = runtime.create_traced_instance(&interface, &first);
            runtime.set_global_property("first", &wrapper).unwrap();
            assert_eq!(runtime.eval("[first.id, first.nextId, first instanceof Node].join()").unwrap(), "1,2,true");
            // The wrapper resolves back to the same native object.
            let resolved = runtime.traced_native::<Node>(&wrapper).unwrap();
            assert!(std::ptr::eq(resolved.get(), first.get()));
            let number = runtime.store(&Value::Number(1.0));
            assert!(runtime.traced_native::<Node>(&number).is_none());

            // Only the member keeps `second` alive once its own root is gone.
            let before = dropped();
            drop(second);
            collect(&mut runtime, before + 1);
            assert_eq!(dropped(), before, "a member keeps its target alive");
            assert_eq!(runtime.eval("first.nextId").unwrap(), "2");

            // With the JS global, the Rust roots and the wrapper handle gone, both natives go,
            // each exactly once.
            drop((first, resolved, wrapper));
            runtime.eval("delete globalThis.first;").unwrap();
            collect(&mut runtime, before + 2);
            assert_eq!(dropped(), before + 2);
        }

        #[test]
        fn cycles_between_natives_and_js_are_collected_but_kept_while_reachable() {
            let mut runtime = Runtime::new();
            let interface = node_interface(&mut runtime);
            let before = dropped();
            {
                // A node whose listener closes over the node's own wrapper: native -> listener
                // -> wrapper -> native. Strong references on either side would leak this.
                let target = runtime.allocate_traced(node(7));
                let wrapper = runtime.create_traced_instance(&interface, &target);
                runtime.set_global_property("target", &wrapper).unwrap();
                let listener = runtime.eval_handle("(() => { const node = target; return () => node.id; })()").unwrap();
                let listener_ref = runtime.js_ref(&listener);
                *target.get().listener.borrow_mut() = Some(listener_ref);
                // A second cycle through native members: a -> b -> a.
                let a = runtime.allocate_traced(node(10));
                let b = runtime.allocate_traced(node(11));
                unsafe_set_next(&a, &b);
                unsafe_set_next(&b, &a);
                runtime.eval("globalThis.keep = target;").unwrap();
                drop((target, wrapper, listener, a, b));
            }
            runtime.eval("delete globalThis.target;").unwrap();
            collect(&mut runtime, before + 2);
            // The a <-> b cycle is collected; the listener cycle is still reachable from `keep`.
            assert_eq!(dropped(), before + 2);
            assert_eq!(runtime.eval("keep.id").unwrap(), "7");
            runtime.eval("delete globalThis.keep;").unwrap();
            collect(&mut runtime, before + 3);
            assert_eq!(dropped(), before + 3, "the listener cycle is collected once unreachable");
        }

        /// Points `from.next` at `to`. Members are normally set while building the native; this
        /// rewires an already-allocated one through a raw pointer, as a DOM mutation would.
        fn unsafe_set_next(from: &GcRoot<Node>, to: &GcRoot<Node>) {
            let from = from.get() as *const Node as *mut Node;
            // SAFETY: test-only exclusive access; no other borrow of `from` exists right now.
            unsafe { (*from).next.set(to) };
        }

        #[test]
        fn a_traced_native_keeps_one_wrapper_with_its_expandos_while_alive() {
            let mut runtime = Runtime::new();
            let interface = node_interface(&mut runtime);
            let before = dropped();
            let native = runtime.allocate_traced(node(3));
            {
                let first = runtime.create_traced_instance(&interface, &native);
                let second = runtime.create_traced_instance(&interface, &native);
                runtime.set_global_property("first", &first).unwrap();
                runtime.set_global_property("second", &second).unwrap();
                assert_eq!(runtime.eval("first === second").unwrap(), "true");
                runtime.eval("first.expando = 'kept'; delete globalThis.first; delete globalThis.second;").unwrap();
            }
            // Only Rust holds the native; JS holds nothing. The wrapper must survive with its
            // expando, because the native traces it.
            collect(&mut runtime, before + 1);
            assert_eq!(dropped(), before);
            let again = runtime.create_traced_instance(&interface, &native);
            runtime.set_global_property("again", &again).unwrap();
            assert_eq!(runtime.eval("[again.expando, again.id].join()").unwrap(), "kept,3");
            // Once neither side is reachable, the native/wrapper pair is collected together.
            drop((again, native));
            runtime.eval("delete globalThis.again;").unwrap();
            collect(&mut runtime, before + 1);
            assert_eq!(dropped(), before + 1);
        }

        #[test]
        fn a_traced_js_reference_keeps_its_value_alive() {
            let mut runtime = Runtime::new();
            let holder = runtime.allocate_traced(node(1));
            {
                let value = runtime.eval_handle("({ marker: 42 })").unwrap();
                let reference = runtime.js_ref(&value);
                *holder.get().listener.borrow_mut() = Some(reference);
            }
            runtime.force_full_gc_for_testing();
            runtime.force_full_gc_for_testing();
            let listener = holder.get().listener.borrow();
            let value = runtime.js_ref_value(listener.as_ref().unwrap()).expect("kept alive by the trace");
            drop(listener);
            runtime.set_global_property("value", &value).unwrap();
            assert_eq!(runtime.eval("value.marker").unwrap(), "42");
        }
    }

    #[test]
    fn constructible_interface_builds_native_objects_from_converted_arguments() {
        thread_local! {
            static CONSTRUCTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
            static DROPPED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        }
        struct Labeled {
            label: Vec<u16>,
            count: u32,
        }
        impl Drop for Labeled {
            fn drop(&mut self) {
                DROPPED.with(|dropped| dropped.set(dropped.get() + 1));
            }
        }
        impl crate::Trace for Labeled {
            fn trace(&self, _tracer: &mut crate::Tracer) {}
        }
        fn construct(arguments: &[Value]) -> Result<crate::TracedNative, crate::WebIdlError> {
            let Value::Utf16String(label) = &arguments[0] else { unreachable!("DOMString argument") };
            let count = match arguments[1] {
                Value::Missing => 1,
                Value::Number(count) => count as u32,
                _ => unreachable!("optional unsigned long argument"),
            };
            CONSTRUCTED.with(|constructed| constructed.set(constructed.get() + 1));
            Ok(crate::TracedNative::new(Labeled { label: label.clone(), count }))
        }

        let mut runtime = Runtime::new();
        let interface = runtime.define_constructible_interface(
            "Labeled",
            None,
            construct,
            &[crate::WebIdlArgumentConversion::DomString, crate::WebIdlArgumentConversion::UnsignedLong],
            &[false, false],
            &[false, true],
            &[None, None],
        );
        runtime
            .define_property(&interface, "label", |native| {
                let labeled = native.downcast_ref::<Labeled>().expect("Labeled wrapper");
                Value::String(String::from_utf16_lossy(&labeled.label))
            })
            .unwrap();
        runtime
            .define_property(&interface, "count", |native| {
                Value::Number(native.downcast_ref::<Labeled>().expect("Labeled wrapper").count as f64)
            })
            .unwrap();
        runtime.expose_interface(&interface).unwrap();

        for (source, expected) in [
            ("Labeled.length", "1"),
            ("const a = new Labeled('first'); [a.label, a.count].join()", "first,1"),
            ("const b = new Labeled({ toString() { return 'second'; } }, 7); [b.label, b.count].join()", "second,7"),
            ("a instanceof Labeled && Object.getPrototypeOf(a) === Labeled.prototype", "true"),
            // A JS subclass constructs through the WebIDL constructor and keeps native state.
            ("class Sub extends Labeled { get twice() { return this.count * 2; } }; const c = new Sub('sub', 4); [c.label, c.twice, c instanceof Labeled].join()", "sub,8,true"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        assert_eq!(CONSTRUCTED.with(std::cell::Cell::get), 3);
        // Calling without `new`, and arguments whose conversion throws, never construct.
        for source in ["Labeled('x')", "new Labeled(Symbol())"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
        assert_eq!(
            runtime.eval("try { new Labeled({ toString() { throw new RangeError('custom'); } }) } catch (e) { e.message }").unwrap(),
            "custom"
        );
        assert_eq!(CONSTRUCTED.with(std::cell::Cell::get), 3);

        // JS-constructed natives get the same exactly-once finalization as create_instance.
        runtime.eval("(() => { for (let i = 0; i < 10; i++) new Labeled('temp'); })()").unwrap();
        assert_eq!(CONSTRUCTED.with(std::cell::Cell::get), 13);
        for _ in 0..30 {
            runtime.force_full_gc_for_testing();
            if DROPPED.with(std::cell::Cell::get) == 10 {
                break;
            }
        }
        assert_eq!(DROPPED.with(std::cell::Cell::get), 10, "each temporary dropped exactly once");
        assert_eq!(runtime.eval("[a.label, b.label, c.label].join()").unwrap(), "first,second,sub");
    }

    #[test]
    fn create_wrapped_stress_many_objects_all_get_collected_exactly_once() {
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        const COUNT: usize = 200;
        {
            let handles: Vec<Handle> = (0..COUNT)
                .map(|_| runtime.create_wrapped(DropCounter(counter.clone())))
                .collect();
            assert_eq!(counter.load(Ordering::SeqCst), 0);
            drop(handles);
        }

        for _ in 0..30 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) as usize == COUNT {
                break;
            }
        }
        assert_eq!(
            counter.load(Ordering::SeqCst) as usize,
            COUNT,
            "every one of {COUNT} wrapped objects should eventually be collected exactly once"
        );
    }

    #[test]
    fn create_wrapped_of_different_types_coexist_independently() {
        // No read-back API exists yet (deliberately Phase 4/WebIDL-bindings scope, see
        // create_wrapped's own doc comment) -- this only exercises that wrapping unrelated
        // types in the same Runtime doesn't panic or corrupt each other's finalizer.
        let mut runtime = Runtime::new();
        let int_counter = Arc::new(AtomicUsize::new(0));
        let string_counter = Arc::new(AtomicUsize::new(0));

        struct TaggedDrop(Arc<AtomicUsize>);
        impl Drop for TaggedDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let a = runtime.create_wrapped((42i32, TaggedDrop(int_counter.clone())));
        let b = runtime.create_wrapped(("hello".to_string(), TaggedDrop(string_counter.clone())));
        drop(a);
        drop(b);

        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if int_counter.load(Ordering::SeqCst) == 1 && string_counter.load(Ordering::SeqCst) == 1
            {
                break;
            }
        }
        assert_eq!(int_counter.load(Ordering::SeqCst), 1);
        assert_eq!(string_counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn get_wrapped_reads_back_the_right_value() {
        let mut runtime = Runtime::new();
        let handle = runtime.create_wrapped(42i32);
        assert_eq!(runtime.get_wrapped::<i32>(&handle), Some(&42));
    }

    #[test]
    fn get_wrapped_rejects_a_type_mismatch() {
        let mut runtime = Runtime::new();
        let handle = runtime.create_wrapped(42i32);
        // Asking for the wrong type must return None, not read 4 bytes of an i32 as if they
        // were something else -- this is exactly what the Box<dyn Any> + downcast_ref check
        // in get_wrapped exists to prevent.
        assert_eq!(runtime.get_wrapped::<String>(&handle), None);
    }

    #[test]
    fn get_wrapped_returns_none_for_a_non_wrapped_handle() {
        let mut runtime = Runtime::new();
        let handle = runtime.store(&Value::Number(42.0));
        assert_eq!(runtime.get_wrapped::<i32>(&handle), None);
    }

    #[test]
    fn get_wrapped_distinguishes_two_different_wrapped_objects() {
        let mut runtime = Runtime::new();
        let a = runtime.create_wrapped(1i32);
        let b = runtime.create_wrapped(2i32);
        assert_eq!(runtime.get_wrapped::<i32>(&a), Some(&1));
        assert_eq!(runtime.get_wrapped::<i32>(&b), Some(&2));
    }

    #[test]
    fn wrapped_objects_referencing_each_other_do_not_leak_the_cycle() {
        // A naive design where cross-references between wrapped objects are held as extra
        // v8::Global handles on the Rust side would leak this forever (each side's strong
        // reference keeps the other alive, even with no external reference at all). `link`
        // deliberately uses a plain JS property instead -- this test proves that choice
        // actually avoids the leak, not just that it's the design.
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let a = runtime.create_wrapped(DropCounter(counter.clone()));
        let b = runtime.create_wrapped(DropCounter(counter.clone()));
        runtime.link(&a, "other", &b).unwrap();
        runtime.link(&b, "other", &a).unwrap();
        drop(a);
        drop(b);

        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) == 2 {
                break;
            }
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "both sides of the reference cycle should be collected, not leaked"
        );
    }

    #[test]
    fn set_global_property_exposes_a_wrapped_object_as_window() {
        // Phase 4's first validation milestone ("global/window exposure", see
        // docs/V8_MIGRATION.md) -- a Rust-backed object reachable from script as `window`, with
        // a property on it also readable, built entirely from this crate's own Phase 2/3
        // primitives (create_wrapped, link, set_global_property), no codegen involved.
        struct FakeWindow;

        let mut runtime = Runtime::new();
        let window = runtime.create_wrapped(FakeWindow);
        let name = runtime.store(&Value::String("Roves".to_string()));
        runtime.link(&window, "name", &name).unwrap();
        runtime.set_global_property("window", &window).unwrap();

        assert_eq!(
            runtime.eval_value("window.name").unwrap(),
            Value::String("Roves".to_string())
        );
        assert_eq!(
            runtime.eval_value("typeof window").unwrap(),
            Value::String("object".to_string())
        );
    }

    #[test]
    fn define_interface_exposes_a_constructor_on_the_global_object() {
        // The constructor is only actually exposed on the global object once the first
        // instance is created (see create_instance's own doc comment on why: exposing it
        // eagerly in define_interface would freeze the prototype before any later
        // define_method/define_property calls for this interface ever ran) -- so this test
        // creates one, even though it doesn't otherwise need it, purely to trigger that.
        struct Node;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime.create_instance(&node_interface, Node);
        assert_eq!(
            runtime.eval_value("typeof Node").unwrap(),
            Value::String("function".to_string())
        );
    }

    #[test]
    fn create_instance_has_the_interfaces_prototype() {
        struct Node;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let node = runtime.create_instance(&node_interface, Node);
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node instanceof Node").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn create_instance_participates_in_prototype_inheritance() {
        // The real WebIDL shape this prototypes: `Element` inheriting from `Node`, e.g.
        // `document.createElement(...) instanceof Node` must be true, not just
        // `instanceof Element`.
        struct Node;
        struct Element;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        // `Node`'s own constructor is only exposed on the global object at its own first
        // create_instance call (see create_instance's own doc comment on why that's deferred) --
        // this throwaway instance exists purely so the JS-side `instanceof Node` check below has
        // a `Node` identifier to resolve at all.
        runtime.create_instance(&node_interface, Node);
        let element = runtime.create_instance(&element_interface, Element);
        runtime.set_global_property("element", &element).unwrap();

        assert_eq!(
            runtime.eval_value("element instanceof Element").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            runtime.eval_value("element instanceof Node").unwrap(),
            Value::Bool(true),
            "an Element instance must also be a Node instance, via the prototype chain"
        );
    }

    #[test]
    fn create_instance_is_not_an_instance_of_an_unrelated_interface() {
        struct Node;
        struct Event;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let event_interface = runtime.define_interface("Event", None);
        let node = runtime.create_instance(&node_interface, Node);
        runtime.set_global_property("node", &node).unwrap();
        // Throwaway instance purely to expose `Event` on the global object -- see the sibling
        // test's own comment on why.
        runtime.create_instance(&event_interface, Event);

        assert_eq!(
            runtime.eval_value("node instanceof Event").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn create_instance_still_supports_typed_read_back_and_gc() {
        // create_instance must keep everything create_wrapped already provides: typed
        // get_wrapped read-back and guaranteed-finalizer GC cleanup -- gaining a prototype
        // chain shouldn't cost either.
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let node_interface = runtime.define_interface("Node", None);

        let node = runtime.create_instance(&node_interface, (7i32, DropCounter(counter.clone())));
        assert_eq!(runtime.get_wrapped::<(i32, DropCounter)>(&node).unwrap().0, 7);

        drop(node);
        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) == 1 {
                break;
            }
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn create_instance_reuses_live_wrapper_by_native_identity_and_recreates_after_gc() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("IdentityNode", None);
        let constructions = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let make_node = || {
            constructions.fetch_add(1, Ordering::SeqCst);
            DropCounter(drops.clone())
        };

        let first = runtime.create_instance_with_identity(&interface, 17, make_node);
        let second = runtime.create_instance_with_identity(&interface, 17, || {
            panic!("the native factory must not run while the wrapper is alive")
        });
        runtime.set_global_property("first", &first).unwrap();
        runtime.set_global_property("second", &second).unwrap();
        assert_eq!(runtime.eval("first === second").unwrap(), "true");
        assert_eq!(constructions.load(Ordering::SeqCst), 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0);

        drop(first);
        drop(second);
        runtime
            .eval("delete globalThis.first; delete globalThis.second")
            .unwrap();
        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if drops.load(Ordering::SeqCst) == 1 {
                break;
            }
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(runtime.wrapper_identities.borrow().is_empty());

        let recreated = runtime.create_instance_with_identity(&interface, 17, make_node);
        assert_eq!(constructions.load(Ordering::SeqCst), 2);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        drop(recreated);
    }

    #[test]
    fn old_identity_finalizer_does_not_remove_a_replacement_wrapper() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("IdentityNode", None);
        let drops = Arc::new(AtomicUsize::new(0));
        let old =
            runtime.create_instance_with_identity(&interface, 29, || DropCounter(drops.clone()));
        runtime.set_global_property("old", &old).unwrap();

        // Simulate the cache entry having gone stale before its guaranteed finalizer runs.
        // This can happen between V8's weak-handle clearing and finalizer passes.
        runtime.wrapper_identities.borrow_mut().remove(&29);
        let replacement =
            runtime.create_instance_with_identity(&interface, 29, || DropCounter(drops.clone()));
        runtime
            .set_global_property("replacement", &replacement)
            .unwrap();
        assert_eq!(runtime.eval("old === replacement").unwrap(), "false");

        drop(old);
        runtime.eval("delete globalThis.old").unwrap();
        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if drops.load(Ordering::SeqCst) == 1 {
                break;
            }
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);

        let reused = runtime.create_instance_with_identity(&interface, 29, || {
            panic!("the replacement wrapper must remain cached after the old finalizer")
        });
        runtime.set_global_property("reused", &reused).unwrap();
        assert_eq!(runtime.eval("replacement === reused").unwrap(), "true");
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn define_property_computes_a_live_getter_from_wrapped_data() {
        // The real WebIDL "attribute" shape: node.nodeName reads Rust-side state live, not a
        // snapshot stored once like `link`'s plain property.
        struct Node {
            name: String,
        }

        fn node_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.name.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_property(&node_interface, "nodeName", node_name)
            .unwrap();

        let node = runtime.create_instance(
            &node_interface,
            Node {
                name: "DIV".to_string(),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node.nodeName").unwrap(),
            Value::String("DIV".to_string())
        );
    }

    #[test]
    fn define_property_is_shared_by_every_instance_of_the_interface() {
        struct Node {
            name: String,
        }
        fn node_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.name.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_property(&node_interface, "nodeName", node_name)
            .unwrap();

        let a = runtime.create_instance(
            &node_interface,
            Node {
                name: "DIV".to_string(),
            },
        );
        let b = runtime.create_instance(
            &node_interface,
            Node {
                name: "SPAN".to_string(),
            },
        );
        runtime.set_global_property("a", &a).unwrap();
        runtime.set_global_property("b", &b).unwrap();

        assert_eq!(
            runtime.eval_value("a.nodeName").unwrap(),
            Value::String("DIV".to_string())
        );
        assert_eq!(
            runtime.eval_value("b.nodeName").unwrap(),
            Value::String("SPAN".to_string())
        );
    }

    #[test]
    fn define_property_on_its_own_interface_works_alongside_instanceof() {
        struct Node;
        struct Element {
            tag: String,
        }
        fn tag_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Element>() {
                Some(element) => Value::String(element.tag.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        runtime
            .define_property(&element_interface, "tagName", tag_name)
            .unwrap();
        // Throwaway instance purely to expose `Node` on the global object for the `instanceof`
        // check below -- see create_instance's own doc comment on why exposure is per-interface
        // and deferred to first instantiation.
        runtime.create_instance(&node_interface, Node);

        let element = runtime.create_instance(
            &element_interface,
            Element {
                tag: "DIV".to_string(),
            },
        );
        runtime.set_global_property("element", &element).unwrap();

        assert_eq!(
            runtime.eval_value("element.tagName").unwrap(),
            Value::String("DIV".to_string())
        );
        // instanceof still sees Element's inheritance from Node (that's prototype-chain-based,
        // unaffected by the instance-template limitation below).
        assert_eq!(
            runtime.eval_value("element instanceof Node").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn define_property_is_inherited_from_a_parent_interface() {
        // A genuine surprise found by testing rather than assuming (the first version of this
        // test asserted the opposite and failed): despite accessors being installed on the
        // *instance* template rather than the prototype template (see define_property's own doc
        // comment on why), V8's `FunctionTemplate::inherit` propagates instance-template
        // accessors down to child interfaces too, not just the prototype chain `instanceof`
        // relies on. A property defined on a parent interface (`Node`) is visible on a child
        // interface's instances (`Element`) -- and, checked here with a getter that actually
        // downcasts rather than a constant, reads that specific instance's own wrapped data
        // correctly, not stale or wrong data from elsewhere.
        struct Element {
            tag: String,
        }
        fn tag_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Element>() {
                Some(element) => Value::String(element.tag.clone()),
                None => Value::Undefined,
            }
        }

        struct Node;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_property(&node_interface, "tagName", tag_name)
            .unwrap();
        // Throwaway instance purely to expose `Node` on the global object for the `instanceof`
        // check below -- see create_instance's own doc comment on why exposure is per-interface
        // and deferred to first instantiation.
        runtime.create_instance(&node_interface, Node);
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        let element = runtime.create_instance(
            &element_interface,
            Element {
                tag: "DIV".to_string(),
            },
        );
        runtime.set_global_property("element", &element).unwrap();

        assert_eq!(
            runtime.eval_value("element instanceof Node").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            runtime.eval_value("element.tagName").unwrap(),
            Value::String("DIV".to_string())
        );
    }

    #[test]
    fn define_settable_property_reads_the_initial_value() {
        struct Node {
            value: std::cell::RefCell<String>,
        }
        fn get_value(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.value.borrow().clone()),
                None => Value::Undefined,
            }
        }
        fn set_value(this: &dyn std::any::Any, new_value: &Value) {
            if let (Some(node), Value::String(s)) = (this.downcast_ref::<Node>(), new_value) {
                *node.value.borrow_mut() = s.clone();
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_settable_property(&node_interface, "nodeValue", get_value, set_value)
            .unwrap();
        let node = runtime.create_instance(
            &node_interface,
            Node {
                value: std::cell::RefCell::new("hello".to_string()),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node.nodeValue").unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn define_settable_property_mutates_the_wrapped_value() {
        struct Node {
            value: std::cell::RefCell<String>,
        }
        fn get_value(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.value.borrow().clone()),
                None => Value::Undefined,
            }
        }
        fn set_value(this: &dyn std::any::Any, new_value: &Value) {
            if let (Some(node), Value::String(s)) = (this.downcast_ref::<Node>(), new_value) {
                *node.value.borrow_mut() = s.clone();
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_settable_property(&node_interface, "nodeValue", get_value, set_value)
            .unwrap();
        let node = runtime.create_instance(
            &node_interface,
            Node {
                value: std::cell::RefCell::new("hello".to_string()),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        // Assigning from JS must both take effect (read back via a later access) and be
        // reflected on the Rust side (read back via get_wrapped), proving the setter actually
        // mutated the same wrapped value the getter/get_wrapped read from, not a copy.
        runtime.eval("node.nodeValue = 'world'").unwrap();
        assert_eq!(
            runtime.eval_value("node.nodeValue").unwrap(),
            Value::String("world".to_string())
        );
        assert_eq!(*runtime.get_wrapped::<Node>(&node).unwrap().value.borrow(), "world");
    }

    #[test]
    fn define_method_is_callable_with_arguments_from_js() {
        struct Node {
            name: String,
        }
        fn greet(this: &dyn std::any::Any, args: &[Value]) -> Value {
            let node = match this.downcast_ref::<Node>() {
                Some(node) => node,
                None => return Value::Undefined,
            };
            let greeting = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => "Hello".to_string(),
            };
            Value::String(format!("{greeting}, {}!", node.name))
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_method(&node_interface, "greet", greet)
            .unwrap();
        let node = runtime.create_instance(
            &node_interface,
            Node {
                name: "World".to_string(),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node.greet('Hi')").unwrap(),
            Value::String("Hi, World!".to_string())
        );
        assert_eq!(
            runtime.eval_value("node.greet()").unwrap(),
            Value::String("Hello, World!".to_string())
        );
    }

    #[test]
    fn define_method_is_inherited_from_a_parent_interface() {
        struct Element {
            tag: String,
        }
        fn describe(this: &dyn std::any::Any, _args: &[Value]) -> Value {
            match this.downcast_ref::<Element>() {
                Some(element) => Value::String(format!("<{}>", element.tag)),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_method(&node_interface, "describe", describe)
            .unwrap();
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        let element = runtime.create_instance(
            &element_interface,
            Element {
                tag: "DIV".to_string(),
            },
        );
        runtime.set_global_property("element", &element).unwrap();

        // Methods live on the prototype template, so they inherit via the normal JS prototype
        // chain -- no surprise here the way there was for instance-template-based accessors.
        assert_eq!(
            runtime.eval_value("element.describe()").unwrap(),
            Value::String("<DIV>".to_string())
        );
    }

    #[test]
    fn define_method_is_shared_by_every_instance_of_the_interface() {
        struct Node {
            name: String,
        }
        fn get_name(this: &dyn std::any::Any, _args: &[Value]) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.name.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_method(&node_interface, "getName", get_name)
            .unwrap();
        let a = runtime.create_instance(
            &node_interface,
            Node {
                name: "A".to_string(),
            },
        );
        let b = runtime.create_instance(
            &node_interface,
            Node {
                name: "B".to_string(),
            },
        );
        runtime.set_global_property("a", &a).unwrap();
        runtime.set_global_property("b", &b).unwrap();

        assert_eq!(
            runtime.eval_value("a.getName()").unwrap(),
            Value::String("A".to_string())
        );
        assert_eq!(
            runtime.eval_value("b.getName()").unwrap(),
            Value::String("B".to_string())
        );
    }

    fn collection_getter(this: &dyn std::any::Any, index: u32) -> Option<Value> {
        this.downcast_ref::<Vec<Value>>()?
            .get(index as usize)
            .cloned()
    }

    #[test]
    fn indexed_getter_reads_each_instances_native_data() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&interface, collection_getter)
            .unwrap();
        let a = runtime.create_instance(&interface, vec![Value::String("A".into())]);
        let b = runtime.create_instance(&interface, vec![Value::String("B".into())]);
        runtime.set_global_property("a", &a).unwrap();
        runtime.set_global_property("b", &b).unwrap();
        assert_eq!(runtime.eval("a[0] + b['0']").unwrap(), "AB");
        assert_eq!(runtime.eval_value("a[1]").unwrap(), Value::Undefined);
        assert_eq!(runtime.eval("a instanceof Collection").unwrap(), "true");
    }

    #[test]
    fn indexed_getter_distinguishes_undefined_from_unhandled_lookup() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&interface, collection_getter)
            .unwrap();
        let collection = runtime.create_instance(&interface, vec![Value::Undefined]);
        runtime
            .set_global_property("collection", &collection)
            .unwrap();
        runtime
            .eval("Collection.prototype[0] = 'masked'; Collection.prototype[1] = 'fallback'")
            .unwrap();
        assert_eq!(
            runtime.eval_value("collection[0]").unwrap(),
            Value::Undefined
        );
        assert_eq!(runtime.eval("collection[1]").unwrap(), "fallback");
        assert_eq!(
            runtime.eval("Object.create(collection)[1]").unwrap(),
            "fallback"
        );
        runtime.eval("collection[2] = 'own'").unwrap();
        assert_eq!(runtime.eval("collection[2]").unwrap(), "own");
    }

    #[test]
    fn indexed_getter_leaves_noncanonical_indices_as_named_properties() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        fn getter(_: &dyn std::any::Any, index: u32) -> Option<Value> {
            Some(Value::Number(index as f64))
        }
        runtime
            .define_indexed_property_getter(&interface, getter)
            .unwrap();
        let collection = runtime.create_instance(&interface, ());
        runtime
            .set_global_property("collection", &collection)
            .unwrap();
        assert_eq!(
            runtime.eval("collection[4294967294]").unwrap(),
            "4294967294"
        );
        assert_eq!(runtime.eval(
            "['01', '-1', '1.5', '4294967295', 'label'].every(k => { collection[k] = 'named'; return collection[k] === 'named'; })"
        ).unwrap(), "true");
    }

    #[test]
    fn indexed_getter_on_derived_interface_keeps_wrapper_finalization() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Collection(Arc<AtomicUsize>);
        impl Drop for Collection {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn getter(this: &dyn std::any::Any, index: u32) -> Option<Value> {
            this.downcast_ref::<Collection>()?;
            (index == 0).then_some(Value::String("inherited".into()))
        }
        let mut runtime = Runtime::new();
        let parent = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&parent, getter)
            .unwrap();
        let child = runtime.define_interface("ChildCollection", Some(&parent));
        // FunctionTemplate::inherit does not copy indexed interceptors, unlike accessors.
        runtime
            .define_indexed_property_getter(&child, getter)
            .unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let collection = runtime.create_instance(&child, Collection(drops.clone()));
        runtime
            .set_global_property("collection", &collection)
            .unwrap();
        assert_eq!(runtime.eval("collection[0]").unwrap(), "inherited");
        assert_eq!(
            runtime.eval("Object.create(collection)[0]").unwrap(),
            "inherited"
        );
        drop(collection);
        runtime.force_full_gc_for_testing();
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        runtime.eval("delete globalThis.collection").unwrap();
        runtime.force_full_gc_for_testing();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        runtime.force_full_gc_for_testing();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn indexed_getter_rejects_registration_after_instance_creation() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        let _collection = runtime.create_instance(&interface, ());
        assert!(
            runtime
                .define_indexed_property_getter(&interface, collection_getter)
                .is_err()
        );
    }

    #[test]
    fn indexed_getter_requires_explicit_registration_on_derived_interfaces() {
        let mut runtime = Runtime::new();
        let parent = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&parent, collection_getter)
            .unwrap();
        let child = runtime.define_interface("ChildCollection", Some(&parent));
        let base = runtime.create_instance(&parent, vec![Value::String("base".into())]);
        let derived = runtime.create_instance(&child, vec![Value::String("derived".into())]);
        runtime.set_global_property("base", &base).unwrap();
        runtime.set_global_property("derived", &derived).unwrap();
        assert_eq!(runtime.eval("base[0]").unwrap(), "base");
        assert_eq!(runtime.eval_value("derived[0]").unwrap(), Value::Undefined);
    }

    #[test]
    fn materializing_a_descendant_rejects_late_members_on_all_ancestors() {
        fn getter(_: &dyn std::any::Any) -> Value { Value::Undefined }
        fn setter(_: &dyn std::any::Any, _: &Value) {}
        fn method(_: &dyn std::any::Any, _: &[Value]) -> Value { Value::Undefined }
        let mut runtime = Runtime::new();
        let parent = runtime.define_interface("Parent", None);
        let middle = runtime.define_interface("Middle", Some(&parent));
        let child = runtime.define_interface("Child", Some(&middle));
        let _instance = runtime.create_instance(&child, ());
        for interface in [&parent, &middle, &child] {
            assert!(runtime.define_property(interface, "read", getter).is_err());
            assert!(runtime.define_settable_property(interface, "write", getter, setter).is_err());
            assert!(runtime.define_method(interface, "method", method).is_err());
            assert!(runtime.define_indexed_property_getter(interface, collection_getter).is_err());
        }
        // Instantiating a sibling through a frozen parent is valid; defining its own
        // members remains possible until the sibling itself is materialized.
        let sibling = runtime.define_interface("Sibling", Some(&parent));
        runtime.define_property(&sibling, "read", getter).unwrap();
        let _sibling = runtime.create_instance(&sibling, ());
    }


    #[test]
    fn completed_finalizer_records_are_reclaimed_without_disarming_live_wrappers() {
        let mut runtime = Runtime::new();
        let live = runtime.create_wrapped(123_i32);
        for _ in 0..10 {
            for _ in 0..20 { drop(runtime.create_wrapped(())); }
            runtime.force_full_gc_for_testing();
            // The next allocation sweeps completed records, retaining the live one.
            let next = runtime.create_wrapped(());
            assert!(runtime.wrapped_finalizers.borrow().len() <= 2);
            assert_eq!(runtime.get_wrapped::<i32>(&live), Some(&123));
            drop(next);
            runtime.force_full_gc_for_testing();
        }
    }


    #[test]
    fn prototype_attributes_have_webidl_descriptors_and_check_receivers() {
        fn getter(this: &dyn std::any::Any) -> Value {
            Value::Number(this.downcast_ref::<std::cell::Cell<i32>>().unwrap().get() as f64)
        }
        fn setter(this: &dyn std::any::Any, value: &Value) {
            if let Value::Number(value) = value { this.downcast_ref::<std::cell::Cell<i32>>().unwrap().set(*value as i32); }
        }
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Native", None);
        runtime.define_settable_property(&interface, "value", getter, setter).unwrap();
        let node = runtime.create_instance(&interface, std::cell::Cell::new(3_i32));
        runtime.set_global_property("node", &node).unwrap();
        assert_eq!(runtime.eval("Object.hasOwn(node, 'value')").unwrap(), "false");
        assert_eq!(runtime.eval("const d = Object.getOwnPropertyDescriptor(Native.prototype, 'value'); [d.get.name, d.get.length, d.set.name, d.set.length, d.enumerable, d.configurable].join(',')").unwrap(), "get value,0,set value,1,true,true");
        assert_eq!(runtime.eval("d.get.call(node)").unwrap(), "3");
        runtime.eval("d.set.call(node, 9)").unwrap();
        assert_eq!(runtime.get_wrapped::<std::cell::Cell<i32>>(&node).map(std::cell::Cell::get), Some(9));
        for expression in ["d.get.call({})", "d.set.call({}, 1)", "d.get.call(Object.create(node))", "new d.get()"] {
            assert!(runtime.eval(expression).unwrap_err().contains("TypeError"), "{expression}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_mutable_primitive_attributes_follow_webidl_conversion() {
        use crate::webidl::mutable_primitives::{MutablePrimitivesBinding, MutablePrimitivesNative};
        use crate::{FiniteF32, FiniteF64};
        assert!(FiniteF32::new(f32::NAN).is_none());
        assert!(FiniteF32::new(f32::INFINITY).is_none());
        assert!(FiniteF64::new(f64::NAN).is_none());
        assert!(FiniteF64::new(f64::NEG_INFINITY).is_none());
        assert_eq!(FiniteF32::new(1.25).unwrap().get(), 1.25);
        assert_eq!(FiniteF64::new(-2.5).unwrap().get(), -2.5);
        #[derive(Default)]
        struct State {
            enabled: std::cell::Cell<bool>,
            ratio: std::cell::Cell<f64>,
            count: std::cell::Cell<u32>,
            optional_enabled: std::cell::Cell<Option<bool>>,
            optional_ratio: std::cell::Cell<Option<f64>>,
            optional_count: std::cell::Cell<Option<u32>>,
        }
        #[allow(non_snake_case)]
        impl MutablePrimitivesNative for State {
            fn Enabled(&self) -> bool { self.enabled.get() }
            fn set_Enabled(&self, value: bool) { self.enabled.set(value); }
            fn Ratio(&self) -> FiniteF64 { FiniteF64::new(self.ratio.get()).expect("test state stores finite double attributes") }
            fn set_Ratio(&self, value: FiniteF64) { self.ratio.set(value.get()); }
            fn Count(&self) -> u32 { self.count.get() }
            fn set_Count(&self, value: u32) { self.count.set(value); }
            fn OptionalEnabled(&self) -> Option<bool> { self.optional_enabled.get() }
            fn set_OptionalEnabled(&self, value: Option<bool>) { self.optional_enabled.set(value); }
            fn OptionalRatio(&self) -> Option<FiniteF64> { self.optional_ratio.get().map(|value| FiniteF64::new(value).expect("test state stores finite nullable doubles")) }
            fn set_OptionalRatio(&self, value: Option<FiniteF64>) { self.optional_ratio.set(value.map(FiniteF64::get)); }
            fn OptionalCount(&self) -> Option<u32> { self.optional_count.get() }
            fn set_OptionalCount(&self, value: Option<u32>) { self.optional_count.set(value); }
            fn Ping(&self) {}
            fn IsEnabled(&self) -> bool { self.enabled.get() }
            fn CurrentRatio(&self) -> FiniteF64 { FiniteF64::new(self.ratio.get()).unwrap_or_else(|| FiniteF64::new(0.0).unwrap()) }
            fn CurrentUnrestrictedRatio(&self) -> f64 { f64::INFINITY }
            fn OptionalUnrestrictedRatioResult(&self) -> Option<f64> { Some(f64::NAN) }
            fn CurrentUnrestrictedFloat(&self) -> f32 { f32::INFINITY }
            fn OptionalUnrestrictedFloatResult(&self) -> Option<f32> { Some(f32::NAN) }
            fn CurrentCount(&self) -> u32 { self.count.get() }
            fn Accepts(&self, value: bool) -> bool { value }
            fn Add(&self, value: FiniteF64) -> FiniteF64 { FiniteF64::new(self.ratio.get() + value.get()).unwrap() }
            fn EchoUnrestricted(&self, value: f64) -> f64 { value }
            fn Wrap(&self, value: u32) -> u32 { value }
            fn EchoByte(&self, value: i8) -> i8 { value }
            fn EchoOctet(&self, value: u8) -> u8 { value }
            fn EchoShort(&self, value: i16) -> i16 { value }
            fn EchoUnsignedShort(&self, value: u16) -> u16 { value }
            fn EchoLong(&self, value: i32) -> i32 { value }
            fn EchoLongLong(&self, value: i64) -> i64 { value }
            fn EchoUnsignedLongLong(&self, value: u64) -> u64 { value }
            fn EchoFloat(&self, value: FiniteF32) -> FiniteF32 { value }
            fn EchoUnrestrictedFloat(&self, value: f32) -> f32 { value }
            fn CurrentLabel(&self) -> Vec<u16> { vec![0xD800, 0x0041] }
            fn OptionalLabel(&self) -> Option<Vec<u16>> { None }
            fn CurrentUsvLabel(&self) -> String { "v8 ?".to_owned() }
            fn OptionalUsvLabel(&self) -> Option<String> { Some("game".to_owned()) }
            fn OptionalEnabledResult(&self) -> Option<bool> { Some(true) }
            fn OptionalRatioResult(&self) -> Option<FiniteF64> { None }
            fn OptionalCountResult(&self) -> Option<u32> { Some(u32::MAX) }
            fn SignedByteResult(&self) -> i8 { -7 }
            fn OctetResult(&self) -> u8 { 250 }
            fn ShortResult(&self) -> i16 { -300 }
            fn UnsignedShortResult(&self) -> u16 { 60_000 }
            fn LongResult(&self) -> i32 { -2_000_000 }
            fn LongLongResult(&self) -> i64 { i64::MAX }
            fn UnsignedLongLongResult(&self) -> u64 { u64::MAX }
            fn FloatResult(&self) -> FiniteF32 { FiniteF32::new(1.25).unwrap() }
            fn NullableFloatResult(&self) -> Option<FiniteF32> { Some(FiniteF32::new(-2.5).unwrap()) }
            fn NullableLongLongResult(&self) -> Option<i64> { None }
        }
        let mut runtime = Runtime::new();
        let binding = MutablePrimitivesBinding::<State>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, State::default());
        runtime.set_global_property("state", &handle).unwrap();
        runtime.eval("state.enabled = 'false'; state.ratio = '2.5'; state.count = -1").unwrap();
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert!(native.enabled.get());
        assert_eq!(native.ratio.get(), 2.5);
        assert_eq!(native.count.get(), u32::MAX);
        runtime.eval("state.enabled = 0; state.count = 4294967297").unwrap();
        assert!(runtime.eval("state.ratio = {}").unwrap_err().contains("TypeError"));
        assert!(runtime.eval("state.ratio = Infinity").unwrap_err().contains("TypeError"));
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert!(!native.enabled.get());
        assert_eq!(native.ratio.get(), 2.5);
        assert_eq!(native.count.get(), 1);
        assert!(runtime.eval("state.count = Symbol() ").is_err());

        assert_eq!(runtime.eval_value("state.add('2.5')").unwrap(), Value::Number(5.0));
        assert_eq!(runtime.eval_value("state.wrap(-1)").unwrap(), Value::Number(u32::MAX as f64));
        assert_eq!(runtime.eval_value("state.echoUnrestricted(Infinity)").unwrap(), Value::Number(f64::INFINITY));
        assert_eq!(runtime.eval_value("state.echoByte(128)").unwrap(), Value::Number(-128.0));
        assert_eq!(runtime.eval_value("state.echoOctet(-1)").unwrap(), Value::Number(255.0));
        assert_eq!(runtime.eval_value("state.echoOctet(NaN)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("state.echoShort(32768)").unwrap(), Value::Number(-32768.0));
        assert_eq!(runtime.eval_value("state.echoUnsignedShort(-1)").unwrap(), Value::Number(65535.0));
        assert_eq!(runtime.eval_value("state.echoLong(4294967295)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("state.echoLong(Infinity)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("state.echoLongLong(9223372036854775808)").unwrap(), Value::Number(i64::MIN as f64));
        assert_eq!(runtime.eval_value("state.echoUnsignedLongLong(-1)").unwrap(), Value::Number(u64::MAX as f64));
        assert_eq!(runtime.eval_value("state.echoFloat('1.5')").unwrap(), Value::Number(1.5));
        assert_eq!(runtime.eval_value("state.echoUnrestrictedFloat(Infinity)").unwrap(), Value::Number(f64::INFINITY));
        assert_eq!(runtime.eval("Number.isNaN(state.echoUnrestrictedFloat(NaN))").unwrap(), "true");
        for source in ["state.add(Infinity)", "state.add(Symbol())", "state.add()", "state.echoFloat(Infinity)", "state.echoFloat(1e300)"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }

        assert_eq!(runtime.eval_value("state.optionalEnabled").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.optionalRatio").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.optionalCount").unwrap(), Value::Null);
        runtime.eval("state.optionalEnabled = null; state.optionalRatio = null; state.optionalCount = null;").unwrap();
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_enabled.get(), None);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_ratio.get(), None);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_count.get(), None);

        runtime.eval("state.optionalEnabled = 'false'; state.optionalRatio = '3.25'; state.optionalCount = -2;").unwrap();
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert_eq!(native.optional_enabled.get(), Some(true));
        assert_eq!(native.optional_ratio.get(), Some(3.25));
        assert_eq!(native.optional_count.get(), Some(u32::MAX - 1));

        runtime.eval("state.optionalRatio = Symbol();").unwrap_err();
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_ratio.get(), Some(3.25));
        assert!(runtime.eval("state.optionalRatio = undefined;").unwrap_err().contains("TypeError"));
        runtime.eval("state.optionalEnabled = undefined; state.optionalCount = undefined;").unwrap();
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert_eq!(native.optional_enabled.get(), Some(false));
        assert_eq!(native.optional_ratio.get(), Some(3.25));
        assert_eq!(native.optional_count.get(), Some(0));
        assert_eq!(runtime.eval_value("state.ping()").unwrap(), Value::Undefined);
        assert_eq!(runtime.eval_value("state.isEnabled()").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("state.currentRatio()").unwrap(), Value::Number(2.5));
        assert_eq!(runtime.eval_value("state.currentCount()").unwrap(), Value::Number(1.0));
        assert_eq!(runtime.eval_value("state.accepts(true)").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.accepts(1)").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.accepts(0)").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("state.accepts()").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("state.accepts(true, false)").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.currentLabel().charCodeAt(0)").unwrap(), Value::Number(0xD800 as f64));
        assert_eq!(runtime.eval_value("state.currentLabel().charCodeAt(1)").unwrap(), Value::Number(65.0));
        assert_eq!(runtime.eval_value("state.optionalLabel()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.currentUsvLabel()").unwrap(), Value::String("v8 ?".to_owned()));
        assert_eq!(runtime.eval_value("state.optionalUsvLabel()").unwrap(), Value::String("game".to_owned()));
        assert_eq!(runtime.eval_value("state.optionalEnabledResult()").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.optionalRatioResult()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.optionalCountResult()").unwrap(), Value::Number(u32::MAX as f64));
        for (expression, expected) in [
            ("state.signedByteResult()", -7.0), ("state.octetResult()", 250.0),
            ("state.shortResult()", -300.0), ("state.unsignedShortResult()", 60_000.0),
            ("state.longResult()", -2_000_000.0), ("state.longLongResult()", i64::MAX as f64),
            ("state.unsignedLongLongResult()", u64::MAX as f64), ("state.floatResult()", 1.25),
            ("state.nullableFloatResult()", -2.5),
        ] {
            assert_eq!(runtime.eval_value(expression).unwrap(), Value::Number(expected), "{expression}");
        }
        assert_eq!(runtime.eval_value("state.nullableLongLongResult()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.currentRatio()").unwrap(), Value::Number(2.5));
        assert!(runtime.eval("state.currentUnrestrictedRatio() === Infinity").unwrap() == "true");
        assert!(runtime.eval("Number.isNaN(state.optionalUnrestrictedRatioResult())").unwrap() == "true");
        assert!(runtime.eval("state.currentUnrestrictedFloat() === Infinity").unwrap() == "true");
        assert!(runtime.eval("Number.isNaN(state.optionalUnrestrictedFloatResult())").unwrap() == "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_required_string_operation_arguments_preserve_webidl_semantics() {
        use crate::webidl::string_operations::{StringOperationsBinding, StringOperationsNative};
        struct EchoStrings;
        #[allow(non_snake_case)]
        impl StringOperationsNative for EchoStrings {
            fn EchoDom(&self, value: Vec<u16>) -> Vec<u16> { value }
            fn EchoUsv(&self, value: String) -> String { value }
        }

        let mut runtime = Runtime::new();
        let binding = StringOperationsBinding::<EchoStrings>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, EchoStrings);
        runtime.set_global_property("strings", &handle).unwrap();
        assert_eq!(runtime.eval_value("strings.echoDom(123)").unwrap(), Value::String("123".into()));
        assert_eq!(runtime.eval_value("strings.echoDom({ toString() { return 'coerced'; } })").unwrap(), Value::String("coerced".into()));
        runtime.eval("globalThis.lone = '\\uD800';").unwrap();
        assert_eq!(runtime.eval_value("strings.echoDom(lone.charAt(0))").unwrap(), Value::Utf16String(vec![0xD800]));
        assert_eq!(runtime.eval_value("strings.echoUsv(lone.charAt(0))").unwrap(), Value::String("\u{FFFD}".into()));
        assert_eq!(runtime.eval_value("strings.echoDom()").unwrap(), Value::String("undefined".into()));
        assert_eq!(runtime.eval("(() => { try { strings.echoUsv(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval("(() => { try { strings.echoDom({ toString() { throw new RangeError('coercion'); } }); } catch (e) { return e instanceof RangeError && e.message === 'coercion'; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_nullable_operation_arguments_preserve_null_and_inner_conversions() {
        use crate::webidl::nullable_operations::{NullableOperationsBinding, NullableOperationsNative};
        struct NullableValues;
        #[allow(non_snake_case)]
        impl NullableOperationsNative for NullableValues {
            fn Flag(&self, value: Option<bool>) -> Option<bool> { value }
            fn Count(&self, value: Option<i32>) -> Option<i32> { value }
            fn Label(&self, value: Option<Vec<u16>>) -> Option<Vec<u16>> { value }
            fn Name(&self, value: Option<String>) -> Option<String> { value }
            fn Mix(&self, value: Option<i32>, addend: i32) -> Option<i32> { value.map(|value| value + addend) }
        }

        let mut runtime = Runtime::new();
        let binding = NullableOperationsBinding::<NullableValues>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, NullableValues);
        runtime.set_global_property("values", &handle).unwrap();

        assert_eq!(runtime.eval_value("values.flag(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.flag(undefined)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.flag()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.flag(0)").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("values.count(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.count(undefined)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.count('4294967295')").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.label(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.label(undefined)").unwrap(), Value::Null);
        runtime.eval("globalThis.lone = '\\uD800';").unwrap();
        assert_eq!(runtime.eval_value("values.label(lone)").unwrap(), Value::Utf16String(vec![0xD800]));
        assert_eq!(runtime.eval_value("values.name(lone)").unwrap(), Value::String("\u{FFFD}".into()));
        assert_eq!(runtime.eval_value("values.name({ toString() { return 'game'; } })").unwrap(), Value::String("game".into()));
        assert_eq!(runtime.eval_value("values.mix(null, 5)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.mix(7, 5)").unwrap(), Value::Number(12.0));
        assert_eq!(runtime.eval("(() => { try { values.count(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_arguments_distinguish_missing_from_nullable_null() {
        use crate::webidl::optional_operations::{OptionalOperationsBinding, OptionalOperationsNative};
        use crate::WebIdlOptionalArgument;
        struct OptionalValues;
        #[allow(non_snake_case)]
        impl OptionalOperationsNative for OptionalValues {
            fn Classify(&self, value: WebIdlOptionalArgument<Option<i32>>) -> Option<i32> {
                match value {
                    WebIdlOptionalArgument::Missing => Some(-1),
                    WebIdlOptionalArgument::Present(None) => None,
                    WebIdlOptionalArgument::Present(Some(value)) => Some(value),
                }
            }
            fn Fallback(&self, value: WebIdlOptionalArgument<i32>) -> i32 {
                match value {
                    WebIdlOptionalArgument::Missing => -1,
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
        }

        let mut runtime = Runtime::new();
        let binding = OptionalOperationsBinding::<OptionalValues>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, OptionalValues);
        runtime.set_global_property("values", &handle).unwrap();

        assert_eq!(runtime.eval_value("values.classify()").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.classify(undefined)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.classify(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.classify('7')").unwrap(), Value::Number(7.0));
        assert_eq!(runtime.eval_value("values.fallback()").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.fallback(undefined)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.fallback(null)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval("(() => { try { values.fallback(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_bytestring_arguments_preserve_bytes_and_enforce_code_unit_range() {
        use crate::webidl::bytestring_operations::{ByteStringOperationsBinding, ByteStringOperationsNative};
        use crate::WebIdlOptionalArgument;
        struct ByteValues;
        #[allow(non_snake_case)]
        impl ByteStringOperationsNative for ByteValues {
            fn Echo(&self, value: Vec<u8>) -> Vec<u16> {
                value.into_iter().map(u16::from).collect()
            }
            fn NullableEcho(&self, value: Option<Vec<u8>>) -> Option<Vec<u16>> {
                value.map(|value| value.into_iter().map(u16::from).collect())
            }
            fn OptionalEcho(&self, value: WebIdlOptionalArgument<Option<Vec<u8>>>) -> Option<Vec<u16>> {
                match value {
                    WebIdlOptionalArgument::Missing => Some("missing".encode_utf16().collect()),
                    WebIdlOptionalArgument::Present(None) => None,
                    WebIdlOptionalArgument::Present(Some(value)) => Some(value.into_iter().map(u16::from).collect()),
                }
            }
        }

        let mut runtime = Runtime::new();
        let binding = ByteStringOperationsBinding::<ByteValues>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, ByteValues);
        runtime.set_global_property("values", &handle).unwrap();

        runtime.eval("globalThis.bytes = values.echo('A\\u00FF');").unwrap();
        assert_eq!(runtime.eval_value("bytes.charCodeAt(0)").unwrap(), Value::Number(65.0));
        assert_eq!(runtime.eval_value("bytes.charCodeAt(1)").unwrap(), Value::Number(255.0));
        assert_eq!(runtime.eval_value("values.nullableEcho(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.nullableEcho(undefined)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.optionalEcho()").unwrap(), Value::String("missing".into()));
        assert_eq!(runtime.eval_value("values.optionalEcho(undefined)").unwrap(), Value::String("missing".into()));
        assert_eq!(runtime.eval_value("values.optionalEcho(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.optionalEcho('\\u00FE')").unwrap(), Value::String("þ".into()));
        assert!(runtime.eval("values.echo('\\u0100')").unwrap_err().contains("TypeError"));
        assert!(runtime.eval("values.echo('\\uD800')").unwrap_err().contains("TypeError"));
        assert!(runtime.eval("values.echo(Symbol())").unwrap_err().contains("TypeError"));
        assert_eq!(runtime.eval("(() => { try { values.echo({ toString() { throw new RangeError('bytes'); } }); } catch (e) { return e instanceof RangeError && e.message === 'bytes'; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_defaults_apply_only_to_missing_or_undefined_arguments() {
        use crate::webidl::optional_defaults::{OptionalDefaultsBinding, OptionalDefaultsNative};
        use crate::{FiniteF64, WebIdlOptionalArgument};
        struct Defaults;
        #[allow(non_snake_case)]
        impl OptionalDefaultsNative for Defaults {
            fn BooleanDefault(&self, value: WebIdlOptionalArgument<bool>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -1, WebIdlOptionalArgument::Present(value) => i32::from(value) }
            }
            fn IntegerDefault(&self, value: WebIdlOptionalArgument<i32>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -1, WebIdlOptionalArgument::Present(value) => value }
            }
            fn UnsignedDefault(&self, value: WebIdlOptionalArgument<u32>) -> u32 {
                match value { WebIdlOptionalArgument::Missing => u32::MAX, WebIdlOptionalArgument::Present(value) => value }
            }
            fn FiniteFloatDefault(&self, value: WebIdlOptionalArgument<crate::FiniteF32>) -> crate::FiniteF32 {
                match value { WebIdlOptionalArgument::Missing => crate::FiniteF32::new(-1.0).unwrap(), WebIdlOptionalArgument::Present(value) => value }
            }
            fn FiniteDefault(&self, value: WebIdlOptionalArgument<FiniteF64>) -> FiniteF64 {
                match value { WebIdlOptionalArgument::Missing => FiniteF64::new(-1.0).unwrap(), WebIdlOptionalArgument::Present(value) => value }
            }
            fn UnrestrictedFloatDefault(&self, value: WebIdlOptionalArgument<f32>) -> f32 {
                match value { WebIdlOptionalArgument::Missing => f32::NEG_INFINITY, WebIdlOptionalArgument::Present(value) => value }
            }
            fn InfinityDefault(&self, value: WebIdlOptionalArgument<f64>) -> f64 {
                match value { WebIdlOptionalArgument::Missing => f64::NEG_INFINITY, WebIdlOptionalArgument::Present(value) => value }
            }
            fn NullableDefault(&self, value: WebIdlOptionalArgument<Option<i32>>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -2, WebIdlOptionalArgument::Present(None) => -1, WebIdlOptionalArgument::Present(Some(value)) => value }
            }
            fn NullableValueDefault(&self, value: WebIdlOptionalArgument<Option<i32>>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -2, WebIdlOptionalArgument::Present(None) => -1, WebIdlOptionalArgument::Present(Some(value)) => value }
            }
        }
        let mut runtime = Runtime::new();
        let binding = OptionalDefaultsBinding::<Defaults>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, Defaults);
        runtime.set_global_property("defaults", &handle).unwrap();
        assert_eq!(runtime.eval_value("defaults.booleanDefault()").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.booleanDefault(undefined)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.booleanDefault(true)").unwrap(), Value::Number(1.0));
        assert_eq!(runtime.eval_value("defaults.booleanDefault(null)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault()").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault(undefined)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault(9)").unwrap(), Value::Number(9.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault(null)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.unsignedDefault()").unwrap(), Value::Number(6.0));
        assert_eq!(runtime.eval_value("defaults.finiteFloatDefault()").unwrap(), Value::Number(2.5));
        assert_eq!(runtime.eval_value("defaults.finiteDefault()").unwrap(), Value::Number(1.5));
        assert_eq!(runtime.eval_value("defaults.unrestrictedFloatDefault()").unwrap(), Value::Number(2.5));
        assert_eq!(runtime.eval_value("defaults.infinityDefault() ").unwrap(), Value::Number(f64::INFINITY));
        assert_eq!(runtime.eval_value("defaults.nullableDefault()").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableDefault(undefined)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableDefault(null)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableDefault(4)").unwrap(), Value::Number(4.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault()").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault(undefined)").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault(null)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault(4)").unwrap(), Value::Number(4.0));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_string_defaults_preserve_domstring_usvstring_and_bytestring() {
        use crate::webidl::optional_string_defaults::{
            OptionalStringDefaultsBinding, OptionalStringDefaultsNative,
        };
        use crate::WebIdlOptionalArgument;
        struct StringDefaults;
        #[allow(non_snake_case)]
        impl OptionalStringDefaultsNative for StringDefaults {
            fn Dom(&self, value: WebIdlOptionalArgument<Vec<u16>>) -> Vec<u16> {
                match value {
                    WebIdlOptionalArgument::Missing => Vec::new(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Usv(&self, value: WebIdlOptionalArgument<String>) -> String {
                match value {
                    WebIdlOptionalArgument::Missing => String::new(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Unicode(&self, value: WebIdlOptionalArgument<Vec<u16>>) -> Vec<u16> {
                match value {
                    WebIdlOptionalArgument::Missing => Vec::new(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Nullable(&self, value: WebIdlOptionalArgument<Option<Vec<u16>>>) -> Option<Vec<u16>> {
                match value {
                    WebIdlOptionalArgument::Missing => None,
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Bytes(&self, value: WebIdlOptionalArgument<Vec<u8>>) -> Vec<u16> {
                match value {
                    WebIdlOptionalArgument::Missing => Vec::new(),
                    WebIdlOptionalArgument::Present(value) => value.into_iter().map(u16::from).collect(),
                }
            }
        }
        let mut runtime = Runtime::new();
        let binding = OptionalStringDefaultsBinding::<StringDefaults>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, StringDefaults);
        runtime.set_global_property("defaults", &handle).unwrap();
        let escaped_dom = Value::String("line\\nquote".into());
        assert_eq!(runtime.eval_value("defaults.dom()").unwrap(), escaped_dom);
        assert_eq!(runtime.eval_value("defaults.dom(undefined)").unwrap(), Value::String("line\\nquote".into()));
        assert_eq!(runtime.eval_value("defaults.dom('explicit')").unwrap(), Value::String("explicit".into()));
        assert_eq!(runtime.eval_value("defaults.usv()").unwrap(), Value::String("rocket \u{1f680}".into()));
        assert_eq!(runtime.eval_value("defaults.unicode()").unwrap(), Value::String("rocket \u{1f680}".into()));
        assert_eq!(runtime.eval_value("defaults.nullable()").unwrap(), Value::String("seed".into()));
        assert_eq!(runtime.eval_value("defaults.nullable(undefined)").unwrap(), Value::String("seed".into()));
        assert_eq!(runtime.eval_value("defaults.nullable(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("defaults.nullable('explicit')").unwrap(), Value::String("explicit".into()));
        assert_eq!(runtime.eval_value("defaults.bytes()").unwrap(), Value::String("abc".into()));
        assert_eq!(runtime.eval_value("defaults.bytes(undefined)").unwrap(), Value::String("abc".into()));
        assert_eq!(runtime.eval_value("defaults.bytes('XYZ')").unwrap(), Value::String("XYZ".into()));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_nullable_string_defaults_distinguish_missing_null_and_values() {
        use crate::webidl::optional_nullable_string_defaults::{
            OptionalNullableStringDefaultsBinding, OptionalNullableStringDefaultsNative,
        };
        use crate::WebIdlOptionalArgument;
        struct NullableDefaults;
        #[allow(non_snake_case)]
        impl OptionalNullableStringDefaultsNative for NullableDefaults {
            fn Dom(&self, value: WebIdlOptionalArgument<Option<Vec<u16>>>) -> i32 {
                match value {
                    WebIdlOptionalArgument::Missing | WebIdlOptionalArgument::Present(None) => 0,
                    WebIdlOptionalArgument::Present(Some(value)) => value.len() as i32,
                }
            }
            fn Usv(&self, value: WebIdlOptionalArgument<Option<String>>) -> i32 {
                match value {
                    WebIdlOptionalArgument::Missing | WebIdlOptionalArgument::Present(None) => 0,
                    WebIdlOptionalArgument::Present(Some(value)) => value.len() as i32,
                }
            }
            fn Bytes(&self, value: WebIdlOptionalArgument<Option<Vec<u8>>>) -> i32 {
                match value {
                    WebIdlOptionalArgument::Missing | WebIdlOptionalArgument::Present(None) => 0,
                    WebIdlOptionalArgument::Present(Some(value)) => value.len() as i32,
                }
            }
        }
        let mut runtime = Runtime::new();
        let binding = OptionalNullableStringDefaultsBinding::<NullableDefaults>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, NullableDefaults);
        runtime.set_global_property("nullableDefaults", &handle).unwrap();
        for method in ["dom", "usv", "bytes"] {
            assert_eq!(runtime.eval_value(&format!("nullableDefaults.{method}()")).unwrap(), Value::Number(0.0));
            assert_eq!(runtime.eval_value(&format!("nullableDefaults.{method}(undefined)")).unwrap(), Value::Number(0.0));
            assert_eq!(runtime.eval_value(&format!("nullableDefaults.{method}(null)")).unwrap(), Value::Number(0.0));
            assert_eq!(runtime.eval_value(&format!("nullableDefaults.{method}('abc')")).unwrap(), Value::Number(3.0));
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_enum_arguments_validate_values_and_apply_optional_defaults() {
        use crate::webidl::enum_operations::{EnumOperationsBinding, EnumOperationsNative};
        use crate::WebIdlOptionalArgument;
        struct DirectionNative;
        #[allow(non_snake_case)]
        impl EnumOperationsNative for DirectionNative {
            fn Echo(&self, value: String) -> String { value }
            fn OptionalValue(&self, value: WebIdlOptionalArgument<String>) -> String {
                match value {
                    WebIdlOptionalArgument::Missing => "missing".into(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
        }
        let mut runtime = Runtime::new();
        let binding = EnumOperationsBinding::<DirectionNative>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, DirectionNative);
        runtime.set_global_property("directions", &handle).unwrap();

        assert_eq!(runtime.eval_value("directions.echo('left')").unwrap(), Value::String("left".into()));
        assert_eq!(runtime.eval_value("directions.echo({toString() { return 'right'; }})").unwrap(), Value::String("right".into()));
        assert_eq!(runtime.eval_value("directions.optionalValue()").unwrap(), Value::String("right".into()));
        assert_eq!(runtime.eval_value("directions.optionalValue(undefined)").unwrap(), Value::String("right".into()));
        assert_eq!(runtime.eval_value("directions.optionalValue('left')").unwrap(), Value::String("left".into()));
        assert_eq!(runtime.eval("(() => { try { directions.echo('up'); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval("(() => { try { directions.echo(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_inherited_interfaces_share_one_native_type_and_prototype_chain() {
        use crate::webidl::inheritance_base::{InheritanceBaseBinding, InheritanceBaseNative};
        use crate::webidl::inheritance_derived::{InheritanceDerivedBinding, InheritanceDerivedNative};
        // One native type per inheritance tree, as with Servo's `D: DomTypes`: the base
        // callbacks downcast to the same type, whichever interface created the wrapper.
        enum Shape {
            Base { depth: u32, flagged: std::cell::Cell<bool> },
            Derived { depth: u32, flagged: std::cell::Cell<bool>, ratio: f64 },
        }
        #[allow(non_snake_case)]
        impl InheritanceBaseNative for Shape {
            fn Depth(&self) -> u32 {
                match self { Shape::Base { depth, .. } | Shape::Derived { depth, .. } => *depth }
            }
            fn Flagged(&self) -> bool {
                match self { Shape::Base { flagged, .. } | Shape::Derived { flagged, .. } => flagged.get() }
            }
            fn set_Flagged(&self, value: bool) {
                match self { Shape::Base { flagged, .. } | Shape::Derived { flagged, .. } => flagged.set(value) }
            }
            fn IsBase(&self) -> bool { matches!(self, Shape::Base { .. }) }
        }
        #[allow(non_snake_case)]
        impl InheritanceDerivedNative for Shape {
            fn Ratio(&self) -> crate::FiniteF64 {
                match self {
                    Shape::Derived { ratio, .. } => crate::FiniteF64::new(*ratio).unwrap(),
                    Shape::Base { .. } => unreachable!("the derived signature rejects base receivers"),
                }
            }
            fn DoubledDepth(&self) -> u32 { self.Depth() * 2 }
        }

        let mut runtime = Runtime::new();
        let base_binding = InheritanceBaseBinding::<Shape>::install(&mut runtime).unwrap();
        let derived_binding = InheritanceDerivedBinding::<Shape>::install(&mut runtime, &base_binding).unwrap();
        let base = base_binding.create(&mut runtime, Shape::Base { depth: 1, flagged: std::cell::Cell::new(false) });
        let derived = derived_binding.create(&mut runtime, Shape::Derived { depth: 3, flagged: std::cell::Cell::new(false), ratio: 0.5 });
        runtime.set_global_property("base", &base).unwrap();
        runtime.set_global_property("derived", &derived).unwrap();

        for (source, expected) in [
            ("Object.getPrototypeOf(InheritanceDerived.prototype) === InheritanceBase.prototype", "true"),
            ("Object.getPrototypeOf(InheritanceDerived) === InheritanceBase", "true"),
            ("derived instanceof InheritanceBase && derived instanceof InheritanceDerived", "true"),
            ("base instanceof InheritanceDerived", "false"),
            // Inherited members read the derived instance's own native state.
            ("derived.depth", "3"),
            ("derived.isBase()", "false"),
            ("base.isBase()", "true"),
            ("derived.ratio", "0.5"),
            ("derived.doubledDepth()", "6"),
            ("Object.hasOwn(InheritanceDerived.prototype, 'depth')", "false"),
            ("Object.hasOwn(InheritanceBase.prototype, 'depth')", "true"),
            ("'ratio' in base", "false"),
            ("Object.prototype.toString.call(derived)", "[object InheritanceDerived]"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        runtime.eval("derived.flagged = true;").unwrap();
        assert_eq!(runtime.eval("derived.flagged && !base.flagged").unwrap(), "true");
        // Derived members keep rejecting receivers that are only a base instance.
        for source in [
            "InheritanceDerived.prototype.doubledDepth.call(base)",
            "Object.getOwnPropertyDescriptor(InheritanceDerived.prototype, 'ratio').get.call(base)",
        ] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_constructors_build_natives_and_chain_with_inheritance() {
        use crate::webidl::constructible_child::{ConstructibleChildBinding, ConstructibleChildNative};
        use crate::webidl::constructible_counter::{ConstructibleCounterBinding, ConstructibleCounterNative};
        use crate::WebIdlOptionalArgument;
        struct Counter {
            value: std::cell::Cell<u32>,
            label: Vec<u16>,
            child: bool,
        }
        impl crate::Trace for Counter {
            fn trace(&self, _tracer: &mut crate::Tracer) {}
        }
        #[allow(non_snake_case)]
        impl ConstructibleCounterNative for Counter {
            fn Constructor(start: u32, label: WebIdlOptionalArgument<Vec<u16>>) -> Self {
                let WebIdlOptionalArgument::Present(label) = label else {
                    unreachable!("the declared default always supplies a label")
                };
                Counter { value: std::cell::Cell::new(start), label, child: false }
            }
            fn Value(&self) -> u32 { self.value.get() }
            fn Label(&self) -> Vec<u16> { self.label.clone() }
            fn Increment(&self) -> u32 {
                self.value.set(self.value.get() + 1);
                self.value.get()
            }
        }
        // Both traits declare `Constructor`; the generated code calls each one qualified.
        #[allow(non_snake_case)]
        impl ConstructibleChildNative for Counter {
            fn Constructor() -> Self {
                Counter { value: std::cell::Cell::new(100), label: "child".encode_utf16().collect(), child: true }
            }
            fn Child(&self) -> bool { self.child }
        }

        let mut runtime = Runtime::new();
        let counter = ConstructibleCounterBinding::<Counter>::install(&mut runtime).unwrap();
        let _child = ConstructibleChildBinding::<Counter>::install(&mut runtime, &counter).unwrap();
        for (source, expected) in [
            ("[ConstructibleCounter.length, ConstructibleChild.length].join()", "1,0"),
            ("const a = new ConstructibleCounter(5); [a.value, a.label, a.increment(), a.value].join()", "5,counter,6,6"),
            ("const b = new ConstructibleCounter('7', 'named'); [b.value, b.label].join()", "7,named"),
            ("const c = new ConstructibleChild(); [c.value, c.label, c.child, c.increment()].join()", "100,child,true,101"),
            ("c instanceof ConstructibleCounter && Object.getPrototypeOf(ConstructibleChild) === ConstructibleCounter", "true"),
            ("'child' in a", "false"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        for source in ["ConstructibleCounter(1)", "new ConstructibleCounter(Symbol())"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_throws_members_raise_the_native_error_as_a_js_exception() {
        use crate::webidl::throwing_operations::{ThrowingOperationsBinding, ThrowingOperationsNative};
        use crate::WebIdlError;
        struct Parser;
        impl crate::Trace for Parser {
            fn trace(&self, _tracer: &mut crate::Tracer) {}
        }
        #[allow(non_snake_case)]
        impl ThrowingOperationsNative for Parser {
            fn Constructor(allow: bool) -> Result<Self, WebIdlError> {
                if allow { Ok(Parser) } else { Err(WebIdlError::TypeError("construction refused".into())) }
            }
            fn Parse(&self, text: Vec<u16>) -> Result<u32, WebIdlError> {
                let text = String::from_utf16_lossy(&text);
                if text == "dom" {
                    return Err(WebIdlError::DomException {
                        name: "InvalidStateError".into(),
                        message: "not now".into(),
                    });
                }
                let value: u64 = text.parse().map_err(|_| WebIdlError::TypeError(format!("not a number: {text}")))?;
                u32::try_from(value).map_err(|_| WebIdlError::RangeError("too large".into()))
            }
            fn Reset(&self, fail: bool) -> Result<(), WebIdlError> {
                if fail { Err(WebIdlError::RangeError("reset failed".into())) } else { Ok(()) }
            }
        }

        let mut runtime = Runtime::new();
        let _binding = ThrowingOperationsBinding::<Parser>::install(&mut runtime).unwrap();
        let caught = |runtime: &mut Runtime, source: &str| {
            runtime
                .eval(&format!("(() => {{ try {{ {source}; return 'no exception'; }} catch (e) {{ return [e.constructor.name, e.name, e.message].join(); }} }})()"))
                .unwrap()
        };
        runtime.eval("var parser = new ThrowingOperations(true);").unwrap();
        assert_eq!(runtime.eval("[parser.parse('12'), parser.reset(false)].join()").unwrap(), "12,");
        assert_eq!(caught(&mut runtime, "new ThrowingOperations(false)"), "TypeError,TypeError,construction refused");
        assert_eq!(caught(&mut runtime, "parser.parse('x')"), "TypeError,TypeError,not a number: x");
        assert_eq!(caught(&mut runtime, "parser.parse('99999999999')"), "RangeError,RangeError,too large");
        assert_eq!(caught(&mut runtime, "parser.reset(true)"), "RangeError,RangeError,reset failed");
        // Without a DOMException interface in the realm, the name is still observable.
        assert_eq!(caught(&mut runtime, "parser.parse('dom')"), "Error,InvalidStateError,not now");
        // With one, the runtime constructs it like `new DOMException(message, name)`.
        runtime
            .eval("globalThis.DOMException = class DOMException extends Error { constructor(message, name) { super(message); this.name = name; } };")
            .unwrap();
        assert_eq!(caught(&mut runtime, "parser.parse('dom')"), "DOMException,InvalidStateError,not now");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_unforgeable_attributes_are_non_configurable_own_properties() {
        use crate::webidl::unforgeable_base::{UnforgeableBaseBinding, UnforgeableBaseNative};
        use crate::webidl::unforgeable_child::{UnforgeableChildBinding, UnforgeableChildNative};
        struct Item {
            trusted: bool,
            child: bool,
        }
        #[allow(non_snake_case)]
        impl UnforgeableBaseNative for Item {
            fn Trusted(&self) -> bool { self.trusted }
            fn Ordinary(&self) -> bool { true }
        }
        // The child trait does not redeclare `Trusted`: the parser's copy of the unforgeable
        // member is skipped, and the base installs it on the inherited instance template.
        #[allow(non_snake_case)]
        impl UnforgeableChildNative for Item {
            fn Child(&self) -> bool { self.child }
        }
        let mut runtime = Runtime::new();
        let base_binding = UnforgeableBaseBinding::<Item>::install(&mut runtime).unwrap();
        let child_binding = UnforgeableChildBinding::<Item>::install(&mut runtime, &base_binding).unwrap();
        let base = base_binding.create(&mut runtime, Item { trusted: true, child: false });
        let child = child_binding.create(&mut runtime, Item { trusted: false, child: true });
        runtime.set_global_property("base", &base).unwrap();
        runtime.set_global_property("child", &child).unwrap();
        for (source, expected) in [
            ("[base.trusted, child.trusted, child.child, child.ordinary].join()", "true,false,true,true"),
            ("[Object.hasOwn(base, 'trusted'), Object.hasOwn(child, 'trusted')].join()", "true,true"),
            ("Object.hasOwn(UnforgeableBase.prototype, 'trusted')", "false"),
            ("Object.hasOwn(base, 'ordinary')", "false"),
            ("const d = Object.getOwnPropertyDescriptor(child, 'trusted'); [typeof d.get, d.set, d.enumerable, d.configurable].join()", "function,,true,false"),
            ("delete child.trusted", "false"),
            ("(() => { try { Object.defineProperty(base, 'trusted', { value: false }); return 'redefined'; } catch (e) { return e.constructor.name; } })()", "TypeError"),
            ("child.trusted", "false"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_constants_are_frozen_on_interface_object_and_prototype() {
        use crate::webidl::web_idl_constants::{WebIdlConstantsBinding, WebIdlConstantsNative};
        struct Phase;
        #[allow(non_snake_case)]
        impl WebIdlConstantsNative for Phase {
            fn Phase(&self) -> u16 { 1 }
        }
        let mut runtime = Runtime::new();
        let binding = WebIdlConstantsBinding::<Phase>::install(&mut runtime).unwrap();
        let instance = binding.create(&mut runtime, Phase);
        runtime.set_global_property("instance", &instance).unwrap();
        for (source, expected) in [
            ("[WebIdlConstants.NONE, WebIdlConstants.CAPTURING_PHASE, WebIdlConstants.LARGEST, WebIdlConstants.NEGATIVE].join()", "0,1,4294967295,-5"),
            ("[WebIdlConstants.POSITIVE_INFINITY, Number.isNaN(WebIdlConstants.NOT_A_NUMBER), WebIdlConstants.ENABLED].join()", "Infinity,true,true"),
            ("instance.phase === instance.CAPTURING_PHASE && WebIdlConstants.prototype.NONE === 0", "true"),
            ("Object.hasOwn(instance, 'NONE')", "false"),
            ("['NONE', 'LARGEST'].map(name => [WebIdlConstants, WebIdlConstants.prototype].map(target => { const d = Object.getOwnPropertyDescriptor(target, name); return [d.writable, d.enumerable, d.configurable].join(); }).join('|')).join(' ')", "false,true,false|false,true,false false,true,false|false,true,false"),
            ("'use strict'; (() => { try { WebIdlConstants.NONE = 5; return 'assigned'; } catch (e) { return e.constructor.name; } })()", "TypeError"),
            ("WebIdlConstants.NONE", "0"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_bindings_wrap_traced_natives_through_an_inheritance_tree() {
        use crate::webidl::inheritance_base::{InheritanceBaseBinding, InheritanceBaseNative};
        use crate::webidl::inheritance_derived::{InheritanceDerivedBinding, InheritanceDerivedNative};
        use crate::{Trace, Tracer};
        struct Shape {
            depth: u32,
            flagged: std::cell::Cell<bool>,
        }
        impl Trace for Shape {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl InheritanceBaseNative for Shape {
            fn Depth(&self) -> u32 { self.depth }
            fn Flagged(&self) -> bool { self.flagged.get() }
            fn set_Flagged(&self, value: bool) { self.flagged.set(value); }
            fn IsBase(&self) -> bool { false }
        }
        #[allow(non_snake_case)]
        impl InheritanceDerivedNative for Shape {
            fn Ratio(&self) -> crate::FiniteF64 { crate::FiniteF64::new(0.25).unwrap() }
            fn DoubledDepth(&self) -> u32 { self.depth * 2 }
        }
        let mut runtime = Runtime::new();
        let base = InheritanceBaseBinding::<Shape>::install(&mut runtime).unwrap();
        let derived = InheritanceDerivedBinding::<Shape>::install(&mut runtime, &base).unwrap();
        let native = runtime.allocate_traced(Shape { depth: 4, flagged: std::cell::Cell::new(false) });
        let wrapper = derived.wrap_traced(&mut runtime, &native);
        runtime.set_global_property("shape", &wrapper).unwrap();
        assert_eq!(
            runtime.eval("shape.flagged = true; [shape instanceof InheritanceBase, shape.depth, shape.doubledDepth(), shape.ratio, shape.flagged].join()").unwrap(),
            "true,4,8,0.25,true"
        );
        // The setter mutated the traced native itself, which Rust observes through its root.
        assert!(native.get().flagged.get());
        let again = derived.wrap_traced(&mut runtime, &native);
        runtime.set_global_property("again", &again).unwrap();
        assert_eq!(runtime.eval("again === shape").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_interface_typed_members_pass_traced_natives_both_ways() {
        use crate::webidl::linked_leaf::{LinkedLeafBinding, LinkedLeafNative};
        use crate::webidl::linked_node::{LinkedNodeBinding, LinkedNodeNative};
        use crate::{GcMember, NativeRef, Trace, Tracer};
        struct Link {
            id: u32,
            leaf: bool,
            next: GcMember<Link>,
        }
        impl Trace for Link {
            fn trace(&self, tracer: &mut Tracer) {
                tracer.member(&self.next);
            }
        }
        fn interface_of(link: &Link) -> &'static str {
            if link.leaf { "LinkedLeaf" } else { "LinkedNode" }
        }
        #[allow(non_snake_case)]
        impl LinkedNodeNative for Link {
            fn Id(&self) -> u32 { self.id }
            fn Next(&self) -> Option<NativeRef> {
                // SAFETY: `self` is reachable (it is the receiver) and traces `next`.
                let next = unsafe { self.next.get() }?;
                unsafe { self.next.native_ref(interface_of(next)) }
            }
            fn Follow(&self, steps: u32) -> Option<NativeRef> {
                let mut current = self;
                for _ in 0..steps {
                    // SAFETY: every link on the chain is reachable from the receiver.
                    current = unsafe { current.next.get() }?;
                }
                if std::ptr::eq(current, self) {
                    return None;
                }
                // Re-derive the member pointing at `current` from its predecessor.
                let mut previous = self;
                for _ in 1..steps {
                    previous = unsafe { previous.next.get() }.unwrap();
                }
                unsafe { previous.next.native_ref(interface_of(current)) }
            }
            fn IsSame(&self, other: NativeRef) -> bool {
                other.get::<Link>().is_some_and(|other| std::ptr::eq(other, self))
            }
            fn IdOr(&self, other: Option<NativeRef>, fallback: u32) -> u32 {
                other.and_then(|other| other.get::<Link>().map(|link| link.id)).unwrap_or(fallback)
            }
        }
        #[allow(non_snake_case)]
        impl LinkedLeafNative for Link {
            fn Leaf(&self) -> bool { self.leaf }
        }

        let mut runtime = Runtime::new();
        let node = LinkedNodeBinding::<Link>::install(&mut runtime).unwrap();
        let leaf_binding = LinkedLeafBinding::<Link>::install(&mut runtime, &node).unwrap();
        // first -> second -> leaf
        let leaf = runtime.allocate_traced(Link { id: 3, leaf: true, next: GcMember::empty() });
        let second = runtime.allocate_traced(Link { id: 2, leaf: false, next: leaf.member() });
        let first = runtime.allocate_traced(Link { id: 1, leaf: false, next: second.member() });
        let first_wrapper = node.wrap_traced(&mut runtime, &first);
        runtime.set_global_property("first", &first_wrapper).unwrap();
        // The leaf already has a wrapper; the others get theirs on first return.
        let leaf_wrapper = leaf_binding.wrap_traced(&mut runtime, &leaf);
        runtime.set_global_property("leaf", &leaf_wrapper).unwrap();

        for (source, expected) in [
            ("[first.next.id, first.next.next.id, first.next.next.next].join()", "2,3,"),
            // A returned native reuses its one wrapper, so identity and expandos hold.
            ("first.next === first.next && first.next.next === leaf", "true"),
            ("first.next.tag = 'kept'; first.follow(1).tag", "kept"),
            // A wrapper created on demand uses the native's concrete interface.
            ("[first.follow(2) instanceof LinkedLeaf, first.follow(2).leaf, first.next instanceof LinkedLeaf].join()", "true,true,false"),
            ("first.follow(0) === null", "true"),
            // Arguments: the receiver itself, a descendant, and null for a nullable parameter.
            ("[first.isSame(first), first.isSame(leaf), first.idOr(leaf, 9), first.idOr(null, 9)].join()", "true,false,3,9"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // An untraced (create_instance) wrapper of the same interface is an API wrapper without
        // a traced native: it must be rejected, not misread.
        let untraced = node.create(&mut runtime, Link { id: 9, leaf: false, next: GcMember::empty() });
        runtime.set_global_property("untraced", &untraced).unwrap();
        assert_eq!(runtime.eval("untraced.id").unwrap(), "9");
        // Anything that is not a traced wrapper of the interface (or a descendant) is rejected
        // before the native runs, including prototype spoofing.
        for source in [
            "first.isSame(untraced)",
            "first.isSame({})",
            "first.isSame(Object.create(LinkedNode.prototype))",
            "first.isSame(null)",
            "first.isSame(1)",
            "first.isSame()",
        ] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
        drop((first_wrapper, leaf_wrapper, first, second, leaf));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_bindings_honour_pref_secure_context_and_no_interface_object() {
        use crate::webidl::exposure_gated::{ExposureGatedBinding, ExposureGatedNative};
        use crate::webidl::hidden_interface::{HiddenInterfaceBinding, HiddenInterfaceNative};
        use crate::{ExposeAll, Exposure};
        struct Gated;
        #[allow(non_snake_case)]
        impl ExposureGatedNative for Gated {
            fn Always(&self) -> bool { true }
            fn Extra(&self) -> bool { true }
            fn Secret(&self) -> bool { true }
        }
        struct Hidden;
        #[allow(non_snake_case)]
        impl HiddenInterfaceNative for Hidden {
            fn Visible(&self) -> bool { true }
        }
        /// Only the extra member's pref is on, and the realm is not a secure context.
        struct Restricted;
        impl Exposure for Restricted {
            fn pref_enabled(&self, name: &str) -> bool { name == "dom_gated_extra_enabled" }
            fn is_secure_context(&self) -> bool { false }
        }

        let mut runtime = Runtime::new();
        let gated = ExposureGatedBinding::<Gated>::install_with(&mut runtime, &Restricted).unwrap();
        let hidden = HiddenInterfaceBinding::<Hidden>::install(&mut runtime).unwrap();
        let gated_instance = gated.create(&mut runtime, Gated);
        let hidden_instance = hidden.create(&mut runtime, Hidden);
        runtime.set_global_property("gated", &gated_instance).unwrap();
        runtime.set_global_property("hidden", &hidden_instance).unwrap();
        for (source, expected) in [
            // The interface's own pref is off: no interface object, but natives still wrap.
            ("[typeof ExposureGated, 'ExposureGated' in globalThis, gated.always].join()", "undefined,false,true"),
            // Member-level conditions: the extra pref is on, the secure-context method is not.
            ("['extra' in gated, gated.extra, 'secret' in gated].join()", "true,true,false"),
            ("[typeof HiddenInterface, hidden.visible, Object.prototype.toString.call(hidden)].join()", "undefined,true,[object HiddenInterface]"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }

        // With everything exposed, the same binding defines everything.
        let mut runtime = Runtime::new();
        let gated = ExposureGatedBinding::<Gated>::install_with(&mut runtime, &ExposeAll).unwrap();
        let instance = gated.create(&mut runtime, Gated);
        runtime.set_global_property("gated", &instance).unwrap();
        assert_eq!(
            runtime.eval("[typeof ExposureGated, gated.extra, gated.secret(), gated instanceof ExposureGated].join()").unwrap(),
            "function,true,true,true"
        );

        // A worker realm with every pref on: a Window-only interface has no interface object.
        struct DedicatedWorker;
        impl Exposure for DedicatedWorker {
            fn pref_enabled(&self, _name: &str) -> bool { true }
            fn is_secure_context(&self) -> bool { true }
            fn global_name(&self) -> &str { "DedicatedWorker" }
        }
        assert!(DedicatedWorker.exposed_in(&["Worker"]) && DedicatedWorker.exposed_in(&["*"]));
        assert!(!DedicatedWorker.exposed_in(&["Window"]) && !ExposeAll.exposed_in(&["Worker"]));
        let mut runtime = Runtime::new();
        let gated = ExposureGatedBinding::<Gated>::install_with(&mut runtime, &DedicatedWorker).unwrap();
        let instance = gated.create(&mut runtime, Gated);
        runtime.set_global_property("gated", &instance).unwrap();
        assert_eq!(runtime.eval("[typeof ExposureGated, gated.always].join()").unwrap(), "undefined,true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_overloads_dispatch_on_argument_count() {
        use crate::webidl::overloaded_operations::{OverloadedOperationsBinding, OverloadedOperationsNative};
        use crate::{WebIdlError, WebIdlOptionalArgument};
        struct Ops(std::cell::Cell<u32>);
        #[allow(non_snake_case)]
        impl OverloadedOperationsNative for Ops {
            fn Describe(&self) -> Vec<u16> { "none".encode_utf16().collect() }
            fn Describe_(&self, count: u32, loud: WebIdlOptionalArgument<bool>) -> Vec<u16> {
                let WebIdlOptionalArgument::Present(loud) = loud else { unreachable!("declared default") };
                let text = format!("{count}{}", if loud { "!" } else { "" });
                text.encode_utf16().collect()
            }
            fn Measure(&self, text: Vec<u16>) -> u32 { text.len() as u32 }
            fn Measure_(&self, text: Vec<u16>, scale: u32, offset: u32) -> u32 { text.len() as u32 * scale + offset }
            fn Reset(&self) -> Result<(), WebIdlError> { self.0.set(0); Ok(()) }
            fn Reset_(&self, hard: bool, deep: bool) -> Result<(), WebIdlError> {
                if hard && deep { Err(WebIdlError::RangeError("too much".into())) } else { self.0.set(1); Ok(()) }
            }
        }
        let mut runtime = Runtime::new();
        let binding = OverloadedOperationsBinding::<Ops>::install(&mut runtime).unwrap();
        let ops = binding.create(&mut runtime, Ops(std::cell::Cell::new(5)));
        runtime.set_global_property("ops", &ops).unwrap();
        for (source, expected) in [
            ("[ops.describe(), ops.describe(3), ops.describe(3, true), ops.describe('4', 0)].join()", "none,3,3!,4"),
            // Extra arguments beyond the longest overload are ignored, as WebIDL requires.
            ("ops.describe(2, false, 'ignored', 9)", "2"),
            ("[ops.measure('abc'), ops.measure('abc', 2, 1)].join()", "3,7"),
            ("[ops.reset(), ops.reset(true, false)].join()", ","),
            ("[OverloadedOperations.prototype.describe.length, OverloadedOperations.prototype.measure.length].join()", "0,1"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // No overload takes two arguments (or none) here, and a throwing overload still throws.
        for source in ["ops.measure()", "ops.measure('a', 2)", "ops.reset(true)", "ops.reset(true, true)"] {
            let error = runtime.eval(&format!("(() => {{ try {{ {source}; return 'none'; }} catch (e) {{ return e.constructor.name; }} }})()")).unwrap();
            let expected = if source.ends_with("(true, true)") { "RangeError" } else { "TypeError" };
            assert_eq!(error, expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_ce_reactions_members_run_inside_the_reaction_hook() {
        use crate::webidl::ce_reactive::{CeReactiveBinding, CeReactiveNative};
        use crate::{CeReactions, WebIdlError};
        thread_local! {
            static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
            static LOG: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        fn log(entry: String) {
            LOG.with(|log| log.borrow_mut().push(entry));
        }
        struct Element {
            title: std::cell::RefCell<Vec<u16>>,
        }
        impl CeReactions for Element {
            fn with_ce_reactions<R>(run: impl FnOnce() -> R) -> R {
                DEPTH.with(|depth| depth.set(depth.get() + 1));
                let result = run();
                DEPTH.with(|depth| depth.set(depth.get() - 1));
                log("reactions".into());
                result
            }
        }
        #[allow(non_snake_case)]
        impl CeReactiveNative for Element {
            fn Title(&self) -> Vec<u16> {
                log(format!("get title at depth {}", DEPTH.with(std::cell::Cell::get)));
                self.title.borrow().clone()
            }
            fn set_Title(&self, value: Vec<u16>) {
                log(format!("set title at depth {}", DEPTH.with(std::cell::Cell::get)));
                *self.title.borrow_mut() = value;
            }
            fn Touch(&self) {
                log(format!("touch at depth {}", DEPTH.with(std::cell::Cell::get)));
            }
            fn Fail(&self) -> Result<(), WebIdlError> {
                log(format!("fail at depth {}", DEPTH.with(std::cell::Cell::get)));
                Err(WebIdlError::TypeError("failed".into()))
            }
            fn ObservedDepth(&self) -> u32 {
                DEPTH.with(std::cell::Cell::get)
            }
        }
        let mut runtime = Runtime::new();
        let binding = CeReactiveBinding::<Element>::install(&mut runtime).unwrap();
        let element = binding.create(&mut runtime, Element { title: std::cell::RefCell::new(Vec::new()) });
        runtime.set_global_property("element", &element).unwrap();
        runtime.eval("element.title = 'hello'; element.title; element.touch(); try { element.fail(); } catch (e) {}").unwrap();
        assert_eq!(runtime.eval("[element.title, element.observedDepth].join()").unwrap(), "hello,0");
        let entries = LOG.with(|log| log.borrow().clone());
        assert_eq!(
            entries[..8],
            [
                "set title at depth 1", "reactions",
                "get title at depth 1", "reactions",
                "touch at depth 1", "reactions",
                "fail at depth 1", "reactions",
            ]
        );
        // The hook unwinds even when the member throws, and plain members run outside it.
        assert_eq!(DEPTH.with(std::cell::Cell::get), 0);
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_contextual_operations_call_back_into_js() {
        use crate::webidl::callback_operations::{CallbackOperationsBinding, CallbackOperationsNative};
        use crate::{Handle, ScriptContext, WebIdlError};
        struct Ops(std::cell::Cell<u32>, std::cell::Cell<u32>);
        #[allow(non_snake_case)]
        impl CallbackOperationsNative for Ops {
            fn Total(&self) -> u32 { self.1.get() }
            fn set_Total(&self, value: u32) { self.1.set(value); }
            fn Apply(&self, cx: &mut ScriptContext, callback: Handle, value: u32) -> Result<u32, WebIdlError> {
                // Any exception the callback throws propagates unchanged.
                let result = cx.call(&callback, &Value::Undefined, &[Value::Number(value as f64)])?;
                let Value::Js(result) = result else { unreachable!("calls return JS values") };
                match cx.value(&result) {
                    Value::Number(number) => Ok(number as u32),
                    _ => Err(WebIdlError::TypeError("callback must return a number".into())),
                }
            }
            fn Echo(&self, _cx: &mut ScriptContext, value: Handle) -> Result<Handle, WebIdlError> {
                Ok(value)
            }
            fn IsObject(&self, cx: &mut ScriptContext, value: Handle) -> Result<bool, WebIdlError> {
                Ok(matches!(cx.value(&value), Value::Js(_)))
            }
            fn CountCalls(&self, cx: &mut ScriptContext, callback: Option<Handle>) -> Result<u32, WebIdlError> {
                if let Some(callback) = callback {
                    cx.call(&callback, &Value::Undefined, &[Value::Number(0.0)])?;
                    self.0.set(self.0.get() + 1);
                }
                Ok(self.0.get())
            }
        }
        let mut runtime = Runtime::new();
        let binding = CallbackOperationsBinding::<Ops>::install(&mut runtime).unwrap();
        let ops = binding.create(&mut runtime, Ops(std::cell::Cell::new(0), std::cell::Cell::new(0)));
        runtime.set_global_property("ops", &ops).unwrap();
        for (source, expected) in [
            ("ops.apply(x => x * 2, 21)", "42"),
            // `any` keeps object identity and primitives alike.
            ("const o = {}; [ops.echo(o) === o, ops.echo(5), ops.echo(undefined) === undefined].join()", "true,5,true"),
            ("[ops.isObject([]), ops.isObject(() => 1)].join()", "true,true"),
            ("[ops.countCalls(null), ops.countCalls(() => 0), ops.countCalls(() => 0)].join()", "0,1,2"),
            // A callback's exception is rethrown as-is, not converted.
            ("class Custom extends Error {}; try { ops.apply(() => { throw new Custom('boom'); }, 1) } catch (e) { [e instanceof Custom, e.message].join() }", "true,boom"),
            // The callback runs with re-entrant access to the same native.
            ("ops.apply(x => ops.countCalls(() => 0) + x, 10)", "13"),
            // A setter re-entered from inside a native call is sound: natives only ever get
            // shared references and mutate through interior mutability.
            ("ops.apply(x => { ops.total = x + 1; return ops.total; }, 6) + ops.total", "14"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        for source in ["ops.apply(1, 1)", "ops.isObject(1)", "ops.apply({}, 1)", "ops.countCalls('x')"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_event_handler_and_any_attributes_store_traced_js_values() {
        use crate::webidl::handler_host::{HandlerHostBinding, HandlerHostNative};
        use crate::{Handle, JsRef, ScriptContext, Trace, Tracer, WebIdlError};
        use std::cell::RefCell;
        /// Stores its JS values as traced references, the GC-correct pattern: a handler that
        /// closes over the host's own wrapper forms a collectable cycle.
        struct Host {
            onping: RefCell<Option<JsRef>>,
            data: RefCell<Option<JsRef>>,
        }
        impl Trace for Host {
            fn trace(&self, tracer: &mut Tracer) {
                for slot in [&self.onping, &self.data] {
                    if let Some(reference) = &*slot.borrow() {
                        tracer.js(reference);
                    }
                }
            }
        }
        fn load(cx: &mut ScriptContext, slot: &RefCell<Option<JsRef>>) -> Option<Handle> {
            slot.borrow().as_ref().and_then(|reference| cx.js_ref_value(reference))
        }
        #[allow(non_snake_case)]
        impl HandlerHostNative for Host {
            fn Onping(&self, cx: &mut ScriptContext) -> Result<Option<Handle>, WebIdlError> {
                Ok(load(cx, &self.onping))
            }
            fn set_Onping(&self, cx: &mut ScriptContext, value: Option<Handle>) -> Result<(), WebIdlError> {
                *self.onping.borrow_mut() = value.map(|value| cx.js_ref(&value));
                Ok(())
            }
            fn Data(&self, cx: &mut ScriptContext) -> Result<Handle, WebIdlError> {
                Ok(load(cx, &self.data).unwrap_or_else(|| cx.handle(&Value::Undefined)))
            }
            fn set_Data(&self, cx: &mut ScriptContext, value: Handle) -> Result<(), WebIdlError> {
                *self.data.borrow_mut() = Some(cx.js_ref(&value));
                Ok(())
            }
            fn Shape(&self, cx: &mut ScriptContext) -> Result<Option<Handle>, WebIdlError> {
                Ok(load(cx, &self.data).filter(|data| matches!(cx.value(data), Value::Js(_))))
            }
            fn Fire(&self, cx: &mut ScriptContext, detail: Handle) -> Result<Handle, WebIdlError> {
                let Some(handler) = load(cx, &self.onping) else {
                    return Ok(cx.handle(&Value::Null));
                };
                match cx.call(&handler, &Value::Undefined, &[Value::Js(detail)])? {
                    Value::Js(result) => Ok(result),
                    other => Ok(cx.handle(&other)),
                }
            }
        }
        let mut runtime = Runtime::new();
        let binding = HandlerHostBinding::<Host>::install(&mut runtime).unwrap();
        let host = runtime.allocate_traced(Host { onping: RefCell::new(None), data: RefCell::new(None) });
        let wrapper = binding.wrap_traced(&mut runtime, &host);
        runtime.set_global_property("host", &wrapper).unwrap();
        for (source, expected) in [
            ("host.onping === null && host.fire(1) === null", "true"),
            ("const handler = d => d * 2; host.onping = handler; [host.onping === handler, host.fire(21)].join()", "true,42"),
            // [LegacyTreatNonObjectAsNull]: non-objects clear the handler instead of throwing.
            ("host.onping = 5; host.onping === null", "true"),
            ("host.onping = handler; host.onping = 'text'; host.onping", "null"),
            // A non-callable object is stored; invoking it is the TypeError.
            ("host.onping = {}; (() => { try { host.fire(0); } catch (e) { return e.constructor.name; } })()", "TypeError"),
            // `any` keeps identity; `object?` derives from it.
            ("const payload = { n: 1 }; host.data = payload; [host.data === payload, host.shape === payload].join()", "true,true"),
            ("host.data = 7; [host.data, host.shape].join()", "7,"),
            ("Object.getOwnPropertyDescriptor(HandlerHost.prototype, 'shape').set", "undefined"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // The traced references keep the handler alive through full GCs.
        runtime.eval("host.onping = (d => d + 1); host.data = { kept: true };").unwrap();
        runtime.force_full_gc_for_testing();
        runtime.force_full_gc_for_testing();
        assert_eq!(runtime.eval("[host.fire(1), host.data.kept].join()").unwrap(), "2,true");
        drop((wrapper, host));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_sequences_convert_iterables_and_return_arrays() {
        use crate::webidl::sequence_operations::{SequenceOperationsBinding, SequenceOperationsNative};
        use crate::{Handle, NativeRef, ScriptContext, Trace, Tracer, WebIdlError, WebIdlOptionalArgument};
        struct Ops(u32);
        impl Trace for Ops {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl SequenceOperationsNative for Ops {
            fn Sum(&self, values: Vec<u32>) -> u32 { values.iter().sum() }
            fn Split(&self, text: Vec<u16>) -> Vec<Vec<u16>> {
                text.split(|unit| *unit == b',' as u16).map(<[u16]>::to_vec).collect()
            }
            fn Grid(&self, size: u32) -> Vec<Vec<i32>> {
                (0..size as i32).map(|row| (0..size as i32).map(|column| row * 10 + column).collect()).collect()
            }
            fn WithHoles(&self, values: Vec<Option<i32>>) -> Vec<Option<i32>> {
                values.into_iter().map(|value| value.map(|value| -value)).collect()
            }
            fn Selves(&self, items: Vec<NativeRef>) -> Vec<NativeRef> {
                items.into_iter().filter(|item| item.get::<Ops>().is_some_and(|ops| ops.0 % 2 == 1)).collect()
            }
            fn CountFlags(&self, flags: WebIdlOptionalArgument<Vec<bool>>) -> u32 {
                let WebIdlOptionalArgument::Present(flags) = flags else { unreachable!("declared default") };
                flags.iter().filter(|flag| **flag).count() as u32
            }
            fn EchoAll(&self, _cx: &mut ScriptContext, values: Vec<Handle>) -> Result<Vec<Handle>, WebIdlError> {
                Ok(values.into_iter().rev().collect())
            }
        }
        let mut runtime = Runtime::new();
        let binding = SequenceOperationsBinding::<Ops>::install(&mut runtime).unwrap();
        let roots: Vec<_> = (1..=3).map(|index| runtime.allocate_traced(Ops(index))).collect();
        for (index, root) in roots.iter().enumerate() {
            let wrapper = binding.wrap_traced(&mut runtime, root);
            runtime.set_global_property(&format!("ops{}", index + 1), &wrapper).unwrap();
        }
        for (source, expected) in [
            ("ops1.sum([1, 2, 3])", "6"),
            // Any iterable converts, with WebIDL element coercion.
            ("ops1.sum(new Set([4, '5']))", "9"),
            ("ops1.sum((function* () { yield 1; yield 2; })())", "3"),
            ("JSON.stringify(ops1.split('a,b,,c'))", "[\"a\",\"b\",\"\",\"c\"]"),
            ("JSON.stringify(ops1.grid(2))", "[[0,1],[10,11]]"),
            ("Array.isArray(ops1.grid(1)) && Array.isArray(ops1.grid(1)[0])", "true"),
            ("JSON.stringify(ops1.withHoles([1, null, 3, undefined]))", "[-1,null,-3,null]"),
            // Interface elements are checked one by one and come back as the same wrappers.
            ("const odd = ops1.selves([ops1, ops2, ops3]); [odd.length, odd[0] === ops1, odd[1] === ops3].join()", "2,true,true"),
            ("[ops1.countFlags(), ops1.countFlags([true, false, true])].join()", "0,2"),
            ("const o = {}; const echoed = ops1.echoAll([1, o, 'x']); [echoed[0], echoed[1] === o, echoed[2]].join()", "x,true,1"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        for source in ["ops1.sum(5)", "ops1.sum({})", "ops1.sum([Symbol()])", "ops1.selves([{}])", "ops1.selves([ops1, null])"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
        // An exception thrown by a user iterator propagates unchanged.
        assert_eq!(
            runtime.eval("try { ops1.sum({ [Symbol.iterator]() { throw new RangeError('iter'); } }) } catch (e) { e.constructor.name }").unwrap(),
            "RangeError"
        );
        drop(roots);
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_dictionaries_follow_webidl_conversion_rules() {
        use crate::webidl::dictionary_probe::{BaseInit, DictionaryProbeBinding, DictionaryProbeNative, ProbeInit};
        /// An Event-like object: a type plus the flags of its init dictionary.
        struct Probe {
            r#type: Vec<u16>,
            init: BaseInit,
        }
        impl crate::Trace for Probe {
            fn trace(&self, _tracer: &mut crate::Tracer) {}
        }
        #[allow(non_snake_case)]
        impl DictionaryProbeNative for Probe {
            fn Constructor(r#type: Vec<u16>, init: BaseInit) -> Self {
                Probe { r#type, init }
            }
            fn Type(&self) -> Vec<u16> { self.r#type.clone() }
            fn Bubbles(&self) -> bool { self.init.bubbles }
            fn Cancelable(&self) -> bool { self.init.cancelable }
            fn Describe(&self, mut init: ProbeInit) -> ProbeInit {
                init.values.push(init.values.len() as i32);
                init.count = init.count.map(|count| count * 2);
                init
            }
        }
        let mut runtime = Runtime::new();
        let _binding = DictionaryProbeBinding::<Probe>::install(&mut runtime).unwrap();
        for (source, expected) in [
            // An omitted, undefined or null init dictionary takes every default.
            ("const a = new DictionaryProbe('click'); [a.type, a.bubbles, a.cancelable].join()", "click,false,false"),
            ("const b = new DictionaryProbe('x', null); [b.bubbles, b.cancelable].join()", "false,false"),
            ("const c = new DictionaryProbe('x', { bubbles: 1, unrelated: true }); [c.bubbles, c.cancelable].join()", "true,false"),
            // Members are read with getters, in WebIDL order (inherited first, then by name).
            ("const order = []; const tracked = new Proxy({ label: 'l' }, { get(target, key) { if (typeof key === 'string') order.push(key); return target[key]; } }); new DictionaryProbe('x', tracked).describe(tracked); order.join()", "bubbles,cancelable,bubbles,cancelable,count,label,nested,values"),
            // Defaults (including the nested `= {}` and `= []`), absent optional members and
            // conversion of the returned dictionary back to a plain object.
            ("JSON.stringify(c.describe({ label: 'probe' }))", "{\"bubbles\":false,\"cancelable\":false,\"label\":\"probe\",\"nested\":{\"deep\":true},\"values\":[0]}"),
            ("const d = c.describe({ label: 7, count: '4', values: new Set([5]), nested: { deep: 0 } }); [d.label, d.count, d.values.join('|'), d.nested.deep].join()", "7,8,5|1,false"),
            ("Object.getPrototypeOf(c.describe({ label: '' })) === Object.prototype", "true"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        for source in [
            // A required member, a non-object dictionary and a bad member value are TypeErrors.
            "new DictionaryProbe('x').describe({})",
            "new DictionaryProbe('x', 5)",
            "new DictionaryProbe('x').describe({ label: 'l', values: 3 })",
        ] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_callback_interfaces_and_unions_drive_an_event_target_like_binding() {
        use crate::webidl::listener_target::{
            ListenOptionsOrBoolean, ListenerTargetBinding, ListenerTargetNative, LongOrStringOrListenerTarget,
        };
        use crate::{Handle, JsRef, ScriptContext, Trace, Tracer, WebIdlError};
        use std::cell::RefCell;
        /// Listeners are traced references with their `once` flag.
        struct Target {
            listeners: RefCell<Vec<(JsRef, bool)>>,
        }
        impl Trace for Target {
            fn trace(&self, tracer: &mut Tracer) {
                for (listener, _) in self.listeners.borrow().iter() {
                    tracer.js(listener);
                }
            }
        }
        #[allow(non_snake_case)]
        impl ListenerTargetNative for Target {
            fn Constructor() -> Self {
                Target { listeners: RefCell::new(Vec::new()) }
            }
            fn Listen(&self, cx: &mut ScriptContext, listener: Option<Handle>, options: ListenOptionsOrBoolean) -> Result<(), WebIdlError> {
                let once = match options {
                    ListenOptionsOrBoolean::ListenOptions(options) => options.once,
                    // As with addEventListener, a boolean is the capture flag, not `once`.
                    ListenOptionsOrBoolean::Boolean(_) => false,
                };
                if let Some(listener) = listener {
                    let reference = cx.js_ref(&listener);
                    self.listeners.borrow_mut().push((reference, once));
                }
                Ok(())
            }
            fn Dispatch(&self, cx: &mut ScriptContext, detail: Handle) -> Result<u32, WebIdlError> {
                // Snapshot first: listeners may add listeners re-entrantly.
                let listeners: Vec<(Handle, bool)> = self
                    .listeners
                    .borrow()
                    .iter()
                    .filter_map(|(listener, once)| cx.js_ref_value(listener).map(|handle| (handle, *once)))
                    .collect();
                self.listeners.borrow_mut().retain(|(_, once)| !once);
                for (listener, _) in &listeners {
                    cx.call_user_object_operation(listener, "handleEvent", &[Value::Js(detail.clone())])?;
                }
                Ok(listeners.len() as u32)
            }
            fn Kind(&self, value: LongOrStringOrListenerTarget) -> Vec<u16> {
                let text = match value {
                    LongOrStringOrListenerTarget::Long(number) => format!("long:{number}"),
                    LongOrStringOrListenerTarget::String(text) => format!("string:{}", String::from_utf16_lossy(&text)),
                    LongOrStringOrListenerTarget::ListenerTarget(_) => "target".to_owned(),
                };
                text.encode_utf16().collect()
            }
        }
        let mut runtime = Runtime::new();
        let _binding = ListenerTargetBinding::<Target>::install(&mut runtime).unwrap();
        for (source, expected) in [
            // A function listener, an object with handleEvent (called with `this` = the object),
            // a once listener, and a null listener that is ignored.
            ("var t = new ListenerTarget(); var log = []; t.listen(d => log.push('fn:' + d)); t.listen({ handleEvent(d) { log.push('obj:' + d + ':' + (this.tag)); }, tag: 'me' }); t.listen(d => log.push('once:' + d), { once: true }); t.listen(null); [t.dispatch(1), t.dispatch(2)].join()", "3,2"),
            ("log.join()", "fn:1,obj:1:me,once:1,fn:2,obj:2:me"),
            // A boolean options value selects the boolean member; {} and undefined the dictionary.
            ("const u = new ListenerTarget(); u.listen(() => 0, true); u.listen(() => 0); u.listen(() => 0, undefined); [u.dispatch(0), u.dispatch(0)].join()", "3,3"),
            // Union selection by the WebIDL algorithm.
            ("[t.kind(5), t.kind('5'), t.kind(t), t.kind(true), t.kind({})].join()", "long:5,string:5,target,string:true,string:[object Object]"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // A listener's exception propagates; a non-callable handleEvent is a TypeError.
        assert_eq!(
            runtime.eval("const v = new ListenerTarget(); v.listen(() => { throw new RangeError('l'); }); try { v.dispatch(0) } catch (e) { e.constructor.name }").unwrap(),
            "RangeError"
        );
        assert_eq!(
            runtime.eval("const w = new ListenerTarget(); w.listen({ handleEvent: 5 }); try { w.dispatch(0) } catch (e) { e.constructor.name }").unwrap(),
            "TypeError"
        );
        // A non-object listener is rejected by the callback-interface conversion.
        assert!(runtime.eval("t.listen(5)").unwrap_err().contains("TypeError"));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_throwing_numeric_and_forwarding_attributes() {
        use crate::webidl::attribute_probe::{AttributeProbeBinding, AttributeProbeNative};
        use crate::webidl::token_probe::{TokenProbeBinding, TokenProbeNative};
        use crate::{GcMember, NativeRef, Trace, Tracer, WebIdlError};
        use std::cell::{Cell, RefCell};
        /// Both interfaces share one native type, as in Servo's DOM.
        enum Probe {
            Tokens(RefCell<Vec<u16>>),
            Owner {
                label: RefCell<Option<Vec<u16>>>,
                flag: Cell<bool>,
                small: Cell<i8>,
                big: Cell<i64>,
                ratio: Cell<f32>,
                tokens: GcMember<Probe>,
            },
        }
        impl Trace for Probe {
            fn trace(&self, tracer: &mut Tracer) {
                if let Probe::Owner { tokens, .. } = self {
                    tracer.member(tokens);
                }
            }
        }
        #[allow(non_snake_case)]
        impl TokenProbeNative for Probe {
            fn Value(&self) -> Vec<u16> {
                let Probe::Tokens(value) = self else { unreachable!() };
                value.borrow().clone()
            }
            fn set_Value(&self, value: Vec<u16>) {
                let Probe::Tokens(slot) = self else { unreachable!() };
                *slot.borrow_mut() = value;
            }
        }
        macro_rules! owner {
            ($self:ident, $field:ident) => {{
                let Probe::Owner { $field, .. } = $self else { unreachable!() };
                $field
            }};
        }
        #[allow(non_snake_case)]
        impl AttributeProbeNative for Probe {
            fn Label(&self) -> Option<Vec<u16>> { owner!(self, label).borrow().clone() }
            fn set_Label(&self, value: Option<Vec<u16>>) -> Result<(), WebIdlError> {
                if value.as_deref() == Some(&[]) {
                    return Err(WebIdlError::DomException { name: "SyntaxError".into(), message: "empty label".into() });
                }
                *owner!(self, label).borrow_mut() = value;
                Ok(())
            }
            fn Checked(&self) -> Result<u32, WebIdlError> {
                owner!(self, label).borrow().as_ref().map(|label| label.len() as u32).ok_or_else(|| WebIdlError::TypeError("no label".into()))
            }
            fn Flag(&self) -> Result<bool, WebIdlError> { Ok(owner!(self, flag).get()) }
            fn set_Flag(&self, value: bool) -> Result<(), WebIdlError> { owner!(self, flag).set(value); Ok(()) }
            fn Small(&self) -> i8 { owner!(self, small).get() }
            fn set_Small(&self, value: i8) { owner!(self, small).set(value); }
            fn Big(&self) -> i64 { owner!(self, big).get() }
            fn set_Big(&self, value: i64) { owner!(self, big).set(value); }
            fn Ratio(&self) -> f32 { owner!(self, ratio).get() }
            fn set_Ratio(&self, value: f32) { owner!(self, ratio).set(value); }
            fn Tokens(&self) -> NativeRef {
                // SAFETY: the owner is the receiver (reachable) and traces `tokens`.
                unsafe { owner!(self, tokens).native_ref("TokenProbe") }.unwrap()
            }
        }
        let mut runtime = Runtime::new();
        let _tokens = TokenProbeBinding::<Probe>::install(&mut runtime).unwrap();
        let probes = AttributeProbeBinding::<Probe>::install(&mut runtime).unwrap();
        let tokens = runtime.allocate_traced(Probe::Tokens(RefCell::new("a b".encode_utf16().collect())));
        let owner = runtime.allocate_traced(Probe::Owner {
            label: RefCell::new(None),
            flag: Cell::new(false),
            small: Cell::new(0),
            big: Cell::new(0),
            ratio: Cell::new(0.0),
            tokens: tokens.member(),
        });
        let wrapper = probes.wrap_traced(&mut runtime, &owner);
        runtime.set_global_property("probe", &wrapper).unwrap();
        let caught = |runtime: &mut Runtime, source: &str| {
            runtime.eval(&format!("(() => {{ try {{ {source}; return 'ok'; }} catch (e) {{ return e.name; }} }})()")).unwrap()
        };
        // [GetterThrows] and [SetterThrows] surface the native errors.
        assert_eq!(caught(&mut runtime, "probe.checked"), "TypeError");
        assert_eq!(caught(&mut runtime, "probe.label = ''"), "SyntaxError");
        for (source, expected) in [
            ("probe.label = 'name'; [probe.label, probe.checked].join()", "name,4"),
            ("probe.label = null; probe.label", "null"),
            ("probe.flag = 1; probe.flag", "true"),
            // WebIDL integer conversions on setters: byte wraps, long long keeps sign.
            ("probe.small = 300; probe.big = -5; [probe.small, probe.big].join()", "44,-5"),
            ("probe.ratio = 0.1; probe.ratio === Math.fround(0.1)", "true"),
            // [PutForwards=value]: assigning the attribute assigns its object's `value`.
            ("const before = probe.tokens; probe.tokens = 'x y'; [probe.tokens.value, probe.tokens === before].join()", "x y,true"),
            ("Object.getOwnPropertyDescriptor(AttributeProbe.prototype, 'tokens').set.length", "1"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        assert_eq!(String::from_utf16_lossy(&TokenProbeNative::Value(tokens.get())), "x y");
        drop((wrapper, owner, tokens));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_variadic_arguments_collect_the_remaining_arguments() {
        use crate::webidl::variadic_operations::{VariadicOperationsBinding, VariadicOperationsNative};
        struct Ops;
        #[allow(non_snake_case)]
        impl VariadicOperationsNative for Ops {
            fn Count(&self, tokens: Vec<Vec<u16>>) -> u32 { tokens.len() as u32 }
            fn Join(&self, separator: Vec<u16>, parts: Vec<Vec<u16>>) -> Vec<u16> {
                parts.join(&separator[..])
            }
            fn Sum(&self, values: Vec<i32>) -> i32 { values.iter().sum() }
        }
        let mut runtime = Runtime::new();
        let binding = VariadicOperationsBinding::<Ops>::install(&mut runtime).unwrap();
        let ops = binding.create(&mut runtime, Ops);
        runtime.set_global_property("ops", &ops).unwrap();
        for (source, expected) in [
            ("[ops.count(), ops.count('a'), ops.count('a', 'b', 'c')].join()", "0,1,3"),
            // Each variadic value is converted (ToString, ToInt32) individually.
            ("[ops.join('-'), ops.join('-', 1, true, null)].join('|')", "|1-true-null"),
            ("ops.sum(1, '2', 3.9, 2 ** 32 + 4)", "10"),
            // Undefined inside the variadic part is a value, not a missing argument.
            ("ops.count(undefined, undefined)", "2"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        assert!(runtime.eval("ops.count('a', Symbol())").unwrap_err().contains("TypeError"));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_static_operations_unscopables_and_default_to_json() {
        use crate::webidl::json_base::{JsonBaseBinding, JsonBaseNative};
        use crate::webidl::json_child::{JsonChildBinding, JsonChildNative};
        use crate::{Handle, NativeRef, ScriptContext, Trace, Tracer, WebIdlError};
        struct Item {
            id: u32,
            name: &'static str,
        }
        impl Trace for Item {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl JsonBaseNative for Item {
            fn Id(&self) -> u32 { self.id }
            fn Flag(&self) -> bool { true }
        }
        #[allow(non_snake_case)]
        impl JsonChildNative for Item {
            fn Hidden(&self, cx: &mut ScriptContext) -> Result<Handle, WebIdlError> { Ok(cx.handle(&Value::Number(1.0))) }
            fn Name(&self) -> Vec<u16> { self.name.encode_utf16().collect() }
            fn Before(&self) {}
            fn Clone(&self) -> NativeRef { unreachable!("not exercised") }
            fn Count(_cx: &mut ScriptContext, base: u32) -> Result<u32, WebIdlError> {
                base.checked_mul(2).ok_or_else(|| WebIdlError::RangeError("overflow".into()))
            }
        }
        let mut runtime = Runtime::new();
        let base = JsonBaseBinding::<Item>::install(&mut runtime).unwrap();
        let child = JsonChildBinding::<Item>::install(&mut runtime, &base).unwrap();
        let item = runtime.allocate_traced(Item { id: 7, name: "seven" });
        let wrapper = child.wrap_traced(&mut runtime, &item);
        runtime.set_global_property("item", &wrapper).unwrap();
        for (source, expected) in [
            // Static operation on the interface object, not on instances.
            ("[JsonChild.count(21), typeof item.count, JsonChild.count.length].join()", "42,undefined,1"),
            ("(() => { try { JsonChild.count(2 ** 31 + 1); } catch (e) { return e.name; } })()", "RangeError"),
            // Default toJSON: JSON-typed attributes, ancestors first; `any` is not collected.
            ("JSON.stringify(item)", "{\"id\":7,\"flag\":true,\"name\":\"seven\"}"),
            ("Object.keys(item.toJSON()).join()", "id,flag,name"),
            ("(() => { try { JsonChild.prototype.toJSON.call({}); } catch (e) { return e.name; } })()", "TypeError"),
            // @@unscopables lists the interface's and its ancestors' unscopable members.
            ("const u = JsonChild.prototype[Symbol.unscopables]; [u.before, u.flag, 'name' in u].join()", "true,true,false"),
            ("(() => { const flag = 'outer'; with (item) { return [flag, typeof before, name].join(); } })()", "outer,undefined,seven"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        drop((wrapper, item));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_namespaces_are_plain_objects_with_static_members() {
        use crate::webidl::namespace_probe::{probeBinding, probeNative};
        use crate::{Handle, ScriptContext, WebIdlError};
        struct Probe;
        #[allow(non_snake_case)]
        impl probeNative for Probe {
            fn Shout(_cx: &mut ScriptContext, text: Vec<u16>) -> Result<Vec<u16>, WebIdlError> {
                if text.is_empty() {
                    return Err(WebIdlError::TypeError("nothing to shout".into()));
                }
                Ok(String::from_utf16_lossy(&text).to_uppercase().encode_utf16().collect())
            }
            fn Count(_cx: &mut ScriptContext, items: Vec<Handle>) -> Result<u32, WebIdlError> { Ok(items.len() as u32) }
            fn Supports(_cx: &mut ScriptContext, property: Vec<u16>, _value: Vec<u16>) -> Result<bool, WebIdlError> {
                Ok(property == "color".encode_utf16().collect::<Vec<_>>())
            }
            fn Supports_(_cx: &mut ScriptContext, condition: Vec<u16>) -> Result<bool, WebIdlError> {
                Ok(condition.starts_with(&"(".encode_utf16().collect::<Vec<_>>()))
            }
            fn Kind(_cx: &mut ScriptContext, value: i32) -> Result<Vec<u16>, WebIdlError> {
                Ok(format!("long {value}").encode_utf16().collect())
            }
            fn Kind_(_cx: &mut ScriptContext, values: Vec<i32>) -> Result<Vec<u16>, WebIdlError> {
                Ok(format!("sequence {}", values.len()).encode_utf16().collect())
            }
            fn Version(_cx: &mut ScriptContext) -> Result<Vec<u16>, WebIdlError> { Ok("1.0".encode_utf16().collect()) }
            fn Gated(_cx: &mut ScriptContext) -> Result<bool, WebIdlError> { Ok(true) }
        }
        struct NoGated;
        impl crate::Exposure for NoGated {
            fn pref_enabled(&self, _pref: &str) -> bool { false }
            fn is_secure_context(&self) -> bool { true }
        }
        let mut runtime = Runtime::new();
        probeBinding::<Probe>::install_with(&mut runtime, &NoGated).unwrap();
        for (source, expected) in [
            // An ordinary object, not a function: no constructor, no prototype object.
            ("[typeof probe, Object.getPrototypeOf(probe) === Object.prototype, 'prototype' in probe].join()", "object,true,false"),
            ("[Object.prototype.toString.call(probe), Object.getOwnPropertyDescriptor(globalThis, 'probe').enumerable].join()", "[object probe],false"),
            ("(() => { try { new probe(); } catch (e) { return e.name; } })()", "TypeError"),
            // Operations: enumerable, writable, configurable data properties.
            ("const d = Object.getOwnPropertyDescriptor(probe, 'shout'); [d.enumerable, d.writable, d.configurable, probe.shout.length].join()", "true,true,true,1"),
            ("probe.shout('hey')", "HEY"),
            ("(() => { try { probe.shout(''); } catch (e) { return e.name; } })()", "TypeError"),
            ("[probe.count(), probe.count(1, 'a', {}), probe.count.length].join()", "0,3,0"),
            // Overloads: by argument count, then by type.
            ("[probe.supports('color', 'red'), probe.supports('(display: grid)'), probe.supports.length].join()", "true,true,1"),
            ("[probe.kind(5), probe.kind([1, 2])].join()", "long 5,sequence 2"),
            // Constants: read-only, and attributes are getters evaluated on every read.
            ("probe.MODE = 9; [probe.MODE, Object.getOwnPropertyDescriptor(probe, 'MODE').writable].join()", "2,false"),
            ("const v = Object.getOwnPropertyDescriptor(probe, 'version'); [typeof v.get, v.set, v.enumerable, probe.version].join()", "function,,true,1.0"),
            // [Pref]-gated members are absent when the pref is off.
            ("'gated' in probe", "false"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        let mut runtime = Runtime::new();
        probeBinding::<Probe>::install(&mut runtime).unwrap();
        assert_eq!(runtime.eval("probe.gated").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_overloaded_constructors_select_by_count_and_type() {
        use crate::webidl::overloaded_constructor::{ShapeBinding, ShapeNative};
        use crate::{Trace, Tracer};
        struct Shape(String);
        impl Trace for Shape {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl ShapeNative for Shape {
            fn Constructor() -> Self { Shape("empty".into()) }
            fn Constructor_(name: Vec<u16>) -> Self { Shape(format!("named {}", String::from_utf16_lossy(&name))) }
            fn Constructor__(sides: u32, filled: crate::WebIdlOptionalArgument<bool>) -> Self {
                let filled = match filled {
                    crate::WebIdlOptionalArgument::Present(filled) => filled.to_string(),
                    crate::WebIdlOptionalArgument::Missing => "missing".into(),
                };
                Shape(format!("{sides} sides, filled {filled}"))
            }
            fn Description(&self) -> Vec<u16> { self.0.encode_utf16().collect() }
        }
        let mut runtime = Runtime::new();
        ShapeBinding::<Shape>::install(&mut runtime).unwrap();
        for (source, expected) in [
            ("new Shape().description", "empty"),
            ("new Shape('box').description", "named box"),
            // Same argument count: the type at the distinguishing index decides.
            ("new Shape(4).description", "4 sides, filled false"),
            ("new Shape(3, true).description", "3 sides, filled true"),
            // Neither number nor string: a string overload takes any other value (ToString).
            ("new Shape({}).description", "named [object Object]"),
            ("[Shape.length, new Shape() instanceof Shape].join()", "0,true"),
            ("(() => { try { Shape(); } catch (e) { return e.name; } })()", "TypeError"),
            // Extra arguments are ignored (overload resolution truncates to the longest overload).
            ("new Shape(1, 2, 3).description", "1 sides, filled true"),
            ("(() => { try { new Shape(Symbol()); } catch (e) { return e.name; } })()", "TypeError"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_legacy_factory_functions_create_interface_instances() {
        use crate::webidl::factory_probe::{FactoryProbeBinding, FactoryProbeNative};
        use crate::{Trace, Tracer, WebIdlError, WebIdlOptionalArgument};
        struct Picture(String);
        impl Trace for Picture {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl FactoryProbeNative for Picture {
            fn Label(&self) -> Vec<u16> { self.0.encode_utf16().collect() }
            fn Picture(width: WebIdlOptionalArgument<u32>, height: WebIdlOptionalArgument<u32>) -> Result<Self, WebIdlError> {
                let size = |value: WebIdlOptionalArgument<u32>| match value {
                    WebIdlOptionalArgument::Present(value) => value.to_string(),
                    WebIdlOptionalArgument::Missing => "auto".into(),
                };
                Ok(Picture(format!("{}x{}", size(width), size(height))))
            }
            fn Snapshot(name: Vec<u16>) -> Result<Self, WebIdlError> {
                if name.is_empty() {
                    return Err(WebIdlError::DomException { name: "SyntaxError".into(), message: "empty".into() });
                }
                Ok(Picture(String::from_utf16_lossy(&name)))
            }
        }
        let mut runtime = Runtime::new();
        FactoryProbeBinding::<Picture>::install(&mut runtime).unwrap();
        for (source, expected) in [
            ("[new Picture().label, new Picture(4).label, new Picture(4, 3).label].join()", "autoxauto,4xauto,4x3"),
            // The factory creates instances of the interface, sharing its prototype object.
            ("const p = new Picture(1); [p instanceof FactoryProbe, p instanceof Picture, Object.getPrototypeOf(p) === FactoryProbe.prototype].join()", "true,true,true"),
            ("const d = Object.getOwnPropertyDescriptor(Picture, 'prototype'); [d.writable, d.enumerable, d.configurable].join()", "false,false,false"),
            ("[Picture.name, Picture.length, Snapshot.length, Object.getOwnPropertyDescriptor(globalThis, 'Picture').enumerable].join()", "Picture,0,1,false"),
            ("new Snapshot('shot').label", "shot"),
            ("(() => { try { new Snapshot(''); } catch (e) { return e.name; } })()", "SyntaxError"),
            ("(() => { try { Picture(); } catch (e) { return e.name; } })()", "TypeError"),
            // The interface itself stays nonconstructible.
            ("(() => { try { new FactoryProbe(); } catch (e) { return e.name; } })()", "TypeError"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_mutable_interface_attributes_accept_only_instances() {
        use crate::webidl::holder::{HolderBinding, HolderNative};
        use crate::{NativeRef, ScriptContext, Trace, Tracer, WebIdlError};
        use std::cell::RefCell;
        struct Holder {
            id: u32,
            next: RefCell<Option<NativeRef>>,
            current: RefCell<Option<NativeRef>>,
        }
        impl Trace for Holder {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl HolderNative for Holder {
            fn Id(&self) -> u32 { self.id }
            fn Next(&self) -> Option<NativeRef> { self.next.borrow().clone() }
            fn set_Next(&self, value: Option<NativeRef>) { *self.next.borrow_mut() = value; }
            fn Current(&self, _cx: &mut ScriptContext) -> Result<NativeRef, WebIdlError> {
                self.current.borrow().clone().ok_or_else(|| WebIdlError::TypeError("no current".into()))
            }
            fn set_Current(&self, _cx: &mut ScriptContext, value: NativeRef) -> Result<(), WebIdlError> {
                if value.get::<Holder>().is_some_and(|holder| std::ptr::eq(holder, self)) {
                    return Err(WebIdlError::DomException { name: "HierarchyRequestError".into(), message: "self".into() });
                }
                *self.current.borrow_mut() = Some(value);
                Ok(())
            }
        }
        let mut runtime = Runtime::new();
        let binding = HolderBinding::<Holder>::install(&mut runtime).unwrap();
        for (name, id) in [("a", 1), ("b", 2)] {
            let holder = runtime.allocate_traced(Holder { id, next: RefCell::new(None), current: RefCell::new(None) });
            let wrapper = binding.wrap_traced(&mut runtime, &holder);
            runtime.set_global_property(name, &wrapper).unwrap();
        }
        for (source, expected) in [
            ("[a.next, (a.next = b, a.next === b), a.next.id].join()", ",true,2"),
            ("a.next = null; a.next", "null"),
            // A non-instance is a TypeError and leaves the value unchanged.
            ("a.next = b; (() => { try { a.next = {}; } catch (e) { return e.name + ' ' + (a.next === b); } })()", "TypeError true"),
            ("(() => { try { a.next = Object.create(Holder.prototype); } catch (e) { return e.name; } })()", "TypeError"),
            // A non-nullable attribute rejects null; its setter may throw.
            ("(() => { try { a.current = null; } catch (e) { return e.name; } })()", "TypeError"),
            ("a.current = b; a.current === b", "true"),
            ("(() => { try { a.current = a; } catch (e) { return e.name + ' ' + (a.current === b); } })()", "HierarchyRequestError true"),
            ("(() => { try { return b.current; } catch (e) { return e.name; } })()", "TypeError"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_static_attributes_and_lenient_setters() {
        use crate::webidl::gate::{GateBinding, GateNative};
        use crate::{ScriptContext, WebIdlError};
        struct Gate;
        #[allow(non_snake_case)]
        impl GateNative for Gate {
            fn Limit(_cx: &mut ScriptContext) -> Result<u32, WebIdlError> { Ok(8) }
            fn Open(&self) -> bool { true }
        }
        let mut runtime = Runtime::new();
        let binding = GateBinding::<Gate>::install(&mut runtime).unwrap();
        let gate = binding.create(&mut runtime, Gate);
        runtime.set_global_property("gate", &gate).unwrap();
        for (source, expected) in [
            // A static attribute is an accessor on the interface object, not on instances.
            ("[Gate.limit, typeof gate.limit, typeof Object.getOwnPropertyDescriptor(Gate, 'limit').get].join()", "8,undefined,function"),
            ("Gate.limit = 1; Gate.limit", "8"),
            // [LegacyLenientSetter]: assigning does not throw, even in strict mode, and changes nothing.
            ("(() => { 'use strict'; gate.open = false; return gate.open; })()", "true"),
            ("typeof Object.getOwnPropertyDescriptor(Gate.prototype, 'open').set", "function"),
            ("Object.getOwnPropertyDescriptor(Gate.prototype, 'open').set.name", "set open"),
            ("(() => { try { Object.getOwnPropertyDescriptor(Gate.prototype, 'open').set.call(gate); } catch (e) { return e.name; } })()", "TypeError"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_pair_iterables_iterate_live_entries() {
        use crate::webidl::pair_list::{PairListBinding, PairListNative};
        use std::cell::RefCell;
        struct Pairs(RefCell<Vec<(String, u32)>>);
        #[allow(non_snake_case)]
        impl PairListNative for Pairs {
            fn Add(&self, key: Vec<u16>, value: u32) { self.0.borrow_mut().push((String::from_utf16_lossy(&key), value)); }
            fn get_iterable_length(&self) -> u32 { self.0.borrow().len() as u32 }
            fn get_key_at_index(&self, index: u32) -> Vec<u16> { self.0.borrow()[index as usize].0.encode_utf16().collect() }
            fn get_value_at_index(&self, index: u32) -> u32 { self.0.borrow()[index as usize].1 }
        }
        let mut runtime = Runtime::new();
        let binding = PairListBinding::<Pairs>::install(&mut runtime).unwrap();
        let list = binding.create(&mut runtime, Pairs(RefCell::new(vec![("a".into(), 1), ("b".into(), 2)])));
        runtime.set_global_property("list", &list).unwrap();
        let other = binding.create(&mut runtime, Pairs(RefCell::new(vec![])));
        runtime.set_global_property("other", &other).unwrap();
        for (source, expected) in [
            ("JSON.stringify([...list])", "[[\"a\",1],[\"b\",2]]"),
            ("[...list.keys()].join() + '|' + [...list.values()].join()", "a,b|1,2"),
            ("JSON.stringify(Array.from(list.entries()))", "[[\"a\",1],[\"b\",2]]"),
            // @@iterator is entries; iterators inherit %IteratorPrototype%.
            ("PairList.prototype[Symbol.iterator] === PairList.prototype.entries", "true"),
            ("const it = list.keys(); [Object.prototype.toString.call(it), typeof it[Symbol.iterator], it[Symbol.iterator]() === it].join()", "[object PairList Iterator],function,true"),
            ("const proto = Object.getPrototypeOf(list.keys()); [Object.getPrototypeOf(proto) === Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]())), proto === Object.getPrototypeOf(list.values())].join()", "true,true"),
            // Iteration is live: entries added mid-iteration are visited.
            ("const live = list.values(); live.next(); list.add('c', 3); [live.next().value, live.next().value, live.next().done].join()", "2,3,true"),
            // forEach(value, key, object) with thisArg.
            ("const seen = []; list.forEach(function (v, k, o) { seen.push(k + '=' + v + (o === list) + this.tag); }, { tag: '!' }); seen.join()", "a=1true!,b=2true!,c=3true!"),
            ("(() => { try { list.forEach(1); } catch (e) { return e.name; } })()", "TypeError"),
            // next() only accepts this interface's iterators; methods only its instances.
            ("(() => { try { Object.getPrototypeOf(list.keys()).next.call({}); } catch (e) { return e.name; } })()", "TypeError"),
            ("(() => { try { PairList.prototype.entries.call({}); } catch (e) { return e.name; } })()", "TypeError"),
            ("[...other].length", "0"),
            ("Object.keys(PairList.prototype).sort().join()", "add,entries,forEach,keys,values"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_records_convert_own_enumerable_properties() {
        use crate::webidl::record_probe::{RecordProbeBinding, RecordProbeNative, StringSequenceSequenceOrStringStringRecordOrString as Init};
        struct Probe;
        #[allow(non_snake_case)]
        impl RecordProbeNative for Probe {
            fn Describe(&self, counts: Vec<(Vec<u16>, i32)>) -> Vec<u16> {
                let parts: Vec<String> = counts
                    .iter()
                    .map(|(key, count)| format!("{}={count}", String::from_utf16_lossy(key)))
                    .collect();
                parts.join(",").encode_utf16().collect()
            }
            fn Tally(&self, words: Vec<String>) -> Vec<(String, u32)> {
                let mut counts: Vec<(String, u32)> = Vec::new();
                for word in words {
                    match counts.iter_mut().find(|(key, _)| *key == word) {
                        Some(entry) => entry.1 += 1,
                        None => counts.push((word, 1)),
                    }
                }
                counts
            }
            fn Groups(&self, groups: Vec<(Vec<u16>, crate::webidl::record_probe::StringOrUndefined)>) -> Vec<u16> {
                use crate::webidl::record_probe::StringOrUndefined;
                let parts: Vec<String> = groups
                    .iter()
                    .map(|(key, value)| match value {
                        StringOrUndefined::String(value) => format!("{}={}", String::from_utf16_lossy(key), String::from_utf16_lossy(value)),
                        StringOrUndefined::Undefined(()) => format!("{}=(none)", String::from_utf16_lossy(key)),
                    })
                    .collect();
                parts.join(",").encode_utf16().collect()
            }
            fn Pick(&self, init: Init) -> Vec<u16> {
                let kind = match init {
                    Init::StringSequenceSequence(_) => "sequence",
                    Init::StringStringRecord(entries) => return format!("record {}", entries.len()).encode_utf16().collect(),
                    Init::String(_) => "string",
                };
                kind.encode_utf16().collect()
            }
        }
        let mut runtime = Runtime::new();
        let binding = RecordProbeBinding::<Probe>::install(&mut runtime).unwrap();
        let probe = binding.create(&mut runtime, Probe);
        runtime.set_global_property("probe", &probe).unwrap();
        for (source, expected) in [
            ("probe.describe({ a: 1, b: '2', c: 3.7 })", "a=1,b=2,c=3"),
            // [[OwnPropertyKeys]] order: integer keys first, then strings in insertion order.
            ("probe.describe({ b: 1, 2: 2, a: 3, 1: 4 })", "1=4,2=2,b=1,a=3"),
            // Only own enumerable properties; inherited ones are ignored.
            ("const o = Object.create({ inherited: 1 }); Object.defineProperty(o, 'hidden', { value: 2 }); o.shown = 3; probe.describe(o)", "shown=3"),
            // Getters run once, in order; proxies' ownKeys/getOwnPropertyDescriptor traps run too.
            ("probe.describe(new Proxy({ x: 1, y: 2 }, { ownKeys: () => ['y', 'x'] }))", "y=2,x=1"),
            ("probe.describe({})", ""),
            ("(() => { try { probe.describe(null); } catch (e) { return e.name; } })()", "TypeError"),
            ("(() => { try { probe.describe(5); } catch (e) { return e.name; } })()", "TypeError"),
            // An enumerable symbol key cannot convert to DOMString.
            ("(() => { try { probe.describe({ [Symbol('s')]: 1 }); } catch (e) { return e.name; } })()", "TypeError"),
            // Results become plain objects.
            ("JSON.stringify(probe.tally(['x', 'y', 'x']))", "{\"x\":2,\"y\":1}"),
            ("Object.getPrototypeOf(probe.tally([])) === Object.prototype", "true"),
            // Unions: iterable objects are sequences, other objects records, the rest strings.
            ("[probe.pick([['a', 'b']]), probe.pick({ a: 'b', c: 'd' }), probe.pick('s'), probe.pick(1)].join()", "sequence,record 2,string,string"),
            // `undefined` selects the union's undefined member; anything else converts to a string.
            ("probe.groups({ a: 'x', b: undefined, c: null, d: 1 })", "a=x,b=(none),c=null,d=1"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_exception_classes_replaceable_and_secure_constructors() {
        use crate::webidl::failure_probe::{FailureProbeBinding, FailureProbeNative};
        use crate::webidl::filter_probe::FilterProbeBinding;
        use crate::{Trace, Tracer};
        use std::cell::Cell;
        struct Failure {
            message: String,
            width: Cell<u64>,
        }
        impl Trace for Failure {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl FailureProbeNative for Failure {
            fn Constructor(message: crate::WebIdlOptionalArgument<Vec<u16>>) -> Self {
                let message = match message {
                    crate::WebIdlOptionalArgument::Present(message) => String::from_utf16_lossy(&message),
                    crate::WebIdlOptionalArgument::Missing => String::new(),
                };
                Failure { message, width: Cell::new(0) }
            }
            fn Message(&self) -> Vec<u16> { self.message.encode_utf16().collect() }
            fn Origin(&self) -> Vec<u16> { "null".encode_utf16().collect() }
            fn Width(&self) -> u64 { self.width.get() }
            fn set_Width(&self, value: u64) { self.width.set(value) }
        }
        // A callback interface binding has no natives: its native type is unused.
        struct NoNative;
        impl crate::webidl::filter_probe::FilterProbeNative for NoNative {}
        struct Insecure;
        impl crate::Exposure for Insecure {
            fn pref_enabled(&self, _pref: &str) -> bool { true }
            fn is_secure_context(&self) -> bool { false }
        }
        let mut runtime = Runtime::new();
        FailureProbeBinding::<Failure>::install(&mut runtime).unwrap();
        FilterProbeBinding::<NoNative>::install(&mut runtime).unwrap();
        for (source, expected) in [
            // [ExceptionClass]: instances are Errors (but not native Error objects).
            ("const f = new FailureProbe('boom'); [f instanceof Error, f.message, Object.getPrototypeOf(FailureProbe.prototype) === Error.prototype].join()", "true,boom,true"),
            ("Object.prototype.toString.call(new FailureProbe())", "[object FailureProbe]"),
            // [Replaceable]: assignment shadows with an own data property; the accessor stays.
            ("const r = new FailureProbe(); r.origin = 5; [r.origin, Object.getOwnPropertyDescriptor(r, 'origin').writable, new FailureProbe().origin].join()", "5,true,null"),
            // [EnforceRange] attribute: out-of-range and non-finite values throw.
            ("const w = new FailureProbe(); w.width = 42; w.width", "42"),
            ("(() => { const w = new FailureProbe(); try { w.width = -1; } catch (e) { return e.name + ' ' + w.width; } })()", "TypeError 0"),
            ("(() => { const w = new FailureProbe(); try { w.width = NaN; } catch (e) { return e.name; } })()", "TypeError"),
            // A callback interface with constants: a function without prototype that throws.
            ("[typeof FilterProbe, FilterProbe.FILTER_SKIP, 'prototype' in FilterProbe].join()", "function,3,false"),
            ("(() => { try { FilterProbe(); } catch (e) { return e.name; } })()", "TypeError"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // Outside a secure context the [SecureContext] constructor is absent: not constructible.
        let mut runtime = Runtime::new();
        FailureProbeBinding::<Failure>::install_with(&mut runtime, &Insecure).unwrap();
        assert_eq!(
            runtime.eval("(() => { try { new FailureProbe(); } catch (e) { return e.name; } })()").unwrap(),
            "TypeError"
        );
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_maplike_and_setlike_follow_map_and_set() {
        use crate::webidl::score_map::{ScoreMapBinding, ScoreMapNative};
        use crate::webidl::token_set::{TokenSetBinding, TokenSetNative};
        use std::cell::RefCell;
        struct Tokens(RefCell<Vec<Vec<u16>>>);
        impl TokenSetNative for Tokens {
            fn get_index(&self, index: u32) -> Option<Vec<u16>> { self.0.borrow().get(index as usize).cloned() }
            fn size(&self) -> u32 { self.0.borrow().len() as u32 }
            fn has(&self, key: Vec<u16>) -> bool { self.0.borrow().contains(&key) }
            fn add(&self, key: Vec<u16>) {
                if !self.has(key.clone()) {
                    self.0.borrow_mut().push(key);
                }
            }
            fn delete(&self, key: Vec<u16>) -> bool {
                let before = self.size();
                self.0.borrow_mut().retain(|item| *item != key);
                before != self.size()
            }
            fn clear(&self) { self.0.borrow_mut().clear() }
        }
        struct Scores(RefCell<Vec<(Vec<u16>, i32)>>);
        impl ScoreMapNative for Scores {
            fn get_index(&self, index: u32) -> Option<(Vec<u16>, i32)> { self.0.borrow().get(index as usize).cloned() }
            fn size(&self) -> u32 { self.0.borrow().len() as u32 }
            fn has(&self, key: Vec<u16>) -> bool { self.0.borrow().iter().any(|(item, _)| *item == key) }
            fn get(&self, key: Vec<u16>) -> Option<i32> {
                self.0.borrow().iter().find(|(item, _)| *item == key).map(|(_, value)| *value)
            }
            fn set(&self, key: Vec<u16>, value: i32) {
                let mut entries = self.0.borrow_mut();
                match entries.iter_mut().find(|(item, _)| *item == key) {
                    Some(entry) => entry.1 = value,
                    None => entries.push((key, value)),
                }
            }
            fn delete(&self, key: Vec<u16>) -> bool {
                let before = self.size();
                self.0.borrow_mut().retain(|(item, _)| *item != key);
                before != self.size()
            }
            fn clear(&self) { self.0.borrow_mut().clear() }
        }
        let mut runtime = Runtime::new();
        let tokens = TokenSetBinding::<Tokens>::install(&mut runtime).unwrap();
        let scores = ScoreMapBinding::<Scores>::install(&mut runtime).unwrap();
        let set = tokens.create(&mut runtime, Tokens(RefCell::new(Vec::new())));
        runtime.set_global_property("set", &set).unwrap();
        let map = scores.create(&mut runtime, Scores(RefCell::new(Vec::new())));
        runtime.set_global_property("map", &map).unwrap();
        for (source, expected) in [
            // Setlike: add returns the set (chainable), keys/@@iterator are values.
            ("set.add('a').add('b').add('a') === set", "true"),
            ("[set.size, set.has('a'), set.has('z'), [...set].join()].join()", "2,true,false,a,b"),
            ("[TokenSet.prototype.keys === TokenSet.prototype.values, TokenSet.prototype[Symbol.iterator] === TokenSet.prototype.values].join()", "true,true"),
            ("JSON.stringify([...set.entries()])", "[[\"a\",\"a\"],[\"b\",\"b\"]]"),
            ("const seen = []; set.forEach((v, k, s) => seen.push(v + k + (s === set))); seen.join()", "aatrue,bbtrue"),
            ("[set.delete('a'), set.delete('a'), set.size].join()", "true,false,1"),
            ("set.clear(); set.size", "0"),
            ("[set.add.length, set.has.length, typeof Object.getOwnPropertyDescriptor(TokenSet.prototype, 'size').get].join()", "1,1,function"),
            ("(() => { try { Object.getOwnPropertyDescriptor(TokenSet.prototype, 'size').get.call({}); } catch (e) { return e.name; } })()", "TypeError"),
            // Maplike: get/set/has, entries/@@iterator, keys and values.
            ("map.set('x', 1).set('y', '2.9').set('x', 3) === map", "true"),
            ("[map.size, map.get('x'), map.get('y'), map.get('z'), map.has('y')].join()", "2,3,2,,true"),
            ("JSON.stringify([...map])", "[[\"x\",3],[\"y\",2]]"),
            ("[[...map.keys()].join(), [...map.values()].join(), ScoreMap.prototype[Symbol.iterator] === ScoreMap.prototype.entries].join('|')", "x,y|3,2|true"),
            ("const pairs = []; map.forEach((v, k) => pairs.push(k + '=' + v)); pairs.join()", "x=3,y=2"),
            ("[map.set.length, map.get.length].join()", "2,1"),
            // Values convert through the value type.
            ("(() => { try { map.set('s', Symbol()); } catch (e) { return e.name + map.size; } })()", "TypeError2"),
            ("[map.delete('x'), map.size, (map.clear(), map.size)].join()", "true,1,0"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_named_setters_deleters_and_override_builtins() {
        use crate::webidl::data_map_probe::{DataMapProbeBinding, DataMapProbeNative};
        use crate::webidl::storage_probe::{StorageProbeBinding, StorageProbeNative};
        use crate::WebIdlError;
        use std::cell::RefCell;
        struct Entries(RefCell<Vec<(Vec<u16>, Vec<u16>)>>);
        impl Entries {
            fn get(&self, name: &[u16]) -> Option<Vec<u16>> {
                self.0.borrow().iter().find(|(key, _)| key == name).map(|(_, value)| value.clone())
            }
            fn put(&self, name: Vec<u16>, value: Vec<u16>) {
                let mut entries = self.0.borrow_mut();
                match entries.iter_mut().find(|(key, _)| *key == name) {
                    Some(entry) => entry.1 = value,
                    None => entries.push((name, value)),
                }
            }
            fn remove(&self, name: &[u16]) { self.0.borrow_mut().retain(|(key, _)| key != name) }
            fn names(&self) -> Vec<Vec<u16>> { self.0.borrow().iter().map(|(key, _)| key.clone()).collect() }
        }
        #[allow(non_snake_case)]
        impl StorageProbeNative for Entries {
            fn Length(&self) -> u32 { self.0.borrow().len() as u32 }
            fn GetItem(&self, name: Vec<u16>) -> Option<Vec<u16>> { self.get(&name) }
            fn SetItem(&self, name: Vec<u16>, value: Vec<u16>) { self.put(name, value) }
            fn RemoveItem(&self, name: Vec<u16>) { self.remove(&name) }
            fn NamedGetter(&self, name: Vec<u16>) -> Option<Vec<u16>> { self.get(&name) }
            fn SupportedPropertyNames(&self) -> Vec<Vec<u16>> { self.names() }
            fn NamedSetter(&self, name: Vec<u16>, value: Vec<u16>) -> Result<(), WebIdlError> {
                if value == "quota".encode_utf16().collect::<Vec<_>>() {
                    return Err(WebIdlError::DomException { name: "QuotaExceededError".into(), message: "full".into() });
                }
                self.put(name, value);
                Ok(())
            }
            fn NamedDeleter(&self, name: Vec<u16>) -> Result<(), WebIdlError> {
                self.remove(&name);
                Ok(())
            }
            fn IndexedGetter(&self, index: u32) -> Option<Vec<u16>> { self.0.borrow().get(index as usize).map(|(_, value)| value.clone()) }
            fn IndexedSetter(&self, index: u32, value: Option<Vec<u16>>) -> Result<(), WebIdlError> {
                let mut entries = self.0.borrow_mut();
                let Some(entry) = entries.get_mut(index as usize) else {
                    return Err(WebIdlError::RangeError("index out of range".into()));
                };
                entry.1 = value.unwrap_or_else(|| "(null)".encode_utf16().collect());
                Ok(())
            }
        }
        #[allow(non_snake_case)]
        impl DataMapProbeNative for Entries {
            fn NamedGetter(&self, name: Vec<u16>) -> Option<Vec<u16>> { self.get(&name) }
            fn SupportedPropertyNames(&self) -> Vec<Vec<u16>> { self.names() }
            fn NamedSetter(&self, name: Vec<u16>, value: Vec<u16>) -> Result<(), WebIdlError> {
                self.put(name, value);
                Ok(())
            }
            fn NamedDeleter(&self, name: Vec<u16>) -> Result<(), WebIdlError> {
                self.remove(&name);
                Ok(())
            }
        }
        let mut runtime = Runtime::new();
        let storage = StorageProbeBinding::<Entries>::install(&mut runtime).unwrap();
        let data = DataMapProbeBinding::<Entries>::install(&mut runtime).unwrap();
        let store = storage.create(&mut runtime, Entries(RefCell::new(Vec::new())));
        runtime.set_global_property("store", &store).unwrap();
        let map = data.create(&mut runtime, Entries(RefCell::new(Vec::new())));
        runtime.set_global_property("map", &map).unwrap();
        for (source, expected) in [
            // Named setter: any string key, converted to DOMString; names are enumerable keys.
            ("store.color = 'red'; store.size = 3; [store.getItem('color'), store.size, store.length].join()", "red,3,2"),
            ("JSON.stringify(Object.keys(store)) + JSON.stringify(Object.entries(store))", "[\"color\",\"size\"][[\"color\",\"red\"],[\"size\",\"3\"]]"),
            // [[Set]] of a prototype member's name still goes to the named setter...
            ("store.setItem = 'x'; [typeof store.setItem, store.getItem('setItem')].join()", "function,x"),
            // ...but without [LegacyOverrideBuiltIns] the prototype member stays visible.
            ("Object.keys(store).includes('setItem')", "false"),
            ("(() => { try { store.big = 'quota'; } catch (e) { return e.name + ' ' + store.big; } })()", "QuotaExceededError undefined"),
            ("[delete store.color, store.color, store.getItem('color')].join()", "true,,"),
            ("store.removeItem('size'); 'size' in store", "false"),
            // Indexed setter: converts (nullable) and may throw.
            ("store[0] = 'first'; store[0]", "first"),
            ("store[0] = null; store[0]", "(null)"),
            ("(() => { try { store[9] = 'x'; } catch (e) { return e.name; } })()", "RangeError"),
            // [LegacyOverrideBuiltIns]: named properties shadow prototype members.
            ("map.toString = 'shadow'; map.hasOwnProperty = 'own'; [map.toString, Object.keys(map).join('+')].join('|')", "shadow|toString+hasOwnProperty"),
            ("[delete map.toString, typeof map.toString].join()", "true,function"),
            ("map['a-b'] = 1; Object.getOwnPropertyDescriptor(map, 'a-b').value", "1"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_unforgeable_interfaces_define_own_members() {
        use crate::webidl::location_probe::{LocationProbeBinding, LocationProbeNative};
        use crate::WebIdlError;
        use std::cell::RefCell;
        struct Location(RefCell<String>);
        #[allow(non_snake_case)]
        impl LocationProbeNative for Location {
            fn Href(&self) -> Result<String, WebIdlError> { Ok(self.0.borrow().clone()) }
            fn set_Href(&self, value: String) -> Result<(), WebIdlError> {
                if !value.contains(':') {
                    return Err(WebIdlError::TypeError("invalid URL".into()));
                }
                *self.0.borrow_mut() = value;
                Ok(())
            }
            fn Origin(&self) -> String { "https://example.test".into() }
            fn Assign(&self, url: String) -> Result<(), WebIdlError> { self.set_Href(url) }
        }
        let mut runtime = Runtime::new();
        let binding = LocationProbeBinding::<Location>::install(&mut runtime).unwrap();
        let location = binding.create(&mut runtime, Location(RefCell::new("https://example.test/a".into())));
        runtime.set_global_property("loc", &location).unwrap();
        for (source, expected) in [
            // Members are own properties of the instance, not of the prototype.
            ("['href', 'origin', 'assign', 'toString', 'valueOf'].map(name => loc.hasOwnProperty(name)).join()", "true,true,true,true,true"),
            ("['href', 'origin', 'assign'].map(name => name in LocationProbe.prototype).join()", "false,false,false"),
            // Non-configurable: they cannot be deleted or redefined.
            ("const d = Object.getOwnPropertyDescriptor(loc, 'href'); [d.configurable, d.enumerable, typeof d.get, typeof d.set].join()", "false,true,function,function"),
            ("const a = Object.getOwnPropertyDescriptor(loc, 'assign'); [a.configurable, a.writable, a.enumerable].join()", "false,false,true"),
            ("[delete loc.href, loc.href].join()", "false,https://example.test/a"),
            ("(() => { try { Object.defineProperty(loc, 'href', { value: 1 }); } catch (e) { return e.name; } })()", "TypeError"),
            // valueOf is %Object.prototype.valueOf%, non-enumerable and read-only.
            ("const v = Object.getOwnPropertyDescriptor(loc, 'valueOf'); [v.value === Object.prototype.valueOf, v.enumerable, v.writable, v.configurable].join()", "true,false,false,false"),
            // The members still work, including the stringifier and the throwing setter.
            ("loc.assign('https://example.test/b'); [String(loc), loc.toString(), loc.origin].join()", "https://example.test/b,https://example.test/b,https://example.test"),
            ("(() => { try { loc.href = 'nope'; } catch (e) { return e.name + ' ' + loc.href; } })()", "TypeError https://example.test/b"),
            // Patching the prototype cannot intercept them.
            ("LocationProbe.prototype.assign = () => 'hijacked'; loc.assign('https://example.test/c'); loc.href", "https://example.test/c"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_global_interfaces_make_the_realm_global() {
        use crate::webidl::global_base::{GlobalBaseBinding, GlobalBaseNative};
        use crate::webidl::global_scope_probe::{GlobalScopeProbeBinding, GlobalScopeProbeNative};
        use crate::webidl::window_probe::{WindowProbeBinding, WindowProbeNative};
        use crate::{NativeRef, Trace, Tracer};
        use std::cell::Cell;
        struct Window {
            counter: Cell<u32>,
            this: std::cell::RefCell<Option<NativeRef>>,
        }
        impl Trace for Window {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl GlobalBaseNative for Window {
            fn BaseName(&self) -> Vec<u16> { "base".encode_utf16().collect() }
        }
        impl GlobalScopeProbeNative for Window {}
        #[allow(non_snake_case)]
        impl WindowProbeNative for Window {
            fn Title(&self) -> Vec<u16> { "probe".encode_utf16().collect() }
            fn Counter(&self) -> u32 { self.counter.get() }
            fn set_Counter(&self, value: u32) { self.counter.set(value) }
            fn Greet(&self, who: Vec<u16>) -> Vec<u16> { format!("hello {}", String::from_utf16_lossy(&who)).encode_utf16().collect() }
            fn Self_(&self) -> NativeRef { self.this.borrow().clone().expect("self is set") }
            fn Length(&self) -> u32 { 0 }
        }
        let mut runtime = Runtime::new();
        let base = GlobalBaseBinding::<Window>::install(&mut runtime).unwrap();
        let scope = GlobalScopeProbeBinding::<Window>::install(&mut runtime, &base).unwrap();
        let window = WindowProbeBinding::<Window>::install(&mut runtime, &scope).unwrap();
        window
            .install_global(&mut runtime, Window { counter: Cell::new(0), this: std::cell::RefCell::new(None) })
            .unwrap();
        let global = runtime.global_ref().expect("a global is installed");
        *global.get::<Window>().unwrap().this.borrow_mut() = Some(global.clone());
        // The realm roots the global's native: it survives a full collection.
        runtime.force_full_gc_for_testing();
        for (source, expected) in [
            // Members of the [Global] interface are own properties of the global object.
            ("[typeof greet, greet('you'), globalThis.title, title].join()", "function,hello you,probe,probe"),
            ("const g = Object.getOwnPropertyDescriptor(globalThis, 'greet'); [g.writable, g.enumerable, g.configurable].join()", "true,true,true"),
            ("[typeof Object.getOwnPropertyDescriptor(globalThis, 'counter').get, 'greet' in WindowProbe.prototype].join()", "function,false"),
            ("counter = 5; globalThis.counter += 2; counter", "7"),
            // The global's prototype chain: WindowProbe.prototype -> GlobalBase.prototype (the
            // [Inline] GlobalScopeProbe is not part of it, and is not exposed).
            ("[Object.getPrototypeOf(globalThis) === WindowProbe.prototype, Object.getPrototypeOf(WindowProbe.prototype) === GlobalBase.prototype].join()", "true,true"),
            ("[globalThis instanceof WindowProbe, globalThis instanceof GlobalBase, typeof GlobalScopeProbe].join()", "true,true,undefined"),
            ("[baseName(), Object.prototype.toString.call(globalThis)].join()", "base,[object WindowProbe]"),
            // Interfaces are exposed in the new realm; the interface object is not constructible.
            ("[typeof WindowProbe, typeof GlobalBase].join()", "function,function"),
            ("(() => { try { new WindowProbe(); } catch (e) { return e.name; } })()", "TypeError"),
            // The native's one wrapper is the global proxy, which is what scripts see.
            ("[self === globalThis, globalThis.self === self, counter].join()", "true,true,7"),
            // [Replaceable] on the global.
            ("length = 'replaced'; length", "replaced"),
            // Receiver checks still apply to the global's own members.
            ("(() => { try { Object.getOwnPropertyDescriptor(globalThis, 'greet').value.call({}, 'x'); } catch (e) { return e.name; } })()", "TypeError"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_promise_operations_resolve_reject_and_settle_later() {
        use crate::webidl::promise_operations::{PromiseOperationsBinding, PromiseOperationsNative};
        use crate::{Handle, PromiseResolver, ScriptContext, WebIdlError};
        use std::cell::RefCell;
        struct Ops {
            pending: RefCell<Option<PromiseResolver>>,
        }
        #[allow(non_snake_case)]
        impl PromiseOperationsNative for Ops {
            fn Twice(&self, cx: &mut ScriptContext, value: u32) -> Result<Handle, WebIdlError> {
                if value > 1000 {
                    // Returned as a rejected promise, never thrown.
                    return Err(WebIdlError::RangeError("too large".into()));
                }
                let (promise, resolver) = cx.new_promise();
                cx.resolve_promise(&resolver, &Value::Number(value as f64 * 2.0));
                Ok(promise)
            }
            fn Later(&self, cx: &mut ScriptContext) -> Result<Handle, WebIdlError> {
                let (promise, resolver) = cx.new_promise();
                *self.pending.borrow_mut() = Some(resolver);
                Ok(promise)
            }
            fn Settle(&self, cx: &mut ScriptContext, succeed: bool) -> Result<(), WebIdlError> {
                if let Some(resolver) = self.pending.borrow_mut().take() {
                    if succeed {
                        cx.resolve_promise(&resolver, &Value::String("settled".into()));
                    } else {
                        cx.reject_promise(&resolver, &WebIdlError::TypeError("refused".into()));
                    }
                }
                Ok(())
            }
            fn Wrap(&self, _cx: &mut ScriptContext, input: Handle) -> Result<Handle, WebIdlError> {
                Ok(input)
            }
            fn Half(cx: &mut ScriptContext, value: u32) -> Result<Handle, WebIdlError> {
                let (promise, resolver) = cx.new_promise();
                cx.resolve_promise(&resolver, &Value::Number((value / 2) as f64));
                Ok(promise)
            }
            fn Count(&self, cx: &mut ScriptContext, text: Vec<u16>) -> Result<Handle, WebIdlError> {
                let (promise, resolver) = cx.new_promise();
                cx.resolve_promise(&resolver, &Value::Number(text.len() as f64));
                Ok(promise)
            }
            fn Count_(&self, cx: &mut ScriptContext, values: Vec<i32>) -> Result<Handle, WebIdlError> {
                let (promise, resolver) = cx.new_promise();
                cx.resolve_promise(&resolver, &Value::Number(values.len() as f64));
                Ok(promise)
            }
        }
        let mut runtime = Runtime::new();
        let binding = PromiseOperationsBinding::<Ops>::install(&mut runtime).unwrap();
        let ops = binding.create(&mut runtime, Ops { pending: RefCell::new(None) });
        runtime.set_global_property("ops", &ops).unwrap();
        assert_eq!(runtime.eval_resolved("ops.twice(21)").unwrap(), Value::Number(42.0));
        let rejection = runtime.eval_resolved("ops.twice(5000)").unwrap_err();
        assert!(rejection.contains("RangeError") && rejection.contains("too large"), "{rejection}");
        assert_eq!(runtime.eval("ops.twice(5000) instanceof Promise").unwrap(), "true");
        // An argument that fails to convert also rejects (WebIDL), instead of throwing.
        assert_eq!(runtime.eval("ops.twice(Symbol()) instanceof Promise").unwrap(), "true");
        let rejection = runtime.eval_resolved("ops.twice(Symbol())").unwrap_err();
        assert!(rejection.contains("TypeError"), "{rejection}");
        // A user valueOf that throws rejects with that very exception.
        assert!(runtime.eval_resolved("ops.twice({ valueOf() { throw new RangeError('mine'); } })").unwrap_err().contains("mine"));
        // Static and overloaded promise operations: conversion and overload-resolution errors
        // reject as well.
        assert_eq!(runtime.eval_resolved("PromiseOperations.half(9)").unwrap(), Value::Number(4.0));
        assert_eq!(runtime.eval("PromiseOperations.half(Symbol()) instanceof Promise").unwrap(), "true");
        assert!(runtime.eval_resolved("PromiseOperations.half(Symbol())").unwrap_err().contains("TypeError"));
        assert_eq!(runtime.eval_resolved("ops.count('abc')").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_resolved("ops.count([1, 2])").unwrap(), Value::Number(2.0));
        assert_eq!(runtime.eval("ops.count([Symbol()]) instanceof Promise").unwrap(), "true");
        assert!(runtime.eval_resolved("ops.count([Symbol()])").unwrap_err().contains("TypeError"));
        assert!(runtime.eval_resolved("ops.count()").unwrap_err().contains("TypeError"));
        // `Promise<any>` arguments are converted like Promise.resolve.
        assert_eq!(runtime.eval_resolved("ops.wrap(5)").unwrap(), Value::Number(5.0));
        assert_eq!(runtime.eval_resolved("ops.wrap(Promise.resolve(7))").unwrap(), Value::Number(7.0));
        // A promise settled re-entrantly from another native call...
        assert_eq!(runtime.eval_resolved("const p = ops.later(); ops.settle(true); p").unwrap(), Value::String("settled".into()));
        assert!(runtime.eval_resolved("const q = ops.later(); ops.settle(false); q").unwrap_err().contains("refused"));
        // ...and one settled later from Rust, outside any native call.
        runtime.eval("globalThis.done = 'no'; ops.later().then(value => { globalThis.done = value; });").unwrap();
        let resolver = runtime.get_wrapped::<Ops>(&ops).unwrap().pending.borrow_mut().take().unwrap();
        runtime.settle_promise(&resolver, Ok(&Value::String("async".into())));
        assert_eq!(runtime.eval("done").unwrap(), "async");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_defaults_convert_through_their_types_and_aliases_expose() {
        use crate::webidl::defaults_probe::{
            BooleanOrLatencyOptions, DefaultsProbeBinding, DefaultsProbeNative, LatencyCategoryOrDouble, LatencyOptions,
        };
        use crate::{Exposure, Handle, ScriptContext, WebIdlError};
        struct Probe;
        fn describe(options: &LatencyOptions) -> String {
            match &options.latencyHint {
                LatencyCategoryOrDouble::LatencyCategory(category) => format!("category:{category}"),
                LatencyCategoryOrDouble::Double(value) => format!("seconds:{}", value.get()),
            }
        }
        #[allow(non_snake_case)]
        impl DefaultsProbeNative for Probe {
            // `LatencyOptions` holds an `any`, so these members receive a context.
            fn Kind(&self, _cx: &mut ScriptContext, options: BooleanOrLatencyOptions) -> Result<Vec<u16>, WebIdlError> {
                let text = match options {
                    BooleanOrLatencyOptions::Boolean(flag) => format!("boolean:{flag}"),
                    BooleanOrLatencyOptions::LatencyOptions(options) => format!("options:{}", describe(&options)),
                };
                Ok(text.encode_utf16().collect())
            }
            fn Context(&self, cx: &mut ScriptContext, id: Vec<u16>, options: Handle) -> Result<Handle, WebIdlError> {
                let kind = match cx.value(&options) {
                    Value::Null => "null".to_owned(),
                    Value::Js(_) => "object".to_owned(),
                    other => format!("{other:?}"),
                };
                Ok(cx.handle(&Value::String(format!("{}:{kind}", String::from_utf16_lossy(&id)))))
            }
            fn Latency(&self, _cx: &mut ScriptContext, options: LatencyOptions) -> Result<Vec<u16>, WebIdlError> {
                Ok(describe(&options).encode_utf16().collect())
            }
        }
        let mut runtime = Runtime::new();
        let binding = DefaultsProbeBinding::<Probe>::install(&mut runtime).unwrap();
        let probe = binding.create(&mut runtime, Probe);
        runtime.set_global_property("probe", &probe).unwrap();
        for (source, expected) in [
            // `= false` on a union selects its boolean member; undefined applies it too.
            ("[probe.kind(), probe.kind(undefined), probe.kind(true), probe.kind({})].join('|')", "boolean:false|boolean:false|boolean:true|options:category:interactive"),
            // `any = null` arrives as null when omitted.
            ("[probe.context('2d'), probe.context('webgl', {})].join('|')", "2d:null|webgl:object"),
            // A union dictionary-member default goes through the union like a passed value.
            ("[probe.latency(), probe.latency({ latencyHint: 'playback' }), probe.latency({ latencyHint: 0.5 })].join('|')", "category:interactive|category:playback|seconds:0.5"),
            // [LegacyWindowAlias]: the same interface object under another global name.
            ("webkitDefaultsProbe === DefaultsProbe && probe instanceof webkitDefaultsProbe", "true"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // [Func] gates the interface object (and its aliases) like [Pref].
        struct FuncOff;
        impl Exposure for FuncOff {
            fn pref_enabled(&self, _name: &str) -> bool { true }
            fn is_secure_context(&self) -> bool { true }
            fn func_enabled(&self, function: &str) -> bool { function != "probe_enabled" }
        }
        let mut runtime = Runtime::new();
        let _binding = DefaultsProbeBinding::<Probe>::install_with(&mut runtime, &FuncOff).unwrap();
        assert_eq!(runtime.eval("[typeof DefaultsProbe, typeof webkitDefaultsProbe].join()").unwrap(), "undefined,undefined");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_buffer_sources_are_accessed_in_place() {
        use crate::webidl::buffer_operations::{ArrayBufferViewOrArrayBuffer, BufferOperationsBinding, BufferOperationsNative};
        use crate::{BufferKind, Handle, ScriptContext, WebIdlError};
        struct Ops;
        #[allow(non_snake_case)]
        impl BufferOperationsNative for Ops {
            fn Sum(&self, cx: &mut ScriptContext, data: Handle) -> Result<u32, WebIdlError> {
                Ok(cx.with_buffer_bytes(&data, |bytes| bytes.iter().map(|byte| *byte as u32).sum()).unwrap())
            }
            fn Fill(&self, cx: &mut ScriptContext, target: Handle, value: u8) -> Result<(), WebIdlError> {
                cx.with_buffer_bytes(&target, |bytes| bytes.fill(value)).unwrap();
                Ok(())
            }
            fn Copy(&self, cx: &mut ScriptContext, source: ArrayBufferViewOrArrayBuffer) -> Result<Handle, WebIdlError> {
                let (ArrayBufferViewOrArrayBuffer::ArrayBufferView(source) | ArrayBufferViewOrArrayBuffer::ArrayBuffer(source)) = source;
                let bytes = cx.with_buffer_bytes(&source, |bytes| bytes.to_vec()).unwrap();
                Ok(cx.new_array_buffer(bytes))
            }
            fn Halves(&self, cx: &mut ScriptContext, count: u32) -> Result<Handle, WebIdlError> {
                let bytes = (0..count).flat_map(|index| (index as f32 / 2.0).to_le_bytes()).collect();
                Ok(cx.new_typed_array(BufferKind::Float32Array, bytes).unwrap())
            }
        }
        let mut runtime = Runtime::new();
        let binding = BufferOperationsBinding::<Ops>::install(&mut runtime).unwrap();
        let ops = binding.create(&mut runtime, Ops);
        runtime.set_global_property("ops", &ops).unwrap();
        for (source, expected) in [
            ("ops.sum(new Uint8Array([1, 2, 3]))", "6"),
            // A view covers only its own range of the underlying buffer.
            ("const shared = new Uint8Array([9, 1, 1, 9]); ops.sum(new Uint8Array(shared.buffer, 1, 2))", "2"),
            ("ops.sum(new DataView(new Uint8Array([4, 4]).buffer))", "8"),
            // Writes land in the live JS memory, limited to the view's range.
            ("const target = new Uint8Array(4); ops.fill(target.subarray(1, 3), 7); target.join()", "0,7,7,0"),
            // A BufferSource union accepts views and buffers; results are fresh buffers.
            ("const copy = ops.copy(new Uint8Array([5, 6]).buffer); [copy instanceof ArrayBuffer, new Uint8Array(copy).join()].join('|')", "true|5,6"),
            ("new Uint8Array(ops.copy(new Uint16Array([258]))).join()", "2,1"),
            ("const halves = ops.halves(3); [halves instanceof Float32Array, halves.join()].join('|')", "true|0,0.5,1"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        for source in ["ops.sum([1, 2])", "ops.sum(new ArrayBuffer(2))", "ops.fill(new Int8Array(2), 1)", "ops.copy('text')"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_overloads_resolve_by_the_distinguishing_argument_type() {
        use crate::webidl::draw_probe::{DrawProbeBinding, DrawProbeNative};
        use crate::webidl::path_probe::{PathProbeBinding, PathProbeNative};
        use crate::{NativeRef, Trace, Tracer, WebIdlOptionalArgument};
        struct Path;
        impl Trace for Path {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        impl PathProbeNative for Path {
            fn Constructor() -> Self { Path }
        }
        struct Draw;
        impl Trace for Draw {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        fn text(value: String) -> Vec<u16> { value.encode_utf16().collect() }
        fn rule(rule: WebIdlOptionalArgument<String>) -> String {
            let WebIdlOptionalArgument::Present(rule) = rule else { unreachable!("declared default") };
            rule
        }
        #[allow(non_snake_case)]
        impl DrawProbeNative for Draw {
            fn Constructor() -> Self { Draw }
            fn Fill(&self, fill_rule: WebIdlOptionalArgument<String>) -> Vec<u16> { text(format!("rule:{}", rule(fill_rule))) }
            fn Fill_(&self, path: NativeRef, fill_rule: WebIdlOptionalArgument<String>) -> Vec<u16> {
                text(format!("path:{}:{}", path.interface(), rule(fill_rule)))
            }
            fn Measure(&self, value: Vec<u16>) -> Vec<u16> { text(format!("text:{}", String::from_utf16_lossy(&value))) }
            fn Measure_(&self, count: u32) -> Vec<u16> { text(format!("count:{count}")) }
            fn Draw(&self, points: Vec<i32>) -> Vec<u16> { text(format!("points:{}", points.len())) }
            fn Draw_(&self, label: Vec<u16>) -> Vec<u16> { text(format!("label:{}", String::from_utf16_lossy(&label))) }
        }
        let mut runtime = Runtime::new();
        let _paths = PathProbeBinding::<Path>::install(&mut runtime).unwrap();
        let _draws = DrawProbeBinding::<Draw>::install(&mut runtime).unwrap();
        for (source, expected) in [
            // Count first: no arguments selects the overload that accepts zero.
            ("const d = new DrawProbe(); d.fill()", "rule:nonzero"),
            // One argument: both overloads accept it; the platform object picks the Path one.
            ("[d.fill('evenodd'), d.fill(new PathProbe()), d.fill(new PathProbe(), 'evenodd')].join('|')", "rule:evenodd|path:PathProbe:nonzero|path:PathProbe:evenodd"),
            // String versus number, with the string fallback for other primitives.
            ("[d.measure('abc'), d.measure(3), d.measure(true)].join('|')", "text:abc|count:3|text:true"),
            // An iterable selects the sequence overload; anything else falls back to the string.
            ("[d.draw([1, 2, 3]), d.draw(new Set([1])), d.draw('x'), d.draw({})].join('|')", "points:3|points:1|label:x|label:[object Object]"),
            ("DrawProbe.prototype.fill.length", "0"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        // The selected overload still converts its own arguments.
        assert!(runtime.eval("d.fill('sideways')").unwrap_err().contains("TypeError"));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_union_and_buffer_attributes_convert_through_their_types() {
        use crate::webidl::gradient_probe::{GradientProbeBinding, GradientProbeNative};
        use crate::webidl::style_probe::{StringOrGradientProbe, StyleProbeBinding, StyleProbeNative, UnsignedLongOrString};
        use crate::{BufferKind, Handle, ScriptContext, Trace, Tracer, WebIdlError};
        use std::cell::RefCell;
        struct Gradient;
        impl Trace for Gradient {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        impl GradientProbeNative for Gradient {
            fn Constructor() -> Self { Gradient }
        }
        /// Holds the fill style as given (the gradient as a traced-native reference).
        struct Style {
            fill: RefCell<StringOrGradientProbe>,
            dash: RefCell<Option<UnsignedLongOrString>>,
        }
        impl Trace for Style {
            fn trace(&self, _tracer: &mut Tracer) {}
        }
        #[allow(non_snake_case)]
        impl StyleProbeNative for Style {
            fn Constructor() -> Self {
                Style { fill: RefCell::new(StringOrGradientProbe::String("#000000".encode_utf16().collect())), dash: RefCell::new(None) }
            }
            fn FillStyle(&self) -> StringOrGradientProbe { self.fill.borrow().clone() }
            fn set_FillStyle(&self, value: StringOrGradientProbe) { *self.fill.borrow_mut() = value; }
            fn LineDash(&self) -> Option<UnsignedLongOrString> { self.dash.borrow().clone() }
            fn set_LineDash(&self, value: Option<UnsignedLongOrString>) { *self.dash.borrow_mut() = value; }
            fn Pixels(&self, cx: &mut ScriptContext) -> Result<Handle, WebIdlError> {
                Ok(cx.new_typed_array(BufferKind::Uint8Array, vec![1, 2, 3, 4]).unwrap())
            }
        }
        let mut runtime = Runtime::new();
        let _gradients = GradientProbeBinding::<Gradient>::install(&mut runtime).unwrap();
        let _styles = StyleProbeBinding::<Style>::install(&mut runtime).unwrap();
        for (source, expected) in [
            ("const ctx = new StyleProbe(); ctx.fillStyle", "#000000"),
            ("ctx.fillStyle = 'red'; ctx.fillStyle", "red"),
            // A platform object selects the interface member and comes back as the same object.
            ("const g = new GradientProbe(); ctx.fillStyle = g; ctx.fillStyle === g", "true"),
            // Other values fall back to the string member, as in browsers.
            ("ctx.fillStyle = 42; ctx.fillStyle", "42"),
            // A nullable union: null clears it, numbers and strings select their members.
            ("ctx.lineDash = 5; const a = ctx.lineDash; ctx.lineDash = '5'; const b = typeof ctx.lineDash; ctx.lineDash = null; [a, b, ctx.lineDash].join()", "5,string,"),
            ("const p = ctx.pixels; [p instanceof Uint8Array, p.join()].join('|')", "true|1,2,3,4"),
            ("Object.getOwnPropertyDescriptor(StyleProbe.prototype, 'pixels').set", "undefined"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_indexed_getters_and_value_iterables_behave_like_node_lists() {
        use crate::webidl::item_list::{ItemListBinding, ItemListNative};
        use crate::webidl::item_list_child::{ItemListChildBinding, ItemListChildNative};
        struct Items(Vec<&'static str>);
        #[allow(non_snake_case)]
        impl ItemListNative for Items {
            fn Item(&self, index: u32) -> Option<Vec<u16>> { self.IndexedGetter(index) }
            fn Length(&self) -> u32 { self.0.len() as u32 }
            fn IndexedGetter(&self, index: u32) -> Option<Vec<u16>> {
                self.0.get(index as usize).map(|item| item.encode_utf16().collect())
            }
        }
        #[allow(non_snake_case)]
        impl ItemListChildNative for Items {
            fn Child(&self) -> bool { true }
        }
        let mut runtime = Runtime::new();
        let lists = ItemListBinding::<Items>::install(&mut runtime).unwrap();
        let children = ItemListChildBinding::<Items>::install(&mut runtime, &lists).unwrap();
        let list = lists.create(&mut runtime, Items(vec!["a", "b", "c"]));
        let child = children.create(&mut runtime, Items(vec!["x", "y"]));
        runtime.set_global_property("list", &list).unwrap();
        runtime.set_global_property("child", &child).unwrap();
        for (source, expected) in [
            // Supported indices read through the getter; others are absent, not null.
            ("[list[0], list[2], list[3], list.item(3)].join('|')", "a|c||"),
            ("list[3] === undefined && list.item(3) === null && list.length === 3", "true"),
            // Value iterable: Array.prototype's iteration methods over the indexed getter.
            ("[...list].join()", "a,b,c"),
            ("const seen = []; for (const item of list) seen.push(item); seen.join()", "a,b,c"),
            ("const each = []; list.forEach((item, index) => each.push(index + item)); each.join()", "0a,1b,2c"),
            ("JSON.stringify([...list.entries()]) + [...list.keys()].join('')", "[[0,\"a\"],[1,\"b\"],[2,\"c\"]]012"),
            ("ItemList.prototype[Symbol.iterator] === Array.prototype.values", "true"),
            ("Object.getOwnPropertyDescriptor(ItemList.prototype, Symbol.iterator).enumerable", "false"),
            // A descendant inherits the iteration methods and gets its own indexed interceptor.
            ("[child[1], [...child].join(), child.child].join('|')", "y|x,y|true"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_named_getters_follow_the_named_property_visibility_rule() {
        use crate::webidl::named_collection::{NamedCollectionBinding, NamedCollectionNative};
        /// Elements by id; every name, including "length" and "toString", is supported.
        struct Collection(Vec<(&'static str, &'static str)>);
        #[allow(non_snake_case)]
        impl NamedCollectionNative for Collection {
            fn Item(&self, index: u32) -> Option<Vec<u16>> { self.IndexedGetter(index) }
            fn Length(&self) -> u32 { self.0.len() as u32 }
            fn IndexedGetter(&self, index: u32) -> Option<Vec<u16>> {
                self.0.get(index as usize).map(|(_, value)| value.encode_utf16().collect())
            }
            fn NamedItem(&self, name: Vec<u16>) -> Option<Vec<u16>> { self.NamedGetter(name) }
            fn NamedGetter(&self, name: Vec<u16>) -> Option<Vec<u16>> {
                let name = String::from_utf16_lossy(&name);
                self.0.iter().find(|(id, _)| *id == name).map(|(_, value)| value.encode_utf16().collect())
            }
            fn SupportedPropertyNames(&self) -> Vec<Vec<u16>> {
                self.0.iter().map(|(id, _)| id.encode_utf16().collect()).collect()
            }
        }
        let mut runtime = Runtime::new();
        let binding = NamedCollectionBinding::<Collection>::install(&mut runtime).unwrap();
        let collection = binding.create(&mut runtime, Collection(vec![("alpha", "A"), ("length", "L"), ("toString", "T")]));
        runtime.set_global_property("c", &collection).unwrap();
        for (source, expected) in [
            ("[c.alpha, c['alpha'], c.namedItem('alpha'), c[0]].join()", "A,A,A,A"),
            ("c.missing === undefined && c.namedItem('missing') === null", "true"),
            // Real properties on the object or its prototypes are never shadowed by names.
            ("[c.length, typeof c.toString].join()", "3,function"),
            // [LegacyUnenumerableNamedProperties]: names are own keys, but not enumerable;
            // names shadowed by the prototype chain are not own keys at all.
            ("Object.keys(c).includes('alpha')", "false"),
            ("[Object.getOwnPropertyNames(c).includes('alpha'), Object.getOwnPropertyNames(c).includes('toString'), 'alpha' in c].join()", "true,false,true"),
            // Without a deleter a visible name cannot be deleted.
            ("[delete c.alpha, c.alpha].join()", "false,A"),
            // Without a named setter, assigning a supported name is ignored (sloppy) or a
            // TypeError (strict); other names become ordinary expandos.
            ("c.alpha = 'own'; c.alpha", "A"),
            ("'use strict'; (() => { try { c.alpha = 'x'; return 'set'; } catch (e) { return e.name; } })()", "TypeError"),
            ("c.beta = 'own'; c.beta", "own"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_lenient_this_and_html_constructor() {
        use crate::webidl::lenient_probe::{LenientProbeBinding, LenientProbeNative};
        use crate::{Handle, ScriptContext, WebIdlError};
        use std::cell::RefCell;
        struct Probe(RefCell<Option<Handle>>);
        #[allow(non_snake_case)]
        impl LenientProbeNative for Probe {
            fn Onmouseenter(&self, _cx: &mut ScriptContext) -> Result<Option<Handle>, WebIdlError> { Ok(self.0.borrow().clone()) }
            fn set_Onmouseenter(&self, _cx: &mut ScriptContext, value: Option<Handle>) -> Result<(), WebIdlError> {
                *self.0.borrow_mut() = value;
                Ok(())
            }
            fn Onclick(&self, _cx: &mut ScriptContext) -> Result<Option<Handle>, WebIdlError> { Ok(None) }
            fn set_Onclick(&self, _cx: &mut ScriptContext, _value: Option<Handle>) -> Result<(), WebIdlError> { Ok(()) }
        }
        let mut runtime = Runtime::new();
        let binding = LenientProbeBinding::<Probe>::install(&mut runtime).unwrap();
        let probe = binding.create(&mut runtime, Probe(RefCell::new(None)));
        runtime.set_global_property("probe", &probe).unwrap();
        // A wrapper of another native type exercises the non-panicking downcast.
        let other = runtime.create_wrapped(42_u32);
        runtime.set_global_property("other", &other).unwrap();
        for (source, expected) in [
            ("const f = () => 1; probe.onmouseenter = f; probe.onmouseenter === f", "true"),
            // [LegacyLenientThis]: invalid receivers read undefined and ignore writes.
            ("const d = Object.getOwnPropertyDescriptor(LenientProbe.prototype, 'onmouseenter'); [d.get.call({}), d.get.call(other), d.set.call({}, f)].join()", ",,"),
            // Without it, an invalid receiver is still a TypeError.
            ("(() => { try { Object.getOwnPropertyDescriptor(LenientProbe.prototype, 'onclick').get.call({}); } catch (e) { return e.name; } })()", "TypeError"),
            // [HTMLConstructor] without a custom element definition: `new` is a TypeError.
            ("(() => { try { new LenientProbe(); } catch (e) { return e.name; } })()", "TypeError"),
            ("probe instanceof LenientProbe", "true"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_redeclared_members_dispatch_to_their_own_native_methods() {
        use crate::webidl::shadow_base::{ShadowBaseBinding, ShadowBaseNative};
        use crate::webidl::shadow_child::{ShadowChildBinding, ShadowChildNative};
        struct Node;
        // One native type implements both traits' `Label` (like HTMLElement/HTMLScriptElement
        // `innerText`); fully qualified generated calls keep them apart.
        #[allow(non_snake_case)]
        impl ShadowBaseNative for Node {
            fn Label(&self) -> Vec<u16> { "base".encode_utf16().collect() }
        }
        #[allow(non_snake_case)]
        impl ShadowChildNative for Node {
            fn Label(&self) -> Vec<u16> { "child".encode_utf16().collect() }
        }
        let mut runtime = Runtime::new();
        let base = ShadowBaseBinding::<Node>::install(&mut runtime).unwrap();
        let child = ShadowChildBinding::<Node>::install(&mut runtime, &base).unwrap();
        let node = child.create(&mut runtime, Node);
        runtime.set_global_property("node", &node).unwrap();
        assert_eq!(
            runtime.eval("[node.label, Object.getOwnPropertyDescriptor(ShadowBase.prototype, 'label').get.call(node)].join()").unwrap(),
            "child,base"
        );
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_enums_unions_bytestrings_and_annotated_integers() {
        use crate::webidl::type_gaps_probe::{StringOrUnsignedLong, TypeGapsProbeBinding, TypeGapsProbeNative};
        use crate::{Handle, ScriptContext, WebIdlError};
        use std::cell::RefCell;
        struct Probe {
            state: RefCell<String>,
            maybe: RefCell<Option<String>>,
        }
        #[allow(non_snake_case)]
        impl TypeGapsProbeNative for Probe {
            fn State(&self) -> String { self.state.borrow().clone() }
            fn set_State(&self, value: String) { *self.state.borrow_mut() = value; }
            fn MaybeState(&self) -> Option<String> { self.maybe.borrow().clone() }
            fn set_MaybeState(&self, value: Option<String>) { *self.maybe.borrow_mut() = value; }
            fn Raw(&self) -> Vec<u8> { vec![104, 105, 255] }
            fn Next(&self) -> String { "closed".to_owned() }
            fn Pick(&self, as_text: bool) -> Option<StringOrUnsignedLong> {
                Some(if as_text { StringOrUnsignedLong::String("seven".encode_utf16().collect()) } else { StringOrUnsignedLong::UnsignedLong(7) })
            }
            fn Header(&self, present: bool) -> Option<Vec<u8>> { present.then(|| b"text/html".to_vec()) }
            fn Clamp(&self, value: u8) -> u8 { value }
            fn Strict(&self, value: i32) -> i32 { value }
            fn Filtered(&self, cx: &mut ScriptContext, filter: Option<Handle>) -> Result<u32, WebIdlError> {
                let Some(filter) = filter else { return Ok(0) };
                match cx.call_user_object_operation(&filter, "acceptNode", &[Value::Null])? {
                    Value::Js(result) => match cx.value(&result) {
                        Value::Number(number) => Ok(number as u32),
                        _ => Ok(0),
                    },
                    _ => Ok(0),
                }
            }
        }
        let mut runtime = Runtime::new();
        let binding = TypeGapsProbeBinding::<Probe>::install(&mut runtime).unwrap();
        let probe = binding.create(&mut runtime, Probe { state: RefCell::new("running".into()), maybe: RefCell::new(None) });
        runtime.set_global_property("p", &probe).unwrap();
        for (source, expected) in [
            // Enumeration attributes: valid values set, invalid ones are silently ignored.
            ("p.state = 'suspended'; p.state = 'bogus'; p.state", "suspended"),
            ("p.maybeState = 'closed'; const a = p.maybeState; p.maybeState = null; [a, p.maybeState].join()", "closed,"),
            ("p.next()", "closed"),
            // ByteString attribute and nullable ByteString result.
            ("[p.raw.length, p.raw.charCodeAt(2), p.header(true), p.header(false)].join()", "3,255,text/html,"),
            // A nullable union result.
            ("[p.pick(true), p.pick(false), typeof p.pick(false)].join()", "seven,7,number"),
            // [Clamp]: clamp and round half to even; [EnforceRange]: truncate in range.
            ("[p.clamp(300), p.clamp(-3), p.clamp(2.5), p.clamp(3.5), p.clamp(NaN)].join()", "255,0,2,4,0"),
            ("[p.strict(5.9), p.strict(-2147483648)].join()", "5,-2147483648"),
            // A nullable callback interface with a `= null` default.
            ("[p.filtered(), p.filtered(null), p.filtered({ acceptNode: () => 3 }), p.filtered(() => 2)].join()", "0,0,3,2"),
        ] {
            assert_eq!(runtime.eval(source).unwrap(), expected, "{source}");
        }
        for source in ["p.strict(2 ** 31)", "p.strict(Infinity)", "p.strict(NaN)"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_nullable_domstring_preserves_null_and_utf16_semantics() {
        use crate::webidl::nullable_domstring::{NullableDomStringBinding, NullableDomStringNative};
        #[derive(Default)]
        struct State(std::cell::RefCell<Option<Vec<u16>>>);
        #[allow(non_snake_case)]
        impl NullableDomStringNative for State {
            fn InitialValue(&self) -> Option<Vec<u16>> { None }
            fn Value(&self) -> Option<Vec<u16>> { self.0.borrow().clone() }
            fn set_Value(&self, value: Option<Vec<u16>>) { *self.0.borrow_mut() = value; }
        }

        let mut runtime = Runtime::new();
        let binding = NullableDomStringBinding::<State>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, State::default());
        runtime.set_global_property("state", &handle).unwrap();
        assert_eq!(runtime.eval_value("state.initialValue").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::Null);

        runtime.eval("state.value = 'hello';").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("hello".into()));
        runtime.eval("state.value = '\\uD800x';").unwrap();
        assert_eq!(runtime.eval_value("state.value.charCodeAt(0)").unwrap(), Value::Number(0xD800 as f64));
        runtime.eval("state.value = null;").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::Null);
        assert_eq!(*runtime.get_wrapped::<State>(&handle).unwrap().0.borrow(), None);

        runtime.eval("state.value = undefined;").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("undefined".into()));
        assert_eq!(runtime.eval("(() => { try { state.value = Symbol(); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("undefined".into()));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_usvstring_replaces_unpaired_surrogates_and_preserves_nullable_values() {
        use crate::webidl::usv_strings::{UsvStringsBinding, UsvStringsNative};
        #[derive(Default)]
        struct State { value: std::cell::RefCell<String>, nullable: std::cell::RefCell<Option<String>> }
        #[allow(non_snake_case)]
        impl UsvStringsNative for State {
            fn Value(&self) -> String { self.value.borrow().clone() }
            fn set_Value(&self, value: String) { *self.value.borrow_mut() = value; }
            fn Nullable(&self) -> Option<String> { self.nullable.borrow().clone() }
            fn set_Nullable(&self, value: Option<String>) { *self.nullable.borrow_mut() = value; }
            fn InitialValue(&self) -> Option<String> { None }
        }

        let mut runtime = Runtime::new();
        let binding = UsvStringsBinding::<State>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, State::default());
        runtime.set_global_property("state", &handle).unwrap();
        assert_eq!(runtime.eval_value("state.initialValue").unwrap(), Value::Null);

        runtime.eval("state.value = '\\uD800x'; state.nullable = '\\uDC00y';").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("\u{FFFD}x".into()));
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::String("\u{FFFD}y".into()));
        runtime.eval("state.value = { toString() { return '\\uD800'; } };").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("\u{FFFD}".into()));

        runtime.eval("state.nullable = null;").unwrap();
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::Null);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().nullable.borrow().clone(), None);
        runtime.eval("state.nullable = undefined;").unwrap();
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::String("undefined".into()));
        assert_eq!(runtime.eval("(() => { try { state.nullable = Symbol(); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::String("undefined".into()));
    }

    #[test]
    fn interfaces_without_constructors_and_foreign_method_receivers_throw() {
        fn method(_: &dyn std::any::Any, _: &[Value]) -> Value { Value::Undefined }
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Native", None);
        runtime.define_method(&interface, "method", method).unwrap();
        let node = runtime.create_instance(&interface, ());
        runtime.set_global_property("node", &node).unwrap();
        for expression in ["Native()", "new Native()", "node.method.call({})", "new node.method()"] {
            assert!(runtime.eval(expression).unwrap_err().contains("TypeError"), "{expression}");
        }
    }

}
