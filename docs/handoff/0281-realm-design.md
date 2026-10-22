# Task 0280 — collectible realms: decision-complete design

Design only: no production source, test, patch, doc or wiki was changed. Baseline: local HEAD
45d88a513e7 (0279 complete, published 47a5cd351e1; wiki local f0cc01c). 0273 WIP untouched.
Anchors: `components/roves-js/src` (below `src/`), installed `v8 = "=152.2.0"` crate
(`~/.cargo/registry/src/index.crates.io-*/v8-152.2.0`: `RV/` = its `src/`, `V8/` = its `v8/`).

## 1. Current state (what keeps every realm alive today)

1. **Strong context root per realm.** `RealmData` is a `Box` in `JSContext.realms`
   (`src/jsapi.rs:127`), owning `context: v8::Global<v8::Context>` (`src/realm_impl.rs:20,36`).
   A `v8::Global` is a strong root: no realm context is ever collected.
2. **Unconditional registry trace.** `RootSet::trace` (`src/rust.rs:204`) calls `trace_realms`
   (`src/realm_impl.rs:165-172`), which traces `global`, `window_proxy` and `drain_function` of
   *every* registered realm.
3. **Second unconditional root (must be fixed in the same scope).** `ClassBox` stores
   `immutable_prototype_value: RefCell<Option<v8::Global<v8::Value>>>` (`src/object.rs:62`),
   set by `JS_SetImmutablePrototype` (`src/jsapi_impl.rs:305-318`). Servo calls it on **every DOM
   global** right after creation (`components/script_bindings/codegen/codegen.py:3459-3463`): the
   global's box (owned by the global proxy) holds a strong `Global` to `Window.prototype` etc. of
   the same context. Removing (1)+(2) alone would collect nothing for DOM globals.
4. **Principals leak.** `JS_NewGlobalObject` holds principals (`src/realm_impl.rs:220-224`);
   nothing ever drops them (no `JS_DropPrincipals` in roves-js outside the API itself).
5. **Registry users** (complete list from targeted rg over `src/` and Servo components):
   - `realm_impl::trace_realms` — removed by this design.
   - `realm_impl::window_proxy_of_global` (`src/realm_impl.rs:261-270`), called on every
     conversion of a class-object global proxy (`src/cell.rs:182`) — O(#realms) scan.
   - `runtime_impl::request_drain` fallback `embedder_realm` (`src/runtime_impl.rs:392-396`).
   - `jsapi_impl::JS_IterateCompartments` (`src/jsapi_impl.rs:1277-1286`), used by Servo
     `select_compartment` (`script_bindings/interface.rs:196-230`; every compartment is
     non-sharable, so the pointer is never used beyond the callback).
   - `register_realm` push (`src/realm_impl.rs:57`).
6. **Current-realm switches.** `EnterRealm`/`LeaveRealm` (`src/realm_impl.rs:121-139`, used by
   `JSAutoRealm` `src/jsimpls.rs:43-49,149-156`, `AutoRealm` `src/realm.rs`, Servo
   `script_bindings/callback.rs:287-291`, `src/object.rs:412-416`, `src/realm_impl.rs:239-241`)
   and raw save/`set_current_realm`/restore pairs in `runtime_impl` (`request_drain` :391-419,
   `csp_allows` :563-570, wasm `consume_stream` :776-780). `current_realm` is never null after
   `Runtime::create` (`src/rust.rs:424-428`); `set_current_realm(null)` is a no-op.
7. **Raw `*mut Realm` / `RealmData` consumers.** roves-js: `realm_data`, `GetRealmGlobalOrNull`,
   `GetRealmPrincipals`, `CurrentGlobal` (returns `&data.global` as a Handle location,
   `src/realm_impl.rs:184-186`, used by `AutoRealm::global`/`CurrentRealm::global`),
   `object.rs:455-457,705`, `proxy.rs:918,984-985,1016-1018,1031-1033`,
   `script_impl.rs:455-458`, `runtime_impl.rs:359,366-370,426-437`. Servo: `wrap.rs:93`
   (pointer compare only), `interface.rs:669,701-708` (`GetFunctionRealm(new_target)` then
   `GetRealmGlobalOrNull`, `new_target` rooted), `proxyhandler.rs:1013-1021`
   (`GetRealmPrincipals` of the current realm and of a rooted object's realm, borrowed without
   refcount), `globalscope.rs:2371,3129`. No Servo code stores a `*mut Realm` beyond the call.
   No `GetRealmPrivate`/`SetRealmPrivate` exists in roves-js or Servo.

## 2. Facts the design relies on (verified on the installed sources)

- **Every JS object keeps its creation context.** `JSReceiver::GetCreationContext`
  (`V8/src/objects/js-objects-inl.h:1124-1132`) reads `map()->map()->native_context_or_null()`:
  the meta map's strong `native_context` field. Objects, functions (also via their `context`
  field), promises, global proxies and native-function instances therefore retain their
  native context with no embedder edge. Natives from `FunctionTemplate`s are such functions.
