# 0424 — rilascio host e import di TLA gia in valutazione

## Stato

Il test di rilascio host e completo su deno_core originale0.413: normal1/1 e JITless1/1,
due percorsi per esecuzione (drop diretto e clear esplicito/idempotente prima del drop).
Conferma TLA entrato/non completato, host posseduto mentre vivo e Weak non aggiornabile
dopo il rilascio. Non conclude raccolta GC complessiva o correttezza di import pendenti.
Il vecchio test chiamato shutdown_with_pending_top_level_await_and_imports_is_clean
controllava solo assenza di crash: ora il nome e le asserzioni descrivono il vero contratto.
La prova di fetch di rete pendente0415 rimane invariata.

## Difetto scoperto, NON risolto

Un modulo avviato con JsRuntime.mod_evaluate entra in await; un successivo import dello
stesso URL risolve prima che l'await termini. Riprodotto anche senza Roves/Servo, usando
StaticModuleLoader, JsRuntime e il pubblico ExternalOpsTracker per la vitalita host.
Prima di release il risultato JSON e [true,null,"resolved"] (entered, completed, import).
Chrome156 Windows via DevToolsMCP sul modulo statico blob riporta invece prima
[true,null,"pending"], e dopo il resolver [true,"resolved",42]. Nessun inputdesktop usato.

Prova minima pubblica (tokio rt + deno_core0.413; nessun filesystem/rete/DOM simulato):

```rust
let url = ModuleSpecifier::parse("https://game.invalid/forever.js").unwrap();
let loader = StaticModuleLoader::with(url.clone(),
    "globalThis.entered=true; await new Promise(r=>globalThis.release=r); globalThis.completed=true;");
let mut js = JsRuntime::new(RuntimeOptions {
    module_loader: Some(Rc::new(loader)), ..Default::default()
});
js.op_state().borrow().external_ops_tracker.ref_op();
let id = reactor.block_on(js.load_main_es_module(&url)).unwrap();
let _evaluation = js.mod_evaluate(id);
js.execute_script("https://game.invalid/launch.js",
    "globalThis.settled='pending'; import('https://game.invalid/forever.js').then(()=>settled='resolved');").unwrap();
let mut cx = Context::from_waker(Waker::noop());
for _ in 0..4 { let _ = js.poll_event_loop(&mut cx, PollEventLoopOptions::default()); }
// Read JSON.stringify([globalThis.entered,globalThis.completed,globalThis.settled]).
```

Il file completo e il log sono target/task-0424/direct-static-with-host.{rs,log}.
La fonte locale mantenuta modules/map/dynamic.rs:81 usa ModuleStatus::Evaluated e cerca
TLA solo in pending_dyn_mod_evaluations; la valutazione statica passa da mod_evaluate.
Anche una dipendenza con TLA e importata prematuramente se la lista pending conserva solo
l'ID della radice. Non confondere stato V8 Evaluated con promise di valutazione conclusa.
Documentazione ufficiale corrente: https://docs.rs/deno_core/0.413.0/deno_core/struct.JsRuntime.html
Latest consultato il29ottobre indica ancora0.413.0. Nessuna nuova versione corretta verificata.

## Candidato rifiutato prima della pubblicazione

Avviare anche le radici HTML tramite import() pubblico Deno rende pendente il secondo
import della STESSA RADICE. Il prototipo eliminava Vec<Future>, conservava solo metadati
ID/nome/started e un marker transitorio per evitare di rimappare la radice HTML. Il test
negativo di importmap rilevava proprio il remap doppio; la guardia lo correggeva preservando
i normali import del gioco. Suite candidata116normal/115JITless verdi, non prove del codice
pubblicato. La nuova regressione su un DISCENDENTE fallisce comunque:
slow:start,dependency-import:ready anziche slow:start prima del resolver.
Quindi il candidato e INCOMPLETO e non va adottato come soluzione generale.

Salvato in target/task-0424/rejected-import-entry/{deno_impl.rs,tests.rs}; log
public-import-proof.log, root-mapping-negative.log, descendant-tla-candidate.log.
Sorgente produzione ripristinato a deno_impl di de071; solo il test ownership completo e
pubblicabile. Nessun fork, modifica registry, parser JS o grafo duplicato introdotto.

## Prossimo passo

Valutare una correzione mantenuta del fast path nella libreria, con regressioni su radice,
discendente, risoluzione/reject, cache e JITless. Un candidato di studio e saltare il fast
path per grafi async e lasciare che Deno attenda la vera promise di valutazione; NON e
ancora verificato. Nessun override di dipendenza o fork e autorizzato in produzione.
Preparare prima un risultato concreto e verificato; non aggirare il limite con URL diversi,
wrapper che duplicano moduli o attese su tutte le radici (rischio deadlock).
Questo difetto e un requisito di correttezza ancora aperto, distinto da C2/C3/preloadJSON,
non una deviazione gia accettata dall'utente. CI verde precedente non lo copriva.
