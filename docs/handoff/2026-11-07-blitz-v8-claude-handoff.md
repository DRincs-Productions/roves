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

## Stato a fine sessione (7 novembre, Claude)

Pubblicato fino a `94c40965ebc` (commit locale successivo: solo TODO). La probe ora copre wrapper DOM,
albero/attributi, eventi trusted via `deno_web`, paint offscreen con controllo pixel, frame loop
headless (rAF, microtask, timer), `element.style`, `getComputedStyle` e Motion 12.38.0; scelta
l'opzione A (niente libreria DOM JS). Dettagli e verifiche negative nel README della probe.

- CI da controllare: run `42373739478` (Blitz probe, prima esecuzione del render GPU offscreen su
  runner Windows: se manca un adapter wgpu il passo di paint fallisce e va deciso un fallback,
  ad esempio WARP o `anyrender_vello_cpu`). Android `42373739406` verde.
- **Vincolo utente del 7 novembre: non compilare in locale, nemmeno la probe.** Fare commit piccoli
  e verificare nella CI della probe.
- Prossimi passi: passaggio a Blitz 0.3 beta (elenco delle modifiche nel TODO), poi finestra
  `blitz-shell` e input reale.

## Incarico completato il 7 novembre (wrapper DOM minimi)

L'incarico sotto è stato eseguito; dettagli nel README del probe. Prossimo passo proposto: progettare il ciclo di vita wrapper/nodo legato al GC (valutando oggetti cppgc di deno_core) prima di ampliare la superficie DOM, poi eventi e frame.

## Incarico (storico)

Esaminare, nel probe isolato, come esporre al JS del gioco un piccolo insieme di nodi Blitz tramite oggetti DOM con identità wrapper stabile. Valutare prima API/librerie mantenute e le API pubbliche Blitz; evitare di costruire una binding generale. Implementare solo una prova minima utile (ad esempio `document.getElementById`, `textContent` e identità stabile dello stesso nodo), aggiornarne README e risultati del probe, quindi eseguire `cargo fmt`, `cargo check --locked` e `cargo run --locked` su Windows. Riportare file modificati, scelte di riuso, comandi/esiti e log sintetici; fermarsi senza commit, push o ulteriori incarichi.

Criteri: nessun runtime JS aggiuntivo; le letture/scritture attraversano il documento Blitz esistente; due letture dello stesso nodo restituiscono lo stesso wrapper JS (`===`); regressioni del probe assenti. Se l’API Blitz richiede accesso privato o una dipendenza nuova senza motivazione concreta, fermarsi e documentare l’impedimento prima di ampliare lo scope.

## Working tree da preservare

Al momento del passaggio risultavano modificati `AGENTS.md`, `CUSTOMIZATIONS.md`, `TODO.md`, `docs/ROVES_PERFORMANCE.md`, il handoff tecnico Blitz e i submodule `roves-action`, `roves-wiki`; erano inoltre non tracciati `.github/workflows/blitz-probe.yml`, `patches/blitz-probe/`, `support/blitz-v8-probe/` e `patches/servo-v0.5.0/0462-gameplay-smoothness-plan.patch`. Il 7 novembre, su richiesta dell'utente, tutte le patch di tracciamento verso Servo (incluse `patches/blitz-probe/` e 0462) sono state eliminate: vedi AGENTS.md. Il file patch 0462 e i submodule modificati sono preesistenti/estranei a questo incarico: non rimuoverli, ripristinarli o includerli per errore. Controllare `git status --short` prima e dopo il lavoro.
