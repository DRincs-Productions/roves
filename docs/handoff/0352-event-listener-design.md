# Incarico0352 — design dello storage tracciato dei listener DOM (solo design)

Stato: piano pronto per l'implementazione, **nessuna modifica di produzione**. Non dichiara
retention risolta. Fonte: mappa0351 (`0351-traced-event-listener-ownership.md`) e lettura
mirata di `callback.rs`, `codegen.py` (5974-6057, 8513-8663), `eventtarget.rs` (108-581,
877-962), `event.rs` (1270-1420), `abortsignal.rs` (36-226) su HEAD9f62b30b1e8.

## 1. Causa precisa della retention

- `CallbackObject::init` (callback.rs:105) registra `permanent_js_root` con `AddRawValueRoot`
  → ogni `Rc<EventListener>` vivo è una radice dal RootSet finché la `Rc` non viene droppata.
- `EventListenerType::Additive(Rc<EventListener>)` (eventtarget.rs:159) conserva quella `Rc`
  per tutta la vita della registrazione. Il listener che cattura il nodo/frame è quindi
  radicato in modo permanente: ciclo nodo→handlers→Rc→radice→closure→nodo mai raccoglibile.
- Seconda copia: `RemovableDomEventListener.listener: Option<Rc<EventListener>>`
  (abortsignal.rs:64), clonata da `add_event_listener` (eventtarget.rs:889-896) in
  `AbortSignal.abort_algorithms`. Cambiare solo EventTarget lascerebbe questa radice.
- Gli altri campi (`Heap callback`, `incumbent: Option<Dom<GlobalScope>>`) sono già
  `JSTraceable`; il solo problema è la radice permanente legata al tempo di vita della `Rc`.

## 2. Principio del design

Nello storage DOM a lunga vita non si conserva una `Rc<CallbackContainer>` (radicata), ma un
record **tracciato** con identità del callback e incumbent catturato alla registrazione.
La `Rc` nativa (radicata, comportamento attuale invariato) esiste solo:
(a) nell'argomento della binding durante `addEventListener`/`removeEventListener`,
(b) ricostruita per la durata di una singola invocazione in `inner_invoke`.
Riuso integrale di `Heap`/`JSTraceable`/`RootedTraceableBox`/`Dom` esistenti; nessuna
dipendenza, nessun contatore manuale, nessun collector nostro, radici globali intatte.

## 3. Modifiche API esatte

### 3.1 `components/script_bindings/callback.rs`

```rust
impl<D: DomTypes> CallbackObject<D> {
    fn new() -> Self {
        Self::new_with_incumbent(D::GlobalScope::incumbent().as_deref())
    }

    /// Callback context supplied by an owner that captured it at registration time.
    fn new_with_incumbent(incumbent: Option<&D::GlobalScope>) -> Self {
        Self {
            callback: Heap::default(),
            permanent_js_root: Heap::default(),
            incumbent: incumbent.map(Dom::from_ref),
        }
    }
}
```

`CallbackFunction::new_with_incumbent` e `CallbackInterface::new_with_incumbent` (pub)
delegano a `CallbackObject`; i rispettivi `new()` restano invariati.
`init`, `Drop`, `PartialEq`, `call_setup` invariati: la `Rc` ricostruita è radicata come oggi.

Trait `CallbackContainer<D>` — un solo metodo nuovo:

```rust
/// Recreate a rooted container for `callback` whose callback context is `incumbent`,
/// instead of the incumbent of the current stack.
///
/// # Safety
/// `callback` must point to a valid, non-null JSObject.
unsafe fn new_with_incumbent(
    cx: &JSContext,
    callback: *mut JSObject,
    incumbent: Option<&D::GlobalScope>,
) -> Rc<Self>;
```

### 3.2 `components/script_bindings/codegen/codegen.py` (`CGCallbackFunctionImpl`, 8630)

Nessuna modifica a `ClassConstructor`/`getConstructors` (il `new` generato resta identico).
`CGCallbackFunctionImpl.__init__` sceglie la base con
`base = "CallbackFunction" if isinstance(callback, IDLCallback) else "CallbackInterface"`
(è istanziata a 8278 con `IDLCallback` e a 8288 con `IDLInterface`) ed emette, accanto a `new`:

