# Incarico0354 — ownership degli handler inline (solo design)

Stato: piano pronto per l'implementazione, **nessuna modifica di produzione**. Non dichiara
retention frame risolta né GC dei frame chiusa. Base: HEAD31036a74536 (0353 locale, non
pubblicato; CI del batch0351 ancora attiva). Letture mirate: `eventtarget.rs` (88-360,
423-444, 557-627, 668-840), `event.rs` (1274-1415), `callback.rs` (64-172), `codegen.py`
(8278-8288, 8630-8646), `macros.rs` (465-572), `messageport.rs` (110-114), `document.rs`
(2084-2119), fixture `events.html`, `errors.html`, `gc-lifecycle.html`, `check_results.py`.

## 1. Causa della retention residua

- `InlineEventListener::Compiled(CommonEventHandler)` (eventtarget.rs:125-129) conserva una
  `Rc<EventHandlerNonNull | OnErrorEventHandlerNonNull | OnBeforeUnloadEventHandlerNonNull>`.
  Ogni `Rc` viva tiene `CallbackObject::permanent_js_root` registrato con `AddRawValueRoot`
  (callback.rs:111-121) → radice permanente finché l'handler resta impostato.
- Ciclo tipico: `div.onclick = () => div.x++` → nodo→handlers→Rc→radice→closure→nodo. Più
  grave per gli handler **compilati da attributo**: la scope chain di `CompileFunction`
  (eventtarget.rs:731-740) contiene documento, form owner ed elemento, quindi ogni
  `onclick="..."` compilato su un nodo staccato lo trattiene sempre, anche senza closure.
- Le tre varianti hanno un'unica differenza: il tipo generato usato per `Call_` (coercizione
  del ritorno, 5 argomenti di onerror). Il callback JS e l'incumbent sono gli stessi dati di
  `TracedEventListener` (0353).

## 2. Principio

Lo stesso modello di0353, senza storage parallelo: lo storage a lunga vita contiene un record
**tracciato** (callback `Box<Heap<*mut JSObject>>` + incumbent `Option<Dom<GlobalScope>>`) più
un tag di tipo; la `Rc` radicata (`CommonEventHandler`, invariato) esiste solo come valore
transitorio durante getter e dispatch, ricostruita con il `CallbackContainer::new_with_incumbent`
già generato da `CGCallbackFunctionImpl` per funzioni (codegen.py:8278) e interfacce (8288).
Nessuna nuova dipendenza, nessuna rimozione globale di radici, nessun conteggio manuale di
`Rc`, nessun collector: le `Rc` native esterne mantengono la radice permanente attuale.

## 3. Modifiche esatte (solo `components/script/dom/event/eventtarget.rs`)

1. **Generalizzare il record di0353** invece di crearne un secondo: rinominare
   `TracedEventListener` → `TracedCallback` (stessi campi, stesse derive/attributi
   `must_root`) con metodi generici sul trait pubblico già esistente:
   ```rust
   fn new<T: CallbackContainer<crate::DomTypeHolder>>(callback: &T) -> Self
   // callback: Heap::boxed(callback.callback()), incumbent: callback.incumbent().map(Dom::from_ref)
   fn has_callback(&self, callback: *mut JSObject) -> bool   // invariato
   unsafe fn to_rooted<T: CallbackContainer<crate::DomTypeHolder>>(&self, cx: &JSContext) -> Rc<T>
   // T::new_with_incumbent(cx, self.callback.get(), self.incumbent.as_deref())
   ```
   Chiamanti0353: `EventListenerType::Additive(TracedCallback)`, `TracedCallback::new(&*listener)`
   (riga966), `listener.to_rooted::<EventListener>(cx)` (riga209, dentro `unsafe` con lo
   stesso commento di sicurezza). Nessun vincolo nuovo sul trait: `new_with_incumbent`,
   `callback()` e `incumbent()` sono già pubblici in `CallbackContainer`; `TracedCallback`
   resta privato al modulo, quindi la firma pubblica non cambia.
2. **Tag di tipo** (enum senza puntatori, come `ListenerPhase`):
   ```rust
   #[derive(Clone, Copy, JSTraceable, MallocSizeOf)]
   enum InlineHandlerKind { Event, Error, BeforeUnload }

   #[derive(JSTraceable, MallocSizeOf)]
   #[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
   struct TracedInlineHandler { kind: InlineHandlerKind, callback: TracedCallback }
   ```
   `TracedInlineHandler::new(kind, &impl CallbackContainer)` e
   `TracedInlineHandler::from_compiled(&CommonEventHandler)` (match variante → kind +
   `TracedCallback::new(&**rc)`); `unsafe fn to_rooted(&self, cx) -> CommonEventHandler` con
   `match kind` su `to_rooted::<EventHandlerNonNull | OnErrorEventHandlerNonNull |
   OnBeforeUnloadEventHandlerNonNull>`.
3. **`InlineEventListener::Compiled(TracedInlineHandler)`**; togliere `Clone, PartialEq` dalle
   derive di `InlineEventListener` (unici usi nel file: `compiled.clone()` di
   `CommonEventHandler`, che sparisce). L'implementatore verifica con una ricerca limitata a
   `components/script` che non esistano altri usi; `CommonEventHandler` resta `Clone`.
