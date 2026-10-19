# Completamento dell’integrazione V8 in Roves

## Obiettivo e stato iniziale

V8 è già il motore JavaScript di produzione tramite `roves-js`. Completare la migrazione
significa verificare e correggere le lacune dell’integrazione, rendere affidabili i controlli
CI e ridurre il codice di compatibilità mantenuto da Roves.

Questo documento è il piano operativo approvato. Le attività sotto non sono dichiarate
completate: verificarle sul codice corrente prima di implementarle. Per ogni attività annotare
problema, evidenza, modifica, verifica locale, commit e risultato CI. Una verifica passata è
un confine di controllo: continuare con l’attività successiva.

Conservare Servo per DOM, WebIDL, networking, layout e rendering; mantenere SDL3 e le API
native indipendenti dai tipi V8. JIT-less deve continuare a funzionare; l’assenza di WebAssembly
nella configurazione attuale deve essere documentata e verificata come limitazione prevista.

WebAudio aggiuntivo, WebGL nei worker e ottimizzazioni generali rimangono nel backlog di
[ROVES_PERFORMANCE.md](ROVES_PERFORMANCE.md). Entrano nella migrazione soltanto quando una
prova identifica un difetto nel collegamento a V8.

## 1. Verificare il backlog e rendere affidabile la CI

- [ ] Ricontrollare ogni lacuna annotata contro codice, test e cronologia recente. Distinguere
  implementazione mancante, comportamento errato e verifica assente.
- [ ] Esaminare `roves-js` cercando funzioni vuote, risultati costanti, API approssimate e
  operazioni che dichiarano successo senza applicare il comportamento richiesto.
- [ ] Per ogni problema registrare chiamanti di produzione, comportamento atteso, riproduzione
  e test di accettazione. Un nome SpiderMonkey, da solo, non è un difetto.
- [x] Correggere il runner smoke: asserzioni fallite, riepiloghi mancanti, crash e timeout prima
  del completamento devono produrre un esito CI negativo. Distinguere il timeout dopo una
  suite completata dal timeout che impedisce di ottenere i risultati.
- [x] Rendere obbligatori identità Window, navigazione e CSP attualmente segnalati soltanto come
  warning. Consentire skip soltanto per capacità esplicitamente non disponibili, come Wasm in
  JIT-less. Uno skip inatteso deve fallire.
- [ ] Verificare il runner con fixture intenzionalmente fallite e incomplete, poi eseguire le
  suite esistenti nelle due modalità.

**Criterio di uscita:** ogni voce residua ha evidenze aggiornate e la CI fallisce realmente
quando una verifica obbligatoria fallisce.

### Audit mirato del codice — 19 ottobre 2026

Questa è una verifica statica mirata, non un audit esaustivo delle API compatibili. Non segnare
come risolti i problemi sotto sulla sola base di CI verde o dei nomi delle API.

