# 0376 — visible named deletion without a deleter (patch 0377)

The authorized 27 October 14:02 Europe/Rome retry succeeded. The one-shot
`ripresa-unica-claude-quota-0376` was deleted. The earlier quota checkpoint is historical,
not an instruction to retry again. No periodic automation or CLAUDE_RESUME.md.

## Implementation and reuse

Use the existing WebIDL parser/generator, supported-name metadata, exact unpinned DOMString
id conversion and ObjectOpResult::fail_cant_delete. The handler table and hook generation
both select the generated delete for non-global named interfaces without a named deleter.
An own expando/unforgeable property takes the existing ordinary path first; only supported,
visible non-indexed names fail deletion. Existing named deleters, symbols and indexed
deletion remain separate. No new dependency, cache or public API.

Support is queried through SupportedPropertyNames, never NamedGetter: Form's actual getter
records past names. Building the temporary names list and scanning/converting names costs
memory and CPU per attempt; underlying metadata collection can also sort/deduplicate. No
performance saving is claimed. The existing prototype helper uses [[HasProperty]], whereas
the normative visibility algorithm walks [[GetOwnProperty]]; this pre-existing discrepancy
remains open and is not presented as full WebIDL conformance.

## Evidence

- Real vendored parser/generator: 14 tests pass on Windows and Linux, including real
  Document/XMLDocument/Form, inherited flags, handler wiring, Dataset and Global controls.
- Old HEAD generator fails the actual Document handler-selection assertion, not an import.
- Four real generated delete hooks and the helper pass Rust syntax parsing; typechecking
  and real browser execution require CI.
- Runner 42 tests pass, 8 POSIX skips on Windows. Browser fixtures cover Reflect/sloppy/
  strict failure, own/unforgeable properties, null/prototype cases, Dataset deletion,
  rename after a never-read delete attempt versus an actual Form read, prototype callback
  short circuit, one callback and exception propagation.
- Windows/Linux patch replay must match all changed paths after LF normalization.
  Local logs, negative, generated hooks and MCP reference: target/task-0376/.

Chrome DevTools MCP became available again: visible Windows Chrome156.0.0.0, fresh
same-origin iframe, text/html, independent names for Reflect/sloppy/strict. Document names
were visible and own before each operation; deletion returned true and removed the exposed
property while leaving the element connected. Form deletion returned true but kept the
named property. HTMLDocument !== Document (distinct constructors). These observations differ
from the normative expectations used in Roves; they do not validate browser parity. The
first preliminary probe reused a deleted key, so only the second independent-key probe
supports the strict-mode comparison. No headless or performance measurement was made.

Previous dac65 batch is terminal green for required checks: V8 7/7, general 10/10,
Android 3/3. macOS/iOS excluded by user instruction, not asserted green. This clears
publication of complete local0375 plus0376 after review/replay. New normal/JITless browser
smoke still needs the next CI. SourceText0361, indexed deletion, property definition,
prototype-query conformance and overall retention remain open.