4. **`get_compiled_handler`** (134-155), preservando la protezione da rientro:
   ```rust
   let uncompiled = match &*inline_listener.borrow() {
       InlineEventListener::Null => return None,
       InlineEventListener::Compiled(handler) => return Some(unsafe { handler.to_rooted(cx) }),
       InlineEventListener::Uncompiled(_) => (),
   };
   // only the uncompiled source is moved out: the slot stays Null while compiling, so a
   // compile error reported to the same window `error` handler cannot recurse (as today)
   let InlineEventListener::Uncompiled(handler) =
       mem::replace(&mut *inline_listener.borrow_mut(), InlineEventListener::Null) else { unreachable!() };
   let compiled = owner.get_compiled_event_handler(cx, handler, ty);
   if let Some(ref compiled) = compiled {
       *inline_listener.borrow_mut() =
           InlineEventListener::Compiled(TracedInlineHandler::from_compiled(compiled));
   }
   compiled
   ```
   Il record compilato non viene mai spostato sullo stack fuori da una struttura tracciata;
   `to_rooted` non esegue JS (solo `AddRawValueRoot`) mentre il `RefCell` è in prestito.
   Fallimento di compilazione → `Null`, identico a oggi.
5. **Setter** (784-826): sostituire `unsafe { XNonNull::new(cx, listener.callback()) }` con
   `TracedInlineHandler::new(InlineHandlerKind::{Event|Error|BeforeUnload}, &*listener)`;
   spariscono tre blocchi `unsafe` e la ricreazione della `Rc`. L'incumbent salvato è quello
   della `Rc` ricevuta dai binding, creata con `new` all'ingresso del setter: stesso valore di
   oggi. `cx` resta nella firma (macro e `messageport.rs` invariati). `set_inline_event_listener`
   invariato: sostituzione in place nella stessa entry, `None` rimuove e marca `removed`.
6. **`get_compiled_event_handler`** (768-780) resta com'è: crea la `Rc` con `new` (incumbent
   corrente, comportamento preservato) che `from_compiled` copia nel record.
7. **Getter** `get_event_handler_common` (829-840) invariato: riceve un `CommonEventHandler`
   radicato e restituisce `CallbackContainer::new(cx, callback)`; l'identità JS è il puntatore
   del callback, quindi `t.onclick === f` resta vero. `CompiledEventListener`,
   `call_or_handle_event` (coercizioni onerror/beforeunload/return false) e `associated_global`
   non cambiano: ricevono la `Rc` ricostruita.

Nessun altro file di produzione: `macros.rs`, `messageport.rs`, `htmlelement.rs`, `element.rs`
usano solo le firme di setter/getter, invariate. Nessun cambio di codegen (il costruttore
`new_with_incumbent` esiste già per i tre tipi). Patch: nuova `0355-…patch` (o il numero
assegnato da Codex) con diff di `eventtarget.rs` e fixture.

## 4. Ownership, tracing e durata delle radici

| Dato | Proprietario | Tracciato da | Radice |
|---|---|---|---|
| `TracedInlineHandler` in `EventListenerEntry` | `EventTarget.handlers` | trace del reflector del target (`HashMapTracedValues` → `Rc<RefCell<EventListenerEntry>>`) | nessuna permanente |
| stesso record durante il dispatch | snapshot `RootedTraceableBox<EventListeners>` (event.rs:1278) | root dello snapshot0353, che condivide le stesse `Rc<RefCell<entry>>` | per la durata di `invoke` |
| `CommonEventHandler` ricostruito | stack di `inner_invoke` / getter | — | `permanent_js_root` per la sola durata della `Rc` |
| `Rc<T>` ricevuta dal setter | binding | — | fino al ritorno del setter |
| incumbent `Dom<GlobalScope>` | record | trace del record | nessuna |

