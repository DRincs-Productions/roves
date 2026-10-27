# 0368 — native DOM hook map for ordinary V8 properties (HTMLDocument/Form/Select)

Documentation only: no production edits, no new tests, no registry/shell/network access.
V8 API facts come only from the installed copies in `target/task-0368/v8-api/{template,
function,object,property_handler_flags}.rs`; behaviour facts only from 0367 (native test
+ Chrome155 MCP probe). Source-text item 0361 stays open and is unaffected.
Reviewed revision: corrects the unreviewed draft (HTMLDocument override, NON_MASKING docs,
over-strong impossibility/achievability claims).

## 1. Verified WebIDL surface (Roves tree)

| Interface | Declaration | File:line |
|---|---|---|
| HTMLDocument | `[LegacyOverrideBuiltIns, Exposed=Window] interface HTMLDocument : Document` + `getter NamedPropertyValue (DOMString name)` | `components/script_bindings/webidls/HTMLDocument.webidl:8-12` |
| Document | `[LegacyOverrideBuiltIns]` commented out on the partial interface; its own `getter NamedPropertyValue` | `Document.webidl:93,109` |
| Document | `[PutForwards=href, LegacyUnforgeable] readonly attribute Location? location` | `Document.webidl:99-100` |
| HTMLFormElement | `[LegacyUnenumerableNamedProperties]`; **no** `[LegacyOverrideBuiltIns]` | `HTMLFormElement.webidl:6` |
| HTMLFormElement | indexed getter (no setter) + named getter (no setter/deleter) | `HTMLFormElement.webidl:34-35` |
| HTMLSelectElement | `getter Element? item(unsigned long index)` + `[CEReactions, Throws] setter` (index) | `HTMLSelectElement.webidl:29,38` |
| HTMLSelectElement | no named getter (`namedItem` is a plain operation); consistent with Chrome probe | same file |

DOM implementations: `htmldocument.rs:28,33` delegate to `document.rs:6194`
SupportedPropertyNames (HashMap-built, unordered) and `:6119` NamedGetter;
`htmlformelement.rs:442/447/453/528`; `htmlselectelement.rs:669/684/689`.

Override semantics in Roves: HTMLDocument **declares** `[LegacyOverrideBuiltIns]`, but
`codegen.py` has no handling for it (only a TODO comment at `:6465-6467`): every generated
named path still guards with `has_property_on_prototype` (`:6468-6477`, `:6729-6741`).
So the IDL intent (override) and generated behaviour (prototype wins) currently differ for
HTMLDocument. Form declares no override, and generated behaviour matches its IDL.
Whether the HTML spec or Chrome uses override for **Form** is **unverified here**: no
primary spec text was consulted and the 0367 MCP probe never tested a prototype collision
(e.g. a control named `action`). The same collision gap applies to Chrome HTMLDocument.

## 2. Current route (unchanged, reference for reuse)

- Handler table: `codegen.py:4101` `CGDefineProxyHandler`, traps at `:4148-4182`
  (`getOwnPropertyDescriptor`, `defineProperty`, `own_property_keys`, `delete_`, `get`,
  `hasOwn`, `getOwnEnumerablePropertyKeys`, trace/finalize).
- Trap bodies: GOPD `codegen.py:6373` (expando first `:6482-6495`, then indexed `:6414`,
  then named guarded by `has_property_on_prototype` `:6468-6477`); define `:6503`
  (indexed setter `:6531`, readonly indexed `failNoIndexedSetter` `:6540-6543`, named
  `fail_no_named_setter` `:6554-6560`, fallthrough to expando `:6561`); delete `:6568`;
  ownKeys `:6607`; enumerable keys `:6646`; hasOwn `:6683`; get `:6764`.
- Special-op call generators: `CGProxyIndexedGetter :6272`, `CGProxyIndexedSetter :6282`,
  `CGProxyNamedGetter :6306`, `CGProxyNamedSetter :6325`, `CGProxyNamedDeleter :6333`;
  unwrap `CGProxyUnwrap :6357` (proxy reserved slot 0).