- **Context embedder data is strong.** `Context::set_embedder_data` stores a V8 value in the
  native context's embedder data array (`RV/context.rs:337-347`). `set_aligned_pointer_in_
  embedder_data` stores an untraced raw pointer (`RV/context.rs:366-380`).
- **`Object::wrap` gives V8 object → cppgc object ownership** (`RV/object.rs:859`, used today
  by `ClassBox` `src/object.rs:752` and `PrivateData` `src/script_metadata.rs:169`). It needs an
  API object (template with an internal field), not `v8::Object::new`.
- **`TracedReference<T>` traces any `v8::Data`** (`RV/handle.rs:1279-1330`; `RV/cppgc.rs:183-190`
  → `cppgc__Visitor__Trace__TracedReference` → `JSVisitor::Trace`, `V8/include/v8-cppgc.h:150,
  186-193`). Non-`Value` precedents already in roves-js: `TracedReference<v8::UnboundScript>`,
  `TracedReference<v8::Module>` (`src/cell.rs:116-118`). `v8::Context` derives from `Data`, so
  `TracedReference<v8::Context>` is supported by the maintained binding; no C++ shim.
- **cppgc addresses are stable.** Compaction exists only for custom spaces that opt in
  (`V8/include/cppgc/custom-space.h:52-61`); rusty_v8 allocates in the default space
  (`make_garbage_collected`, binding `MakeGarbageCollected<RustObj>`). A `RealmData` address is
  valid from allocation to finalization.
- **Finalization is lazy.** rusty_v8 heaps use incremental+concurrent sweeping
  (`RV/cppgc.rs:236-237`); `WeakPersistent::get` turns `None` at the end of marking, possibly
  before the destructor runs. Destructors run on the mutator thread (the assumption already made
  by `ClassBox::drop` and `PrivateData::drop`, which call embedder hooks).
- **Weak registry precedent:** `WeakPersistent` tables with amortized pruning
  (`src/cell.rs:31,59`, `src/script_metadata.rs:109-124`) and holder+`WeakPersistent`
  (`src/script_metadata.rs:145-171`).

Rejected alternatives: rusty_v8 `Context::set_slot` annex (`RV/context.rs:177-248`) stores `Rc`
values freed by a weak finalizer and has no cppgc tracing, so it cannot keep the global/drain
cells; `v8::Weak<Context>` + `Box` has the same problem; keeping `v8::Global` is the current
leak. No maintained API is missing for this design.

## 3. Target ownership graph

```
rooted object / function / promise / wrapper ─(map→meta map)→ NativeContext
NativeContext ─embedder data[HOLDER_SLOT]→ holder (API object) ─Object::wrap→ RealmData (cppgc)
RealmData ─TracedReference<Context>→ NativeContext
RealmData ─Member(cell)→ global cell → global proxy (→ NativeContext)
RealmData ─Member(cell)→ window_proxy cell, drain_function cell
NativeContext ─embedder data[REALM_SLOT]→ raw *RealmData (untraced, valid by the cycle above)
RootSet ─→ current realm, saved realms (EnterRealm stack / RealmSwitch)       [roots]
JSContext.context (v8::Global, unchanged) ─→ initial runtime context          [root]
JSContext.realms: Vec<RealmEntry{ WeakPersistent<RealmData>, Rc<PrincipalsSlot> }>  [weak]
```

The cycle context → holder → RealmData → context/global is entirely inside the unified heap,
so it is collected when nothing outside reaches it. Bidirectional lifetime:

- **Context reachable ⇒ RealmData alive:** the native context strongly holds the holder, the
  holder owns RealmData through `Object::wrap`. Hence REALM_SLOT never dangles while any code
  can obtain the context (from a Local, a live object's creation context, the entered/current
  context stack, a microtask's context).
- **RealmData reachable ⇒ context alive:** `TracedReference<Context>` and the global cell.
- RealmData is finalized only after a marking in which the context was also unreachable; dead
  contexts are not handed to embedder callbacks, so no reader of REALM_SLOT exists afterwards.
  The finalizer does not need to clear REALM_SLOT.

## 4. Decisions

### 4.1 `RealmData` (src/realm_impl.rs)

```rust
pub(crate) struct RealmData {
    context: TracedReference<v8::Context>,
    pub(crate) global: Cell<*mut JSObject>,        // set once during registration
    pub(crate) window_proxy: Cell<*mut JSObject>,
    pub(crate) drain_function: Cell<*mut JSObject>,
    principals: Rc<Cell<*mut JSPrincipals>>,        // shared with the registry entry
}
unsafe impl GarbageCollected for RealmData {
    fn trace(&self, v) { v.trace(&self.context); trace_cell(global); trace_cell(window_proxy);
                         trace_cell(drain_function); }
}
impl Drop for RealmData { fn drop(&mut self) { let _gc = GcScope::enter();
                                               release_principals(&self.principals); } }