```rust
unsafe fn new_with_incumbent(
    cx: &JSContext,
    callback: *mut JSObject,
    incumbent: Option<&D::GlobalScope>,
) -> Rc<{type}> {
    let mut ret = Rc::new({name} {
        parent: {base}::new_with_incumbent(incumbent),
    });
    // Note: callback cannot be moved after calling init.
    match Rc::get_mut(&mut ret) {
        Some(ref mut callback_obj) => callback_obj.parent.init(cx, callback),
        None => unreachable!(),
    };
    ret
}
```

Stessa forma di `ClassConstructor.getBody` (6030-6044): campo unico `parent`, init dopo
l'ultimo move. L'incumbent **non** viene ricatturato al dispatch: proviene dal record.
Verifica: rigenerazione con `PYTHONHASHSEED=0` e diff deterministico dei binding generati
(solo il metodo aggiunto per ogni callback/callback interface).

### 3.3 `components/script/dom/event/eventtarget.rs`

Nuovo record (privato al modulo):

```rust
/// Registration-time state of an additive listener. Traced through its EventTarget (or a
/// rooted dispatch snapshot); unlike `Rc<EventListener>` it is not a permanent root.
#[derive(JSTraceable, MallocSizeOf)]
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
struct TracedEventListener {
    #[ignore_malloc_size_of = "measured by mozjs"]
    callback: Box<Heap<*mut JSObject>>,
    incumbent: Option<Dom<GlobalScope>>,
}

impl TracedEventListener {
    fn new(listener: &EventListener) -> Self {
        Self {
            callback: Heap::boxed(listener.callback()),
            incumbent: listener.incumbent().map(Dom::from_ref),
        }
    }

    fn is(&self, listener: &EventListener) -> bool {
        self.callback.get() == listener.callback()
    }

    /// Caller must keep `self` traced (EventTarget or rooted snapshot) during the call.
    #[expect(unsafe_code)]
    fn to_rooted(&self, cx: &JSContext) -> Rc<EventListener> {
        unsafe {
            EventListener::new_with_incumbent(cx, self.callback.get(), self.incumbent.as_deref())
        }
    }
}
```

`Box<Heap<_>>` (convenzione Servo, `Heap::boxed` esiste in roves-js gc/root.rs:506) evita
qualsiasi dipendenza dalla stabilità dell'indirizzo dell'enum/entry.

- `EventListenerType::Additive(TracedEventListener)`; rimuovere `Clone` e `PartialEq`
  derivati da `EventListenerType`/`EventListenerEntry` (usati solo nel confronto duplicati e
  nel `listener_type` di `remove_event_listener`; i tre file sono gli unici utenti dei tipi).
  `EventListeners` resta `Clone` (clona solo le `Rc<RefCell<Entry>>`). Se il compilatore
  trova un altro clone, implementarlo a mano con `Heap::boxed(self.callback.get())`.
