# 0374 — reuse LegacyWindowAlias for HTMLDocument

The HTML standard requires Window.HTMLDocument to be the Document interface object,
writable/configurable and non-enumerable:
https://html.spec.whatwg.org/multipage/nav-history-apis.html#the-window-object
Roves instead declared a separate HTMLDocument interface and an unused Rust subclass.
Actual documents are Document/XMLDocument reflectors, as confirmed by the0370 CI failure.

Add LegacyWindowAlias=HTMLDocument to Document's maintained WebIDL declaration. Reuse the
existing generator/runtime alias path used by DOMMatrix, SVGMatrix and WebKitCSSMatrix.
Remove HTMLDocument.webidl, html/htmldocument.rs, its module declaration and obsolete
Bindings.conf entry. No new factory, alias state, dependency or runtime class-name branch.
IsHTMLDocument doctype flags and CreateHTMLDocument APIs remain unchanged. Codegen retains
one Document binding and maps both global names to its same define/enabled functions.

## Validation

-9 real/synthetic generator tests pass on Windows and Linux. The real parser verifies the
 alias metadata, absence of a distinct descriptor and real generated global-name JSON.
- Old real IDL negative maps HTMLDocument to a distinct HTMLDocumentBinding and fails the
 alias regression (zero import errors), proving the former route was different.
- Required browser smoke uses fresh iframes for alias-first, Document-first and descriptor-
 first access; checks constructor/prototype identity, instanceof, data-property attributes,
 assignment changing only the alias, and restoration. Real normal/JITless run remains CI-only.
- Runner42 tests pass with8 POSIX skips; final syntax/contract run follows the last fixture
 addition. Logs/prompts under target/task-0374. No fresh Chrome MCP reference is claimed
 (Target closed); expected behavior is from the primary HTML standard.

This resolves the alias route in the implementation while reducing unused bindings. It
requires confirmation in the new browser CI batch. It does not resolve scoped-handler
sourceText0361, numeric named/index duplicate cases, define/delete visibility or overall
page retention. No measured CPU/RAM saving is claimed.
