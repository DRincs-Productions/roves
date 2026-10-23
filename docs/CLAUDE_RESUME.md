# Ripresa per Claude — checkpoint del 23 ottobre 2026

## Stato e pubblicazione

- Pubblicato: `a4599fb2a5e` (commit vuoto di retry sopra `2e96c8663aa`, che contiene 0282–0286),
  wiki `0928dba`. Claude lavora su Linux/WSL **senza credenziali push**: commit solo locali,
  l'utente pubblica periodicamente con `git push origin main` (wiki prima del puntatore root).
- Batch `8799e4e` (0287) tutto verde: V8 `39917959741`, generale `39917959712` (anche il job
  Ubuntu portable, timeout di ieri risolto), Android/iOS.
- `9781833`: V8/generale annullate dal push successivo (non verdi, superate da `5967ef4`).
- `5967ef4` (0288–0293): V8 `39923807410` ROSSA solo per i due job Ubuntu servoshell, timeout
  apt in `mach bootstrap` (485 pacchetti, mirror Azure lento); runtime su tutte le piattaforme
  e smoke Windows/macOS verdi (0289 si collega con MSVC). Correzione locale in `v8.yml`
  (niente pacchetti GStreamer su Linux). Generale `39923807266` in corso, Android/iOS verdi.
- Pubblicato `607b2e5` (fino a 0301 e correzioni apt): TUTTE le CI verdi (V8 `39937024546`,
  generale `39937024442`, Android, iOS). Bootstrap Linux 46–204 s invece dei timeout.
- Locale non pubblicato: aggiornamenti wiki/doc, decisioni WindowProxy, 0302/0303 pubblicate (`17654e7`, CI in corso); locale: piano aggiornato, 0304 diagnostica frame, 0305 test, 0306 unload prima del passaggio del proxy, 0307 correzione del panic 0303 (realm della vecchia Window), 0308 wrapper cross-realm che seguono la navigazione. CI `17654e7`: smoke Ubuntu rosso per quel panic.
- PAT di sola lettura: l'utente lo fornisce in chat; usarlo inline, mai salvarlo.

## Decisioni aperte per l'utente (23 ottobre)

1. **Identità WindowProxy — IMPLEMENTATA in locale (0303), da verificare in CI/browser.**
   Approccio più piccolo del previsto: il proxy si riusa a `Window::new` (canale
   `RovesReuseWindowProxyForNextGlobal` da `ScriptThread::load`), la vecchia cella global va su
   un oggetto nascosto, `session_history_max_length`=0 (avanti/indietro ricaricano). Smoke:
   `events.html` iframe contentWindow; `nav1` ora `persisted=false`. Analisi precedente:
   **scelta utente 23/10: riuso + ricarica.**
   Analisi successiva: in roves-js il global della Window E il global proxy sono lo stesso
   oggetto V8 (`JS_NewGlobalObject` attacca la cella del global a `context.global()`, cioè al
   proxy; `NewWindowProxy` aggiunge una seconda cella allo stesso oggetto). V8 azzera lo
   stato del proxy riusato: la cella del global della vecchia Window (reflector Servo)
   punterebbe alla nuova. Prerequisito: separare global interno (JSGlobalObject = reflector
   Window) e global proxy (= WindowProxy) come Blink, rivedendo this/holder dei binding Window,
   cache wrapper, `window === globalThis` e tracing; poi plumbing in script e ricarica alla
   ripresa (eviction della pipeline precedente). Lavoro di più giorni, confermato all'utente
   prima di iniziare. Testo precedente: Correggerla con V8 richiede di riusare il global
   proxy (`DetachGlobal` + `ContextOptions::global_object`) al momento della creazione della
   nuova Window. Un documento sospeso (cronologia) che ha ceduto il proxy non può più riaverlo:
   andrebbe ricaricato invece che ripreso, come fa Blink per le pagine non in bfcache. Opzioni:
   (a) riuso + ricarica alla ripresa; (b) lasciare l'identità per documento (attuale). Per
   giochi a pagina singola l'impatto pratico è basso.