```

- `*mut Realm` stays the RealmData payload address (`UnsafePtr<RealmData>` ↔ raw, the
  `cell::to_raw/from_raw` pattern, with a size assertion). Public `Realm`, `JSAutoRealm`,
  `AutoRealm`, `CurrentRealm` and all JSAPI signatures stay unchanged.
- `realm_data(realm) -> Option<&RealmData>` keeps its shape; its doc states the validity
  contract (4.9). Add `RealmData::context(&self, scope) -> Local<Context>` via
  `TracedReference::get`; `with_scope` uses it (`src/jsapi.rs:161-164`).
- `#[cfg(test)]` thread-local `REALMS_FINALIZED` counter incremented in `Drop`.

### 4.2 Holder in the context (dedicated slot)

- New `HOLDER_SLOT: i32 = 4` (REALM_SLOT 2 and CACHE_SLOT 3 unchanged). New `REALM_WRAP_TAG:
  u16 = 3` (ClassBox 1, PrivateData 2).
- Holder = instance of the existing one-internal-field holder template. Factor the template out
  of `ScriptMetadata` (`src/script_metadata.rs:102-105`) into one shared
  `holder_template(cx, scope)` (lazily created, stored once per runtime) used by both
  `ScriptMetadata::new_holder_of` and realm registration; internal field 0 set to null (not a
  ClassBox), as today.
- The CACHE_SLOT object stays a plain `Object::new` (it cannot be wrapped).

### 4.3 Registration order (`register_realm`), allocation-safe

Inside the caller's handle scope, with `context: Local<Context>` alive (it roots the context and
transitively the holder and RealmData for the whole function):

1. `holder = holder_template.new_instance(scope)`; set internal field 0 to null.
2. `data = make_garbage_collected(RealmData { context: TracedReference::new(scope, context),
   global: null, window_proxy: null, drain_function: null, principals: Rc::new(null) })`.
3. Immediately `Object::wrap::<REALM_WRAP_TAG, RealmData>(scope, holder, &data)` — no V8/cppgc
   allocation between 2 and 3 (same contract as `make_class_box`).
