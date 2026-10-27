# 0379 — isolate the Apple gamepad probe dependency (patch 0381)

The735a general run40520566879 skipped all packaging and Steam jobs because the required
sdl3-gamepad-probe matrix aggregated a cancelled macOS leg. This did not demonstrate a
Linux/Windows build failure, but required coverage was missing and was not called green.

Keep Windows/Linux in the required matrix and run macOS in a separate enabled job with
identical toolchain, steps and timeout. Preserve all three previous visible check names.
build-and-publish and Steam needs/definitions are unchanged: required probe/shared failures
still block them; the Apple probe no longer participates in their dependency aggregate.
No continue-on-error, always() bypass, removed workflow, bundle flag, pin or artifact change.
The user's Apple publication exception remains the only reason for this split.

Parsed with maintained PyYAML installed only in target/task-0379/pydeps for validation:
verified required matrix platforms, equal Apple/required steps and timeout, unchanged
build/Steam definitions, stable check names and no dependent job on the Apple probe.
No custom YAML parser or new product dependency. roves-action README mirrors the distinction;
action.yml requires no input/behavior change. Logs and review in target/task-0379/.

The next batch must confirm required packaging/Steam actually execute. The old skipped
outputs have not been recovered or treated as passed. macOS/iOS remain visible and enabled,
excluded from publication gating rather than described as green. Heavy builds stay online.