2. **Codice di fase 1 in `roves-v8` — RIMOSSO (0302)** dopo il chiarimento dell'utente: non serve
   ai confronti prestazioni (support/perf) né alla produzione. Testo storico: La produzione usa solo `initialize_engine*`,
   `foreground` e `incumbent`. `Runtime`, i tipi WebIDL, `roves-v8-derive`, il backend V8 di
   `codegen.py` e `test_v8.py` servono solo come strumento di misura del pilot WebIDL (decisione
   del piano: conservarli). Rimuoverli toglierebbe migliaia di righe nostre.

## Verifiche locali (Linux/WSL)

`cd components/roves-v8 && CARGO_TARGET_DIR=<repo>/target/roves-v8 cargo test [--features jitless]`
e analogo per `components/roves-js` con `target/roves-js`. Stato 0289: motore 68+2 doctest,
runtime 102/101 (2/3 ignorati). Log `target/task-0287/` … `target/task-0289/`.

## Istruzioni operative attuali

Leggere AGENTS.md, questo file e soltanto l'intestazione corrente di docs/V8_MIGRATION.md.
La chat non serve. Dal 23 ottobre l'utente ha ripreso con Claude, che sviluppa in autonomia
con commit locali. Non creare automazioni o tentare cambi modello/aggiramenti di quota.
Nessun tetto o contabilità per incarico. Lavoro circoscritto e completo, verifiche senza
duplicazioni, build Servo pesanti online. Aggiornare questo file a ogni checkpoint.
docs/handoff/0281-incomplete-realm-registry.patch è SOLO archivio storico: NON riapplicarla.

## Cosa è stato implementato

| Passo | Stato e risultato locale |
| --- | --- |
| 0281 | RealmData cppgc, Context tracciato, holder nel context, registro debole, current/saved roots, RealmSwitch RAII, principal una volta. Corretto holder/cache nel context di destinazione.94/93 test. |
| 0282 | drain_pending per RuntimeHooks invece diTLS: un drain abbandonato non sopprime i job del successivo runtime sullo stesso thread.95/94 test. |
| 0283 | Timer cattura Weak<Registration> originale invece di cercare il handler per indirizzo isolate alla scadenza. Rimozione/sostituzione invalida la registrazione. Motore65+2doctest normal/JITless, runtime95/94. |
| 0284 | Payload Dispatchable con owner debole; non esegue task in runtime sbagliato o già chiuso. Close impedisce inserzioni dopo shutdown.97/96 test. |
| 0286 | Rilascio callback ritirate fuori dal mutex globale: evita deadlock da distruttori reentranti. Motore66+2doctest normal/JITless, runtime100/99 (2/3ignorati); negativo e replay2 precedenti verificati. |
| 0287 | `set_handler(None)` distrugge i task ritardati della registrazione ancora nel timer, fuori dai lock; accodamento sotto lock registro. Test reale `Atomics.waitAsync`, negativo verificato. Motore67+2doctest, runtime100/99. |
| 0288 | Ex 0273: trap Proxy native per Function/generator/async; `import()` nel corpo usa il chiamante. Corpo identico riusa il primo referrer come V8 nativo (cache eval; disattivata solo con debugger). Runtime101/100. |
| 0289 | Incumbent V8: `GetIncumbentContext`/`BackupIncumbentScope` via simboli C++ esportati (Itanium/MSVC, verificati con llvm-nm). Caller global = frame più interno; Hide/Unhide = scope con sentinella. Runtime102/101. Link Windows/macOS da CI. |
| 0290 | Costruttori dinamici come funzioni native con nome (non Proxy): `toString` come V8 nativo. Runtime102/101. |
| 0294 | `entry_global()` di Servo: fallback al realm del frame più interno quando V8 esegue JS da un proprio task (cleanup FinalizationRegistry) senza voce Entry; prima panic con drawImage/Location. Crate script non compilato localmente: verifica CI + smoke gc-lifecycle. |
| 0285 | Runtime possiede gli slot Arc dei task accettati, consuma Option<Task> una volta e distrugge nativi prima di dismettere isolate; payload tardivi vuoti, rifiuti recuperati a shutdown, metadati completati ripuliti ammortizzati.100/99 test. |

