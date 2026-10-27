# 0380 — single-document product scope and non-disruptive diagnostics

## Product decision, 27 October 2026

Keep the Servo shell, DOM/layout/rendering initially. Roves runs one game document in
one window, with internal client-side routes such as TanStack Router. Same-document
History API (pushState, replaceState, popstate/back/forward) and fragment changes are in
scope. Tabs, additional game windows and iframes are not required. Full document reloads
or page replacements must not be accessible to game code. Initial loading by the shell
and normal application shutdown remain necessary lifecycle operations, not game navigation.

This changes the earlier browser-compatible assessment: preserving WindowProxy identity
across document replacements and multiple frame contexts are no longer adoption gates.
Task0378's public-API findings remain historical evidence for its former wider scope;
they do not establish a NO-GO for this single-document product. deno_core must be
re-evaluated, not described as adopted or already proven compatible.

## Planned behavior for unsupported operations

Block unsupported navigation/window/frame requests centrally before a document or native
window is created/replaced, not merely by hiding shell controls. Cover script and native
navigation paths, including reload/location assignment, document-replacing links/forms,
refresh/redirect requests and attempted additional windows/frames. Preserve the current
game/document, route and state. Do not automatically turn an ordinary link into a router
transition; supported internal routing remains the router's job. User-confirmed external
links open the system browser only following a real user click/native activation. Automatic
requests are blocked and logged. A synthetic click alone must not grant that permission.
The external page never replaces the game document or creates a Roves tab/window.
Actual implementation must respect existing API result contracts; do not add arbitrary
exceptions, crashes or process termination merely to signal a blocked operation. A game
that depends on an unsupported feature may still lack that feature; no universal guarantee
of compatibility is implied.

Every blocked attempt must be diagnosable through the existing console/logging facility:
warning identifies the requested operation and the reason it is unsupported. Prefer
existing diagnostics, logging and UI components over new frameworks/dependencies. Repeated
identical requests must not flood logs or produce repeated dialogs. Logging is the minimum
required behavior. User-confirmed behavior is a discreet, non-modal notification in
development and logs only in the distributed game. No player-facing warning popup in the
distributed game; development notices must not pause the game.
Do not use blocking JavaScript alert() as the enforcement/notification mechanism.

These are planned requirements only: no runtime policy, blocking or new warnings were
implemented by this documentation change. Existing multi-frame/navigation fixtures are
not evidence that the new product policy is enforced; do not silently delete them before
replacement coverage exists.

## Confirmed implementation order

First complete the isolated single-document deno_core feasibility probe. Then implement
navigation enforcement, external-link handling and diagnostics with the selected runtime.
Do not implement the policy twice merely to keep both runtimes in parallel. Do not present
these product choices as a GO result or as already working enforcement.

## Next bounded deno_core evaluation

Use one document/context rather than mandatory frame/navigation lifecycle. Evaluate
public-API integration with Servo DOM/bindings/events, native ownership/GC, CSP/codegen,
module loading (static/dynamic/JSON/cycles/TLA), combined HTML/Promise microtask order and
JITless. Resolve one compatible V8 version and bootstrap globals/security before adoption.
Replace complete Servo services and remove the old path; do not keep two permanent module
or async services. Do not expose Deno filesystem/network/process capabilities to games.
No separate Chromium or Deno installation. Heavy builds remain online.

Acceptance coverage must include TanStack-style push/replace/back/forward and fragments
without document/context replacement; blocked reload/location/link/form/refresh/popup/frame
attempts retain game state and emit a useful warning; repeated attempts are bounded;
development shows a discreet non-modal notice and distributed games log only; real-click
external links open the system browser while automatic/synthetic-click attempts are blocked
and logged. No path creates a Roves window or replaces the game document. Logging never
causes a blocking dialog. Initial shell load and clean shutdown still work.
Test normal/JITless, DOM/events, security and teardown separately. No CPU/RAM/startup gains
or completed integration are claimed until measured. SourceText0361, other legacy DOM
gaps and overall retention remain open, subject to the revised product scope.