Casi di dispatch coperti dallo snapshot0353 senza codice aggiuntivo: handler impostato a
`null` durante il dispatch (entry `removed`, saltata, ancora tracciata dallo snapshot);
sostituzione durante il dispatch (stessa entry, nuovo record; il vecchio callback, se già in
esecuzione, è radicato dalla `Rc` sullo stack); compilazione pigra durante il dispatch (il
record scritto nell'entry condivisa è tracciato sia dal target sia dallo snapshot). Window e
WorkerGlobalScope: i loro handler sono tracciati dal reflector globale, quindi onerror/
onbeforeunload restano vivi finché vive il globale. Limite invariato rispetto a0353: nessuna
GC forzabile in modo sincrono dentro un dispatch, quindi questi casi sono verificati per
revisione e per semantica, non con una GC durante l'handler.

## 5. Copertura attuale delle fixture

| Semantica | Copertura oggi |
|---|---|
| `window.onerror` 5 argomenti + `return true` su errore reale | `errors.html:9` (non verifica `defaultPrevented`) |
| setter/getter JS di `onmessage`/`onload`/`onerror` usati come strumento | molte pagine, nessuna asserzione su identità o ordine |
| identità getter, `null`, posizione dopo sostituzione/reinserimento | **assente** |
| handler compilato da attributo, cache, `return false`, `removeAttribute` | **assente** |
| ramo `BeforeUnloadEvent` (returnValue/preventDefault) e ramo "Otherwise" | **assente** |
| GC: nodi staccati con handler inline raccolti / controllo trattenuto | **assente** (0353 copre solo additivi) |

## 6. Prove da aggiungere

**A. `events.html`, nuovo check richiesto** "inline event handlers keep identity, position
and return semantics" (+1 voce in `REQUIRED_CHECKS` di `check_results.py`). Restituisce la
prima discrepanza come stringa. Atteso da fissare prima con Chrome via DevTools MCP (Codex):
1. `d.onclick = f` → `d.onclick === f`; `d.onclick = null` → `null` e non invocato.
2. Ordine: `onclick = h1; addEventListener(L); onclick = h2` → `h2,L`;
   `onclick = null; onclick = h3` → `L,h3`.
3. Attributo: `setAttribute("onclick", "event.rovesSeen = this.id; return false")` →
   `typeof d.onclick === "function"`, `d.onclick === d.onclick` (cache), click cancelabile
   → `defaultPrevented` e `rovesSeen` corretto; `removeAttribute` → `d.onclick === null`.
4. Sostituzione durante il dispatch: `addEventListener(L)` poi `onclick = h1`; `L` imposta
   `onclick = h2` → registrare quale handler gira (valore atteso **solo** dal riferimento
   Chrome; non fissarlo a priori).
5. `window.onerror = (m, s, l, c, e) => { args = [...]; return true; }` + `ErrorEvent`
   sintetico cancelabile → 5 argomenti con messaggio stringa e `defaultPrevented`; ripristino
   `null`.
6. `document.createEvent("BeforeUnloadEvent")` + `initEvent("beforeunload", false, true)`,
   `window.onbeforeunload = () => "msg"` → `defaultPrevented` e `returnValue === "msg"`;
   `new Event("beforeunload", {cancelable: true})` con ritorno stringa → non annullato (ramo
   "Otherwise"). Ripristino `null`. Usa document.rs:5645, nessuna navigazione né prompt.
   Divergenza preesistente nota, **non da correggere qui**: `img.onerror` (non window)
   scarta il valore di ritorno (eventtarget.rs:287-298); non aggiungere un'asserzione su
   `return false` in quel ramo.

**B. `gc-lifecycle.html`, in `checkListenerOwnership` o funzione gemella** (stesso schema
WeakRef/`expectCollected`, un solo gruppo nuovo per limitare a +8 il caso peggiore di
collezioni; limite pagina 40 s invariato):
- Negativo (deve essere raccolto): 20 `div` staccati, metà con `node.onclick = () =>
  node.rovesHits++`, metà con `setAttribute("onclick", "this.rovesHits = 1")` **compilato**
  leggendo `node.onclick`. Con il codice di HEAD questo gruppo fallisce (radice permanente):
  è l'evidenza negativa attesa prima del fix, da registrare come atteso e non come test Servo
  eseguito.
- Controllo zero-callback: nodi con attributo mai compilato raccolti (verifica la fixture).
- Controllo positivo (trattenuto): nodo tenuto con `onclick` JS e `onfocus` da attributo
  compilato; `const ref = new WeakRef(kept.onfocus)`; due collezioni; poi
  `ref.deref() === kept.onfocus` (record tracciato, non ricompilato né raccolto), dispatch
  di click/focus sintetici che incrementano i contatori, `kept.onclick === originale`.
- Globale: `window.onerror` e `window.onbeforeunload` con closure non referenziate altrove,
  due collezioni, poi `ErrorEvent` sintetico (5 argomenti, `return true` → annullato) e
  `Event("beforeunload")` → handler eseguiti; ripristino `null`.
- `run.sh`/`test_runner.py`: aggiornare il nome dello scenario fake solo se il check cambia
  nome; nessun nuovo flag mach, nessuna modifica a `test.yml`, mirror roves-action intatto.

## 7. Blocchi

Nessun blocco genuino. Rischi da controllare in implementazione: (a) un uso nascosto di
`Clone`/`PartialEq` su `InlineEventListener` emergerebbe solo in compilazione CI (ricerca
mirata prima); (b) la conversione `&*listener` per `T: CallbackContainer` richiede `T: Sized`,
vero per tutti i tipi generati; (c) la compilazione e la GC Servo sono verificabili solo in CI.

## 8. Prossimo incarico concreto (0355)

Implementare §3 in `eventtarget.rs`, aggiungere le prove §6A (dopo il riferimento Chrome via
MCP fornito da Codex per i punti 2, 4, 6) e §6B, aggiornare `REQUIRED_CHECKS`, patch,
`CUSTOMIZATIONS.md` e handoff. Verifiche leggere locali: `test_runner.py`, ricerca usi di
`InlineEventListener`/`TracedEventListener`, replay della patch su scratch. Nessuna build
Servo locale; compilazione e GC solo in CI, da pubblicare quando il batch precedente lo consente.