| Area | Evidenza e chiamanti di produzione | Verifica richiesta per chiudere |
| --- | --- | --- |
| Prototipi immutabili | `jsapi_impl::JS_SetImmutablePrototype` scrive `succeeded=true` senza cambiare l’oggetto; il generatore WebIDL lo chiama durante l’inizializzazione dei globali. | Stesso prototipo accettato; prototipo diverso respinto da Object/Reflect, senza bloccare la definizione di proprietà ordinarie. |
| Realm | `realm_impl::register_realm` conserva un `Global<Context>` in `cx.realms`; `trace_realms` traccia ogni globale/proxy/drain. | Navigazioni ripetute con rilascio dei realm irraggiungibili; mantenere riferimenti cross-realm e bfcache ancora vivi. |
| Script privates | `script_impl::Scripts::privates` è una mappa tracciata dal runtime; `globalscope/script_execution.rs` la alimenta con `SetScriptPrivate`. | Raccolta dopo perdita dello script e dei suoi callable; conservazione dei privati necessari ai callable ancora vivi. |
| Module privates | `modules_impl::Modules::entries` e `trace_modules` tengono vivi record e privati; `script_module.rs` chiama `SetModulePrivate`. | Raccolta dei grafi modulo irraggiungibili senza rompere import dinamico, namespace conservati e risoluzione dei moduli. |
| TrustedScript | `runtime_impl::code_generation_from_strings` permette i non-stringa senza usare `codeForEvalGets`; Servo registra quel callback in `script_runtime.rs`. | Eval con stringhe normali, oggetti normali e TrustedScript sotto CSP/Trusted Types; coercizioni e report nel realm corretto. Non attribuire bypass senza una riproduzione. |
| Window cross-origin | `window/windowproxy.rs::PROXY_TRAPS` conserva TODO su same-origin e getPrototype; esiste già `maybe_cross_origin_set_prototype_rawcx` nei binding. | Pagina/iframe su due origini, permessi get/set/ownKeys/prototipo, navigazione tra origini e identità del proxy. Distinguere lacune Servo da regressioni V8. |
| Callable class boxes | `object::new_callable_class_object` usa un holder tracciato da V8/cppgc; esiste `callable_classes_follow_js_reachability_instead_of_runtime_lifetime` nei test roves-js. | Rieseguire il test normale/JIT-less in CI. La conservazione incondizionata fino alla fine del runtime è già stata rimossa dalla patch 0236. |

### Aggiornamento implementazione — 19 ottobre 2026

Corretto `JS_SetImmutablePrototype` in `components/roves-js`: il flag era dichiarato riuscito
senza memorizzare alcun vincolo. `ClassBox` ora mantiene il prototipo originale con un handle
V8 forte cppgc-tracciato; `JS_SetPrototype` accetta l'identità originale e rifiuta una
sostituzione, senza rendere non estensibili gli oggetti né impedire proprietà normali. Il trap
`setPrototypeOf` predefinito applica lo stesso vincolo al target proxy.

La prima esecuzione CI ha riprodotto un errore nel bundle Linux: la named-properties proxy di
Window presenta la `ClassBox` tramite la cella JSAPI, non nei campi interni dell’oggetto Proxy
V8. Il lookup in `JS_SetImmutablePrototype` e `JS_SetPrototype` è stato corretto per usare la
cella, con regressione su un proxy DOM della stessa classe. Suite locale aggiornata:
`cargo test --locked --manifest-path components/roves-js/Cargo.toml` passa (41 passati,
2 benchmark ignorati); con `--features jitless` passa (40 passati, 3 ignorati). La verifica
browser `Object/Reflect` su global e vero `WindowProxy` resta da eseguire in CI prima di chiudere
la voce nella tabella. Nessuna libreria aggiunta: rusty_v8 già offre
handle `Global` e cppgc; serde_v8 non serve per identità DOM, deno_core duplicherà il runtime
senza sostituire l’event loop/realm/CSP Servo, web-audio-api-rs resta fuori ambito secondo il
piano.

Il callback CSP per stringhe e Wasm è già installato in `runtime_impl`; non ripianificarne
l’intera implementazione. La lacuna TrustedScript riguarda il percorso degli oggetti code-like.
La prossima modifica di ownership deve riusare weak handle/cppgc di V8 e conservare tutte le
radici necessarie; eliminare una voce dalla mappa senza un contratto di raggiungibilità non è
una correzione sicura.

### Verifica del runner — 19 ottobre 2026

Implementata nella patch `0239-strict-v8-smoke-results.patch`. Python stdlib verifica risultati,
conteggi finali e skip autorizzati; Window/CSP/navigazione sono obbligatori. Le eccezioni hardware
richiedono `V8_SMOKE_ALLOW_MISSING_DEVICES=1`, la modalità è `V8_SMOKE_MODE=normal|jitless`.
Un adapter GPU indisponibile può essere saltato; una richiesta che non termina fallisce.

Verifica locale: sei test unittest, inclusi casi multipli e l’esecuzione del runner shell reale
con launcher simulato, passano. Verificati crash, asserzioni false, totali mancanti/incoerenti,
skip inattesi, risultati Window/CSP/navigazione mancanti e timeout prima/dopo il completamento.
Replay ordinato delle patch sui file smoke originali, dry-run della 0239 e confronto dei quattro
file ricostruiti passano.

