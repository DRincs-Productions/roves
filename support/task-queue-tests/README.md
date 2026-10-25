# Task queue ownership contracts

`cargo test --locked --manifest-path support/task-queue-tests/Cargo.toml`

This small standalone test crate compiles the actual `components/script/task_queue.rs`
implementation and the actual TaskSourceName variants. Its build script removes only the
allocator-accounting annotations and inner module documentation. It never copies or mirrors
the queue algorithm. The ordinary channel/hash/enum libraries are the same maintained
packages used by Servo; no production dependency was added.

To avoid linking Servo, the test shell replaces DomRefCell with RefCell and supplies minimal
PipelineId, worker, event-category and TaskBox types. Tests check capture destruction, thread
ownership, reentrancy, inactive/ready/throttled queues, late tasks and unrelated pipelines.
They do not validate Servo type integration, real Trusted captures or browser collection;
the full CI build and frame-retention snapshot cover that integration.

`TASK_QUEUE_SOURCE` can select an alternate source file for a negative fixture. The checked
build-script annotation counts deliberately fail when the source layout changes unexpectedly.
