# 0367 — ordinary V8 properties preserve external receivers

The standalone experimental diff beside this file adds one generic native test,
not a production adapter. Apply only to an isolated checkout at5464c544278.
No new dependency or custom property engine: existing maintained V8 intercepts a
fixed read-only data name and delegates ordinary expandos/accessors to V8.

Codex ran `api_object_interceptors` normal and JITless: one test passes in each.
The first compile required the named setter ReturnValue<Boolean> signature, not
ReturnValue<()>; assertions were unchanged. Logs: target/task-0367/{normal,jitless}.log.
Actual public API observations in both modes:
- foreign Reflect.get and inherited accessor reads preserve their receiver;
- foreign Reflect.set invokes no setter interceptor and ordinary writes stay correct;
- own writes invoke the setter once per key, with the holder itself;
- a real own named property hides the supported name until deleted;
- exact own-key order is `5,x,acc,Symbol(s),named`, not Roves' current Proxy order
  `named,5,x,acc,Symbol(s)`. No unordered comparison is accepted.

The proof's fixed generic enumerator is therefore NOT a compatible DOM replacement.
No codegen routing or runtime source was kept; tests.rs restored after preservation.

## Chrome DevTools MCP reference

Chrome155.0.0.0, Windows, visible about:blank, attached same-origin iframe document.
Created an img named roves0367named, form/input with that name, select/option with
that id; then attempted numeric5, string x, Symbol(s), and accessor acc expandos.
Document: keys `5,location,roves0367named,x,acc,Symbol(s)`; name is own/enumerable.
Form: keys `0,x,acc,Symbol(s)`; supported name is own but non-enumerable and absent
from ownKeys. Numeric5 assignment does not create an own property.
Select: keys `0,x,acc,Symbol(s)`; the name is not an own property; assigning string
at index5 throws TypeError. Accessor foreign receivers work on all three.
An initial detached Document did not expose the named property, so it is not used
as the live Document reference. All temporary MCP tabs were closed.
These are focused semantic probes, not performance measurements or a conformance
claim. The generic V8 test differs from the Chrome Document ordering. Next gate must
map actual generated WebIDL hooks and public V8 descriptor/definer behavior before
any representation change; source-text item0361 remains open.
