# Handoff chat — Proseguimento probe Blitz + Deno/V8

- **Data:** 7 novembre 2026 (Europe/Rome)
- **Repository:** `C:\Users\simon\3D Objects\roves`
- **Destinatario:** Claude Code, incarico di implementazione delegato dall’utente.
- **Handoff tecnico di riferimento:** [2026-11-06-blitz-v8-migration.md](2026-11-06-blitz-v8-migration.md). Consultarlo per la motivazione, fonti, vincoli completi e gate di migrazione; questo handoff ne riassume lo stato operativo corrente.

## Obiettivo e vincoli

Proseguire la valutazione/integrazione autorizzata di Blitz per DOM, CSS, layout e rendering, mantenendo **Deno + V8 come unico runtime JavaScript**, proprietario di moduli, microtask ed event loop. Il ponte tra JS e Blitz deve restare integrato nel runtime esistente. Non rimuovere Servo dalla produzione finché non siano dimostrate parità funzionale, sicurezza e supporto delle piattaforme, secondo l’handoff tecnico.

Prima di ogni modifica valuta le librerie mantenute già disponibili per ridurre il codice Roves. Non aggiungere un secondo runtime JS/event loop, non esporre API privilegiate Deno al gioco. Mantieni CSP/origini, `game://`, JITless e policy single-document. Nessun commit/push di WIP; preserva le modifiche preesistenti nel working tree.

## Stato verificato

Il probe isolato `support/blitz-v8-probe/` usa la linea ufficiale DioxusLabs Blitz e `deno_core 0.413.0` / `deno_v8 0.5.0`:

- `cargo fmt`, `cargo check --locked` e `cargo run --locked` completati con successo su Windows.
- Il test esegue JS in V8, chiama un op Rust sincrono di deno_core, modifica un nodo testo in Blitz e risolve lo stile/layout a 640×360.
- `cargo tree -i v8` mostra una sola V8 (`150.4.0`) tramite `deno_v8` → `deno_core`.
- Esito registrato dal probe: mutazione DOM da JS e passaggio stile/layout superati. Non sono state misurate prestazioni o fluidità.
- **Aggiornamento 7 novembre (Claude):** il probe espone al JS `document.getElementById` e `textContent` (lettura/scrittura) tramite tre op deno_core sul documento Blitz esistente; due letture dello stesso nodo restituiscono lo stesso wrapper (`===`), anche dopo la mutazione. `cargo fmt`, `cargo check --locked`, `cargo run --locked` superati su Windows; `cargo tree -i v8` mostra ancora una sola `v8 150.4.0`. Verifica negativa: senza cache wrapper il test fallisce. Vincolo emerso: Blitz riusa gli id dei nodi eliminati (slab), quindi il setter stacca i figli con `remove_node` senza liberarli; serve un ciclo di vita dei wrapper legato al GC prima di liberare nodi. Eventi, frame, rendering visibile e GC dei wrapper restano non implementati.
- Il fork PathScale `ps-blitz 0.6.5` è stato scartato per questo probe: il check Windows fallisce con 197 errori nella dipendenza POSIX-only `nagoya 0.1.19`. Non ripetere questo confronto salvo nuove evidenze/versioni.
- È stato aggiunto un workflow Windows manuale/path-filtered per check e run del probe; non è ancora stato eseguito in CI perché le modifiche non sono state pubblicate.

Per dettagli implementativi e fonti usare il README del probe e il handoff tecnico. Non presentare il probe come browser funzionante o come risultato prestazionale.

## Delegazione Claude già tentata

Claude è stato incaricato di creare il probe circoscritto. Due tentativi si sono fermati per diniego degli strumenti: inizialmente accesso web/Cargo, poi scrittura nella cartella `support/blitz-v8-probe/`. **Claude non ha prodotto modifiche**; il probe attuale è stato creato e verificato direttamente da Codex. Se il workspace o i permessi disponibili a Claude consentono di lavorare, procedere con un incarico singolo e limitato; se la scrittura viene nuovamente negata, fermarsi e riferire il blocco, senza aggirare il diniego.

## Stato all'8 novembre (handoff corrente, Claude)