- Runtime helpers: `script_bindings/proxyhandler.rs:101` define (to expando), `:127` delete,
  `:201/:216` get/ensure expando, `:239` `set_property_descriptor`, `:870-944` ownKeys
  (indices, then names, then expando keys incl. symbols), `:956` enumerable keys;
  `utils.rs:147` `get_array_index_from_id`, `utils.rs:335` `has_property_on_prototype`.
- Wrap: `script_bindings/wrap.rs:60-88` (`NewProxyObject`, DOM pointer in proxy slot 0),
  `:115-137` unforgeables copied onto the expando for proxies, onto `obj` otherwise;
  codegen counterpart `codegen.py:3320-3351`, `CGWrapMethod :3354`.
- Adapter: `roves-js/src/proxy.rs:158-193` builds `v8::Proxy(target, handler)`; target is
  a `ClassTemplates` instance keyed by class (`PROXY_CLASS` for every DOM proxy, since
  `wrap.rs:69/78` pass a null class); 11 JS-function traps `proxy.rs:246-258`.
  Expando/private rooted by the class box (`proxy.rs:181`, traced via `:306-310`).
- Existing interceptor precedent: `roves-js/src/object.rs:392-405` installs a named
  handler on a per-class template with `ONLY_INTERCEPT_STRINGS`.

## 3. Public V8 API available (from the copies)

- `NamedPropertyHandlerConfiguration` getter/setter/query/deleter/enumerator/definer/
  descriptor/data/flags: `template.rs:315-461`; indexed twin `:464-615`;
  `ObjectTemplate::set_named_property_handler :1000`, `set_indexed_property_handler :1021`.
  Documented contracts: setter/definer/deleter "do not set ⇒ not intercepted" (`:178-238`);
  query returns `PropertyAttribute` bits, may be used by `hasOwnProperty` (`:190-202`);
  named enumerator returns the names the getter intercepts (`:221-226`).
- `property_handler_flags.rs:11-21`: `NON_MASKING` is documented as "will not call into
  interceptor for properties on the receiver or prototype chain, i.e., only call into
  interceptor for properties that do not exist. Currently only valid for named
  interceptors"; `ONLY_INTERCEPT_STRINGS` (named only); `HAS_NO_SIDE_EFFECT` (getter/
  query/enumerator). The comment says nothing about GOPD/hasOwn/define/delete/ownKeys
  paths or key order: NON_MASKING needs a native behavioural gate before use.
- `Object::has_real_named_property :1086`, `get_real_named_property :1073`,
  `set_accessor{,_with_setter,_with_configuration} :566-612` (getter per read),
  `set_lazy_data_property :618` (documented to cache the first value — unsuitable for live
  named values), `define_own_property :473`, `get_own_property_names :691`.

## 4. Proven vs missing (per hook)

| Hook | Status |
|---|---|
| Expandos/accessors/symbols as ordinary V8 own properties; foreign receivers; OrdinarySet | Proven 0367 (normal+JITless), generic template only |
| Own real property hides supported name | Proven 0367 with default flags, via explicit `has_real_named_property` in callbacks (patch `:25-27`) |
| Named getter/query attrs (READ_ONLY, enumerable) / GOPD shape | Proven 0367 for one Document-like configuration |
| Named setter: own-receiver writes reach it once, with holder; foreign do not | Proven 0367 |
| Named key order | **Tested configuration differs**: 0367 template (`ONLY_INTERCEPT_STRINGS`, named enumerator) gave `5,x,acc,Symbol(s),named`; Chrome HTMLDocument `5,location,named,x,acc,Symbol(s)`. Other public configurations untested |
| Override vs prototype-wins (HTMLDocument IDL says override) | Missing: no collision tested in V8 or Chrome; NON_MASKING behaviour unverified |
| Form: names hasOwn true, non-enumerable, absent from ownKeys | Missing; candidate = named handler, query `DONT_ENUM\|READ_ONLY`, no enumerator |
| Indexed readonly (Form `o[5]=…` creates nothing, strict throws; definer fails) | Missing; indexed setter+definer would have to intercept every array index |
| Indexed setter conversion error (Select string ⇒ TypeError) | Missing; would reuse `CGProxyIndexedSetter` |
| Indexed keys first, then strings, then symbols with indexed interceptor | Missing |
| Descriptor/definer/deleter callbacks vs current trap results | Missing |
| Unforgeables | Candidate reuse: `wrap.rs:135` non-proxy branch copies onto the object |
| GC roots / trace / finalize | Missing: DOM pointer and trace/finalize must move from proxy slot/handler to the API object's class box; untested |