La CI `39287531308`, commit `1af334ab062`, ha eseguito gli smoke Linux normale e
JIT-less: entrambe le richieste WebGPU sono scadute dopo 25 secondi. La causa era
nel percorso Servo della Constellation: gli smoke usano URL `file://`, privi di host;
`handle_wgpu_request` scartava il messaggio prima di rispondere al callback. La
correzione risponde `None` alle richieste di adapter/canale senza host, così l’API
può risolvere con adapter assente. Il controllo mantiene il timeout come errore per
una richiesta che resta bloccata. Un secondo falso negativo era nel runner: la riga
DOM include anche titolo e numero di figli, mentre il confronto esatto cercava solo
il prefisso; ora il fixture riproduce l’output reale e il matcher verifica quel prefisso.

Patch incrementali `0242-webgpu-no-host-response.patch` e
`0243-v8-smoke-dom-output.patch`. Verifica locale e nuova CI normale/JIT-less ancora
da eseguire; la fase 1 resta aperta fino all’esito.

## 2. Correggere semantica e ciclo di vita

- [ ] Verificare WindowProxy durante navigazione, accessi tra origini e cambi di origine.
  Preservare identità, controlli di accesso, descrittori ed eccezioni previsti.
- [ ] Correggere i prototipi immutabili dove l’API compatibile non applica il vincolo.
- [ ] Verificare `TrustedScript` e CSP insieme a `eval`, `Function` e compilazione Wasm;
  preservare le decisioni di sicurezza del runtime web.
- [ ] Esaminare proprietà e radici di realm, script, moduli e callback. Rilasciare gli oggetti
  non più raggiungibili dopo navigazione o chiusura, preservando quelli richiesti da
  riferimenti JS e bfcache.
- [ ] Ricontrollare i costruttori callable: il collegamento alla raggiungibilità JS è già stato
  introdotto e non deve essere ripianificato come interamente mancante.
- [ ] Verificare creazione/terminazione worker, interruzione di script, shutdown e callback
  pendenti, evitando accessi a isolate o contesti distrutti.

**Criterio di uscita:** test ripetuti di navigazione, worker e GC dimostrano identità corretta,
finalizzazione senza duplicazioni e rilascio degli oggetti diventati irraggiungibili.

## 3. Ridurre il codice attraverso librerie mantenute

Prima di ciascuna sostituzione confrontare manutenzione, licenza, piattaforme, JIT-less e costi
CPU/RAM/GPU. Aggiungere dipendenze soltanto insieme a un uso concreto e alla rimozione del
codice sostituito. Documentare anche le valutazioni che concludono di non aggiungere librerie.

- [ ] **rusty_v8 già presente:** preferire handle, cppgc, BackingStore, typed array e serializer
  forniti da V8. Migrare un gruppo coerente di chiamanti alla volta ed eliminare gli adattamenti
  diventati inutilizzati.
- [ ] **Servo WebIDL già presente:** conservare parser e semantica del generatore. Valutare il
  backend V8 sperimentale come materiale riutilizzabile, senza equiparare la copertura delle
  forme WebIDL alla compatibilità del DOM di produzione.
- [ ] **serde_v8:** provarla su una conversione concreta di dati delle API native Roves.
  Adottarla soltanto se riduce codice senza perdere semantica o introdurre costi rilevanti;
  evitare serializzazione generica per identità DOM e percorsi frequenti. Verificare la
  compatibilità della versione con il crate `v8` fissato dal progetto.
- [ ] **deno_core:** eseguire una valutazione isolata di moduli, callback e gestione runtime.
  Confrontare il codice eliminabile con l’adattamento per event loop Servo, realm multipli,
  CSP, loader browser, cppgc e JIT-less. Non integrarlo in produzione senza una riduzione
  dimostrata del codice complessivo; non mantenere due runtime concorrenti.
