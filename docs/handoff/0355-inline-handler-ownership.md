# Incarico0355 — handler inline tracciati dal proprietario DOM (implementazione, da revisionare)

Stato: implementazione completa locale del design0354, **non committata, non pubblicata**.
Base: HEAD31036a74536 (0353 locale, non pubblicato). Batch0351: V8 10/10 verde, run generale
ancora attiva → nessun push; commit e CI li gestisce Codex. Il design note0354 resta invariato.

## Modifiche

Unico file di produzione: `components/script/dom/event/eventtarget.rs`.

- `TracedEventListener` (0353) → `TracedCallback`, privato al modulo, stessi campi (callback
  `Box<Heap<*mut JSObject>>`, incumbent `Option<Dom<GlobalScope>>`) e stesso `must_root`;
  `new`/`to_rooted` generici su `T: CallbackContainer<DomTypeHolder>` usando solo metodi
  pubblici già esistenti (`callback()`, `incumbent()`, `new_with_incumbent`). `to_rooted` resta
  una funzione privata sicura con il blocco `unsafe` interno, come in0353: nessuna nuova API
  pubblica `unsafe`. Chiamanti additivi: `TracedCallback::new(&*listener)`,
  `listener.to_rooted::<EventListener>(cx)`.
- Nuovi `InlineHandlerKind { Event, Error, BeforeUnload }` (tag senza puntatori) e
  `TracedInlineHandler { kind, callback: TracedCallback }` con `new`, `from_compiled` e
  `to_rooted` → `CommonEventHandler` (invariato, ancora `Clone`, solo transitorio).
- `InlineEventListener::Compiled(TracedInlineHandler)`; tolti `Clone`/`PartialEq` (ricerca in
  `components/script`: nessun altro uso di `InlineEventListener`, `TracedEventListener` o della
  clonazione rimossa).
- `get_compiled_handler`: `Compiled` → `to_rooted` mentre il record resta nell'entry tracciata;
  `Uncompiled` → si sposta fuori solo il sorgente, lo slot resta `Null` durante la compilazione
  (protezione dal rientro dell'errore su window `error` preservata), fallimento → `Null`.
- Setter (`set_event_handler_common`, `set_error_event_handler`,
  `set_beforeunload_event_handler`): record costruito dalla `Rc` dei binding (callback e suo
  incumbent), nessuna ricattura dell'incumbent, niente più `unsafe` né ricreazione della `Rc`.
  `cx` resta in firma come `_cx` (macro e `messageport.rs` invariati).
- Invariati: `get_compiled_event_handler` (crea la `Rc` con `new`, il record ne copia
  l'incumbent), getter (identità = puntatore del callback), `call_or_handle_event`
  (onerror 5 argomenti/`return true`, beforeunload returnValue/"Otherwise", `return false`),
  `associated_global`, `set_inline_event_listener` (posizioni e flag `removed`), `Rc` native
  esterne con radice permanente.

## Prove aggiunte

- `support/v8-smoke/events.html`: check richiesto "inline event handlers keep identity, position
  and return semantics" (+1 voce `REQUIRED_CHECKS` in `check_results.py`), valori attesi dal
  riferimento Chrome155 fornito da Codex (DevTools MCP, finestra visibile, about:blank, senza
  GC): identità/`null`; `h2,L`; `L,h3`; `L,h2` con sostituzione durante il dispatch; attributo
  compilato in cache, `return false` annulla, `removeAttribute` → `null`; window.onerror
  `["probe","probe.js",2,3,7]` con `return true` che annulla; `BeforeUnloadEvent` da
  `createEvent` annullato con returnValue `msg`; `Event("beforeunload")` non annullato.
  Chrome non è stato rieseguito.
- `support/v8-smoke/gc-lifecycle.html`: `checkInlineHandlerOwnership` dopo
  `checkListenerOwnership`. Un solo gruppo `expectCollected` (30 nodi staccati: 10 closure IDL
  auto-catturanti, 10 attributi compilati leggendo `onclick`, 10 attributi mai compilati come
  controllo della fixture) + riga `diagnostic inline handler nodes alive` per tipo; controlli
  positivi dopo 2 collezioni: nodo tenuto con `onclick` JS e `onfocus` compilato (stessa
  funzione via WeakRef, contatori, identità), `window.onerror`/`onbeforeunload` con closure non
  referenziate altrove (5 argomenti, annullamento, handler beforeunload eseguito). Caso peggiore
  +8+2 collezioni, limite pagina 40 s invariato, ripristino degli handler globali in `finally`.
  Il listener globale `error` della pagina ignora l'ErrorEvent di prova (`INLINE_PROBE`).
- `run.sh`: 3 nuovi marker richiesti; `test_runner.py`: marker nel fake e caso di fallimento
  `gc-inline`. Nessun flag mach, nessuna modifica a `test.yml`/`v8.yml`, mirror roves-action
  non toccato.

## Verifiche

Solo Read/Edit come richiesto: nessuna esecuzione locale (né `test_runner.py`, né rustfmt, né
replay patch, né build). Da eseguire da Codex: `python support/v8-smoke/test_runner.py`,
generazione e replay della patch0355, eventuale controllo di formato limitato alle righe
nuove. Compilazione Rust, smoke browser e GC Servo: solo CI. Il negativo del nuovo gruppo GC
prima del fix (radici permanenti) è un atteso, **non** un test Servo eseguito.

## Documentazione

`CUSTOMIZATIONS.md` (voce0355), `README.md` (paragrafo listener), `roves-wiki/content/docs/
shell.mdx` (paragrafo listener), questo handoff, sezione corrente di `docs/V8_MIGRATION.md`,
`target/task-0355/RESULTS.md`. Patch da generare da Codex:
`patches/servo-v0.5.0/0355-trace-inline-event-handlers-from-their-dom-owners.patch` con il diff
esatto di:

- `components/script/dom/event/eventtarget.rs`
- `support/v8-smoke/events.html`
- `support/v8-smoke/gc-lifecycle.html`
- `support/v8-smoke/run.sh`
- `support/v8-smoke/check_results.py`
- `support/v8-smoke/test_runner.py`

(`CUSTOMIZATIONS.md`, `README.md`, docs e wiki non fanno parte della patch, come in0353.)

## Limiti aperti

Retention frame complessiva APERTA (diagnostica FrameRetention invariata); altri storage di
callback restano `Rc`; nessuna GC forzabile dentro un dispatch, quindi i casi di dispatch sono
verificati per revisione e semantica. Nessuna affermazione su RAM o prestazioni.

## Revisione Codex completata
Runner42test34pass8skipPOSIX; node--check delle2pagine e parsingRust riusciti. Fixtureinline
reale true inChrome155 DevToolsMCP visibile aboutblank, senzaGCforzata. Patch0355 rigenerata
esatta, dry-run/apply6/6fileLF-identici. Nessun buildServo locale, GC e tipi allaCI. Batch0351
concluso tuttoverde:V8 10/10,generale13/13,mobileverdi. Pubblicazione0353+0355 consentita.