## 5. Codegen: reuse vs narrower generation

Reusable candidates: special-op generators (§2), unforgeable holder, wrap non-proxy branch,
`has_property_on_prototype`, `set_property_descriptor` attribute logic.
Narrower generation needed (no new engine): per-interface interceptor bodies that return
"not intercepted" instead of falling through to the expando (`codegen.py:6561`, `:6600`,
`:6482-6495`, `:6747-6755`), split into indexed vs named halves, enumerators emitting
indices only / names only, and — if override is adopted — a real
`LegacyOverrideBuiltIns` branch that codegen lacks today. Descriptor flags:
`LegacyUnenumerableNamedProperties` ⇒ candidate "no named enumerator, query adds
`DONT_ENUM`"; `IndexedGetter && !IndexedSetter` ⇒ reject all indices; no named getter ⇒
no named handler (Select). A generic set of V8 callbacks in `roves-js` could dispatch via
`data: External` to generated functions keyed by handler pointer instead of `PROXY_CLASS`
(`object.rs:370`, `proxy.rs:166`).

## 6. Gate answer (provisional)

- **Select / Form: provisionally mappable** to indexed/named interceptors + ordinary
  expandos. Not closed: descriptor, definer, deleter, indexed-readonly, conversion errors,
  key order with indexed interceptors, unforgeables and GC/trace all still need native
  tests (§4). Form override semantics remain unverified (§1).
- **HTMLDocument: no verified compatible route yet.** The single 0367 configuration places
  named keys after symbols, unlike Chrome. That does not prove every public V8
  configuration incompatible; NON_MASKING and other combinations are untested. How Chrome
  produces its order (interceptor, real properties, or otherwise) is not known from our
  evidence and must not be assumed.
- **HTMLDocument via real accessor properties refreshed by DOM hooks: not recommended
  now.** It would need a new roves-js accessor define/delete API, refresh at
  `document.rs:1185-1198` (name path has no `cx`) plus img `name` attribute changes,
  wrap-time install from an unordered HashMap, and expando-shadow bookkeeping — more code
  and mutation-time CPU than an interceptor, duplicating DOM map state. Its order
  compatibility with Chrome is also unproven.
- **Libraries:** no suitable maintained alternative was identified in this task (not an
  exhaustive search); maintained V8 interceptors/properties remain the integration point.
- **Open decision:** HTMLDocument ownKeys order and override behaviour. Options: keep it on
  the existing Proxy route, or adopt an interceptor route after the gates below.

## 7. Next native gates (bounded, not done here)

1. One rusty_v8 test, same harness as 0367, for the Form/Select candidate (indexed
   readonly + named without enumerator, query `DONT_ENUM|READ_ONLY`): ownKeys exact,
   hasOwn, GOPD, `o[5]=1` sloppy/strict, `defineProperty(o,5,…)`. Passing it narrows but
   does not close Form/Select; descriptor/definer/deleter and GC tests remain.
2. One NON_MASKING behavioural probe (get, hasOwn, GOPD, ownKeys with a prototype name and
   an own expando) to decide HTMLDocument override/prototype handling and order.
3. A Chrome MCP collision probe (prototype-named control/img on Form and HTMLDocument)
   before claiming Chrome override behaviour.