- Uguaglianza dei listener: helper `EventListenerType::is_additive(&self, &EventListener)`
  e `EventListenerEntry::matches(phase, &EventListener)`; `Inline` non è mai uguale a
  un additive (come oggi: `Rc` vs `RefCell` diversi nell'enum derivato).
- `get_compiled_listener` (164-178): `Additive(ref t) => Some(Listener(t.to_rooted(cx)))`
  (serve `&JSContext`: `cx` è già `&mut JSContext`, reborrow condiviso).
- `add_event_listener(ty, Option<Rc<EventListener>>, options)`: firma pubblica invariata
  (IDL `AddEventListener` 1071 e `MediaQueryList::AddListener` passano `Rc`). Il ramo
  signal costruisce `RemovableDomEventListener { listener: listener.as_deref().map(|l| Heap::boxed(l.callback())), .. }`
  (vedi 3.5); il duplicato si controlla con `entries.iter().any(|e| e.borrow().matches(phase, &listener))`
  prima di costruire l'entry, invece di `contains(&new_entry)`. Comportamento: identico
  (tipo implicito dalla mappa, callback per identità, capture per fase).
- `remove_event_listener(ty, &Option<Rc<EventListener>>, options)`: firma pubblica invariata;
  delega a `remove_event_listener_by_callback(ty, *mut JSObject, options)` che confronta
  `callback.get()` e fase. L'abort usa direttamente la variante `by_callback`.
- La `Rc` argomento viene droppata alla fine della chiamata della binding: la sua radice
  permanente sparisce, il record resta raggiungibile solo via traccia dell'EventTarget.

### 3.4 `components/script/dom/event/event.rs` (protezione della ricostruzione)

In `invoke` (1274) lo snapshot `listeners` vive solo sullo stack Rust. Oggi è sicuro perché
ogni entry tiene una `Rc` radicata; dopo il cambio le entry rimosse durante il dispatch
(`removeEventListener` di un listener precedente, `once` a 1359-1361 che rimuove **prima**
di `get_compiled_listener`, `remove_all_listeners`) non sono più tracciate dall'EventTarget.
Quindi:

```rust
let listeners = RootedTraceableBox::new(segment.invocation_target.get_listeners_for(&event.type_()));
```

e `inner_invoke(cx, event, &listeners, ...)` (deref a `&EventListeners`, firma invariata).
`EventListeners` è già `JSTraceable`; `Rc`/`RefCell`/`Vec`/`Box` sono `Traceable` in
roves-js (gc/traceable.rs:136/143/164/210). Il box traccia l'intero record — callback **e**
`incumbent` — per tutta la durata di entrambe le chiamate a `inner_invoke` (anche il ramo
legacy-type 9.3). Durante `call_or_handle_event` la `Rc` ricostruita è inoltre radicata in
modo permanente come oggi, quindi nemmeno un `removeEventListener` dentro il listener stesso
può invalidarla. Costo: un'allocazione e una registrazione root per segmento del path di
dispatch, solo quando ci sono listener (si può saltare il box se `listeners.is_empty()`).
Nessuna promessa prestazionale: registrare il costo del box; eventuali profili costosi
vanno annunciati e concordati nel contesto delle misure, senza confonderli con correttezza.

### 3.5 `components/script/dom/abort/abortsignal.rs`

```rust
pub(crate) struct RemovableDomEventListener {
    pub(crate) event_target: Dom<EventTarget>,
    pub(crate) ty: DOMString,
    #[ignore_malloc_size_of = "measured by mozjs"]
    pub(crate) listener: Option<Box<Heap<*mut JSObject>>>,
    pub(crate) options: EventListenerOptions,
}
```

Per la rimozione serve solo l'identità del callback, non l'incumbent. `Clone` manuale
(`listener.as_ref().map(|h| Heap::boxed(h.get()))`), necessario perché
`AbortSignal::add(&AbortAlgorithm)` clona. Abort step (205-211) →
`event_target.remove_event_listener_by_callback(ty, listener.get(), &options)` se `Some`.
Il record è tracciato tramite `AbortSignal.abort_algorithms` (dom_struct) e, durante
`run_the_abort_steps`, resta nel vettore fino al `clear()` successivo all'iterazione;
`remove_event_listener_by_callback` non esegue JS. Dopo l'abort il vettore è svuotato →
rilascio delle capture. Nessun secondo owner radicato rimane.

## 4. Tempi di vita risultanti

| Owner | Cosa tiene | Tipo di edge | Quando finisce |
|---|---|---|---|
| EventTarget.handlers | callback + incumbent | traccia (reflector del target) | remove/once/abort/remove_all o GC del target |
| AbortSignal.abort_algorithms | callback + target | traccia (reflector del signal) | abort (clear) o GC del signal |
| Snapshot di dispatch | entry intere | `RootedTraceableBox` (root temporanea) | fine di `invoke` |
| Rc argomento binding | callback | radice permanente attuale | fine chiamata IDL |
| Rc ricostruita | callback + incumbent copiato | radice permanente attuale | fine invocazione del listener |
| Rc nativa esterna (Rust) | callback | radice permanente attuale | invariato |

Nessun puntatore Rust non tracciato al callback sopravvive a una possibile GC.

## 5. Incumbent

Catturato una volta da `CallbackObject::new()` alla conversione dell'argomento IDL
(contesto di chi chiama `addEventListener`), copiato nel record, e passato a
`new_with_incumbent` a ogni ricostruzione: `call_setup` (callback.rs:299-302) usa quindi lo
stesso `run_a_callback(incumbent)` di oggi. `Dom<GlobalScope>` nel record mantiene l'incumbent
vivo esattamente come la `Rc` attuale (nessuna nuova radice cross-global).

## 6. Strategia di prova (prima di pubblicare)

1. Statiche/leggere: rigenerazione binding `PYTHONHASHSEED=0`, diff limitato al nuovo metodo;
   test `script_bindings`/unit normali e JITless (Codex) se toccano runtime.
2. Retention (browser CI, fixture gc-lifecycle esistente o nuova pagina gemella): nodo
   detached con listener che cattura il nodo, `WeakRef`/`FinalizationRegistry` + GC forzata →
   raccolto. Controprova: riferimento esterno vivo → non raccolto e listener ancora
   invocabile dopo la GC. Variante con `signal` non abortito (owner AbortSignal) e abortito.
3. Semantica: add duplicato (stesso callback/capture → una sola invocazione), capture≠bubble
   distinti, remove con capture sbagliato no-op, `once` (invocato una volta, GC tra due
   dispatch), oggetto `{handleEvent}` con `handleEvent` sostituito dopo la registrazione,
   rimozione/aggiunta durante il dispatch (rimosso non chiamato, aggiunto non chiamato),
   `removeEventListener` del listener corrente seguito da GC forzata dentro il listener,
   `MediaQueryList.addListener/removeListener`.
4. AbortSignal: abort rimuove il listener; dopo l'abort il signal e il target non mantengono
   la closure (WeakRef raccolto); `AbortSignal.any` dipendente.
5. Incumbent: callback creato nel realm A, registrato da codice del realm B, dispatch avviato
   da C. Osservabile: listener = `A.postMessage.bind(A, m, "*", [])` (nessun frame JS proprio,
   quindi conta il callback context) → `event.source` del messaggio = window B.
   Raccogliere prima il riferimento Chrome (DevTools MCP, versione/condizioni registrate)
   e fissare le attese solo dopo; poi negativo: con `new` invece di `new_with_incumbent`
   il test deve fallire.
6. Snapshot frame-retention online: atteso solo che sparisca l'edge
   `CallbackObject::root` del listener iniettato; le precedenti radici FontFaceSet/Performance sono già assenti nel census0348;
   non attribuirle di nuovo a questo follow-up. Possono restare altre radici/callback. Usare MemLab/strumenti mantenuti, niente parser nostri.

## 7. Fuori perimetro (residuo esplicito)

- Handler inline (`onclick=`/attributi, `CommonEventHandler` in `InlineEventListener::Compiled`)
  restano `Rc` radicate: stessi cicli possibili. Incarico separato, stesso pattern.
- Altri storage di callback a lunga vita (MutationObserver, timer, rAF, Promise reaction,
  `MediaSession` action handlers, ecc.) invariati.
- Signal dipendenti registrati sul global (`register_dependent_abort_signal`) restano vivi
  finché il global lo decide; non cambiato.
- Costo del box di dispatch da misurare; se significativo, alternativa equivalente:
  `rooted_vec!` sulle sole entry additive.

## 8. Blocchi

Nessun blocco tecnico trovato: tutte le primitive esistono (Heap::boxed, RootedTraceableBox,
traccia di Rc/RefCell/Vec, factory generata nello stesso crate dei binding). Rischio residuo
da verificare solo in compilazione: lint crown (non eseguito in questa build) e eventuali
cloni non visti di `EventListenerEntry`.

## Prossima azione

Codex delega a Claude l'implementazione 3.1-3.5 in un solo incarico (5 file + patch
dedicata + CUSTOMIZATIONS), con le prove 1-4 locali/CI; prova 5 dopo il riferimento Chrome.

## Riferimento Chrome MCP raccolto da Codex
Chrome155.0.0.0 Windows, pagina about:blank visibile e3iframe srcdoc same-origin, nessun
profilo o GC forzata. CallbackA = postMessage.bind(window, message, '*', []), registrazione
inB, dispatch inC: sourceB true; sourceA/sourceC/sourceParent false. Il terzo argomento []
è necessario perché l'Event passato al callback altrimenti viene trattato come transferlist:
la prima prova senza [] ha prodotto timeout e non è evidenza del comportamento incumbent.
Tab isolata chiusa. Attesa da preservare = realmB della registrazione.
