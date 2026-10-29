# 0425 — proposta verificata per il fast path TLA di deno_core

## Decisione risolta il29ottobre2026

L'utente ha autorizzato la patch temporanea per difetti che bloccano giochi, fork, issue/PR
upstream e TODO di ricontrollo. Vedi handoff0428 per l'integrazione attuale. Forkcreato in
BlackRam-oss/deno, issue37097 e draftPR37098. Le condizioni sotto conservano evidenze e
limiti della proposta originale; non chiedere nuovamente l'autorizzazione gia ricevuta.

Diff portabile: 0425-deno-core-static-tla-proposal.patch, una condizione:

```diff
-if module.get_status() == v8::ModuleStatus::Evaluated {
+if module.get_status() == v8::ModuleStatus::Evaluated && !module.is_graph_async() {
```

Lascia invariato il fast path dei grafi sincroni. Per un grafo async usa il percorso Deno
esistente che attende la vera promise di valutazione. Non aggiunge callback, parser, grafo
Roves o API al gioco. Non richiede la strategia di import interno rifiutata in0424.
La produzione conserva esattamente l'adapter preesistente basato su mod_evaluate.
La fonte upstream copiata e la crate originale0.413.0, licenza MIT e file preservati.

## Prove reali

Esperimento separato in target/task-0425/deno-core-proposal, override SOLO nella workspace
nativa ignorata. Nessuna modifica nella registry, nel Cargo root o nel vendoring prodotto.
Native workspace ripristinata a dipendenza originale e sorgenti pubblicati dopo la prova.

- Suite completa proposta:117pass/2ignored normale,116pass/3ignored con vera featureJITless.
- Nuova matrice: radice/discendente x risoluzione/rifiuto, import realmente pending prima
  del resolver, namespace/valore corretto dopo, stessa eccezione dopo reject, cache conserva
  identita del namespace, fetch dei due moduli ed esecuzione una sola volta.
- Il test registra la risoluzione PRIMA di leggere export potenzialmente in TDZ: altrimenti
  un errore del callback potrebbe nascondere una risoluzione prematura.
- Negativa senza patch: settlements.length1 anziche0 prima del resolver, caso radice/successo.
- Patch dry-run/apply su copia pulita di dynamic.rs e confronto identico con copia provata.
- Native116/115 del candidato import-interno0424 NON sono queste prove: candidato rifiutato.

Log: target/task-0425/{all-normal,all-jitless,baseline-negative}.log.
Sorgente matrice completa conservato in target/task-0425/proposed-tests.rs; include le
regressioni root e descendant oltre al test di host-release precedente. Non copiarlo tutto
alla cieca: porta soltanto regressioni nuove sull'HEAD effettivo, che ha il test ownership0424.
Il test aggiuntivo e cached_static_tla_import_waits_for_resolution_and_rejection.

## Se autorizzata l'eccezione

Preparare integrazione riproducibile e versionata del SOLO diff nel pacchetto upstream,
preservando licenza, pin e una sola versione V8. Non modificare cache/registry locali, non
usare API private o build che testino sorgenti diversi dal pacchetto pubblicato. Aggiornare
lock, patch/replay, README/wiki/CUSTOMIZATIONS/handoff e note di rimozione dell'eccezione.
Aggiungere copertura browser per modulo statico TLA e relativo discendente; normali/JITless,
C2, CSP, mappeimport, JSON e identita cache devono restare verdi. Build complete online.
Non promettere un miglioramento prestazionale; i grafi async perdono questo fast path.
Adottare solo dopo la scelta utente e revisione; CI di questa proposta nativa non e una
verifica browser della dipendenza modificata. Se non autorizzata, tenere gap0424 aperto e
attendere una soluzione mantenuta, senza presentare la migrazione come completata.
