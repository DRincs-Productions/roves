# 0430 — isolamento dei job durante il caricamento statico

## Risultato

Una regressione nativa aggiunta a components/roves-js/src/tests.rs usa il loader host
esistente e std::future::poll_fn, senza nuove dipendenze. Il fetch restituisce Pending
almeno una volta e si risveglia; vengono coperti successo e fallimento. Una Promise del
gioco già accodata non deve eseguirsi dentro load_module_graph. Il modulo non viene
valutato durante il caricamento; dopo la valutazione esplicita/checkpoint i job riprendono.
Non si impongono soglie temporali e non si simulano clic reali.

## Evidenza e controllo negativo

- Negativa nella sola copia nativa: Loader::pump_event_loop_during_load restituisce true.
  Il test fallisce sull'assert di isolamento: log contiene job durante il load sospeso.
- Override negativo rimosso; deno_impl.rs nativo e produzione hanno SHA256 identico.
- Suite finale normale: 118 passati, 2 ignorati; JITless reale: 117 passati, 3 ignorati.
- Dopo il ripristino aggiornare il timestamp del file nella copia: Copy-Item può conservare
  un timestamp anteriore alla compilazione negativa e lasciare artefatti incrementali vecchi.
  Un primo run normale intermedio era ancora negativo; il run ricompilato è verde.
- Log: target/task-0430/{negative,normal-restored,jitless,replay}.log.
- Patch0430: dry-run, applicazione ed uguaglianza LF sul replay upstream riusciti.

Il codice produttivo non cambia: si protegge il default sicuro dell'API pubblica Deno.
Questo non dimostra assenza generale di crash, rilascio complessivo della memoria o
chiusura della finestra durante un fetch: quelle prove restano separate.

## JSON preload: valutazione mirata

JsRuntime::load_side_es_module e load_main_es_module, con o senza codice, non espongono
il tipo richiesto della radice. Usare JSON come radice ES aggira il requisito type json,
come già dimostrato dall'esperimento scartato0385. Il preload è opzionale, non un blocco
di gioco: nessun nuovo fork per questa ottimizzazione. Resta il TODO e la segnalazione
esplicita di indisponibilità; gli import JSON ordinari mantengono il controllo del tipo.

## CI del precedente push3cee4b8602b

Tutte le verifiche richieste terminate verdi: V8 otto job Linux/Windows, generale dieci
job non Apple, Android tre job. Browser moduli23/23 confermati dai log in tutte e quattro
le combinazioni Linux/Windows normali/JITless. Apple esclusa; job cancellati non chiamati
verdi. Il batch consente il prossimo push completo. Windows desktop ancora indisponibile.
