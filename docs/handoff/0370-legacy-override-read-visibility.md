# 0370 — honor LegacyOverrideBuiltIns in generated read hooks

The existing WebIDL generator ignored the interface attribute when resolving named
properties. It searched the prototype first even for HTMLDocument and DOMStringMap.
The primary visibility algorithm requires own/unforgeable properties first, then
supported named properties before the prototype when the attribute is present:
https://webidl.spec.whatwg.org/#dfn-named-property-visibility

Reuse the maintained parser, descriptor attribute API and existing special-operation
generators. No dependency, runtime class-name check, new DOM backend or property engine.
The compile-time branch changes get/getOwnPropertyDescriptor/hasOwn only. Indexed
properties and expando/unforgeable precedence are preserved. Nine generated no-attribute
bodies compare byte-for-byte equal to the old generator (including the base Document).
HTMLDocument and DOMStringMap are the existing real interfaces affected.

## Validation

- Real vendored WebIDL parser and generators:7 focused tests pass, including actual
  HTMLDocument/DOMStringMap IDL and synthetic indexed/named, attribute/no-attribute cases.
- Old generator negative:2 behavioral failures, zero import errors.
- Eighteen generated Rust hook bodies parse successfully with rustfmt.
- Required browser smoke covers prototype collisions, descriptor/hasOwn agreement,
  removal restoring prototype lookup, own expando and unforgeable location precedence,
  the no-attribute Form control, and dataset.toString shadowing via data-to-string.
- The smoke runner rejects omitted/failed/skipped required checks; local runner and
  syntax validation recorded in target/task-0370. Runtime/browser builds remain CI-only.
- The V8 workflow adds a lightweight generator test step in normal mode; generator
  behavior is independent of JIT. No mach flags or test.yml packaging changes, so
  roves-action and Packmaster build/bundle inputs need no synchronization.

Chrome DevTools MCP was unavailable (Target closed) during this assignment; no fresh
Chrome comparison is claimed. The primary WebIDL algorithm supplies the expectation;
normal/JITless browser smoke remains pending the new CI batch at publication.

This fixes read visibility, not full LegacyOverrideBuiltIns conformance. Pre-existing
definition/deletion/key-enumeration gaps and scoped-handler sourceText0361 remain open.
No broad removed-frame retention claim is made. No unchanged native suite was repeated.
