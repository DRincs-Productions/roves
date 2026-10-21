# Completamento dell’integrazione V8 in Roves

## Punto della situazione — 21 ottobre 2026, ripresa dalla CI 39491426868

Rilevazione sul commit `273c1d9db2e` (`main`) e sulla CI avviata dal rollback
`a26cb6ca39b`. Il PAT condiviso in chat ha restituito `401 Bad credentials`; non è stato
salvato né riutilizzato. Run e job sono stati controllati tramite il connettore GitHub.
**La migrazione resta incompleta.** L’utente ha autorizzato di continuare l’implementazione e
ha indicato di preferire build pesanti nella CI online dopo lo sviluppo, per ridurre il carico
sulla macchina locale.

### Risultati effettivi dell’ultimo commit

| Workflow | Esito conclusivo | Evidenza |
| --- | --- | --- |
| [V8 migration checks — 39491426868](https://github.com/DRincs-Productions/roves/actions/runs/39491426868) | Fallita: 6 job verdi, 2 falliti e 2 cancellati | Quattro runtime Linux/Windows normal/JIT-less e tutte le quattro build servoshell verdi. Smoke Linux normal/JIT-less falliti su iframe cross-origin e Function(TrustedScript); smoke Windows/macOS saltati, runtime macOS cancellati. |
| [V8 migration checks — 39480383126](https://github.com/DRincs-Productions/roves/actions/runs/39480383126) | Fallita: 6 job verdi, 2 smoke falliti, 2 runtime macOS cancellati | Runtime Linux e Windows normal/JIT-less verdi; build servoshell Windows/macOS normal e Linux normal/JIT-less verdi. Entrambi gli smoke Linux falliscono su Window, iframe cross-origin e Function con TrustedScript. Runtime macOS cancellati dopo i fallimenti Linux. |
| [V8 migration checks — 39478697257](https://github.com/DRincs-Productions/roves/actions/runs/39478697257) | Esito non verificabile dai log accessibili | L’ultima lettura della pagina pubblica mostrava 3/6 runtime completati, 2 errori Ubuntu e 0/4 build servoshell conclusi. I dettagli degli errori sono protetti; il rate limit API impedisce di verificare se i job siano poi terminati. |
| [V8 migration checks — 39473515801](https://github.com/DRincs-Productions/roves/actions/runs/39473515801) | Fallito: browser smoke Linux falliti; job macOS normal in retry nell’attempt 2 | Build servoshell Windows normal e Linux normal/JIT-less verdi. Tutte le nove suite superano gli smoke Linux; restano i marker browser di prototipo Window, WindowProxy cross-origin e Function(TrustedScript). CSP Wasm e trusted eval passano nelle due modalità. Il runtime macOS normal del primo tentativo era cancellato; smoke browser macOS non verificato. |
| [V8 migration checks — 39467094586](https://github.com/DRincs-Productions/roves/actions/runs/39467094586) | Fallito: 6 job verdi, 3 falliti, 1 cancellato | Runtime Linux/Windows normal e JIT-less verdi; macOS normal supera il timeout nel test del runner, macOS JIT-less cancellato. Le quattro build servoshell passano; gli smoke Linux falliscono su Window e iframe. I nuovi controlli trusted Function e Wasm CSP non sono inclusi. |
| [V8 migration checks — 39320063351](https://github.com/DRincs-Productions/roves/actions/runs/39320063351) | Fallito, 8 job verdi su 10 | Tutti i 6 job runtime (Linux/Windows/macOS, normal/JIT-less) passano. Build servoshell Windows/macOS verdi. Le due build Linux passano, ma entrambi gli smoke browser falliscono. |
| [Build Servo Test Binaries — 39320063236](https://github.com/DRincs-Productions/roves/actions/runs/39320063236) | Verde, 13/13 job | Replay patch, SDL3, sei bundle portable/installer e smoke Steam verdi; Windows MSI incluso. |
| [Android — 39320063387](https://github.com/DRincs-Productions/roves/actions/runs/39320063387) | Verde | Workflow concluso con successo. |
| [iOS — 39320063333](https://github.com/DRincs-Productions/roves/actions/runs/39320063333) | Esito aggregato fallito | Job `ios` ed `ensure-test-release` verdi; `ios-release-signing-smoke` cancellato. Nessuno step fallito esposto nei job consultati: non attribuire la causa al codice V8 senza ulteriori log. |

Le nove suite Linux (`suite`, `suite2`, `games`, `modules`, `errors`, `events`, `graphics`,
`stress`, `saves`) riportano riepiloghi validi in entrambe le modalità, con gli skip consentiti.
Anche DOM, eventi, Promise, timer e identità `window === globalThis` sono confermati. La run
394803 ripete i risultati delle nove suite, ma individua in entrambe le modalità tre failure
browser da risolvere o qualificare:

1. **Prototipo immutabile Window non confermato.** In 394803 il log riporta `object true true`:
   `Reflect.setPrototypeOf(window, {})` è riuscito, mentre le proprietà ordinarie funzionano.
   Applicare `ObjectTemplate::set_immutable_proto()` al template di `Window` fa abortire
   `JS_SetPrototype` durante l’inizializzazione WebIDL (`WindowBinding`), prima di avviare gli
   script. La patch 0252 è ritirata. Il percorso deve permettere l’installazione iniziale del
   prototipo e renderlo immutabile soltanto dopo.
2. **Prototipo cross-origin WindowProxy non confermato.** In 394803 non compare alcun evento
   iframe. La nuova fixture carica la pagina principale da loopback e l’iframe dallo stesso
   host su una porta diversa, così entrambe le pagine hanno origine HTTP e l’iframe è davvero
   cross-origin. Il runner conserva i log HTTP quando fallisce, per distinguere richiesta,
   navigazione e dispatch degli eventi.
3. **TrustedScript in `eval` e CSP Wasm verificati; `Function` resta aperto.** La run conferma il
   valore `42` per eval in normal e JIT-less, ma fallisce il caso aggiunto per
   `new Function(trustedParameter, trustedBody)(41)` con entrambi gli argomenti `TrustedScript`;
   il test WPT [eval-function-constructor.html](https://github.com/web-platform-tests/wpt/blob/master/trusted-types/eval-function-constructor.html)
   richiede che il caso all-trusted funzioni. La run verifica il blocco Wasm senza
   `wasm-unsafe-eval` in normal e l’indisponibilità/rifiuto in JIT-less.

Chrome DevTools MCP è ora disponibile. Il 20 ottobre 2026 Chrome `155.0.0.0` su Windows x64,
scheda MCP in background, condizione visibile/headless non determinata, ha eseguito le fixture
CSP locale `file://` e Window su `http://127.0.0.1:18765` con iframe su porta 18766. Window
rifiuta la sostituzione del prototipo, l’iframe dispatcha `load` e il controllo cross-origin dà
`true true`; questo verifica la fixture, non Servo. TrustedScript eval produce `42`, ma Function
con soli argomenti TrustedScript produce `EvalError`. La WPT corrente citata sopra richiede invece
successo; Chrome non è quindi un oracolo per quel caso. L’API V8 `ModifyCodeGenerationFromStrings` riceve il testo
già composto per il costruttore Function, non gli argomenti originali; consentire il codice in
base al solo testo perderebbe il controllo del brand TrustedScript e dell’eventuale `toString`
contraffatto. Serve un’integrazione che conservi gli argomenti originali; non aggirare il
controllo CSP. Le verifiche Chrome sono funzionali, non prestazionali.

In 394803 gli smoke browser Linux falliscono sui tre casi residui; entrambi i job runtime Linux e
Windows sono verdi. La CI 39486988220 compila Linux normal/JIT-less e Windows, ma lo smoke Linux
normal fallisce perché la flag V8 causa l’assertion `JS_SetPrototype` durante il setup dei
binding; le successive pagine vuote derivano da quell’abort. Gli altri job macOS sono cancellati
dal `fail-fast`; runtime macOS normal passa.
Nella run 394670 quattro build servoshell compilano, mentre il runtime
macOS normal supera il timeout di 20 secondi nel test runner e quello JIT-less è cancellato.
Il test locale `roves-js::immutable_prototypes_reject_replacement_but_allow_properties` passa.
La build completa locale di servoshell è stata interrotta su indicazione dell’utente per evitare
carico elevato; non ha modificato Cargo.toml/Cargo.lock e non ha prodotto una nuova build
utilizzabile. Nessuna nuova misura prestazionale browser è stata raccolta; i controlli funzionali
Chrome sono riportati sopra. Per il prossimo controllo pesante
preferire il flusso online della CI dopo aver finito e revisionato le modifiche.

### Implementazione presente e lavoro ancora aperto

- Patch 0248: `js_disable_jit` passa `--jitless` all’inizializzazione V8; rimosso il no-op
  `DisableJitBackend`. Compilazione e CI runtime verdi; la selezione effettiva della preferenza
  richiede ancora una verifica dedicata, distinta dalla feature Cargo JIT-less.
- Patch 0249: direct eval inoltra `bodyString` a CSP. Test runtime verdi; la fixture aggiornata
  controlla adesso il risultato numerico, ancora da verificare in browser CI.
- Patch 0250: gestione del prototipo immutabile nei proxy e regressione JSAPI. Il commit
  `4d0579409ed` rimuove il riferimento globale della fixture prima della finalizzazione.
  Test runtime verdi; comportamento Window nel browser ancora aperto.
- Replay sequenziale 0248–0250 verificato localmente sui sei file sorgente, con confronto
  SHA256, e validazione complessiva delle patch verde nella CI generale.
- Questa ripresa aggiorna le fixture CSP/Window/Wasm e `README.md`, `CUSTOMIZATIONS.md`, il presente
  piano e `roves-wiki/content/docs/shell.mdx`. Le fixture modificate sono file
  repository-only: la workflow V8 le usa direttamente e non le include nel replay delle patch Servo.
- Aggiunti i casi browser per `Function` con parametro e corpo `TrustedScript` e per CSP Wasm
  senza `wasm-unsafe-eval`; in 394735 il secondo passa e il primo espone una lacuna runtime. I
  test sintetici verificano il fallimento se mancano i marker. Test locale del runner: 6 eseguiti,
  4 passati e 2 integrazioni POSIX saltate su Windows.
- La run 394786 si è fermata nei due test runtime Linux, prima delle suite V8: il caso
  sintetico `cross-origin-prototype` si aspettava un marker iframe che quello scenario non emette.
  La diagnostica smoke include invece correttamente il testo “cross-origin WindowProxy prototype
  not confirmed”. L’asserzione è stata corretta per verificare quel messaggio; la nuova CI deve
  confermare l’intero runner Linux. Runtime Windows normal/JIT-less passano; la matrice servoshell
  era ancora in corso al momento della diagnosi.
- Patch 0251: rimosse preferenze Baseline/Ion, warmup, regexp, asm.js e Wasm ignorate da V8,
  setup senza effetto, callback build ID SpiderMonkey e workaround Android sui tier rimossi.
  `js_disable_jit` rimane. Replay sequenziale dei tre file da Servo v0.5.0 pristine, dry-run
  0251 e confronto finale con i sorgenti correnti passati; compilazione da confermare in CI.
- Oltre ai test restano: audit completo delle API `roves-js`; diagnosi/correzione del percorso
  Window/WindowProxy e dei trap origin-aware residui; preservazione degli argomenti Trusted
  Types per `Function` e controllo del percorso Wasm; progetto del rilascio di realm, script,
  moduli e callback; eventuali correzioni di shutdown/worker risultanti dall’audit.
- La validazione finale resta aperta: WPT selezionati, stress GC/lifecycle, copertura applicativa,
  misure per modifiche di ownership, artefatti e release/pin. Il dettaglio resta nelle fasi 1–4.

Prossimi passi: diagnosticare l’accesso `contentWindow` e i callback iframe nella nuova CI,
poi correggere Window/WindowProxy e conservare gli argomenti originali TrustedScript di Function.
La CI 394914 conferma le nove suite, eval trusted (42), CSP Wasm e navigazione nelle due modalità
Linux; conferma inoltre la compilazione della patch 0251 sulle tre piattaforme. Non verifica
l’immutabilità del Window browser. Il precedente checkpoint chiamava erroneamente “smoke
Windows verde” lo step saltato: Windows e macOS verificano soltanto la build servoshell.

La fixture ora registra append, load e accesso contentWindow separatamente; il timeout non
legge il proxy e quindi può segnalare il mancato caricamento anche se quel getter fallisce.
Un listener registra gli errori script. In caso di fallimento il runner conserva le ultime 80
righe di ciascuna pagina browser, oltre ai log HTTP. Riutilizzati Bash e Python stdlib:
nessuna dipendenza nuova e nessuna variazione al comportamento runtime. Il risultato browser
della nuova diagnostica resta da confermare in CI. Continuare le fasi 1–4 oltre il checkpoint;
release e pin restano subordinati alla validazione della migrazione.

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

- [x] Ricontrollare le lacune annotate contro codice, test e cronologia recente; continuare a distinguere
  implementazione mancante, comportamento errato e verifica assente.
- [ ] Completare l’audit di `roves-js` cercando funzioni vuote, risultati costanti, API approssimate e
  operazioni che dichiarano successo senza applicare il comportamento richiesto.
- [ ] Per ogni problema registrare chiamanti di produzione, comportamento atteso, riproduzione
  e test di accettazione. Un nome SpiderMonkey, da solo, non è un difetto.
- [x] Correggere il runner smoke: asserzioni fallite, riepiloghi mancanti, crash e timeout prima
  del completamento devono produrre un esito CI negativo. Distinguere il timeout dopo una
  suite completata dal timeout che impedisce di ottenere i risultati.
- [x] Rendere obbligatori identità Window, navigazione e CSP attualmente segnalati soltanto come
  warning. Consentire skip soltanto per capacità esplicitamente non disponibili, come Wasm in
  JIT-less. Uno skip inatteso deve fallire.
- [x] Verificare il runner con fixture intenzionalmente fallite e incomplete, poi eseguire le
  suite esistenti nelle due modalità.

**Criterio di uscita:** ogni voce residua ha evidenze aggiornate e la CI fallisce realmente
quando una verifica obbligatoria fallisce.

### Audit mirato del codice — 19 ottobre 2026

Questa è una verifica statica mirata, non un audit esaustivo delle API compatibili. Non segnare
come risolti i problemi sotto sulla sola base di CI verde o dei nomi delle API.

| Area | Evidenza e chiamanti di produzione | Verifica richiesta per chiudere |
| --- | --- | --- |
| Prototipi immutabili | `JS_SetImmutablePrototype` conserva il prototipo originale; patch 0250 nel dispatcher proxy. | CI `39320063351`: tutti i test runtime verdi, ma marker Window ancora non confermato nei due smoke Linux. Diagnosi browser aperta. |
| Realm | `realm_impl::register_realm` conserva un `Global<Context>` in `cx.realms`; `trace_realms` traccia ogni globale/proxy/drain. | Navigazioni ripetute con rilascio dei realm irraggiungibili; mantenere riferimenti cross-realm e bfcache ancora vivi. |
| Script privates | `script_impl::Scripts::privates` è una mappa tracciata dal runtime; `globalscope/script_execution.rs` la alimenta con `SetScriptPrivate`. | Raccolta dopo perdita dello script e dei suoi callable; conservazione dei privati necessari ai callable ancora vivi. |
| Module privates | `modules_impl::Modules::entries` e `trace_modules` tengono vivi record e privati; `script_module.rs` chiama `SetModulePrivate`. | Raccolta dei grafi modulo irraggiungibili senza rompere import dinamico, namespace conservati e risoluzione dei moduli. |
| TrustedScript | Patch 0249 inoltra il body direct eval; Servo registra `codeForEvalGets` in `script_runtime.rs`. | CI `39462392615` conferma `eval(TrustedScript) === 42`. Nuovi smoke WPT-all-trusted `Function` e CSP Wasm sono aggiunti ma attendono CI. Chrome 155 blocca `Function` con `EvalError`, in conflitto con WPT corrente. |
| Window cross-origin | `window/windowproxy.rs` collega i trap prototipo agli helper Servo; gli altri trap conservano TODO origin-aware. | Le fixture `data:` e HTTP delle run `39462392615` e `39467094586` non confermano il marker. Il watchdog ora viene annullato su load/error. Verificare accessi, navigazione e identità. |
| Callable class boxes | `object::new_callable_class_object` usa un holder tracciato da V8/cppgc; test di raggiungibilità presente. | I sei job runtime di `39320063351` sono verdi. La retention incondizionata è rimossa dalla patch 0236; l’audit lifecycle complessivo resta aperto. |

### Preferenza js_disable_jit — 19 ottobre 2026

L’audit ha trovato `script::init` che dichiarava di disabilitare JIT chiamando il no-op
`DisableJitBackend` in `roves-js`. V8 non espone un toggle JIT per isolate dopo l’avvio:
`--jitless` deve essere impostato prima di inizializzare il platform process-wide. Aggiunto
`JSEngine::init_with_flags` e usato dalla preferenza Servo `js_disable_jit`; l’inizializzazione
predefinita e la feature di build `roves-js/jitless` restano invariate. Rimossa la funzione
compatibilità senza effetto. Replay patch e CI runtime `39320063351` verdi. Resta da verificare
direttamente la preferenza, poiché la matrice JIT-less usa anche la feature Cargo dedicata.

### Esito dell’audit mirato delle API JSAPI — 19 ottobre 2026

Controllati i candidati `return true`, hook vuoti e operazioni apparentemente no-op nei
percorsi chiamati da `script`/`script_bindings`. Non ogni stub mozjs è un difetto: V8 non ha
compartments o CCW, quindi `JS_WrapObject/Value` conserva l’identità; l’outerizing di Window è
eseguito dal helper Servo `maybe_wrap_object_value`. Gli hook legacy
`JobQueueIsEmpty/JobQueueMayNotBeEmpty` non eseguono Promise jobs nel backend V8: V8 segnala i
job con il promise hook, e `request_drain` inserisce il checkpoint nella microtask queue Servo.
`JS_IdToValue` è chiamata nei record e nella conversione chiavi; l’id void è escluso nei
percorsi che ne richiedono il controllo. `JS_ValueToSource` è dichiarata approssimata e resta
una differenza da tracciare se una chiamata di produzione ne dipende per comportamento web.

Confermata la retention di script/moduli: `SetScriptPrivate` usa gli hook add/release Servo per
conteggiare le `Rc` top-level, mentre `trace_scripts` e `trace_modules` mantengono le celle e i
privati tracciabili. Rimuovere tali entry dopo il GC senza una relazione di raggiungibilità
V8-side romperebbe `HostResolveImportedModule`/dynamic import o lascerebbe puntatori non validi.
Il rilascio coordinato di realm, script e moduli resta quindi un’attività di lifecycle da
progettare e verificare con stress GC, non una cancellazione sicura della mappa. Nessuna
modifica è stata fatta a questi punti in questo audit.

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
browser `Object/Reflect` su global e vero `WindowProxy` è stato aggiunto allo smoke CI e resta da
confermare prima di chiudere la voce nella tabella. Dopo il fallimento `39312754922` è stata
aggiunta la patch 0250 al dispatcher dei proxy per il ramo con trap `setPrototypeOf`
personalizzato. La correzione intende rifiutare il prototipo diverso anche in quel ramo e aggiunge una
regressione JSAPI proxy; il primo test mirato ha trovato un riferimento globale trattenuto dalla
fixture, ora cancellato prima del controllo di finalizzazione. Il test proxy mirato passa dopo la
correzione. In `39320063351` tutti i test runtime passano, ma lo smoke browser continua a non
confermare il prototipo immutabile Window: causa e correzione di produzione restano aperte.
Nessuna libreria aggiunta: rusty_v8 già offre
handle `Global` e cppgc; serde_v8 non serve per identità DOM, deno_core duplicherà il runtime
senza sostituire l’event loop/realm/CSP Servo, web-audio-api-rs resta fuori ambito secondo il
piano.

### Correzione TrustedScript in `eval` — 19 ottobre 2026

L’hook rusty_v8 passa al callback il valore originale di `eval`. Per i valori stringa il
callback CSP continua a controllare la compilazione; per gli oggetti usa ora il callback Servo
`codeForEvalGets`, che estrae sorgente solo da `TrustedScript`, e inoltra l’oggetto originale
come `bodyArg` al controllo Trusted Types/CSP. Gli oggetti ordinari restano invariati come
richiesto dalla semantica di `eval`. Un test `roves-js` verifica sorgente estratta, passaggio
dell’oggetto al callback e identità degli oggetti normali. Lo smoke CSP browser verifica
`TrustedScript` consentito sotto una policy che richiede trusted types. La run V8
`39312754922` ha fallito sia in normal sia in JIT-less: `csp.html` ha ricevuto `EvalError`.
La causa isolata è il `bodyString` vuoto passato da `csp_allows` per direct eval, mentre Servo
confronta quel valore con i dati di `TrustedScript`. La patch 0249 passa come body la sorgente
completa di direct eval e aggiorna le asserzioni del test runtime. In `39320063351` i sei job
runtime passano e lo smoke stampa `trusted eval allowed` in normal e JIT-less. Il runner
cerca però `trusted eval 42`, impossibile con l’attuale helper `attempt` che scarta il risultato.
La verifica del valore va corretta; `Function(TrustedScript)` e parametri trusted della Function
constructor restano da esaminare separatamente.

Il callback CSP per stringhe e Wasm era già installato in `runtime_impl`; la lacuna era
l’integrazione dell’operando oggetto di `eval` con il callback Servo esistente. La prossima
modifica di ownership deve riusare weak handle/cppgc di V8 e conservare tutte le radici
necessarie; eliminare una voce dalla mappa senza un contratto di raggiungibilità non è una
correzione sicura.

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
`0243-v8-smoke-dom-output.patch`. La run `39295524530` sul commit
`37e1e3f9f12` è verde su Linux normale/JIT-less, Windows e macOS: tutti i test
WebGPU (12/12, un hardware skip autorizzato) e gli smoke di DOM, Window/globalThis,
CSP e navigazione passano in entrambe le modalità Linux. Il test locale del parser
passa; i due test d’integrazione shell sono saltati su Windows perché richiedono Bash
POSIX, e sono eseguiti nella matrice Linux/macOS. La fase 1 resta aperta per l’audit
residuo del backlog e la copertura applicativa/WPT elencata nelle fasi successive.

### Trap del prototipo WindowProxy — 19 ottobre 2026 (implementato, CI fallita)

`WindowProxy` non collegava i trap `getPrototype`/`setPrototype`; la variante cross-origin
lasciava anche il controllo `getPrototypeIfOrdinary` al comportamento predefinito. Collegati gli
helper Servo già presenti: `GetPrototypeOf` restituisce il prototipo Window per oggetti same-origin
e `null` cross-origin; `SetPrototypeOf` accetta solo lo stesso prototipo. La variante cross-origin
ora dichiara il trap prototype non ordinario. Il dispatcher V8 applica inoltre il flag di prototipo
immutabile anche in presenza di trap Servo personalizzati, preservando il loro controllo
dell’origine. Nessuna nuova libreria o logica duplicata. Lo smoke
strict richiede il marker cross-origin per un iframe `data:` con origine opaca. Riferimento Chrome
DevTools: Chrome 155.0.0.0, Windows x64, pagina `file://` del test e iframe `data:`; il marker
`true true` è stato osservato in Chrome, mentre Chrome blocca anche la lettura diretta di `Location.origin`; questo non verifica Servo.
Le compilazioni locali di `script_bindings` e `script` passano. `cargo check --locked --offline`
`-p servo-script` passa con `AWS_LC_SYS_NO_ASM=1`, necessario perché la build locale di
`aws-lc-sys` non trova NASM. La run V8 `39312754922` ha però fallito in normal e JIT-less: la
smoke headless non ha confermato il prototipo immutabile del Window, il prototipo cross-origin
del WindowProxy né `eval(TrustedScript)`. La successiva `39320063351` conferma i test runtime
dei fix ma continua a non confermare entrambi i marker di prototipo nel browser. Trusted eval
è ora consentito, con un errore nel matcher della fixture descritto nel punto della situazione.
Il marker cross-origin resta senza diagnosi e potrebbe non essere eseguito dopo un’eccezione
nei controlli Window che lo precedono.
Il controllo Chrome DevTools conferma soltanto il comportamento di Chrome 155, non quello di
Servo. La diagnosi di queste regressioni precede la chiusura della migrazione.

### Verifica della smoke MSI Windows — 19 ottobre 2026

La matrice `39296023222` ha fallito solo il controllo del gamepad nell’MSI: l’installazione
e l’avvio erano riusciti, ma la pagina di prova diventava pronta circa 24,5 secondi dopo
l’avvio e il processo veniva terminato a 25 secondi, prima del primo evento (driver SDL
ogni 500 ms). La prima correzione dinamica ha atteso il gamepad, poi ha terminato prima del
risultato `roves:save_file`: Windows portable e MSI sono entrambi falliti con il marker di
salvataggio mancante nella matrice `39302751406`. La nuova attesa richiede sia l’evento gamepad
(dove il test è abilitato) sia il marker `roves:save_file`, con limite di 75 secondi;
Linux/macOS usano la stessa condizione per il marker, Linux richiede anche l’input sintetico.
La matrice generale `39320063236` sul commit `4d0579409ed` è terminata verde (13/13 job),
inclusi Windows portable/MSI e gli altri quattro bundle desktop: questa regressione del runner
risulta superata nella CI corrente.

## 2. Correggere semantica e ciclo di vita

- [ ] Verificare WindowProxy durante navigazione, accessi tra origini e cambi di origine.
  I trap prototipo riusano gli helper Servo; lo smoke strict ora copre prototipo same-origin,
  `null` cross-origin, scrittura invariata/respinta e preserva identità, descrittori ed eccezioni.
- [ ] Completare la correzione di `JS_SetImmutablePrototype` sul percorso Window di produzione:
  test JSAPI verdi, smoke browser ancora fallito in `39320063351`.
- [ ] Verificare `TrustedScript` e CSP insieme a `eval`, `Function` e compilazione Wasm; il
  CI V8 `39462392615` conferma il risultato `eval(TrustedScript)`; la nuova fixture controlla
  parametri/corpo trusted di Function secondo WPT e la compilazione Wasm senza
  `wasm-unsafe-eval`. Attendere la CI per chiudere questi due percorsi e preservare le decisioni
  di sicurezza del runtime web.
- [ ] Esaminare proprietà e radici di realm, script, moduli e callback. Rilasciare gli oggetti
  non più raggiungibili dopo navigazione o chiusura, preservando quelli richiesti da
  riferimenti JS e bfcache.
- [x] Ricontrollare i costruttori callable: il collegamento alla raggiungibilità JS è già stato
  introdotto e non deve essere ripianificato come interamente mancante.
- [ ] Verificare creazione/terminazione worker, interruzione di script, shutdown e callback
  pendenti, evitando accessi a isolate o contesti distrutti.

**Criterio di uscita:** test ripetuti di navigazione, worker e GC dimostrano identità corretta,
finalizzazione senza duplicazioni e rilascio degli oggetti diventati irraggiungibili.

## 3. Ridurre il codice attraverso librerie mantenute

Prima di ciascuna sostituzione confrontare manutenzione, licenza, piattaforme, JIT-less e costi
CPU/RAM/GPU. Aggiungere dipendenze soltanto insieme a un uso concreto e alla rimozione del
codice sostituito. Documentare anche le valutazioni che concludono di non aggiungere librerie.

- [x] **rusty_v8 già presente — valutazione completata:** `roves-js` usa già i suoi `Global`,
  cppgc, `BackingStore`, typed array e `ValueSerializer`. L’ispezione dei percorsi concreti non
  ha trovato un adattatore parallelo sostituibile senza cambiare i contratti JSAPI/DOM. Restare
  sulla API V8 diretta per nuove migrazioni e rimuovere gli adattamenti solo quando i chiamanti
  risultano sostituiti.
- [x] **Servo WebIDL — valutazione completata:** conservare parser e semantica del generatore.
  `v8_coverage.py` accetta 477/477 definizioni alla fase di generazione; questa misura non
  compila né esegue i binding. Il pilot opt-in esporta solo `Screen` e `ValidityState`, non è
  usato dal DOM di produzione e non dimostra compatibilità di runtime. Non adottarlo come
  backend di produzione finché non copre e verifica un insieme ampio di interfacce reali.
- [x] **serde_v8 — non adottata:** nessuna conversione di API native Roves da tipi Serde è stata
  identificata; l’interfaccia corrente passa valori JS/DOM e le conversioni frequenti devono
  preservare identità. Il crate non è nel `Cargo.lock`; l’upstream dedicato è archiviato e il
  codice è stato spostato in `denoland/deno`. Nessun nuovo costo CPU/RAM e nessun rischio di
  compatibilità V8 introdotti. Rivalutare solo con una API concreta che elimini codice esistente.
- [x] **deno_core — non adottato:** il repository autonomo è archiviato e integrato in
  `denoland/deno`; aggiungerlo porterebbe un secondo livello runtime oltre a quello Servo, senza
  eliminare event loop browser, realm, CSP, loader, cppgc o integrazione JIT-less. Nessuna
  riduzione complessiva dimostrata; memoria e complessità di versionamento aumenterebbero.
- [x] **web-audio-api-rs — non adottata nella migrazione V8:** il progetto è attivo e MIT, ma
  fornisce un grafo WebAudio Rust con backend audio nativi, non sostituisce il binding browser
  Servo né certifica AudioWorklet JS V8. Sposterebbe lavoro CPU/audio e gestione dispositivi
  senza ridurre il bridge DOM oggetto di questo piano; valutarla nel backlog WebAudio separato.
- [x] Rimosso il wiring Cargo/runtime del pilot WebIDL: non generava binding nelle build normali,
  ma aggiungeva una feature e una dipendenza opzionale da `roves-v8` solo per esporre due moduli
  sperimentali. Il driver Python, il backend di generazione e i test restano strumenti di misura;
  non sono codice runtime. Continuare a eliminare feature senza effetto solo quando i chiamanti
  sono verificati, preservando rooting/tracing e senza imporre la rimozione di `roves-js`.

**Criterio di uscita:** ogni sostituzione elimina codice effettivamente usato in precedenza,
passa i test pertinenti e ha una decisione documentata. Una valutazione negativa motivata
chiude il relativo candidato senza introdurre una dipendenza.

Riferimenti dei candidati:

- rusty_v8: <https://github.com/denoland/rusty_v8>
- serde_v8: <https://docs.rs/serde_v8/>
- deno_core: <https://github.com/denoland/deno_core>
- web-audio-api-rs: <https://github.com/orottier/web-audio-api-rs>

La valutazione di stato/manutenzione è stata ricontrollata il 19 ottobre 2026 sulle pagine
upstream: `deno_core` è archiviato e confluito in Deno; `serde_v8` è archiviato come repository
separato e rimandato alla directory `serde_v8` di Deno; `web-audio-api-rs` mantiene release
recenti. Rileggere queste decisioni prima di un futuro aggiornamento versioni o di una modifica
alle API native.

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
Alla revisione del 20 ottobre 2026 la CI V8 `39467094586` è fallita: quattro job runtime e
quattro compilazioni servoshell passano, il runtime macOS normal supera il timeout del test
runner e gli smoke Linux falliscono; macOS JIT-less è cancellato. Il dettaglio e i limiti
delle evidenze sono in apertura. Le nuove asserzioni trusted Function e Wasm CSP attendono CI.
Il workflow generale precedente `39320063236` è verde, 13/13 job.

I tipi V8 restano confinati al livello scripting. Non sono previste nuove API pubbliche Roves;
eventuali cambiamenti del comportamento web devono seguire gli standard e avere test dedicati.

- [ ] Trasferire l’architettura stabile nella documentazione permanente e i problemi
  prestazionali in `ROVES_PERFORMANCE.md`.
- [ ] Eliminare i documenti temporanei `V8_MIGRATION*` quando non contengono più attività aperte,
  aggiornando i riferimenti nelle istruzioni e nel README. La cronologia resta in git.

Procedere per modifiche buildabili: verifica locale pertinente, commit, push, attesa della CI e
correzione dei fallimenti prima del passo successivo. Non dichiarare conclusa la migrazione
sulla base di sole compilazioni o dell’avvio della CI.