Il primo numero dei test è normal, il secondo JITless; restano2/3 test ignorati come prima.
Non attribuire questi risultati alla CI browser. Patch source reali0281–0286 sotto
patches/servo-v0.5.0, replay su pristinev0.5.0+patch precedenti e confronto byte per byte
con sorgenti/copia testata verificati. Test negativi mirati rilevano gli edge mancanti.
Per0285 i test consegna/rifiuto usano Task V8 reali da FinalizationRegistry, non puntatori finti.

## CI da controllare prima di ogni altro push

Vedi "Stato e pubblicazione" in cima. Regole: una rossa va analizzata e corretta; run
cancellate/saltate non sono verdi; non cancellare run senza istruzione esplicita.

## Prossimi punti della migrazione

1. Dare priorità agli errori delle run CI effettive dopo ogni push; non presumere corretto il
   frontend solo perché il runtime isolato passa.
2. Proseguire lo sviluppo del piano V8, studiando prima riuso di API/librerie mantenute.
   Navigazione/identità WindowProxy resta aperta (Function chiusa in 0288, incumbent in 0289).
   Worker/termination/shutdown e WPT/integration richiedono ulteriore copertura.
3. Task consegnati (0285), ritardati alla rimozione handler (0287) e postati tra close e
   rimozione (0299) sono recuperati; task immediati senza handler, task in consegna durante
   la rimozione, sostituzione handler e post ancora in volo dopo `retire` restano
   conservativi: non dichiarare assenza leak.
   Dati callback in-flight devono rispettare il lifetime dell'embedder.
4. Servo mantiene global e mappe moduli; JSON linkati mai valutati restano forti; realm
   iniziale intenzionalmente forte. Non dichiarare raccolta pagina o risparmioCPU/RAM.
5. Non pubblicare release/pin consumer finché la migrazione e i criteri del piano non lo
   giustificano. Nessun flag build/bundle/workflow è cambiato in0281–0288.

Function: chiusa in 0288 (trap native). Il WIP 0273 su Windows è superato, non riapplicarlo.
Corpo identico = primo referrer è comportamento V8 nativo (cache eval), documentato.
WindowProxy: JS_TransplantObject sui global attualmente ritorna il replacement: vecchi riferimenti
vedono il vecchio Window. Serve progettare riuso/detach del global proxy alla creazioneContext,
coordinato con attivazione documento, evitando retarget anticipato o perdita del vecchio global.
Verificato 23/10: rusty_v8 152 espone `ContextOptions::global_object` (riuso del global proxy
in Context::new); `Context::DetachGlobal` non è bindato ma è esportato e si collega come in
0289 (`_ZN2v87Context12DetachGlobalEv`, MSVC `?DetachGlobal@Context@v8@@QEAAXXZ`). Il blocco
è di progetto: V8 collega un global proxy a un solo context e solo alla creazione, mentre Servo
riprende documenti sospesi (Document::set_activity → Window::resume → set_currently_active) e
attiva la nuova Window dopo l'avvio del parsing (script_thread.rs ~3089). Riusare il proxy
toglierebbe l'identità al documento ripreso: serve decidere una politica (es. ricaricare
invece di riprendere quando il proxy è passato a un nuovo documento) prima del codice.

## Verifiche e artefatti locali riutilizzabili

Workspace isolato target/v8-incumbent-audit (roves-js,roves-v8,derive); se manca ricrearlo da
manifest/sorgenti correnti, non da vecchioWIP. CARGO_TARGET_DIR assoluto:
C:/Users/simon/3D Objects/roves/target/roves-js. Cargo offline normal e --features jitless.
Non ripetere test su copie identiche senza un dubbio concreto. Copie sorgenti vanno confrontate
prima di riusare prove. Log target/task-0281 ... task-0286 (ignored, non disponibili da clone).
I dettagli verificabili restano nelle voci CUSTOMIZATIONS e nel piano V8. Replay più recente:
target/task-0286/replay.log; patch source macchina-applicabile è versionata. Su Windows
GNU patch è C:/Program Files/Git/usr/bin/patch.exe; roves-js/roves-v8 non esistono pristine.
Usare sezioni delle patch precedenti sui file interessati; confrontare prima con HEAD, poi
applicare nuova patch dry-run/apply e confrontare con working tree. Non usare tagv0.4.0.