4. `context.set_embedder_data(HOLDER_SLOT, holder)` (may allocate; holder Local owns data).
5. `context.set_aligned_pointer_in_embedder_data(REALM_SLOT, raw(data))`.
6. Global: `global` argument if non-null (the ClassBox cell, kept by the global proxy's box);
   else `from_v8(context.global())` (initial realm; the cell is stored at once, no allocation
   between creation and `data.global.set`).
7. Security token, CACHE_SLOT, `set_allow_generation_from_strings(false)` as today.
8. Registry: push `RealmEntry { data: WeakPersistent::new(&data), principals: data.principals
   .clone() }` with amortized pruning (4.6).
9. `initialize_window_wrappers`, `initialize_metadata`, `dynamic_code::install` as today
   (still `assert!`; on panic nothing dangles: only the context Local roots the realm).

`JS_NewGlobalObject` keeps its order; principals are installed **last**, after every fallible
step (`global.delete(console)` etc.), through `RealmData::hold_principals(p)` =
`JS_HoldPrincipals(p)` then `slot.set(p)` with `debug_assert!(slot.get().is_null())`. If the
function returns null earlier, the unreferenced context and RealmData are collected and no
principals were held. Between `register_realm` and return, the `context` Local roots the realm;
the returned global cell follows the normal "caller roots it" JSAPI contract.

### 4.4 Roots: initial, current and saved realms

- **Initial runtime realm:** kept by the existing strong `JSContext.context` Global
  (`src/jsapi.rs:123`) → context → holder → RealmData. Justified exception: `ScriptMetadata.cache`
  (WeakMap/set/get captured before user code, `src/script_metadata.rs:86-101`) and other
  intrinsics captured once per runtime live in it. No extra Persistent.
- **Current realm:** `RootSet::trace` traces `current_realm` (raw → `UnsafePtr<RealmData>` →
  `Member` → `visitor.trace`).
- **Saved realms:** new `JSContext.saved_realms: RefCell<Vec<*mut Realm>>`, all traced by
  `RootSet`. Borrows are confined to push/pop/trace (never across a V8 call); trace uses
  `borrow()` (a violated invariant aborts instead of silently skipping roots).
  - `EnterRealm(cx, target)`: `old = current`; push `old` (also when `target` has no realm,
    so every Enter has exactly one entry); set current to `realm_of_object(target)` if non-null;
    return `old`.
  - `LeaveRealm(cx, old)`: if the top entry equals `old`, pop it; otherwise (non-LIFO misuse,
    `debug_assert!` in debug builds) remove the last occurrence of `old` and leave the other
    entries in place. Then set current to `old` if non-null (unchanged semantics). Over-retention on a
    panic that skips LeaveRealm is acceptable; nothing dangles.
  - New RAII `RealmSwitch<'a>` (`JSContext::switch_realm(realm) -> RealmSwitch`) with the same
    push/set and pop/restore; replaces the three raw save/restore pairs in `runtime_impl`
    (`request_drain`, `csp_allows`, wasm `consume_stream`). `set_current_realm` becomes
    private to these two mechanisms (plus `Runtime::create`).
  - The realm being switched *to* is rooted from the moment it becomes current; the previous
    one is rooted by the stack even if no JS object, frame or Servo root reaches it.
- **`with_scope`:** the `Local<Context>` it creates roots the current RealmData for the whole
  closure, even if the closure changes the current realm. `current_realm` is never null after
  `Runtime::create`; the existing fallback to `JSContext.context` stays for creation time only.
- `CurrentGlobal` location (`&data.global`) is valid while the realm is current or saved: the
  address is a stable field of a rooted cppgc object; `AutoRealm::global`/`CurrentRealm::global`
  lifetimes already bind the handle to the borrow of the realm.

### 4.5 Remove the unconditional traces

- Delete `trace_realms` and its call in `RootSet::trace` (`src/rust.rs:204`); replace with
  `trace_realm_roots(cx, visitor)` (current + saved only).
- `immutable_prototype_value` becomes `RefCell<Option<TracedReference<v8::Value>>>`, traced in
  `ClassBox::trace` (before the early returns, like `private`). `immutable_prototype(object)` /
  `immutable_prototype_for_cell(obj)` take the scope and return `Option<Local<v8::Value>>`;
  update the three callers (`src/jsapi_impl.rs:290-292`, `src/proxy.rs:520-521,551-552`).

### 4.6 Weak registry

`JSContext.realms: RefCell<Vec<RealmEntry>>` + `realm_sweep: Cell<usize>` (threshold, the
`NATIVE_FUNCTION_SWEEP_MIN`/script-metadata pattern, minimum 16). On insertion when
`len >= threshold`: `retain(|e| e.data.get().is_some() || !e.principals.get().is_null())`, then
threshold = `max(16, 2 * len)`. An entry whose realm is dead but not yet finalized keeps its
principals slot discoverable for shutdown. Users:

- `window_proxy_of_global`: no registry. In `new_cell`, the class-object branch already has the
  V8 object: `object.get_creation_context(scope)` → `realm_of_context` → `realm_data` → return
  `window_proxy` if `data.global == cell`. Signature becomes `(scope, object, cell)`.
- `request_drain` fallback: first entry with `data.get()` live and an embedder global; take its
  raw pointer and enter it through `RealmSwitch` before any allocation (`drain_function` may
  allocate). No GC can run between `WeakPersistent::get` and the switch.
- `JS_IterateCompartments`: snapshot `Vec<v8::cppgc::Persistent<RealmData>>` of live entries
  (strong for the duration of the iteration, so a callback that allocates cannot invalidate a
  later pointer), release the borrow, call the callback per element.
- Tests: `#[cfg(test)] live_realm_count(cx)` and `realm_weak(realm) -> WeakPersistent<RealmData>`.

### 4.7 Principals: exactly-once release

`fn release_principals(slot: &Cell<*mut JSPrincipals>) { let p = slot.replace(null);
if !p.is_null() { JS_DropPrincipals(null, p) } }` (cx is unused by roves-js; the destroy hook is
Servo's `destroy_servo_jsprincipal`, which only frees the boxed origin).

- **Finalization:** `RealmData::drop` calls it (inside `GcScope`).
- **Runtime drop:** `Runtime::drop` (`src/rust.rs:484-498`) calls `release_realm_principals(&raw)`
  right after `release_runtime_privates`, before `CURRENT` is cleared: iterate every registry
  entry (live, or dead-and-unfinalized via the shared `Rc` slot) and release.
- `replace(null)` makes the two paths mutually exclusive; a destructor that runs later (lazy
  sweep or `CppHeap` teardown during isolate drop) finds null and touches nothing but its own
  `Rc`. The `Rc` keeps the slot memory valid regardless of which side drops last.
- `GetRealmPrincipals` returns `slot.get()`, borrowed: valid while the realm is reachable
  (Servo's `ServoJSPrincipalsRef` in `proxyhandler.rs:1018-1021` uses the current realm and a
  rooted object's realm — both satisfy it).
- Behavior change to document: principals are now released (previously leaked).

### 4.8 Other paths, verified to need no new edge

- **Global ClassBox:** no realm field; the box is owned by the global proxy, whose map retains
  the context. No GC edge added.
- **WindowProxy (local Window):** the proxy cell is a second cell on the same global proxy;
  RealmData traces it. **Foreign wrappers** (`proxy::wrap_window_for_realm`): wrapper box
  `private` = owner cell, traced by `ClassBox::trace`; `realm_of_object(wrapper)` already maps to
  the owner (`src/realm_impl.rs:101-107`). Wrapper → owner cell → owner global proxy → owner
  context → owner RealmData. The cache in the foreign realm is a WeakMap keyed by the owner
  (ephemeron): it does not retain the owner. Remote `NewWindowProxy` targets are plain proxies.
- **Native class/function cells:** JS functions from templates carry their context; 0276 made
  native-function state weak, so a native function retains its realm exactly while it is
  reachable. `drain_function` is traced by its RealmData (inside the cycle).
- **Promise jobs:** V8 microtask queue entries hold their contexts strongly; Servo's job queue
  roots the enqueued drain function (→ its realm). `promise_hook` reads the creation context of
  a live promise.
- **Incumbent / entered / callbacks:** `GetScriptedCallerGlobal` reads the entered context
  (on V8's entered stack); `csp_allows`/`consume_stream`/`code_generation_from_strings` read
  `scope.get_current_context()` (live Local) and switch through `RealmSwitch`, so both the
  compiling realm and the restored one are rooted for the callback's duration.
- **Modules:** an instantiated module's code/function retains its context; unlinked records
  hold only shared function info and need no realm (callers enter one). Servo's ModuleScript
  owner (0272) traces its global.
- **Per-realm caches:** CACHE_SLOT (helpers, window-wrapper WeakMap, dynamic Function adapters)
  live in the context and die with it. `class_templates`, private keys and atoms are isolate
  wide and hold no context.

### 4.9 Raw-pointer validity contract (documented on `realm_data`, `GetRealmGlobalOrNull`,
`GetRealmPrincipals`, `CurrentGlobal`, `GetObjectRealmOrNull`, `GetFunctionRealm`)

A `*mut Realm` is valid while at least one holds: the realm is current or saved; the caller
holds a rooted object/function of the realm (or a wrapper whose owner is in it); the caller is
inside a V8 frame/scope with a Local of its context (including entered/microtask contexts).
Derived values (`global` cell, principals borrow, `CurrentGlobal` location) share that validity.
After the realm is unreachable at a GC, dereferencing the pointer is UB; only
`WeakPersistent` (tests, registry) may observe it. All current consumers (§1.7) satisfy this.

## 5. Limitations that remain (state them, do not claim more)

- Servo itself roots Window/Worker globals (ScriptThread documents, DOM roots, `GlobalScope`
  fields); module maps on `GlobalScope` and the `Modules` table's strong entries (JSON left
  `Instantiated`, records not yet anchored — 0279 residual) keep records and their realms. This
  task makes realms *collectible by roves-js*; it does not prove page-level collection after
  navigation or iframe removal, and nothing is measured (no CPU/RAM/GPU claim).
- The initial runtime realm is never collected (by design, §4.4).
- A pending exception, a pending dynamic import or a queued job legitimately keeps its realm.
- Over-retention if a panic skips `LeaveRealm`.
- Global proxy detach/reuse (navigation identity) is out of scope.

## 6. Implementation scope (one coherent change, patch `0280-collectible-realms.patch`)

Files: `src/realm_impl.rs` (RealmData, registration, roots, registry, principals, Enter/Leave,
`window_proxy_of_global`), `src/jsapi.rs` (`realms`, `saved_realms`, `realm_sweep`,
`with_scope`, `set_current_realm` visibility, `RealmSwitch`), `src/rust.rs` (RootSet trace,
Runtime drop), `src/runtime_impl.rs` (three switches, drain fallback), `src/jsapi_impl.rs`
(`JS_IterateCompartments`, immutable prototype), `src/object.rs` (`immutable_prototype_value`,
trace, accessors), `src/proxy.rs` (two callers), `src/cell.rs` (`new_cell` call),
`src/script_metadata.rs` (shared holder template), `src/tests.rs`. No Servo source change, no
new dependency, no C++.

Order: (1) holder template factoring; (2) RealmData cppgc + registration + slots; (3) roots and
`RealmSwitch` + Enter/Leave; (4) remove `trace_realms`; (5) weak registry users; (6) principals;
(7) immutable prototype TracedReference; (8) tests; (9) audit existing tests in `tests.rs` that
call `JS_NewGlobalObject` (lines ~1022-1097, 1445-1561, 4100, 4221, 4581-4716, 4927) and root
any global used after a GC or after leaving its realm — legitimate under the JSAPI contract,
not a weakening.

## 7. Required tests (`mod realms`, roves-js)

Helpers: `collect(rt)` = up to 8 precise `gc_for_testing` calls, stopping early when the
awaited condition holds (V8 retains maps for a few GCs; the same bound is used for retention
assertions, which must hold after all 8). Destroy-principals counter installed with
`JS_InitDestroyPrincipalsCallback` (counts, then `DestroyRustJSPrincipals`). Every survival
test checks liveness through `realm_weak` **before** any raw dereference; if dead, it calls a
`#[cfg(test)] reset_realms_for_test(cx)` (current = initial realm, clear saved stack) and then
panics, so a failing run never restores a dangling realm.

1. `unreachable_realm_cycle_is_collected`: DOM-class global with principals and
   `JS_SetImmutablePrototype`, window proxy set, drain function created, a private/cached helper
   created; drop all roots, leave the realm → RealmData finalized, `v8::Weak<Context>` empty,
   registry count down, principals destroyed exactly once.
2. `rooted_object_keeps_its_realm`: only a rooted plain object → realm alive after 8 GCs,
   `GetObjectRealmOrNull` same pointer, `GetRealmGlobalOrNull` valid, principals not destroyed;
   unroot → collected.
3. `rooted_function_keeps_its_realm`: exported JS function and a `JS_NewFunction` native; call
   after GC runs in its realm (`CurrentGlobalOrNull` inside the native = that global).
4. `current_realm_survives_gc`: enter via `EnterRealm`, drop every root, GC → alive,
   `CurrentGlobal` handle valid; leave → collected.
5. `saved_realm_survives_nested_enter`: enter A, enter B, drop roots, GC → A and B alive; leave B
   → B collected, A alive; leave A → A collected.
6. `realm_switch_roots_both_realms`: `RealmSwitch` from an unrooted current realm to another;
   GC inside; both alive; restored correctly.
7. `principals_released_once_on_collection_and_shutdown`: one realm collected before drop, one
   alive at `Runtime` drop → counter 2 total, never 3; a third created and dropped right before
   runtime drop (dead possibly unfinalized) → still exactly one release each.
8. `pending_promise_jobs_keep_their_realm`: queue a reaction in realm B, drop roots, GC → alive;
   run jobs → reaction ran in B; then collectible.
9. `security_callback_runs_in_compiling_realm`: CSP callback from `eval` in B while A was current
   with no roots; callback sees B; after return current = A and A alive.
10. `foreign_window_wrapper_keeps_owner_realm`: Window-class A, wrapper made in B via
    `JS_WrapObject`; root only the wrapper; GC → A alive, `GetObjectRealmOrNull(wrapper) == A`;
    unroot → A collected, B intact.
11. `registry_users_skip_collected_realms`: after a collection `JS_IterateCompartments` does not
    report it; drain fallback picks a live embedder realm.

Negative scratch runs (not committed, logged under `target/task-0280/`): (a) skip the saved-stack
trace → test 5 fails at its weak-handle assertion without a crash; (b) do not set HOLDER_SLOT →
tests 2/3/10 fail (realm collected while rooted, observed through the weak handle);
(c) restore the unconditional registry trace or the immutable-prototype `Global` → test 1 fails.

Verification (local, light, as in 0275-0279): roves-js normal and JIT-less suites, warning count
unchanged (772), copy-tested sources identical, replay of previous patches on pristine v0.5.0
identical to HEAD and 0280 applying identically. Browser CI (Codex): extend
`support/v8-smoke/gc-lifecycle.html` with a survival-only phase — an iframe defines a closure
returning `new Array()`; the parent keeps only the closure, removes the iframe, calls
`TestUtils.gc()`, then checks the result is an Array from a different realm (`!(r instanceof
Array) && r.constructor.name === "Array"`); `run.sh` requires `ROVES-V8 gc detached realm
function true` in Linux normal and JIT-less. It proves retention and no crash, not collection.
Docs at implementation time: `CUSTOMIZATIONS.md` entry, patch 0280, README + wiki `shell.mdx`
line on realm lifetime and principals release (no internals per wiki rules), `V8_MIGRATION.md`
handoff.

## 8. Risks and fallbacks

- If test 1 shows the context surviving because `TracedReference<Context>` misbehaves, drop the
  field and derive the context from the global cell (`get_creation_context` of the global proxy)
  — same cycle, no API gap. If it survives for another reason, find the extra root with
  `--trace-detached-contexts`/heap snapshot before changing the design.
- Finalizers after `Runtime::drop` touch only `Rc`/null: safe during `CppHeap` teardown.
- Existing tests relying on unrooted globals will fail visibly (fix by rooting, §6 step 9).

## 9. Handoff

Design complete; nothing implemented. Next assignment: implement §6 exactly, with §7 tests,
negatives and verifications. Current CI facts unchanged from the handoff: published
47a5cd351e1; general 39822504002 still active; V8 39822504067 JIT-less bootstrap retry
requested; no push until that batch finishes. 0279 local commit 45d88a513e7 / wiki f0cc01c.