- [ ] **web-audio-api-rs:** conservarla come candidato nel backlog WebAudio. Richiedere una prova
  di integrazione con grafo audio e processori JS V8 prima di proporne l’adozione; il supporto
  AudioWorklet Rust non certifica quello JavaScript. Questa prova appartiene al lavoro WebAudio
  separato e non è un requisito per chiudere la migrazione V8.
- [ ] Eliminare progressivamente feature senza effetto, percorsi pilot inutilizzati e commenti
  falsi. Conservare rooting/tracing necessari finché la sostituzione non preserva sicurezza e
  comportamento. Non imporre l’eliminazione totale di `roves-js` se aumenterebbe il codice.

**Criterio di uscita:** ogni sostituzione elimina codice effettivamente usato in precedenza,
passa i test pertinenti e ha una decisione documentata. Una valutazione negativa motivata
chiude il relativo candidato senza introdurre una dipendenza.

Riferimenti dei candidati:

- rusty_v8: <https://github.com/denoland/rusty_v8>
- serde_v8: <https://docs.rs/serde_v8/>
- deno_core: <https://github.com/denoland/deno_core>
- web-audio-api-rs: <https://github.com/orottier/web-audio-api-rs>

## 4. Validazione e distribuzione

- [ ] Aggiungere regressioni per ogni difetto corretto, privilegiando il percorso di produzione.
- [ ] Eseguire WPT selezionati per WindowProxy, realm, prototipi, CSP, moduli, Promise,
  structured clone e worker. Registrare risultati e limitazioni; non richiedere conformità
  all’intera piattaforma web. Se manca il runner WPT, predisporre l’esecuzione selettiva senza
  reinserire indiscriminatamente gli strumenti e la suite upstream rimossi.
- [ ] Eseguire stress GC con cicli DOM↔JS, wrapper identity, riferimenti deboli, callback,
  navigazioni e terminazione worker.
- [ ] Eseguire smoke PixiJS/Three.js, eventi DOM, moduli dinamici, storage e bridge nativi Roves;
  verificare Wasm nella build normale e lo skip previsto in JIT-less.
- [ ] Confrontare memoria e prestazioni sulle stesse fixture e condizioni per modifiche a
  ownership o conversioni. Registrare build, scena, visibilità e protocollo; non usare la
  parità generale con Chrome come criterio di completamento. Usare Chrome DevTools MCP quando
  disponibile e dichiararne esplicitamente l’assenza quando non disponibile.
- [ ] Per ogni modifica aggiornare `CUSTOMIZATIONS.md`, patch applicabili e documentazione
  pertinente; verificare il replay delle patch. Sincronizzare `roves-action` quando cambia la
  superficie build/CI. Aggiornare README e wiki per i comportamenti documentati interessati.
- [ ] Dopo CI finale verde verificare l’artefatto distribuito. Per modifiche destinate agli
  utenti pubblicare la release, controllare tutti i sei archivi plain/Steam e aggiornare i pin
  di Action e Packmaster, verificando i consumer.

## Completamento e documentazione finale

La migrazione è conclusa quando le verifiche obbligatorie sono bloccanti e verdi, le lacune
d’integrazione individuate sono risolte, i test di ciclo di vita passano e il codice obsoleto
sostituito è rimosso. Le attività indipendenti del runtime web restano nel backlog performance.

I tipi V8 restano confinati al livello scripting. Non sono previste nuove API pubbliche Roves;
eventuali cambiamenti del comportamento web devono seguire gli standard e avere test dedicati.

- [ ] Trasferire l’architettura stabile nella documentazione permanente e i problemi
  prestazionali in `ROVES_PERFORMANCE.md`.
- [ ] Eliminare i documenti temporanei `V8_MIGRATION*` quando non contengono più attività aperte,
  aggiornando i riferimenti nelle istruzioni e nel README. La cronologia resta in git.

Procedere per modifiche buildabili: verifica locale pertinente, commit, push, attesa della CI e
correzione dei fallimenti prima del passo successivo. Non dichiarare conclusa la migrazione
sulla base di sole compilazioni o dell’avvio della CI.
