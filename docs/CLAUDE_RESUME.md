# Ripresa per Claude — checkpoint del22ottobre2026

## Istruzioni operative attuali

Leggere AGENTS.md, questo file e soltanto l'intestazione corrente di docs/V8_MIGRATION.md.
La chat non serve. Oggi Codex sviluppa direttamente: l'utente aveva letto male la quota
(Codex97% residuo), mentre Claude ha esaurito la finestra sessione; nessuna invocazione Claude
è stata fatta dopo lo stop. Domani l'utente vuole riprendere manualmente con Claude.
Non creare automazioni o tentare cambi modello/aggiramenti di quota. Nessun tetto25/50 o
contabilità per incarico. Un solo implementatore, lavoro circoscritto e completo, verifiche
senza duplicazioni, build Servo pesanti online. Aggiornare questo file a ogni checkpoint.

## Git: verificare lo stato reale prima di toccare i sorgenti

- Pubblicato Roves8b536187562 (contiene0281), wiki dcfd9bd.
- Commit locali completi successivi: e0478fb5acd (regola sviluppo duranteCI),7813d44dbc0
  (0282),d53e6b51031 (0283),7bfad886f07 (0284), e7f96b0cf99 (0285), più il checkpoint0286 corrente.
- Wiki locale successiva:2cc0a62/91f35c8/0c5b380, 05365d3 (0285), più0286 corrente.
- Nessun WIP di questi interventi: sono completi e verificati localmente. Leggere git log e
  git status per gli SHA finali del checkpoint e del submodule. Non resettare lavoro altrui.
- docs/handoff/0281-incomplete-realm-registry.patch è SOLO archivio storico:0281 è stata
  completata e pubblicata. NON riapplicarla. Anche il relativo design è storico.

## Cosa è stato implementato

| Passo | Stato e risultato locale |
| --- | --- |
| 0281 | RealmData cppgc, Context tracciato, holder nel context, registro debole, current/saved roots, RealmSwitch RAII, principal una volta. Corretto holder/cache nel context di destinazione.94/93 test. |
| 0282 | drain_pending per RuntimeHooks invece diTLS: un drain abbandonato non sopprime i job del successivo runtime sullo stesso thread.95/94 test. |
| 0283 | Timer cattura Weak<Registration> originale invece di cercare il handler per indirizzo isolate alla scadenza. Rimozione/sostituzione invalida la registrazione. Motore65+2doctest normal/JITless, runtime95/94. |
| 0284 | Payload Dispatchable con owner debole; non esegue task in runtime sbagliato o già chiuso. Close impedisce inserzioni dopo shutdown.97/96 test. |
| 0286 | Rilascio callback ritirate fuori dal mutex globale: evita deadlock da distruttori reentranti. Motore66+2doctest normal/JITless, runtime100/99 (2/3ignorati); negativo e replay2 precedenti verificati. |
| 0285 | Runtime possiede gli slot Arc dei task accettati, consuma Option<Task> una volta e distrugge nativi prima di dismettere isolate; payload tardivi vuoti, rifiuti recuperati a shutdown, metadati completati ripuliti ammortizzati.100/99 test. |

Il primo numero dei test è normal, il secondo JITless; restano2/3 test ignorati come prima.
Non attribuire questi risultati alla CI browser. Patch source reali0281–0286 sotto
patches/servo-v0.5.0, replay su pristinev0.5.0+patch precedenti e confronto byte per byte
con sorgenti/copia testata verificati. Test negativi mirati rilevano gli edge mancanti.
Per0285 i test consegna/rifiuto usano Task V8 reali da FinalizationRegistry, non puntatori finti.

## CI da controllare prima di ogni altro push

Batch pubblicato8b536187562:
- V8 run39844591657:9/10 verdi; Ubuntu normal scaduto nel download apt al bootstrap30min, prima della compilazione. Retry del solo fallito richiesto una volta; controllarne esito.
- Generale run39844592094: attiva all'ultimo controllo.
- Android39844592038 e iOS39844591645: verdi.

Non cancellare né soppiantare queste run senza un'altra istruzione esplicita. Se non ci sono
rosse e il batch è attivo, continuare modifiche/revisioni/commit LOCALI, senza push.
Al completamento di ogni implementazione controllare il batch aggregato. Una rossa va
analizzata subito e corretta; attendere anche le altre run prima del push. Tutte verdi:
pubblicare il lavoro completo accumulato, wiki prima del puntatore submodule root, quindi
proseguire mentre gira la nuovaCI. Run cancellate/saltate non sono verdi.

Il batch1094b88f051 era stato annullato su esplicita richiesta utente per pubblicare0281;
non è un precedente che autorizza cancellazioni automatiche. Su47a5cd351e1 erano verdi
V8/Android/iOS e12/13 generale; Steam era scaduto nel bootstrap90min prima di compilare.

## Prossimi punti della migrazione

1. Dare priorità a errori delle run effettive del checkpoint0281; non presumere corretto il
   frontend solo perché il runtime isolato passa. Non avviare Claude oggi.
2. Proseguire lo sviluppo del piano V8, studiando prima riuso di API/librerie mantenute.
   Cache Function0273, navigazione/identità WindowProxy e incumbent sono ancora aperti.
   Worker/termination/shutdown e WPT/integration richiedono ulteriore copertura.
3. Task accettati consegnati sono ora recuperati, ma task senza handler, timer pendenti e
   post tardivi dopo chiusura mantengono politiche conservative: non dichiarare assenza leak.
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
