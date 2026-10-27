# 0384 — native async-op completion and shutdown cleanup

Isolated experiment, not a production task-source or navigation-policy implementation.
Uses maintained op2/Extension/OpState, Deno's async driver and public event-loop polling,
Tokio yield_now/Notify and a single controlled std::future::poll_fn. No new executor,
queue, waker framework, timer service, I/O, dependency, private API or NoHeapPointers.

One intentional fixture function captures its op during trusted extension bootstrap;
Deno/internal globals are removed before the game. The default runtime still registers
no fixture extensions. Per-runtime counters/gates are cloned out of OpState before await;
owned inputs/results and Drop guards distinguish native first poll, completion and
cancellation. The same single V8 test thread now runs twelve logical scenarios in two
Rust tests, normal/JITless. Earlier class0383 assertions remain unverified until next CI.

New assertions: native futures already started/suspended when the classic script returns;
Promise jobs precede result callbacks; fulfilled value/chain and BadResource failure;
unhandled rejection surfaces from run_event_loop. A separate runtime starts a genuinely
pending op, one event-loop poll stays Pending, its guard/payload are live, shutdown drops
them, and notifying the gate afterwards cannot resume the operation. Count/Rc/Weak checks
make this more than a JS-only Promise test. No ordering between unrelated native ops is
assumed. Exact eager-poll, rejection text and synchronous shutdown observations await CI.

Metadata, Rust syntax and workflow/lock checks pass locally; heavy build/runtime tests
are online only. New source and all authored expectations require the next probe CI.
No universal resource cancellation, Servo task ordering, HTML CSP/DOM/router integration,
general leak freedom or performance result is claimed. Deno's internal promise-id Symbol
is explicitly observed as visible, not considered a privileged capability by this test.

Changed probe paths: native_async.rs, lib.rs (small registration/exports + reused op
prefix diagnostic), tests/probe.rs and README.md. Manifest/lock/workflow unchanged.
Public API evidence and local logs: target/task-0384/. Next integration question: how Servo
task sources enter the single Deno event loop and which existing queue/service is removed;
study a bounded replacement map before attempting a partial production runtime port.
