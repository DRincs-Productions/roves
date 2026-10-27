# 0383 — native class, owned metadata and cppgc proof

Independent experimental fixture only; production DOM/bindings and runtime are unchanged.
Uses maintained deno_core op2/extension object registration, cppgc wrapping and SameObject,
plus rusty_v8 Member/Persistent tracing and low_memory_notification. No new dependencies,
private slots, binding framework, manual wrapper cache, FFI or NoHeapPointers.

NativeClass holds owned Rc<ClassMeta> (runtime-created name), per-instance Rust value,
a traced native-only child and a SameObject-backed info wrapper. Two instances share the
metadata; its creator releases its Rc before JS runs. A temporary Persistent roots each
child through wrapper allocation. The default probe runtime remains bootstrap-cleaned.

The same single V8 test entry point now covers eleven logical scenarios in two Rust tests.
New assertions check prototype/getters, separate native values/children, stable per-instance
info identity, receiver brand failures, metadata ownership count, live-wrapper survival
after a native low-memory notification and exact finalizer/metadata release at shutdown.
Collection before shutdown is only observed/bounded, not guaranteed. GC may conservatively
retain stack references; this is not proof that arbitrary cycles or full DOM frames collect.
Static V8 class/GC type names remain static as required; the owned metadata is separate.

Reviewed against actual deno_ops0.289.0 source: remove unsupported #[global] return marker;
v8::Global<Object> uses the maintained ToV8 conversion without an annotation. Checked
getter/string/scope syntax, absent-constructor behavior and wrapper-less receiver rejection.
Metadata, Rust syntax and workflow/lock checks pass locally; macro expansion/typechecking
and runtime assertions await CI. No heavy build or performance measurement locally.
Windows/Linux patch replay should reproduce all changed paths. Local source/API/logs:
target/task-0383/; sources of op2 are public crate inputs, not production dependencies added.

Class declaration lives under Deno.core.ops before bootstrap cleanup; there is no global DOM
interface object, inheritance or Web IDL conversion proof. SameObject's internal Global root
can participate in back-reference cycles; this fixture does not construct one. Real Servo
integration, HTML checkpoint order, CSP/Trusted Types callback gap, module/async single owner
and general reclamation remain adoption checks. Next independent proof: native async-op
completion/cancellation/cleanup under the same cleaned single-document runtime.

At review, bootstrap0382 CI40556912335 is green4/4; general40556912161 required checks have
five successes and packaging/Steam still active, no required failure. No push while they
remain active. Apple checks excluded. Keep complete commits local and continue useful
bounded assignments; no periodic polling or active retry automation.
