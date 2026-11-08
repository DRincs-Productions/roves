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
