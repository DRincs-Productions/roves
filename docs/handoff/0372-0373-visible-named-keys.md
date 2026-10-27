# 0372/0373 — visible named keys and the actual Document reflector

## CI correction0373

Published0370 (f3d59895e36) built successfully on Windows and its native checks passed,
but both Linux browser modes failed the new guard: the iframe is not an HTMLDocument
instance. Roves wraps Box<Document::new_inherited> in document/document.rs; the separate
HTMLDocument class is not that reflector. The actual Document IDL still commented out
LegacyOverrideBuiltIns. This was a production routing gap, not a reason to drop the test.

The HTML standard applies the attribute to the Document partial interface:
https://html.spec.whatwg.org/multipage/dom.html#the-document-object
The vendored parser rejects it on partial interfaces, so declare it on the main Document
interface and use the existing maintained generator. The attribute also applies to objects
implementing descendant interfaces; the generator now walks IDL ancestors (XMLDocument).
No HTML/XML runtime class-name checks, new factory or HTMLDocument alias changes.

Fixtures now require a text/html Document and its actual Document.prototype, then retain
all prototype-collision, getter-not-called, descriptor/hasOwn, own/unforgeable and removal
assertions. Required labels say Document consistently. The HTMLDocument alias/prototype
compatibility gap remains separate and unresolved.

## Visibility0372

ownKeys previously appended every supported name, even when hidden by the prototype or
by an own expando/unforgeable property. Pass the maintained IDL attribute as a compile-time
flag and reuse existing JSAPI own-property/prototype checks. Keep index/name/expando/symbol
ordering and the0371 exact-string/unpinned conversion.

Own properties short-circuit prototype checks. For non-override interfaces, collect names
surviving prototype checks before final expando filtering, because a Proxy has trap can run
script and add expandos for earlier/current names. The final no-script phase excludes those
names before appending expando keys. Override interfaces need one pass and no prototype
queries. The bounded cost for non-override interfaces is one temporary Vec of moved names,
second id conversion and own check for surviving names; no cache/dependency/GPU change.
No performance saving is claimed. A null prototype now means not found, rather than panic.

## Verification

-8 generator tests pass on Windows and Linux, using all real WebIDL files, maintained parser
 and actual Bindings.conf, plus small synthetic controls.
- Negative configuration removes the actual Document attribute and demonstrates prototype-first
 read hooks/false ownKeys flag. The old helper additionally fails2 inheritance/read-order
 checks (zero import errors). Earlier ownKeys config negative also fails as expected.
-27 generated read-hook bodies and both changed Rust helpers parse with rustfmt.
- Runner42 pass (8 POSIX skips). Required smoke covers Document/Form visibility, own vs
 prototype short circuit, null prototype and reentrant creation of an own property. Normal/
 JITless real browser validation remains CI-only and pending for this correction.
- Logs and prompts: target/task-0372 and target/task-0373. Chrome DevTools MCP unavailable;
 no fresh Chrome reference or full browser build is claimed locally.

Pre-existing numeric named/index duplicate cases, define/delete visibility, sourceText0361,
and overall page retention remain open. This is not full WebIDL conformance.
