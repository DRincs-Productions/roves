# Ripresa per Claude — checkpoint del 23 ottobre 2026

## Stato e pubblicazione

- Pubblicato: `a4599fb2a5e` (commit vuoto di retry sopra `2e96c8663aa`, che contiene 0282–0286),
  wiki `0928dba`. Claude lavora su Linux/WSL **senza credenziali push**: commit solo locali,
  l'utente pubblica periodicamente con `git push origin main` (wiki prima del puntatore root).
- Locale non pubblicato: 0287 (commit root + wiki, vedi `git log`).
- CI su `2e96c86`: V8 `39852305242` verde, Android/iOS verdi; generale `39852305257` rossa solo
  per timeout `mach bootstrap` (apt lento, 90 min, prima della compilazione) del job
  `linux portable`. Il commit vuoto ha avviato solo Android/iOS (verdi), perché generale e V8
  sono filtrate per percorso. Il prossimo push con 0287 (tocca `patches/**` e `components/roves-v8/**`)
  rilancia entrambe: controllarle; se il timeout si ripete, valutare `--no-install-recommends`.
- PAT di sola lettura: l'utente lo fornisce in chat; usarlo inline, mai salvarlo.

## Verifiche locali (Linux/WSL)

`cd components/roves-v8 && CARGO_TARGET_DIR=<repo>/target/roves-v8 cargo test [--features jitless]`
e analogo per `components/roves-js` con `target/roves-js`. Stato 0287: motore 67+2 doctest,
runtime 100/99 (2/3 ignorati). Log `target/task-0287/`.

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

1. Dare priorità a errori delle run effettive del checkpoint0281; non presumere corretto il
   frontend solo perché il runtime isolato passa. Non avviare Claude oggi.
2. Proseguire lo sviluppo del piano V8, studiando prima riuso di API/librerie mantenute.
   Cache Function0273, navigazione/identità WindowProxy e incumbent sono ancora aperti.
   Worker/termination/shutdown e WPT/integration richiedono ulteriore copertura.
3. Task consegnati (0285) e ritardati alla rimozione handler (0287) sono recuperati; task
   immediati senza handler, task in consegna durante la rimozione, sostituzione handler e post
   tardivi dopo chiusura restano conservativi: non dichiarare assenza leak.
   Dati callback in-flight devono rispettare il lifetime dell'embedder.
4. Servo mantiene global e mappe moduli; JSON linkati mai valutati restano forti; realm
   iniziale intenzionalmente forte. Non dichiarare raccolta pagina o risparmioCPU/RAM.
5. Non pubblicare release/pin consumer finché la migrazione e i criteri del piano non lo
   giustificano. Nessun flag build/bundle/workflow è cambiato in0281–0286.

Cache Function: il WIP0273 locale target/0273-incomplete-native-traps.patch NON va applicato
alla cieca. Native Proxy traps eliminano il frameJS helper, ma V8 GetFunctionFromEval cerca
la cache con outer_info empty_function prima di correggere eval_from_shared dal chiamante;
corpi identici nello stesso realm possono riusare l'origine precedente. Non mascherare con
commenti/whitespace nel codice visibile o disabilitare globalmente la cache senza valutazione.
WindowProxy: JS_TransplantObject sui global attualmente ritorna il replacement: vecchi riferimenti
vedono il vecchio Window. Serve progettare riuso/detach del global proxy alla creazioneContext,
coordinato con attivazione documento, evitando retarget anticipato o perdita del vecchio global.

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
