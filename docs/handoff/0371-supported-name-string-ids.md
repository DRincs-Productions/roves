# 0371 — enumerate dynamic DOM names without C strings or permanent pins

Follow-up0373 aligns smoke labels/prototype checks with the actual Document reflector.
The string/id conversion itself is unchanged; see0372-0373-visible-named-keys.md.

SupportedPropertyNames returns page-controlled DOMString values. The old ownKeys loop
converted each to CString with unwrap (embedded NUL panics) and atomized/pinned every
name for the runtime lifetime. Static cross-origin property atoms remain unchanged.

Reuse DOMString.safe_to_jsval, standard SpiderMonkey JS_StringToId and the existing
V8 key_id/atom cache. Add the missing compatibility entry and regenerate wrappers2.in.rs
with support/roves_js/gen_wrappers2.py from the real mozjs0.21.6 lists. No dependency,
new parser, UTF-8/Latin-1 roundtrip, alternate property engine or change to key ordering.
StringToId writes its output only on success; null strings and conversion failures
return false.
The DOM loop roots the value/string/id and propagates conversion/append failures.

## Verification

- Native atoms module:6 normal +6 JITless checks pass, including exact NUL, Unicode,
  supplementary and unpaired UTF-16 units, canonical integer ids, identity and GC.
- A temporary pin-forcing mutant is rejected by the transient-atom collection assertion
  (16 collected vs16 control); isolated source restored exactly, normal checks rerun green.
- The old CString conversion panic is reproduced with an isolated standard-library
  fragment, not a browser build. Logs and probes: target/task-0371/.
- Runner:42 tests pass (8 POSIX skips on Windows), including regeneration from real mozjs
  wrapper lists. The first run caught hand-inserted wrapper ordering; official regeneration
  fixes it without changing signatures. proxyhandler.rs parses with rustfmt.
- Required events smoke checks exact HTMLDocument ownKeys/read/descriptor behavior for
  img names containing NUL and supplementary Unicode, repeated enumeration and removal.
  Real browser normal/JITless execution remains in CI. No local heavy build was run.

The API preserves unpaired UTF-16 units natively; no claim is made that DOM name storage
preserves such units (the smoke uses valid Unicode). Chrome DevTools MCP unavailable in
this session; no fresh browser reference claimed. This removes only the unconditional
name pin and C-string panic; overall page retention/sourceText0361 remain open.