- **Git:** `main` pubblicato fino a `71eb318aa71`; run di quel commit tutte verdi (blitz-probe
  `42419238504`, test.yml `42419238573`, android `42419238472`). Il commit `d40009db20c`
  (MutationObserver e superficie DOM per React/Vite) viene pubblicato con questo handoff, quindi
  va controllata la sua CI. I submodule `roves-action` e `roves-wiki` risultano modificati da prima e
  non vanno inclusi per errore.
- **Decisioni dell'utente:**
  - nessun rilascio: si finisce lo sviluppo;
  - nessuna build locale, nemmeno della probe; si può però scaricare ed eseguire ciò che produce la CI;
  - i test li fa Claude quando possibile;
  - per WebGPU usare le estensioni Deno, con nel TODO l'opzione di passare a Deno per la presentazione.
- **CI per commit (test.yml):** oltre alla test page, la gamba Windows portable impacchetta
  DRincs-Productions/visual-novel-template e pubblica `visual-novel-template_windows.zip` sulla
  release rolling `test`, insieme all'artifact omonimo. Scaricato ed estratto l'8 novembre in `C:oves-vnisual-novel-template-windows` (`play.exe`; l'URL diretto della release dava 504, funziona l'endpoint API dell'asset con `Accept: application/octet-stream`); avviato, il processo resta vivo ma il rendering non è verificato (finestra non in primo piano nello screenshot). Il pacchetto gira sul motore di produzione
  attuale (Servo + V8), non su Blitz. Va scaricato in `C:\roves-vn` e avviato. Il download
  diretto della release ha restituito una volta un 504 di GitHub; in quel caso riprovare.
- **Coda di test locale** in `C:\roves-probe`: exe CI della probe, `1-finestra-clic.cmd`,
  `2-webgpu-pixi.cmd`, `3-jitless.cmd`, `LEGGIMI.txt`. Tutti e tre sono passati in locale (GTX 1050, scala 100%).
- **Scoperta `--game`:** blitz-probe.yml esegue `--game visual-novel-template/dist` in sola
  segnalazione. Al primo giro pagina e modulo si caricavano, mancava `MutationObserver` e
  `#root` restava vuoto. `d40009db20c` aggiunge MutationObserver e altre API di avvio
  (readyState, DOMContentLoaded/load, MessageChannel, requestIdleCallback, commenti, cloneNode).
- **Prossimo passo concreto:**
  1. leggere il report `game_page` della nuova run blitz-probe, dal log o dall'artifact `game` (serve un PAT inline);
  2. implementare le API mancanti successive, ripetendo finché React non monta `#root`;
  3. dopo l'avvio di React, nell'ordine: presentazione del canvas WebGPU nella finestra, Canvas 2D per il testo Pixi, Web Audio per Tone.js (candidato crate `web-audio-api`), scroll/wheel/IME/touch, loop finestra integrato con tokio, snapshot di avvio (gli exe senza snapshot leggono il JS delle estensioni dai percorsi crate assoluti; per questo la CI usa `CARGO_HOME=C:/roves-cargo` e in locale esiste una junction).
- **Issue upstream ancora da aprire** (il PAT è in sola lettura e `gh` non è installato):
  - deno_web 0.291.0, `dispatchEvent` senza listener sul target salta gli antenati;
  - deno_webgpu 0.227.0, un secondo GPUDevice porta all'OOM in `createTexture`.

  Entrambe sono descritte nel TODO.

## Stato a fine sessione (7 novembre, Claude)

Pubblicato e verde in CI Windows (normale e JIT-less) fino al commit della correzione dispatch,
run `42389990508`. La probe (Blitz `0.3.0-beta.4`, deno_core/V8 unico) copre: wrapper DOM con GC dei
nodi staccati, albero/selettori/`classList`/`innerHTML`, geometria, `matchMedia`, focus, eventi
pointer/mouse/tastiera trusted, frame loop (rAF/microtask/timer), `style`/`getComputedStyle`,
Motion 12.38.0, paint CPU con controlli pixel, `localStorage`/`sessionStorage` persistenti
(deno_webstorage), History/Location a documento unico e una finestra reale `blitz-shell` con clic OS
iniettato e screenshot. Dettagli, test e verifiche negative nel README della probe.

- **Vincolo utente: non compilare in locale, nemmeno la probe.** Commit piccoli, verifica in CI; i log
  completi richiedono un PAT in sola lettura passato inline.
- Bug deno_web 0.291.0 aggirato (dispatch che salta gli antenati): issue upstream da aprire, vedi TODO.
- WebGPU/PixiJS 8.19.0 verificato su GPU reale (GTX 1050) con l'eseguibile CI eseguito in locale; due bug/limiti Deno documentati nel TODO.
- Restano: presentazione del canvas WebGPU nella finestra Blitz (wgpu 29 vs 30), Canvas 2D (testo Pixi), Web Audio, scroll/wheel/testo/IME/touch nella finestra,
  Linux/macOS/mobile, compatibilità salvataggi.

### Prossimi passi (fine 7 novembre)

Ordine per far girare il template, con analisi e candidati nel TODO ("Template sulla probe"):
presentazione del canvas WebGPU nella finestra (architettura proposta: compositing con il wgpu di
deno_webgpu), caricamento immagini (`deno_image` più polyfill `copyExternalImageToTexture`), Web Audio,
Canvas 2D per il testo Pixi. Verifica: CI per build e test headless; GPU e finestra eseguendo in
locale l'eseguibile CI (junction `C:/roves-cargo/registry/src/...` già creata su questa macchina).

## Incarico completato il 7 novembre (wrapper DOM minimi)

L'incarico sotto è stato eseguito; dettagli nel README del probe. Prossimo passo proposto: progettare il ciclo di vita wrapper/nodo legato al GC (valutando oggetti cppgc di deno_core) prima di ampliare la superficie DOM, poi eventi e frame.

## Incarico (storico)

Esaminare, nel probe isolato, come esporre al JS del gioco un piccolo insieme di nodi Blitz tramite oggetti DOM con identità wrapper stabile. Valutare prima API/librerie mantenute e le API pubbliche Blitz; evitare di costruire una binding generale. Implementare solo una prova minima utile (ad esempio `document.getElementById`, `textContent` e identità stabile dello stesso nodo), aggiornarne README e risultati del probe, quindi eseguire `cargo fmt`, `cargo check --locked` e `cargo run --locked` su Windows. Riportare file modificati, scelte di riuso, comandi/esiti e log sintetici; fermarsi senza commit, push o ulteriori incarichi.

Criteri: nessun runtime JS aggiuntivo; le letture/scritture attraversano il documento Blitz esistente; due letture dello stesso nodo restituiscono lo stesso wrapper JS (`===`); regressioni del probe assenti. Se l’API Blitz richiede accesso privato o una dipendenza nuova senza motivazione concreta, fermarsi e documentare l’impedimento prima di ampliare lo scope.

## Working tree da preservare

Al momento del passaggio risultavano modificati `AGENTS.md`, `CUSTOMIZATIONS.md`, `TODO.md`, `docs/ROVES_PERFORMANCE.md`, il handoff tecnico Blitz e i submodule `roves-action`, `roves-wiki`; erano inoltre non tracciati `.github/workflows/blitz-probe.yml`, `patches/blitz-probe/`, `support/blitz-v8-probe/` e `patches/servo-v0.5.0/0462-gameplay-smoothness-plan.patch`. Il 7 novembre, su richiesta dell'utente, tutte le patch di tracciamento verso Servo (incluse `patches/blitz-probe/` e 0462) sono state eliminate: vedi AGENTS.md. Il file patch 0462 e i submodule modificati sono preesistenti/estranei a questo incarico: non rimuoverli, ripristinarli o includerli per errore. Controllare `git status --short` prima e dopo il lavoro.

## Aggiornamento 8 novembre 2026 — CI e test aggiornati

- **Claude ha completato** un incarico limitato ai file `.github/workflows/blitz-probe.yml` e `CUSTOMIZATIONS.md`. Non ha fatto commit/push né build locali. Il workflow ritenta `cargo check --locked` al massimo tre volte solo se il log include sia `Failed to download V8 prebuilt archive` sia `HTTP Error 502/503/504`; conserva i log per tentativo, li carica nell'artifact e rende il passo diagnostico tollerante quando manca il log del probe. Claude ha validato YAML e Bash, simulando 504 ripetuti (codice 101 dopo 3 tentativi), 503 poi successo (0 dopo 2), errore Rust (101 dopo 1). `actionlint` non disponibile. Revisione Codex: diff confinato ai due file previsti; non è stata ancora eseguita la CI sulla modifica.
- **Run Blitz `42470758193`, SHA `0571fe85a475eddfc88e1e0a2b5d351f5c8676a8`:** Windows JIT-less verde, Windows normale rosso in `cargo check` per HTTP 504 al prebuilt V8. L'artifact JIT-less `blitz-probe-jitless` (`13613624049`) contiene report e binario CI. Il probe sul `visual-novel-template` aggiornato esegue 120 frame, `load_errors: []`, ma React non monta (`root_children: 0`, `body_elements: 1`, testo vuoto); errore `unhandledrejection: TypeError: Cannot read properties of undefined (reading 'value')` a `game://content/assets/index-mzyslO5I.js:20:110676`. Non è un test di fluidità: è una diagnosi di compatibilità API nel probe Blitz. Prima di delegare una modifica, identificare quale oggetto DOM/integrazione produce il `value` indefinito; possibile correlazione con input controllati React, ma non ancora provata. Il report non verifica la grafica del gioco.
- **Prova desktop Servo+V8:** eseguito `C:\roves-vn\visual-novel-template-windows\play.exe`, avanzati vari step di storia, input nome accettato e dialoghi/sfondi/personaggi aggiornati. È una prova funzionale visibile del pacchetto di produzione Servo, non misura prestazioni e non valida Blitz. Il pacchetto contiene file `.pack`, non `dist/index.html`, quindi Chrome non ha potuto aprire il gioco da quel percorso (`ERR_FILE_NOT_FOUND`); nessuna build locale è stata eseguita.
- **Android sullo stesso SHA:** dopo la seconda retry, `android` fallisce scaricando `gradle-9.5.1-bin.zip` da `services.gradle.org` con HTTP 504; `android-release-signing` va in timeout dopo 10 s sul medesimo download. `ensure-test-release` passa. Trattare come blocco download CI; non dichiarare il batch verde.
- Working tree atteso: modifiche Codex/Claude in `.github/workflows/blitz-probe.yml` e `CUSTOMIZATIONS.md`; `roves-action` e `roves-wiki` restano modifiche preesistenti da preservare. Nessuna CI nuova finché la modifica non viene pubblicata; rispettare i gate CI prima del push.

### Prossimi passi

1. Rivedere i due file di Claude, completare controlli statici leggeri del workflow e registrare questo handoff; non compilare localmente.
2. Per proseguire oltre la diagnosi, ispezionare gli accessi `.value` nel bundle template alla posizione riportata e confrontarli con la probe (`dom.js` espone `DOMTokenList.value`, ma non implementa ancora `HTMLInputElement`/`HTMLTextAreaElement`). Verificare la chiamata concreta prima di scegliere una API da aggiungere. Ogni codice resta delegato a Claude.
3. Il batch precedente è rosso (V8/Gradle download): non fare push finché i retry ammessi non si concludono o finché la correzione del workflow e il nuovo checkpoint CI non chiariscono lo stato. Non ripetere polling ravvicinato.
4. Per confronto Chrome/Blitz ottenere un percorso servibile della build CI o hostare l'output pack correttamente; DevTools/Chrome locale è disponibile come PC ma la policy Work in questa sessione disabilita browser locali via MCP. Nessuna metrica Chrome raccolta.

### Vincolo confermato e blocco diagnostico

Il template originale su main e il riferimento di compatibilita. Gli adattamenti vanno nella probe Roves. Main osservato: bb9ea77910dda74aced20b001131c8daa22e7cfc. Il sito pubblico serve index-CyEUUAGo.js, quindi non permette di interpretare la posizione nel bundle CI index-mzyslO5I.js.

Claude CLI ha diagnosticato l'assenza del bundle esatto. Un incarico separato per registrare SHA del template e contesto delle righe di stack nel workflow e stato bloccato dal diniego di scrittura della sessione non interattiva. La diagnostica proposta resta da applicare e verificare con una sessione Claude autorizzata. Nessun limite architetturale dimostrato. La CI precedente rimane rossa per download V8/Gradle.

### Riproduzione locale con il binario CI e il bundle esatto

Il pacchetto Windows Servo contiene lo stesso asset hash della run Blitz. Estratti tutti i .pack con tar in una cartella temporanea e scaricato il binario JIT-less dell'artifact 13613624049. Il binario della run 42470758193 con --game sul template estratto e storage nuovo riproduce 120 frame, root_children 0 e TypeError su value. Primo frame completo: Object.get volumeAll (game://content/assets/pixi-vn-CzUJeT_J.js:210:44538), chiamato da index-mzyslO5I.js:20:110146 e :20:110676. Il getter legge md().volume.value, con md() uguale alla destinazione Tone.js. Il probe non espone AudioContext. Web Audio va quindi valutato prima di poter considerare raggiunto il mount. Nessuna modifica al template e nessuna build locale. La diagnostica CI proposta in precedenza non serve per identificare questo errore.

Claude ha confermato sui sorgenti Tone 15.1.22 che senza AudioContext usa DummyContext, con destination vuota. Proposta: valutare web-audio-api-rs per una prova con grafo reale e sink senza uscita in CI, con GainNode, ConstantSource, AudioParam e listener. standardized-audio-context verifica anche disconnect e puo richiedere ScriptProcessor: verificarne il supporto prima di fissare lo scope. Gate: init Tone, controllo numerico del render offline, template originale con storage nuovo, report del mount. La CLI Claude aveva negato la scrittura nel precedente incarico; questa analisi era in sola lettura. Nessuna implementazione audio o prova positiva ancora disponibile.

### Tentativo di implementazione Web Audio

La sessione Claude non interattiva con acceptEdits ha negato npm install, lettura del registry Cargo esterno e accesso web. Claude si e fermato prima della patch. La documentazione di web-audio-api 1.7.0 conferma AudioContextOptions.sink_id. Restano da verificare l'integrazione ScriptProcessor e la risoluzione dei lockfile prima del codice. I retry delle run 42470758193 e 42470758131 sono poi passati per tutti i job Windows e Android sullo SHA 0571fe85a47. Questo checkpoint non include Web Audio.

### Primo passo implementato da Codex

Il probe ora esegue un controllo offline su web-audio-api 1.7.0, senza backend dispositivo. Render di 256 campioni a 0,25 da sorgente costante e gain. Lockfile risolto senza aggiornare le dipendenze esistenti. Restano il ponte JS AudioContext, la regressione Tone e il mount del template originale. Nessuna build locale.

### Ponte V8, checkpoint corrente

Il gate offline e verde in entrambe le modalita Windows nella run 42496314721. Il commit fbc680feb5d pubblica il test V8 di creazione, lettura e chiusura del contesto headless. Run 42497588883 in corso. Il commit locale successivo estende il test al GainNode nativo con valore di default, scrittura e lettura, e rifiuto di Infinity. Il bridge e ancora privato. Il prossimo passo e aggiungere i wrapper Web per il grafo di Tone e verificare il bootstrap sul template originale. La delega resta sospesa fino alla verifica della parte critica.

### Dopo Tone: `history.scrollRestoration`

La run 42510350982 e la riproduzione locale col suo exe superano Tone e si fermano su `TypeError: Cannot set property scrollRestoration of #<History> which has only a getter` (`narration-hooks-LJVJnQ7D.js:1:195585` in `lb.update`, `root_children` 0). Claude ha aggiunto in `src/history.js` il setter secondo HTML/WebIDL: default `auto`, `auto`/`manual` salvati sulla voce corrente, copiati da `pushState` e dai frammenti, mantenuti da `replaceState`; valori non validi ignorati senza eccezione (regola degli attributi enum WebIDL, come Chrome; solo `Symbol` dà `TypeError`). Il wrapper di `dom.js` espone l'oggetto `history` direttamente, quindi non andava cambiato. Regressione in `history_test.js` (assegnazione strict-mode come nel template, valori invalidi, copia) e `history_check.js` (modalità della voce dopo `back()`). Nessuna libreria Deno copre History. Non eseguito in locale: niente build e Node non autorizzato nella sessione. Prossimo passo: CI verde, nuovo report `game_page`, prossimo errore del template.
