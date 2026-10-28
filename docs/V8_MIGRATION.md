# Migrazione V8 / deno_core — punto operativo autorevole

## Checkpoint di integrazione del 28 ottobre 2026

**0413 completa: fix dell'unico fallimento SRI.** Batch0aa2 ora tutto terminale:
generale tutti i10nonApple verdi; Android verde; V8 runtime e buildWindows verdi,
solo smokeLinux normal/JITless rossi sul SRI. Sourcecollector conservava solo chunk dopo
metadata; per SRI fetch manda body prima di metadata e veniva perso. Corretto per bufferizzare
anche con stato ancora ignoto; finish_module_fetch continua a richiedere metadata/EOF validi.
Negativa riprodotta sul binario Windows CI scaricato (124981837byte), percorso in
 target/task-0385/ci-windows-0aa2-path.txt. Load valido=true, contatore0; mismatch respinto;
modulo ordinario incrementa. Rust/Node syntax e replay2file0413 verdi. Positiva richiede CI.
Log ci-0aa2-module-order-runtime.log e ci-windows-0aa2-{sri-probe,isolated}.log.
Pubblicare0410-0413 e wiki; poi identificare run e sospendere per quotaCodex96% consumata,
con unico retry al reset reale+2min. Non usare Claude. Al risveglio riprendere tutto il loop.

**Git e CI correnti:** pubblicati root0aa2a55040e, actionb001464, wikifc6cbd1.
Locali completi0410 root039aa80685b (metriche),0411 rootfb7f2820acd (classi errori)
e0412 nel commit di questo checkpoint (posizioni errori); wiki locale2f26015.
La riga vuota utente TODO resta fuori dai commit. Non usare Claude. Il trigger5ore riprende
il loop completo; nessun polling continuo o implementazioni duplicate.

Batch pubblicato: V840760300642, generale40760300801, Android40760300574;
iOS40760300584/Apple escluse. Ultimo controllo: runtime Linux/Windows normal/JITless,
Android, pristine replay e controlli condivisi VERDI; browser e packaging richiesti ATTIVI,
nessuna rossa. Niente push finche tutti i richiesti terminano e gli errori sono corretti.

**0412 verificata localmente:** il report di un parse error Deno in located.js3:14 prima
restituiva undefined:0:0 (negativa riprodotta). Ora dati pubblici JsError (frame e formatter)
sono conservati sull'oggetto errore: posizione in proprieta privata V8, stack originale non
enumerabile. Nessun registro/root aggiuntivo, parser o grafo duplicato. I report Servo
PendingExceptionStackInfo/JS_ErrorFromException leggono quei dati; errori classici invariati.
Suite nativa completa112normal/2ignore e111JITless/3ignore verdi (benchmark, piu Wasm
in JITless). Browser fixture obbligatoria verifica filename/line3/column14/stack. Helper
Linux8/8, Node syntax e patch0412 dry-run/replay/confronto6file verdi. Log
static-location-{negative,all-normal,all-jitless,fixture-checks}.log sotto target/task-0385.
Non risolve l'offset della riga HTML dei moduli inline ne retention/SourceText0361.

**0411 verificata:** type css veniva ricreato SyntaxError: Uncaught TypeError. Usare
la conversione pubblica deno_core::error::to_v8_error; NON CoreError::to_v8_error, che in
0.413 crea sempre TypeError e rompe SyntaxError. Tre combinazioni attributi illegali non
fanno fetch; JSON tipizzato successivo valido. Contratti moduli12/12 normal e JITless verdi
prima0412, ora13/13 nella suite completa. Browser TypeError fixture obbligatoria aggiunta.

**0410 verificata:** rapporti JSON cold/warm/slow richiedono durate finite non negative,
contatori timer/frame interi positivi e visibility valida. Mancanti/duplicati/malformati/
post-total falliscono; nessuna soglia prestazionale. Chrome DevTools MCP156 su Windows:
tab attiva visible929x861; avvio headed/headless non verificato. Un run10.9/0.6/261.4ms,
slow server250ms, con campioni timer/frame continuati. Fixture0410 Chrome14/17 per le tre
aspettative Deno differenti; non prova le fixture successive. Dati/limiti in
ROVES_PERFORMANCE.md, log locale chrome-pause-0410.log. Nessuna misura Roves ottenuta.

**Gia pubblicati0407–0409:** log per processo/cold cache e artifact anche su successo;
loader installato dopo init_document e prima delle interfacce (fix panic comune browser/
bundle del batch6efa); --hard-fail solo per smoke usa-e-getta. La nuova CI deve confermare
avvio e contratti browser. Le vecchie CI verdi native non equivalgono a browser funzionante.
Prossimo: analizzare eventuali nuovi errori CI al termine di un incarico, completarli;
quando il batch richiesto e terminale e le correzioni complete, pubblicare wiki prima dei
gitlink root, identificare le run una volta e continuare il loop. Restano policy UI/clic
reale/chiusura con fetch-TLA pendenti, misure Roves pausa, inline HTML offset e retention.

Leggere questo blocco e AGENTS prima di modificare. Lo storico dopo FINE DEL PUNTO OPERATIVO
contiene scelte e backup superati; consultarlo solo per un problema concreto.
Il candidato sorgente adotta Deno e la policy documento unico in un cambiamento atomico.
La validazione browser/cross-platform NON e ancora conclusa: non dichiarare migrazione o
retention risolte. Il prossimo passo e la CI completa, seguita dalle correzioni necessarie.

### 1. Vincoli e decisioni già approvate: non richiederle di nuovo

- Mantenere shell Servo, DOM, layout e rendering. Usare librerie mantenute per ridurre il
  codice Roves; rusty_v8 e deno_core sono approvati. Non serve installare Deno o Chromium.
- Un documento di gioco, una finestra; route History API/hash tipo TanStack. Non servono
  frame multipli, tab o identità WindowProxy tra pagine sostituite. I vecchi NO-GO per quel
  perimetro browser NON vietano deno_core per questo prodotto.
- Bloccare reload, sostituzione della pagina, iframe e finestre aggiuntive prima di cambiare
  documento/stato. Consentire caricamento iniziale della shell, route interne e chiusura.
  Non trasformare automaticamente una navigazione vietata in una route del router.
- Segnalazione: log/warning con motivo, ripetizioni limitate; avviso discreto non modale in
  sviluppo, solo log nel gioco distribuito. Nessun alert modale o crash come segnalazione.
- Link esterni: browser di sistema solo dopo clic reale. `.click()`, eventi sintetici,
  timer e navigazione automatica non autorizzano aperture esterne. Non dedurre il clic
  dall'URL o da un intervallo temporale; usare fiducia nativa dell'evento.
- PAUSA STATICA PROVVISORIA: il fetch/istanziamento statico può sospendere input/timer;
  valutazione e top-level await non devono attendere il completamento dell'await.
  Import dinamici asincroni. Misurare startup e script module aggiunti durante il gioco,
  sorgenti locali/rete, freddo/caldo/rete lenta controllata, frame/input/timer/errori/chiusura.
  Nessuna durata o soglia accettabile è stata misurata o concordata. Tema DA RIVEDERE.
- C2 ACCETTATO: soli errori di VALUTAZIONE statica tramite `unhandledrejection` nel backend
  Deno invece di `error/window.onerror`. Overlay basati solo su onerror possono perderli.
  Errori classici, fetch, parse/link e import dinamici mantengono contratti distinti.
- C3 ACCETTATO PROVVISORIAMENTE: solo i moduli INLINE espongono un frammento distinto
  `#roves-inline-module-N` in `import.meta.url`. Import relativi e asset conservano la base;
  URL dei moduli ESTERNI invariati. Non generalizzare la riscrittura a ogni URL.
- Nessuna API Deno privilegiata ai giochi, fork Deno/V8, parser JS nostro, layout V8 privato
  o secondo grafo permanente. Non togliere test senza copertura sostitutiva motivata.

### 2. Ultime decisioni, proprieta e quota

- Preload JSON temporaneamente NON disponibile, scelta esplicita utente: warning limitato
  e link error asincrono, niente falso load o interruzione del gioco. Import JSON tipizzati
  funzionanti; senza type json respinti. Ripristino documentato in TODO.md con criteri.
- L'utente autorizza lo sviluppo diretto Codex. Nessun Claude attivo; non usarlo prima del
  30ottobre2026 alle17:30 Europe/Rome. Promemoria unico ripresa-claude-il-30-ottobre-alle-17-30.
  Verificare /usage alla scadenza, non assumere reset settimanale. Nessun retry concatenato.
- Codex vicino al limite5ore (<=10% rimanente): get_usage_limits, handoff aggiornato e un
  solo tentativo al reset reale piu2minuti; disattivarlo dopo. Nessuna pianificazione stimata,
  polling, cambio modello, riscatto reset o aggiramento del limite settimanale.
- Non ricreare CLAUDE_RESUME.md. TODO.md contiene una riga utente preesistente: preservarla;
  l'utente ha autorizzato esplicitamente SOLO l'aggiunta del nuovo backlog JSON.
- Continuare il loop: implementazione completa/revisione/verifiche, commit locale, controllo
  aggregato del batch precedente; push solo se nessuna run richiesta attiva o errore irrisolto.
  Apple esclusa dal gate ma non disabilitata. Build browser pesanti solo online.

### 3. Git, CI e significato dei riferimenti

Il checkpoint corrente iniziale e il riferimento autorevole: pubblicato0aa2a55040e e
serie locale0410–0412. Controllare Git reale: il commit di questo handoff ha uno SHA nuovo.
Batch precedente6efa terminato: compilazione e runtime verdi, startup browser/bundle
fallito per Document prima di init, fix0408 pubblicata. Batch0aa2 ancora da concludere;
pristine replay verde fino0409. Non pushare0410–0412 durante le run richieste attive.
ProbeDeno40564575701 verde4/4 resta una prova isolata, NON Servo+DOM.

### 4. Cosa cambia nel candidato sorgente

| Area | File | Stato e intento |
| --- | --- | --- |
| Deno runtime | roves-js/src/deno_impl.rs, rust.rs, runtime_impl.rs, realm_impl.rs | Deno possiede isolate/context/grafo/event loop; primo global adotta il context; altri rifiutati |
| V8 pubblico | roves-v8/build.rs, src/shim.cc, compat.rs, codegen.rs, incumbent.rs | v8=150.4.0 condiviso con deno_core0.413.0; shim compilato contro header pubblici, niente layout privati |
| HTML/fetch | script/script_module.rs | Servo import map, CSP, MIME, SRI, credenziali, referrer, timing; callback rete -> oneshot senza pump JS durante il fetch statico |
| Classici/worker | globalscope/script_execution.rs, script_runtime.rs | Loader prima dei classici; source private conserva fetch options; worker iniziale conserva RequestClient/Referrer; dinamici usano destinazione Script |
| Chiusura | script_module clear_module_loader | PENDING_GRAPHS svuotato prima della distruzione runtime; task residue senza entry diventano no-op |
| Documento unico | script/navigation.rs, htmliframeelement.rs, constellation.rs | Rifiuto precoce full nav/reload/iframe; preflight History prima dei drain; iniziale shell usa NewWebView/new_pipeline |
| Warning/UI | shared/constellation/game_document_policy.rs, shared/embedder, servo, servoshell | Warning per categoria/processo; messaggio nativo tipizzato; avviso solo sviluppo |
| Link esterni | links.rs, anchor/area, EmbedderMsg, WebViewDelegate, running_app_state | isTrusted nativo, HTTP(S) cross-origin prima scelta target, SDL3 open_url; headless non avvia OS; window.open resta rifiutato |
| Preload JSON | script_module.rs, html/documentmetadata/htmllinkelement.rs | Non disponibile; callback errore e queue_simple_event della task source networking |
| Rimozioni | roves-js/modules_impl.rs, roves-v8/global_proxy.rs, script/module_loading.rs | Vecchio grafo scollegato/eliminato; gen_wrappers2 adattato |
| CI | v8.yml, roves-action README | Copia build.rs e crate interi, trigger su script/constellation/embedder/servo/shell, snapshot document-retention |

Nessuna API Deno privilegiata esposta: bootstrap globals rimossi; namespace ext/node/npm/
jsr/deno respinti prima di cache, dopo remap/redirect e in import.meta.resolve. Errori di
risoluzione restano TypeError. Nessun parser JS o secondo grafo permanente.

V8 Function puo riutilizzare il referrer della prima compilazione per una funzione uguale
in un altro script. Riprodotto left:left:left:right:right:left invece di ultimo right.
Guard pubblico --no-compilation-cache risolve; cache moduli Deno resta attiva. Costo avvio/
compilazione NON misurato. Non rimuovere senza prova negativa/positiva alla versione nuova.

### 5. Prove reali, comandi e limiti

Log sotto target/task-0385, locali/ignorati da Git. Su un clone nuovo vanno rigenerati.

| Prova | Esito | Log |
| --- | --- | --- |
| Suite nativa Windows, sorgenti0412 | 112pass,2benchmarkignore normale | static-location-all-normal.log |
| Stessa suite con VERA feature JITless | 111pass,3ignore (benchmark+Wasm) | static-location-all-jitless.log |
| Contratti moduli0412 | 13/13 normale e JITless nella suite completa | static-location-all-*.log |
| Runner Linux0404 | 59/59 senza skip | linux-runner-complete.log |
| Helper/runner pertinenti0412 | 8/8 Linux | static-location-fixture-checks.log |
| Root e2standalone Cargo metadata | --locked --offline validi | root-metadata.json e output locale |
| Rust e Bash | sintassi modifiche verificata, non typecheck browser | *-syntax.rs / bash -n |
| HTTP fixture | Referer presente/assente e file statico verificati | esecuzione locale stdlibserver |
| Browser/Servo attuale, clic reale, noticeUI, misure pausa | NON verificati | nuove CI/manuale necessari |

V8_TEST_MODE DA SOLO NON abilita JITless. Usare --features roves-js/jitless nella workspace
isolata o --features jitless nel crate standalone, come fa CI. Non citare come provaJITless
i vecchi log *jitless che omettevano la feature (specificati nel checkpoint storico).

```powershell
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target/roves-js'
# Prima sincronizzare i file modificati in target/task-0385/native-workspace/.
cargo test --manifest-path target/task-0385/native-workspace/Cargo.toml -p roves-js -j1 -- --test-threads=1
cargo test --manifest-path target/task-0385/native-workspace/Cargo.toml -p roves-js --features roves-js/jitless -j1 -- --test-threads=1
python -m unittest discover -s support/v8-smoke -p 'test_*.py'
```

Controllare LASTEXITCODE dopo ogni comando PowerShell. Salvare output ampi nei log; non
riversare warning legacy, JSON API o interi storici. Non ripetere prove senza modifica/dubbio.

### 6. Fixture browser e copertura da verificare in CI

- single-document: reload/assign/replace/iframe/open non cambiano gioco; History/hash
  funzionano. Due serverHTTP devono non ricevere il target vietato. Clic sintetici negati.
- module-order: ordineHTML/body/TLA/DOMContentLoaded, C2rejection, C3inline URL/base, classici
  e statici parse/link/fetch distinti, JSON typed/untyped/preloaderror asincrono, referrer
  classici/eval/Function, Referer del modulo importatore e CORS consentito/negato.
- module-csp: nonce dello script classico autorizza il suo import; script senza nonce negato.
- cold/warm/slow inserimento module runtime: log durata/gap timer e ripresa tick/frame;
  delay250ms del server e solo una condizione controllata, NON soglia accettata di prodotto.
- events: named-property/ownKeys/deleter invariati su documento unico con cleanup sincrono;
  document-interface-alias eseguito in3processi freschi per distinguere il primo accesso.
- gc-lifecycle mantiene controlli DOM/cicli/listener/signal/inlinehandler/importer/cleanup
  settings, moduli/eval/resolver prima/dopo GC. Non conclude raccolta del grafo.
- document-retention:8GC, controlloDOM conservato obbligatorio, WeakRef diagnostici e vero
  snapshotJSON V8 validato. Mock Node non prova raccolta reale. Vecchie prove frame sono
  archiviate in legacy-multidocument e i loro mock restano distinguibili da test attivi.
- http_server.py riusa stdlib SimpleHTTPRequestHandler/ThreadingHTTPServer su loopback;
  aggiunge solo endpoint Referer, CORS e ritardo. Nessun nuovo servizio o dipendenza prodotto.

### 7. Patch, backup e attenzione al replay

Patch0402 processo/quota+policy JSON; patch0403 sorgente del port. Candidato in
 target/task-0385/cutover-review-candidate.patch. Windows+WSL tail replay verificati su81
percorsi sorgente dopo la serie gia ricostruita; normalizzazioneLF ed eliminazioni verificate.
NON e un full-tree pristine replay: Cargo.lock root/README/vecchio probe hanno eccezioni
metadata storiche. La CI validate-servo-patches applica l'intera serie da pristine v0.5.0.
Conservare0403 pubblicata; aggiungere patch incrementali per i successivi cambiamenti.
0404/0405/0406 superano dry-run/replay e confronto LF con tutti i file correnti.
Scratch Windows gia applicato fino0413:
 C:/Users/simon/AppData/Local/Temp/roves-0315-replay-gAXo/w/src
Non riapplicare il candidato senza controllare lo stato; fare reverse verificato o usare
nuova copia. Root lock e i lock standalone aggiornati sono controllati separatamente.

NON ripristinare target/task-0385/incomplete-cutover-2026-10-28/incomplete.patch sul codice
corrente: precede molte correzioni. Anche handoff-2026-10-28/manifest.json e un backup WIP
precedente, non il sorgente attuale. Usare Git reale e diff correnti, niente reset/clean.
Esperimento JSON root SCARTATO: json-root-rejected-experiment.rs + log
json-root-diagnostic-negative-fixed.log. Accettava import senza type json, quindi non e una
soluzione pubblicabile. La copia native-workspace e stata ripulita da quel test sperimentale.

### Diagnostica inline: problema circoscritto da verificare

Il port ignora attualmente line_number in fetch_inline_module_script (circa1112):
le posizioni iniziano dalla prima riga del modulo invece della riga HTML. Il chiamante
HTMLScriptElement passa ancora la riga originale. Deno0.413 ModuleSource e
load_side_es_module_from_code non espongono un offset; modules/map.rs script_origin
usa offset0. ModuleLoader.get_source_map e pubblico ma dichiarato prossimo a deprecazione:
valutare una soluzione mantenuta e verificare insieme error.stack, parse/link e gli
attributi degli eventi. Non assumere che una mappa dello stack corregga v8::Message.
Non introdurre parser, fork o una semplice aggiunta di righe senza verificare hashbang,
sourcemap preesistenti, righe/colonne e sorgenti esterne. Nessuna correzione e stata fatta.
Prima superare il panic di avvio nella CI browser; la raccolta0407 non risolve questo.

### 8. Prossimo ciclo operativo e criteri di completamento

1. Revisionare lo staged set esplicito e preservare la riga utente in TODO. Commit coordinati
   root/wiki/action delle modifiche complete; nessun riferimento gitlink non pubblicato.
2. Concludere batch0aa2 e correggere i nuovi errori prima del push locale0410–0412.
   Identificare nuove run e SHA una volta; continuare sviluppo mentre procedono.
3. Se una run nonApple fallisce: leggere log bounded, correggere causa e ripetere verifiche
   pertinenti; aspettare conclusione altre richieste prima del push successivo. Se attive
   senza errori, continuare lavoro utile/commit locali senza push; niente polling continuo.
4. Verificare browser Windows/Linux normal/JITless, Android e controlli condivisi. Appena
   compila, raccogliere risultati di tutte le fixture nuove e correggere contratti mancanti.
5. Verifica controllata di clic reale e OS handoff (mai browserOS nel runner headless),
   rendering avviso sviluppo/logs-only bundle, chiusura con fetch/TLA/import pendenti.
6. Misurare startup e runtime inserimento moduli, freddo/caldo/rete lenta, input/timer/frame,
   errori e chiusura. Usare DevTools MCP per confronto Chrome se disponibile; dichiarare
   indisponibilita/blocco. Nessun risultato MCP o numero prestazionale e stato ottenuto qui.
7. Confrontare codice rimosso/aggiunto, avvio/RAM senza promettere risparmi; rivalutare pausa
   e guard no-compilation-cache con dati. Nessun obbligo di attendere preload JSON futuro.

Dire integrazione completata SOLO con codice coerente senza doppio grafo/backend, patch
pristine valida e verifiche necessarie browser/native/nonApple verdi, policy/documento e
sicurezza confermate, limiti/misure documentati. Retention complessiva e SourceText0361
restano problemi distinti aperti: non chiuderli solo perche Deno compila. Le CI Apple escluse
non vanno segnate verdi o eseguite per finta. Nessun polling continuo; resta autorizzato il solo trigger5ore di ripresa loop.

**FINE DEL PUNTO OPERATIVO — sotto solo storico, leggere per un problema concreto.**

## Storico recente — 26–27 ottobre (non è il punto di ripresa)

### Implementazione0374 — HTMLDocument aliasstandard, rimosso codiceinutilizzato

[Alias0374](handoff/0374-html-document-alias.md): LegacyWindowAlias sulDocumentvero,
riusando generatore/runtime giàusati daDOMMatrix. Rimossi sottoclasseRustHTMLDocument,
IDLseparata, moddeclarationeconfigobsolete. Nessunfactorynuovo odipendenza/cache.
DoctypeIsHTMLDocument eCreateHTMLDocument invariati. 9testgeneratorerealiverdi Win/Linux;
vecchioIDLroute diverso e negativalias fallisce senzaerroriimport. Runner42(8skipPOSIX),
smoke richiesto alias-first/Document-first/descriptor-first, identità/prototipo/attributi
escritturaseparata; verificabrowsernormal/JITlesspendingCI. NessunrefrescoMCPChrome.

Batchprecedentef3d59895e36 ORA tutto terminale nonApple: generale40481102827verde10/10,
Android40481102823verde3/3; V840481102911runtime4/4verde+browserWindowsverde,
browserLinux2rosse perguardHTMLDocument, causa efixDocument0373completi. Pubblicazione
permessa con quellecorrezioni incluse, nessunarunrichiestaattiva; Appleesclusa.
0371locale f9e024eea0a,0372/0373locale051f403fd65; pubblicareinsiemea0374doporeplay.
Restano sourceText0361,numericdup,define/delete e retentionglobale: non dichiarare completamigrazione.

### Batchlocale0372/0373 — chiavi visibili e correzioneDocument reale

[Handoff0372/0373](handoff/0372-0373-visible-named-keys.md): ownKeys filtra nomi
nascosti da prototype/own/unforgeable, rispettando corto circuito e rientranza; nullproto
noncausa piùpanic. FlagdaIDL, nessunbackend/cache/dipendenzanuova. Nonoverride usaVec
transitorio e seconda conversione pernomisopravvissuti; nessunrisparmioCPU promesso.

CI0370V8`40481102911`: Linuxnormal`122111336685` eJITless`122111336640` fallite:
iframe nonHTMLDocument. Il reflector reale èDocument, il cui attributo era commentato.
SpecHTML confermaDocument; attivato sulmainIDL perchéparsernonloaccettasullepartial,
con eredità versoXMLDocument. FixturevincolatoDocumentreale/prototipovero, tuttii check
comportamentali mantenuti. AliasHTMLDocument preesistente nonrisolto. Storici crosslinkati.
8test realiIDL Windows/Linuxverdi, negativaoldhelper2fallimentiveri/zeroerroriimport;
27hookRustparse, utils/proxyparse, runner42verdi(8skip). Nuovismokebrowser da verificareCI.

0371 giàlocale`f9e024eea0a`; replay0372/0373 Windows/Linux11/11percorsiidenticiLF.
Correzionecompleta inlocale; generale40481102827 ormai9/10nonAppleverdi,
Linuxportable122112403272ancoraattiva. Niente push finché quella run restaattiva. Numericduplicate,define/delete,
sourceText0361 e retentionglobale restanoaperti. ChromeMCP indisponibile, nessunrefrescoChrome.

### Implementazione0371 — nomi DOM senza CString e pin permanente, completa locale

[Fix0371](handoff/0371-supported-name-string-ids.md): conversioneDOMString esistente
più JS_StringToId standard, shimV8 tramitekey_id giàpresente. Rimuove panicNUL e pin
incondizionato dei nomi enumerati, preserva unitàstringa eordine. Wrapper rigenerato
con generatoreufficiale e liste realimozjs. Staticcross-originatoms invariati.
6normal/6JITless verdi; pinforzato respinto dall'assertGC; sorgenteisolato ripristinato,
normaleripetutoverde. VecchioCStringpanic riprodotto in frammentoRust, nonbrowser.
Runner42verdi(8skipPOSIX), parseRustverde; browserNUL/Unicode richiesto nei dueCI V8.
Replaypatch applicato,7/7percorsiidentici normalizzandoLF.
Nessuna promessa di retentionglobale osourceText risolta. Modifica completa solo inlocale.

Pubblicato0370 `f3d59895e36`, wiki`eeb3a53`. BatchV8`40481102911`, generale
`40481102827`, Android`40481102823` (verde3/3), iOS`40481102821` esclusa.
Al controllo di fineimplementazione0371 nessunfallimento: runtimeV8 Linux/Windows4/4
verdi, browserV8 e buildgenerali ancoraattivi. Niente push di0371 finché richiestiattivi.

### Implementazione0370 — letture LegacyOverrideBuiltIns complete localmente

[Fix0370](handoff/0370-legacy-override-read-visibility.md): get/GOPD/hasOwn usano
l'attributo del parser WebIDL, senza nomi classe a runtime. HTMLDocument e dataset
leggono il nome supportato prima del prototype, dopo indexed e own/unforgeable.
7 test parser/generatore verdi suWindows eLinux; runner42 verdi(8skipPOSIX).
ReplayLinux0361–0370 con baseline mirata:18/18fileidentici normalizzandoLF.
Vecchio generatore:2 fallimenti di comportamento,
zero errori import;9 corpi senza attributo identici;18 hook Rust superano il parse.
Smoke obbligatori HTMLDocument, dataset e controlloForm da eseguire normal/JITlessCI.
Nessuna buildpesante locale. ChromeDevToolsMCP indisponibile; fonte primariaWebIDL,
nessun riferimentoChrome fresco rivendicato. Backendproxy invariato e0361ancoraaperta.
Batch precedente fa7164e52f8 richiesto interamente verde. Revisione, replay e runner
conclusi; pronto al push. Mappa0368 revisionata nel commitlocale c7152ae07d1.

### Ripresa 27 ottobre — revisione 0368 conclusa, CI del fix verde

Claude disponibile al tentativo di ripresa: [mappa0368](handoff/0368-native-dom-hook-map.md)
corretta e revisionata. HTMLDocument dichiara LegacyOverrideBuiltIns; il generatore ignora
ancora l'attributo nelle letture. Le ipotesi su Form/Select e sull'ordine delle chiavi V8
restano provvisorie, senza dichiarazioni di impossibilità generale o backend già compatibile.
La fonte primaria WebIDL conferma la precedenza: indexed, own/unforgeable, poi named prima
del prototype solo con LegacyOverrideBuiltIns. Nessuna nuova dipendenza necessaria.

Batch pubblicato fa7164e52f8: generale40399631700, dieci job nonApple tutti verdi,
compresa validazione completa delle patch e build Windows/Linux; Android40399631807
verde3/3. Le build prima saltate sono ora verificate dal nuovo batch. Apple esclusa.
Ricontrollo `ripresa-unica-claude-dopo-quota-0347` eliminato dopo il tentativo.
Il risveglio effettivo è avvenuto il27ottobre10:13 Europe/Rome, dopo l'orario previsto.

Prossimo incarico0370 delegato: correggere solo le letture LegacyOverrideBuiltIns nel
generatore (get/GOPD/hasOwn), con test del generatore e smoke richiesti normali/JITless.
Riusa parserWebIDL e generatori esistenti. Non cambia backend, né risolve sourceText0361.
ChromeDevToolsMCP indisponibile alla ripresa (Targetclosed); nessun nuovo riferimentoChrome
rivendicato. Heavybuild soloCI. Commit/push dopo revisione e verifiche necessarie.

### Stop per quota 0368 — reset 27 ottobre 01:30, singola ripresa 01:32

Claude si è fermato durante la revisione della mappa 0368. Nessuna modifica di produzione
incompleta è rimasta nei sorgenti pubblicabili. `/usage`, aperto nella CLI il 26 ottobre
alle 21:54, conferma sessione al 100% e reset alle 01:30 Europe/Rome del 27 ottobre.
La CLI è stata chiusa senza richieste al modello. Un solo tentativo alle 01:32 è stato
programmato riusando `ripresa-unica-claude-dopo-quota-0347`: disattivarlo dopo il tentativo.

Bozza NON revisionata: `target/task-0368/map-unreviewed.md`, 10041 byte, SHA256
`88ee476b5accf4e0fcc6475ac22e89f629cc5fd861952586dcf76111844bc1d7`.
Prompt di correzione: `target/task-0368/review.txt`; copie delle API pubbliche V8 nella
sottocartella `v8-api/`. Alla ripresa Claude ripristina e corregge la bozza, Codex revisiona.
Non ripetere la ricerca già svolta. Correggere il riferimento a HTMLDocument, che dichiara
LegacyOverrideBuiltIns, e le conclusioni assolute non dimostrate sul backend V8.
Nessun routing codegen adottato; la compatibilità sourceText 0361 resta aperta.

Pubblicato `fa7164e52f8`, wiki `01f0db1`. Nuove CI generale `40399631700` e Android
`40399631807` attive alla prima lettura; iOS `40399631579` esclusa insieme a macOS.
Nessun polling o push aggiuntivo. Questo checkpoint resta locale finché il batch non
consente la pubblicazione. Se alla ripresa persiste la stessa quota, registrare l'esito
e fermarsi senza concatenare altri tentativi.

### CorrezioneCI0369 — percorsi patch portabili verificati suLinux

Generale40397621958: validatepatches121834747087 rossa, build saltate (non superate).
Causa:0361 creava un nome con backslash suLinux,0363 cercava il percorso con slash.
[Fix0369](handoff/0369-linux-patch-path-fix.md): dueheader corretti daClaude;
negativa riprodotta, replayLinux0361–0367verde,10/10 fileidentici normalizzandoLF.
Android40397621986verde3/3; Apple esclusa. Correzione pronta per prossimo push,
nessunaltro jobnonApple attivo nelbatch. Non dichiarareverificatelebuildsaltate.
Mappa0368inrevisione: HTMLDocumenthaLegacyOverrideBuiltIns; nonconfondereDocument
baseconHTMLDocument. NessunroutingV8adottato;0361compatibilitàsourceTextrestaaperta.

### Checkpoint0367 — proprietà ordinarie provate normal/JITless

[Prova0367](handoff/0367-ordinary-properties-proof.md):1normal/1JITless verdi.
Accessor/receiver esterni sono corretti senza simulazione; ordine esatto generico
`5,x,acc,Symbol(s),named` diverso dal Proxy attuale e dal riferimento DocumentChrome.
ChromeDevToolsMCP155 visibile: misurate Document/form/select reali in iframe;
nessuna adozione del backend generico. Diff sperimentale durabile, tests.rs ripristinato.
Prossimo gate0368 delegato: mappare hookWebIDL reali e API mantenute descriptor/definer;
nessunroutingcodegen prima di una mappa compatibile.0361 resta aperta.
Pubblicato5464c544278/wiki dd08535; nuoveCI generale40397621958,
Android40397621986, iOS40397621971 esclusa. V8non riavviata (nessuna modifica runtime).

### Checkpoint0366 — backend respinto, sorgenti ripristinati

[Diagnosi0366](handoff/0366-api-backend-blockers.md): il compile conferma che manca
il receiver nell’API pubblica V8; usare holder altererebbe gli accessor con receiver
esterno. Respinto anche il confronto delle chiavi come insiemi. WIP salvato in
target/task-0366/incomplete-api-backend.patch, cinque sorgenti ripristinati aHEAD.
Nessuna implementazione incompleta pubblicata;0361 resta aperta. Prova0367 delegata:
proprietà ordinarie V8 per expando/accessor, interceptor solo per dati supportati;
misurare receiver e ordine esatto prima di modificare codegen.
Batch3e25 completato: V8 10/10, generale13/13, Android3/3 verdi. iOS esclusa;
macOS non blocca per istruzione utente. Commit di documentazione locali0363–0365
ancora da pubblicare; prossimo controllo CI dopo pubblicazione, nessun polling.

### Ripresa autorizzata26ottobre — Apple esclusa dal blocco, pubblicazione consentita

Nuova istruzione utente: ignorare CI macOS/iOS e proseguire. AGENTS/workflow allineati:
nonApple obbligatori verdi/terminali, Apple documentata come esclusa, non superata.
Batch378153: generale40366152303 TUTTA verde13/13, V840366152633 Linux/Windows7/7verdi
più runtimeMac2verdi/browserMaccancelled escluso; Android40366152179verde; iOS40366152391
cancelled esclusa. Pubblicare0357–0360 e documentazione0361/0362 consentito. NessunWIP.
C:~0.99GiBlibero, D:530.67GiB. Evitare nuove copie di build; build pesanti soloCI.
Prossimo incarico0363: prova mantenuta V8 per scopeProxy/funzioniwrapped/sourceText (piano0361),
senza overrideglobale/parserprivato. Claude implementa una prova circoscritta, Codex revisiona.

### Checkpoint0364/0365 — design e primo gate verificati, runtime invariato

[Design0364](handoff/0364-real-dom-api-reflector-design.md) pronto come mappa, non implementato.
[Prova0365](handoff/0365-api-reflector-proof.md):2normal/2JITless verdi. V8rifiutagiàpreventExtensions
coninterceptor; oggetti REALI comecatena(scope) preservanothis/sourceText/GC. DifferenzaChromeDoc
named-onlyancoradistinta. Fixturecorretta dopoabort:0internalfields perchéClassBoxnoninizializzato
fuori perimetro; nientecambioproduzionepernasconderlo. Logtarget/task-0365/. Prototiposalvatoin
diffdurabile, tests.rs ripristinatoHEAD.0361aperta, prossimo0366 gateClassBox/ProxyTraps reale
prima di routingcodegen. Mac/iOS nonbloccano pernuovaistruzione; altriCIrimangonoobbligatori.

### Claude0365 — gate nativo API object (prima esecuzione Codex, correzione fixture)

Esecuzione1: compilazione riuscita (firme indexed verificate). Test preventExtensions VERDE:
normale/API senza interceptor diventano non estensibili; named e named+indexed false/TypeError
e restano estensibili: V8 lo impone già, come form/select Chrome; diverso da Document Chrome.
Test scope ABORTITO: il fixture aveva un internal field non inizializzato letto come ClassBox
(`scope-backtrace.log`). Correzione solo fixture: zero internal field, asserzioni invariate,
nessun ClassBox reale ancora coperto. In attesa della riesecuzione Codex.

Sessione solo Read/Edit (shell, registry e git negati). In `tests.rs` nuovo modulo isolato
`api_reflector_proof`: `api_object_interceptors_refuse_prevent_extensions_natively` e
`api_object_scope_extensions_keep_bare_call_receivers_and_source_text`. API object generici
(ObjectTemplate, 1 internal field) con interceptor named-only e named+indexed su store JS, non
DOM Servo. Atteso: V8 rifiuta nativamente preventExtensions con interceptor (Reflect false,
Object TypeError, resta estensibile) come form/select Chrome, ma diverso da Document Chrome
(riesce): differenza di configurazione da decidere in un gate successivo. Atteso anche
`who() === rovesDocument` con l'oggetto stesso come extension, sourceText esatto, identità e GC.
Gate0: firme indexed non verificate. Ripresa Codex: compilare, test normale/JITless, esiti reali in
`target/task-0365/RESULTS.md`; se diverge salvare diff/log e ripristinare tests.rs. README/wiki invariati.

### Claude0364 — design reflector DOM come API object (da revisionare, nessun codice)

Solo documentazione: [design0364](handoff/0364-real-dom-api-reflector-design.md). Per i proxy
DOM eleggibili (famiglia DOM, prototipo non lazy, nessun trap prototype/call/construct; routing
codegen per interfacce Node) il valore V8 diventa il target API object già esistente, con
interceptor named/indexed che chiamano gli stessi ProxyTraps; `GetProxyHandler`/private/slot/
trace/finalize leggono già il ClassBox. `CompileFunction` usa le extension V8 solo se nessun
oggetto della catena è un `v8::Proxy`, altrimenti resta il `with`. Rischi aperti: nessun hook V8
per [[PreventExtensions]] (riferimento Chrome non raccolto: MCP negato), API interceptor rusty_v8
indexed/descriptor/definer/`this()` non verificate (registry e docs.rs negati), creazione expando
via `Reflect.set`. Refactor ampio sconsigliato ora. Prossimo passo unico: 0365 prova nativa in
roves-js con gate di compilazione e test differenziale Proxy vs API object. Patch doc0364 da
generare (Codex). README/wiki invariati.

### Checkpoint0363 — facade provata normal/JITless e respinta, runtime invariato

Testmirato1normal/1JITless verdi suAPIpubblicheV8; dati/logtarget/task-0363/. Firma setter
corretta inrevisione. SourceText/lookup/scritture/delete/unscopables/GC funzionano, ma chiamata
nonqualificata ricevefacade inveceoggettoDOMreale: regressione nonaccettabile. Nessunfixproduzione.
Diff verificato e diagnosi in[prova0363](handoff/0363-scope-facade-proof.md); tests.rs ripristinatoaHEAD,
nessunprototipoinutilizzato nei sorgentipubblicabili.0361ancoraaperta. Passosuccessivo0364:
designlimitato reflectorDOMreali APIobject+interceptor, riusandohookesistenti, senzaWindowProxy.
C:~0.86GiB, riusareworkspaceisolati, niente nuovecache/buildpesantilocali.

### Claude0363 — prova nativa facade con interceptor (da revisionare, non eseguita, non committato)

Solo test, nessuna modifica di produzione. In `components/roves-js/src/tests.rs`, modulo
`scripts`: `scope_facades_match_source_text_and_lookups_but_rebind_bare_call_receivers` con helper
`scope_facade`/`compile_handler_with_facades`/`run_scoped`. Ogni Proxy della catena diventa un
oggetto `ObjectTemplate` con interceptor nominali (stringhe e simboli) getter/query/setter/deleter
che inoltrano al Proxy (dato dell'handler, tracciato da V8; prototipo null; niente enumerator),
passato a `CompileFunction` V8 come context extension. Atteso: sourceText identico a V8 senza
scope e Chrome155; esiti e log esatto delle trap uguali al wrapper `with` di produzione (lettura,
scrittura, delete, accessor ereditati con receiver Proxy, typeof mancante, unscopables, ordine,
strict/sloppy) e sopravvivenza dopo GC completa. BLOCCO asserito come osservato: una chiamata
nuda di un metodo dello scope (`who()`) riceve la facade come `this` (WithBaseObject = oggetto
del binding); nessuna API pubblica lo ricollega. `submit()`/`getElementById()` negli handler
fallirebbero, quindi produzione invariata e lacuna0361 ancora aperta. Unica via API mantenuta
rimasta: reflector Document/form come oggetti API V8 con interceptor invece di Proxy JS (come
Blink), modifica separata e ampia. Prima compilazione Codex: E0631 sul setter (rusty_v8 152
vuole `ReturnValue<Boolean>`); corretto: restituisce l'esito inoltrato di `set`, lancia
TypeError solo se rifiutato con should_throw_on_error, nessun panic nei callback. Shell negata in questo
incarico: diff/patch0363 e copia in target/task-0363 non creati, li genera Codex. Se il test
fallisce, spostarlo in target/task-0363 e ripristinare tests.rs a HEAD. README/wiki invariati.
Ripresa Codex: test normale e JITless di `scripts`, poi patch0363.

### Checkpoint0361 — diagnosi salvata, blocchi reali disco e runner

0357–0360 restano completi locali, nessunpush.0361 è diagnosi della lacunaHTMLsourceText:
[handoffdurabile](handoff/0361-source-text-gap.md) con stringhe osservate/specprimaria e
prossimopasso APIpubblicaV8. TestWIPsalvati intarget/task-0361/incomplete-source-text-tests.patch
(SHA256167cf52d7c3a555c6a1f10b5bb7d143e0d33c1b58ffd027dbbae149f67e4ff86), tests.rs
ripristinatoaHEAD. Nessuntestfallenteignorato nei sorgentipubblicabili. Patch0361 è SOLIdocumentazione.
C: arrivato0liberi; recuperati500MiB da artefattigeneratidella sessione (soloexe/pdb), ora~81MiB.
Normaleinizialeverde/JITlessfinaleverde, relinknormalefinaleLNK1201perrisorse: non dichiarare
compilazionefinalenormaleverificata. Servespazioadeguatoprimadialtribuild, niente puliziecachesgeneriche.
RetryV8termina9verdi1cancelled(Macbrowser121735908007); retryiOS2verdi1cancelled(ios121736581176).
Annotazioniripetono mancataacquisizionehostedARM64; nessunretryconcatenato. Generaleultimo11verdi2attivi.
Il loop si sospende perquestiblocchireali, nonperfineincarico/commit o solaCIincorso. Nessunpolling/
automazioneperiodica. NessunClaude/server/tabtestattivo. Riprenderequando risorse/capacitàcambiano.

### Claude0361 — sourceText degli handler con scope: diagnosi + regressione (da revisionare, non committato)

Nessuna modifica di produzione. In `roves-js/src/tests.rs` (`scripts`) solo il test pubblicabile
`scoped_handlers_keep_their_source_text_name_and_scopes` (catena vuota contro Proxy reale:
`name` onclick, `length` 1, corpo presente, nessun `with (`/`this[0]`, `this` e scope; stampa
le stringhe con `--nocapture`). NON copre il contratto completo. Normale verde (Codex). Il test
di uguaglianza proposto, atteso fallire, è fuori dal sorgente in
`target/task-0361/proposed-source-text.diff`. LACUNA HTML APERTA: il passo 3.9 di
https://html.spec.whatwg.org/multipage/webappapis.html#getting-the-current-value-of-the-event-handler
(verificato da Codex) rende normativo `function nome(parametri) {`+LF+corpo+LF+`}`; con scope il
letterale interno perde nome e LF. Nessuna API V8 mantenuta: niente NFE con nome (binding
locale che maschera proprietà), override globale, layout privato o parser. README/wiki/CUSTOM
documentano la lacuna. Ripresa Codex: JITless, stringhe, patch0361. Dettagli RESULTS.

### Checkpoint Codex0360 — negativo unsafe-hashes verificato locale

Chrome155Windows visibileHTTP19060: exacthashsenzaunsafe-hashes bloccaelemento/body, true true,
contatore0. Runner42test34pass8skipPOSIX, node/bashsyntaxriusciti, patch0360dryrun/apply3/3
LF-identici. Tab/serverchiusi, runtimeimmutatononritestato. NessunServoresultdichiaratoprimadellaCI.
Alla fineincarico: generale40366152303 6successi7attivi; V8403661526339successi1queued(browserMac),
iOS40366152391 2successi1queued; Androidverde. Nessunrosso nei retry. Nonpushare0357–0360.
Continuare con verifica precisa del sourceText/toString degli handler compilati: ambito non
verificato in0356/0357, niente formatter/parser JS proprio se manca un'API mantenuta.

### Claude0360 — fixture CSP negativa: hash corretto senza unsafe-hashes (da revisionare, non committato)

Nessuna modifica di produzione. Nuova pagina `support/v8-smoke/csp-attr-no-unsafe-hashes.html`:
`script-src 'nonce-roves0359'; script-src-attr 'sha256-u2Dl…rlo=' 'nonce-roves0359'` senza
`'unsafe-hashes'`. Button e `<body onpopstate>` (inoltrato a Window) col corpo esatto
`window.rovesHashRuns += 1` devono restare bloccati (delta 0, getter `null`): verifica che si usi
la policy ScriptAttribute e non ScriptElement. Marker richiesto in `run.sh`:
`ROVES-V8 csp attribute no-unsafe-hashes blocked true true`; negativo runner
`csp-no-unsafe-hashes-body`. Log aggiunto alla diagnostica e all'elenco linger. Nessuna verifica
eseguita da Claude (solo Read/Edit/Write). Ripresa Codex: runner/sintassi, patch0360 (nuova,
dry-run/replay), riferimento Chrome MCP (atteso blocked true true, non ancora eseguito).
Dettagli `target/task-0360/RESULTS.md`.

### Checkpoint Codex0359 — fixturehashCSP complete locali, retryCI avanza senza rossi

Hashstdlib sulcorpoesatto: sha256-u2DlkGs1w7OVohm5ziC+x+MCMKVoP4mY+P4A8n5+rlo=.
Chrome155Windows visibileHTTP19059: perentrambelepagine allowed true true eblocked true true true;
nonce non autorizzaattributi. Runner42test34pass8skipPOSIX, node/bashsyntaxriusciti. Patch0359
replay4/4LF-identici. Server/tabchiusi. Nessunruntimecambiato, suitesnativeimmutatenonripetute.
Alla fineincarico: generale40366152303 6verdi5attivi2queued, V8403661526337verdi2attivi1queued,
iOS403661523912queued1verde; Androidverde. Nessunrosso nei retry, macOScominciaaessereacquisito.
NONPUSH0357/0358/0359 finoaconclusioneverde di tutte leprecedentirun. Continuareincarichicircoscritti.

### Claude0359 — fixture CSP con hash per handler da attributo (da revisionare, non committato)

Nessuna modifica di produzione. Nuove pagine `support/v8-smoke/csp-attr-hash.html`
(`script-src-attr 'unsafe-hashes' 'sha256-…' 'nonce-roves0359'`) e `csp-attr-fallback.html`
(solo `script-src`, fallback): corpo esatto consentito `window.rovesHashRuns += 1` su button e
`<body onpopstate>` inoltrato a Window; bloccati `+= 2` (button), `+= 8` (`<body onhashchange>`)
e button con `nonce` `+= 4`. Marker richiesti in `run.sh`: `ROVES-V8 csp attribute {hash,fallback}
allowed true true` e `... blocked true true true`; negativi runner `csp-hash-allowed`,
`csp-hash-nonce`, `csp-fallback-allowed`, `csp-fallback-body`. **Segnaposto da sostituire
prima di tutto:** `ROVES_0359_SHA256` (2 file) con base64 SHA-256 hashlib del corpo esatto.
Nessuna verifica eseguita da Claude (solo Read/Edit). Ripresa Codex: hash, runner/sintassi,
patch0359, riferimento Chrome MCP. Dettagli `target/task-0359/RESULTS.md`.

### Handoff finale26ottobre — retryinfra in corso, sorgenti puliti, pubblicazione sospesa

Pubblicato37815319987/wiki1fa4cad. Locali: d4bce0a8648(colonne0357),4c230254c05(CSP0358),
06c271cc5c9(checkpoint). Wiki locale84ae0ce include954fe44. NessunWIP. Tuttiitestleggeri,
negativi motore0357 e replay riportati sotto; nessunnuovopush mentre batch incompleto.
Retrygeneral40366152303: macOSprobe121734699395 inprogress alla conferma. RetryV8
40366152633: macOSbrowser121735908007, runtime121735908725/121735908738 queued. Nessunjob
riuscito ripetuto. AncheiOS40366152391 è terminata con2cancelled peridentica capacitàARM64:
121724386920 e121724809162 nonacquisiti, annotazioni confermate; avviato UNrerun-failed-jobs
iOS viaGitHubApp. Nessunretryconcatenato se capacitàpersiste. Android40366152179verde.
Prossimo stepnecessario: raccogliere esiti retry; correggere eventuali difetti veri e ripetere
verifichemancanti prima di pubblicare0357/0358. Nonsegnarecancelled/skippedverdi. La ricerca
su nuovi owner non è giustificata dallascena frame attuale, raccolta; proseguire solo con
un difetto concreto o regressione necessaria, senza auditillimitati durante l'attesa.
NessunClaudeattivo, nessunserverlocale/tabtest rimasti. Nessunpolling/automazioneperiodica.

### Evidenza CI0356 aggiornata —26ottobre, retrymacOS mirati, NO PUSH

Batch378153: Linuxbrowsernormal121724285356 eJITless121724285389 VERDI. Entrambi: selfcapturing
inline raccolti true, controlloinline true, windowinline true, gc resultpass. Anche Windowsbrowser
verde; runtimeLinux/Windows4/4 verdi. Normalframe rilascia elemento/documento/probe comeprima.
V840366152633 termina7successi3macOScancelled (runtimeMacnormal121724285460 aggiunto ai2noti).
Codex avvia UNrerun-failed-jobs delworkflowV8 viaGitHubApp dopo conclusione, perrecuperareMac
nonacquisiti (capacitàhostedARM64), senza ripetere jobriusciti. Generale40366152303 giàretry
mirato delmacOSprobe avviato; iOS40366152391 ancoraqueued, Androidverde. Questi retry NON sono
checkpointverificato: nessunpush di d4bce0a8648/0358 finché batch nonconsente. Nessunpolling
continuo o retryconcatenati della stessacapacità se persiste; resta bloccoesternorunner.
GCinline non è più soloattesa: scenari0355 confermati inCI normal/JITless dopo fix0356.
CSPbody0358 e colonne0357 restano commitlocali, convalidazionelocale descritta sotto.

### Checkpoint Codex0358 — fixCSP completo locale, batch0356 bloccato da capacitàmacOS

0357 locale d4bce0a8648/wiki954fe44 (nonpubblicati).0358 controlla sempre l'elemento cheporta
l'attributo, anche per i targetWindow da body; riusa l'helperCSP/crate esistente. FixtureHTTP
Chrome155Windows visibile:3bloccati true,2consentiti true conTrustedScript. Runner42test34pass
8skipPOSIX; sintassiJS e parsing3Rust riusciti. Patch0358dryrun/apply7/7fileLF-identici;
negativoServo precedente è ATTESO, non eseguito. Dati/logtarget/task-0358/. Nessunbuildpesante.

Batch37815319987: generale40366152303 failure per macOS121724284612 CANCELLED(primaavvio),
steam/buildsaltati; V840366152633 ancoraattiva conMac121724285268 e121724285503cancelled,
Macnormalruntime ancoraqueued. AnnotazioniAPI: 'The job was not acquired by Runner of type
hosted even after multiple attempts'; vincolocapacitàmacOSARM64, runner_namevuoto e nessunlog.
Codex ha avviato UNretry deljobgenerale121724284612 tramiteGitHubApp(write), nessunPATinfile.
Non cambiarepiattaformasilenziosamente, nonritentareinloop lo stessobloccocapacità. AltermineV8
valutare unretrymirato deicancellati per completare le verifichemancanti, non unnuovopush.
Nessunpush di0357/0358 finché tutte leprecedentirun nonsono terminali e lemancanti verificate.
Prossimo: esitoLinux0356 attributi/GC e recupero jobmacOS. Niente automazioneperiodica/polling.

### Claude0358 — audit CSP degli handler da attributo (da revisionare, non committato)

Il controllo CSP non sta nel compilatore: per HTML (attribute change steps, passo 1.3) avviene
all'impostazione dell'attributo in `EventTarget::set_event_handler_uncompiled`, con l'helper
esistente `should_elements_inline_type_behavior_be_blocked` (crate `content-security-policy`,
`ScriptAttribute`: `script-src-attr` con fallback `script-src`, unsafe-inline/hash/report). Se
bloccato l'handler non viene salvato, quindi `get_compiled_event_handler` non vede mai sorgente
bloccata. Lacuna reale (ereditata da upstream): il controllo partiva solo se il target era un
`Element`; gli attributi di `<body>` inoltrati a `Window` (`onpopstate`, `onresize`, …) lo
saltavano. Fix minimo: il metodo riceve l'elemento dell'attributo (`element.rs` passa `self`,
`htmlbodyelement.rs` il body). `element.onclick = "stringa"` non compila (IDL → `null`).
Regressione: nuovo `support/v8-smoke/csp-attr.html` (`script-src-attr 'none'`) richiede
`ROVES-V8 csp attribute blocked true true true` (markup, setAttribute, body inoltrato; il codice
vecchio dà il terzo `false`); `csp.html` aggiunge il controllo consentito
`ROVES-V8 csp attribute allowed true true` (TrustedScript). Negativi runner `csp-attribute` e
`csp-attribute-allowed`. Nessuna verifica eseguita da Claude (solo Read/Edit). Ripresa Codex:
generare `0358-csp-forwarded-body-attribute-handlers.patch` (3 sorgenti + 4 file smoke, nuovo
file incluso), runner/sintassi, riferimento Chrome MCP di `csp-attr.html`/`csp.html`; build e
browser in CI. Memoria invariata: frame normale036abe elemento/documento/probe rilasciati,
GC additivo4 marker in entrambi imodi, GC inline attende la CI0356. Dettagli
`target/task-0358/RESULTS.md`.

### Checkpoint Codex0357 — colonne e scope verificati, commit locale durante CI0356

0356 pubblicato3781531998742f435352c4dd4e5bcb5fe37ad0d2/wiki1fa4cad. Nuovo batch:
V840366152633(gli ultimi conteggi4verdi3attivi3queued), generale40366152303(5verdi1queued),
Android40366152179verde, iOS40366152391(1verde2queued); nessunrosso alcheckpoint0357.
NON PUSHARE0357 finché il batch non termina e gli eventuali errori non sono risolti.
0357 usa ScriptOrigin offset negativo e supera139normal/138JITless,2/3ignorati. Vecchiaorigine
compila e fallisce in entrambimodi: prima riga colonna61 invece6. Sintassi, stack, nomi,
strictmode/unscopables/commentofinale e scope dopoDropEnvironmentChain+GC coperti dai2testnuovi.
Patchdryrun/apply riusciti,2/2fileLF-identici. Limitecolonne0356 risolto nei testmotore;
non ancora confermato nelbrowser. Logtarget/task-0357/. Prossimo controllo: policyCSP degli
attributi inline nelpercorso appenaabilitato, conprimitiveCSPesistenti, diagnosicircoscritta.

### Claude0357 — posizioni sorgente degli handler con scope (da revisionare, non committato)

Base37815319987. Il wrapper `with … return function (args) {` della0356 condivide la prima riga
del corpo: errori runtime e stack su quella riga avevano colonne spostate della lunghezza del
prefisso. V8 applica il column offset di `ScriptOrigin` solo alla prima riga: l'origine del
wrapper (stesso helper `script_origin_with_data`) sottrae ora la lunghezza UTF-16 del prefisso
(offset negativo con colonna1). Righe, colonne successive, compilazione del solo corpo e catena
vuota invariate. Offset negativo NON ancora provato a runtime: nessun test eseguito qui
(solo Read/Edit). Nuovi test in `roves-js` `scripts`:
`scoped_handlers_keep_body_names_locations_strictness_and_scopes` (confronto con la
compilazione V8 non avvolta, colonne assolute, frame `onclick`, strict, unscopables, commento
finale) e `scoped_handlers_keep_their_scopes_after_the_chain_and_a_full_gc` (catena eliminata,
`JS_GC`+cppgc). Se il primo fallisce sulla colonna assoluta della prima riga, ripristinare la sola
modifica dell'origine e lasciare il limite documentato. Patch0357 da generare. GC inline e
browser restano pendenti nella CI0356. Dettagli in `target/task-0357/RESULTS.md`.

### Checkpoint Codex0356 — fix scope proxy verificato localmente

Batch36abe concluso: V8402417384778/10 (soliLinux normal/JITless rossi per handlerdaattributo),
generale4024173847513/13, Android40241738569 e iOS40241738470 verdi.0356 corregge il
compilatore, non indebolisce fixture: completo137normal/136JITless verdi,2/3ignorati;
vecchio script_impl.rs compila e fallisce la nuova catenaProxy in entrambimodi. Runner42test
34pass8skipPOSIX, node--check e patchdryrun/apply riusciti,3/3fileLF-identici. Logtarget/task-0356/.
Resta la limitazione dichiarata: colonna prima riga del corpo spostata dal prefisso generato;
le righe e gli errori di sintassi validati sul corpo originale restano conservati. Nessun
buildServo locale. Pubblicazione0356 consentita ora; il nuovo browserCI confermerà attributi/GC.

Evidenza memoria aggiornata del batch36abe: i4marker additiviGC sono tutti veri in normal/JITless.
Il frame-retention del jobLinuxnormal121314800007 rilascia elemento/documento/probe dopo8GC:
caso specifico verificato, non garanzia globale. Snapshotartifact12617651629(run40241738477)
2729205byte, SHA256f7e6d0b4138c03f136f91b240dc48e84b9dfa2a16572e9483578cd8e667cabbb.
MemLab2.0.5 sullo stesso scriptcensus mantenuto: RootSet20,40archi, nessuna closure diretta, unaPromise duplicata e realm/globalparent.
Nessuna deduzioneRAM dai byte. GCinline ancora da confermare: paginainterrotta prima dei check.
Vecchie note di frame non rilasciato sotto sono storiche; non riutilizzarle come statoattuale.

### Claude0356 (correzione) — handler da attributo compilati con scope proxy (da revisionare, non committato)

Base5d7912535e6. V8 run40241738477, Linux normal121314800007 e JITless121314800006: events.html
fallisce il controllo obbligatorio `inline event handlers keep identity, position and return
semantics` con `attribute compile: object` (`onclick`), gc-lifecycle.html si interrompe con
`TypeError WeakRef: invalid target` su `kept.onfocus`. Nessun errore di compilazione nei log.
La prima consegna0356 affermava che il controllo `onclick` di events.html passava: era FALSO.
Ogni handler da attributo di un elemento restituiva null, a prescindere dal nome.
Causa dimostrata: la catena di scope contiene il reflector Document, che è un proxy DOM
(`DocumentBinding` `proxy_handler: Some`, quindi `v8::Proxy` da `NewProxyObject`). V8
`ScriptCompiler::CompileFunction` accetta come context extension solo oggetti JS ordinari e
restituisce un handle vuoto senza eccezione. Il compilatore non è cambiato dalla0355: la lacuna
era preesistente, e la0355 ha aggiunto i primi test che leggono un handler compilato.
Fix (solo glue, API V8 esistente): `roves-js` `CompileFunction` compila prima il corpo da solo
(errori di sintassi con posizione reale). Con una catena non vuota lo avvolge in una funzione
sloppy `with (this[i]) … return function (args) {corpo\n}`, chiamata con l'array della catena
come `this`. Le righe restano invariate; cambiano solo le colonne della prima riga.
Regressione nativa in `roves-js/src/tests.rs`: catena reale con `Proxy` e oggetto interno con
`Symbol.unscopables`, più il caso di sintassi errata. Fixture GC: guardia `weakCompiledHandler`
ridotta a nome/tipo/valore. Rimossi i probe speculativi e i 2 mock Node della prima bozza;
`test_runner.py` torna uguale a HEAD. Nessuna verifica eseguita qui: per Codex test roves-js
(normale/JITless), `node --check`, runner, patch0356 e CI. Dettagli in
`target/task-0356/RESULTS.md`. Retention frame complessiva APERTA.

### Pubblicato0353+0355 —36abe9363db; nuovo batch pendente

Rootpubblicato36abe9363dbae986926d2a59af2ae74b69d25aa8, wiki e76e098. Nuove run:
V840241738477 inprogress, generale40241738475/Android40241738569/iOS40241738470 queued
alla lettura aggregata dopo push. Nessun nuovo push finché questo batch non lo consente.
Precedente9f62 tuttoverde. Prossimo lavoro utile richiede l'esito tipi/GC e lo snapshot di
questo batch: controllare i marker nuovi normal/JITless e il censusRootSet conMemLab;
solo un owner residuo dimostrato giustifica un'altra modifica di ownership. Nessun polling
continuo o automazione periodica. Questo aggiornamento checkpoint resta commitlocale.

### Checkpoint Codex —0353 e0355 complete, batch0351 tutto verde

Batch9f62b30b1e8 concluso: V84023847395210/10, generale4023847390013/13,
Android40238473901/iOS40238473983 verdi. Pubblicazione ora consentita; include commitlocale
31036a74536 (listeneradditivi0353/wiki949d720) e handlerinline0355/wiki successiva.
0355 revisionata: runner42test34pass8skipPOSIX, node--check delle2pagine e parsingRust
riusciti; fixtureinline reale true suChrome155DevToolsMCP visibile/aboutblank, senzaGCforzata.
Patch0355 generata dal diff reale, dry-run/apply6/6fileLF-identici nello scratch. Codegen0353
28file43metodi limitato al nuovo costruttore già verificato;0355 non cambia il generatore.
Nessun buildServo o fullsuiteroves-jsimmutata ripetuti. TipiServo, GCnuovi e raccoltaframe
rimangono da verificare nel nuovo batch. NegativiGC descritti sono attesi, non eseguiti.
Prossimo passo: verificare CI e nuovo snapshot con strumenti mantenuti; identificare l'owner
residuo soltanto da evidenza aggiornata prima di cambiare altri storage di callback.
Nessuna pianificazione periodica e nessun polling continuo. Note sotto conservate come
consegne storiche, superate da questo checkpoint.

### Claude0355 — handler inline tracciati dal proprietario DOM (da revisionare, non committato)

Base31036a74536 (0353 locale), batch0351 con V8 10/10 verde e generale ancora attiva: nessun
push. Implementato il design0354 nel solo `eventtarget.rs`: `TracedEventListener` →
`TracedCallback` generico su `CallbackContainer` (nessuna API pubblica `unsafe` nuova),
`InlineEventListener::Compiled(TracedInlineHandler)` = `InlineHandlerKind` + `TracedCallback`,
`CommonEventHandler` solo transitorio; setter senza ricattura dell'incumbent, protezione dal
rientro di compilazione preservata, getter/dispatch/coercizioni invariati. Prove: events.html
+1 REQUIRED_CHECK con i valori Chrome155 forniti da Codex; gc-lifecycle.html nuovo gruppo GC
handler inline (closure, attributo compilato, controllo mai compilato) + controlli trattenuti
(nodo tenuto, window onerror/onbeforeunload); run.sh 3 marker, fake `gc-inline`. Nessuna
verifica eseguita localmente (solo Read/Edit): runner, patch0355 e replay a Codex; compilazione
e GC solo in CI; il negativo GC pre-fix è un atteso. Retention frame complessiva APERTA.
Dettagli e percorsi patch in `docs/handoff/0355-inline-handler-ownership.md` e
`target/task-0355/RESULTS.md`.

### Claude0354 — design ownership handler inline (da revisionare, nessun codice)

Base31036a74536 (0353 locale), batch0351 ancora attivo: nessun push. Design in
`docs/handoff/0354-inline-handler-ownership.md`: `InlineEventListener::Compiled` passa da
`CommonEventHandler` (Rc radicate) a `TracedInlineHandler` = tag di tipo + record0353
generalizzato (`TracedEventListener` → `TracedCallback`, metodi generici su
`CallbackContainer::new_with_incumbent`); `CommonEventHandler` resta transitorio per getter e
dispatch, coercizioni onerror/beforeunload invariate; setter senza ricreare la Rc; protezione
dal rientro di compilazione preservata. Solo `eventtarget.rs` + fixture. Fixture mancanti:
identità/ordine/attributo/BeforeUnloadEvent in events.html, GC handler inline in
gc-lifecycle.html. Nessun blocco. Retention frame complessiva APERTA. Prossimo: 0355
implementazione dopo riferimento Chrome via MCP per i punti indicati.

### Checkpoint Codex0353 — implementazione completa locale, CI precedente ancora attiva

0351 pubblicato9f62b30b1e8/wiki673cb44. Batch: V8402384739529/10 verdi (browserWindows
attivo), generale40238473900 ancora attiva senza rossi, Android40238473901/iOS40238473983 verdi.
Linuxbrowser normal/JITless e macOS confermano il fix WindowProxy0351. NESSUN PUSH di0353
finché batch non termina; committare localmente consentito. Replay0353 dry-run/apply riusciti,
11/11 file modificati LF-identici, incluso v8.yml (non escluso dalla patch). Codegen/runner/
Chrome come sotto. Non considerare verificati tipiServo o GCnuovi prima della loro CI.
Il negativo incumbent descritto è un atteso della fixture, non un testServo già eseguito.
0352 revisionato e implementato da0353. Prossimo: studiare ownership handler inline nello
stesso modello mantenendo Rc native e callbackcontext, senza duplicare0353.

### Claude0353 — listener additivi tracciati dal proprietario DOM (revisionato, non committato)

Base9f62b30b1e8, CI del push in corso: nessun push. Implementato il design0352 nei5 file:
`CallbackObject/CallbackFunction/CallbackInterface::new_with_incumbent`, metodo di trait
`CallbackContainer::new_with_incumbent` generato da `CGCallbackFunctionImpl` (forma identica al
`new` generato); `EventListenerType::Additive(TracedEventListener)` con callback
`Box<Heap<*mut JSObject>>` e incumbent `Dom` della registrazione; `RemovableDomEventListener`
conserva solo l'identità del callback (Clone manuale) e l'abort usa
`remove_event_listener_by_callback`; snapshot di `invoke` in `RootedTraceableBox`.
`remove_listener` (once) confronta con `Rc::ptr_eq`. Firme pubbliche e Rc native esterne invariate.
Prove: events.html +2 REQUIRED_CHECKS (incumbent A/B/C con atteso Chrome155 = B, negativo
ricattura → c/parent; semantica lista + MediaQueryList); gc-lifecycle.html: nodi detached con
listener auto-catturante raccolti (anche signal irraggiungibile non abortito e abortito con
controller vivo) + controllo nodo tenuto (closure/handleEvent/once/signal dopo GC); run.sh e
test fake aggiornati (scenario `gc-listener`). Limiti: handler inline e altri storage di callback
restano Rc; nessuna GC forzabile dentro un dispatch (TestUtils.gc asincrona), snapshot radicato
verificato solo per revisione. Validazione revisione Codex: i 2 nuovi check events.html
risultano entrambi true in Chrome155 via DevTools MCP (finestra visibile, pagina about:blank;
nessuna GC forzata in Chrome: solo riferimento dei valori attesi, nessun esito GC Servo
dichiarato); test_runner.py 42 test, 34 pass, 8 skip POSIX; codegen reale con
PYTHONHASHSEED=0 riuscito, rispetto al generatore HEAD 28 file Bindings e 43 metodi cambiano,
solo `new_with_incumbent`. roves-js invariato: suite nativa non rieseguita. Rust modificato
analizzabile; rustfmt sull'intero file riformatta let-chain upstream preesistenti, quindi
nessuna dichiarazione di formato pulito rispetto alla base. Limite pagina GC: 40 s invariati
(caso peggiore +3×8+2 collezioni, raggiunto solo con nodi trattenuti, che già falliscono;
un timeout fallisce comunque per marker `result` mancante). v8.yml ora parte anche su
`components/script/dom/event/**` e `components/script/dom/abort/**`; nessun flag mach né
test.yml cambiato, mirror roves-action non toccato. Compilazione e smoke browser Servo: solo
CI. Patch0353 rigenerata dal diff esatto e applicata allo scratch;11/11 file LF-identici. Retention frame complessiva APERTA.
Dettagli in `target/task-0353/RESULTS.md`.

### Claude0352 — design storage tracciato dei listener DOM (da revisionare, nessun codice)

Base pubblicata9f62b30b1e8 (0351); CI del push in corso, gestita da Codex. Design in
`docs/handoff/0352-event-listener-design.md`: `EventListenerType::Additive` e
`RemovableDomEventListener` passano da `Rc<EventListener>` (radice permanente
`CallbackObject::root`) a record tracciati `Box<Heap<*mut JSObject>>` (+ incumbent `Dom` per
il target); al dispatch la `Rc` radicata è ricostruita con il nuovo
`CallbackContainer::new_with_incumbent` generato da `CGCallbackFunctionImpl`, che conserva
l'incumbent della registrazione; lo snapshot di `invoke` diventa `RootedTraceableBox` per
coprire `once`/rimozioni durante il dispatch. Rc native esterne e firme IDL invariate.
Nessun blocco trovato; residuo: handler inline e altri storage di callback. Nessuna
retention dichiarata risolta. Prossima azione: Codex revisiona e delega l'implementazione
(callback.rs, codegen.py, eventtarget.rs, event.rs, abortsignal.rs + patch/CUSTOMIZATIONS)
con le prove 1-4 del documento; prova incumbent dopo riferimento Chrome via MCP.

### Claude0351-CI — WindowProxy cross-origin dopo0349 (revisionata e verificata localmente, browser in attesa CI)

Run V8 40234323634: browser Linux normal121292529003/JITless121292529029 rossi su events.html
"opaque-a-0: cross-origin allowlist unusable". Nella run0348 events.html (eseguita prima di
gc-lifecycle, unico rosso) era verde; 0349 cambia solo l'ingresso native: causa = realm switch.
Meccanismo: il documento opaco sandbox è un DissimilarOriginWindow nel thread del genitore, con
origine clonata dal genitore; il trap `get` segue il ramo same-origin e inoltra a
`window`/`parent`/`top`, accessor nativi del realm segnaposto. Dopo0349 girano lì (principal
corretti) e `JS_WrapValue` restituisce il wrapper di quel realm, non `held`/`window` del genitore.
Fix in `call_native`: se il risultato è una WindowProxy e il realm del chiamante differisce
dal callee, si riconverte con `wrap_window_for_realm` + RealmSwitch esistenti. Chiamante =
contesto incumbent V8 (`roves_v8::incumbent`, frame JS più interno: copre trap, job Promise con
realm JSAPI stale e funzioni di un terzo realm annidate); solo col sentinel hidden-caller si
usa `get_entered_or_microtask_context`. Callee e controlli d'origine invariati; nessuna fixture
indebolita, nessuna dipendenza. Ambito residuo: native chiamato da codice embedder dentro un
altro native converte per il chiamante JS esterno (Servo riconverte a ogni ritorno).
Regressione `natives_of_a_remote_window_realm_return_the_callers_window_wrapper` (trap inoltrato,
chiamata diretta, entrambi in job Promise senza realm JSAPI, caller annidato di terzo realm;
getter sempre nel realm remoto). Revisione Codex: primo compile con4 E0425 (import test),
corretti; poi asyncTrap falliva (realm_tests313): nel job Promise il trap girava nel realm
JSAPI stale e l'helper `Reflect.get` (builtin TurboFan con linkage JS, frame JS per l'incumbent)
diventava il chiamante. Fix: `trap_context` entra nel realm `caller_realm` per tutto il trap
(RealmSwitch), come SpiderMonkey; niente contesto dell'handler, controlli d'origine coi principal
del codice che accede. Toglie anche il lookup stale nei trap in job Promise (osservazione sopra). Verifiche Codex finali:137 test normali verdi/2 ignorati,136 JITless verdi/3 ignorati.
Negativo con coppia native.rs+proxy.rs di HEAD0349 compila e fallisce sul getter inoltrato
in entrambe le modalità (non un errore compilazione). La prima fixture con solo native.rs
non compilava perché proxy.rs nuovo dipende da caller_realm: sostituita con coppia coerente.
Patch0351 rigenerata dal diff reale, dry-run/apply riusciti; i3 sorgenti sono LF-identici
allo scratch. Anche patch processo0350 applicata come nuovo AGENTS.md, assente nell'upstream.
Nessun buildServo locale. Batch e351 concluso: generale13/13, mobile verdi; V8 8/10 con
soli2 errori WindowProxy sopra. Prossimo push include0350, processo e fix0351; conferma browser
solo nella nuova CI. Scheduler precedente disattivato: quota verificata/ripristinata e incarico
ripreso manualmente. Non dichiarare retention frame complessiva risolta.
Patch0351, CUSTOMIZATIONS/README/wiki aggiornati; dettagli `target/task-0351-ci/RESULTS.md`.

### Pubblicato0349 — e351c546261;0350 completa in locale durante CI

Fix0349 pubblicato, wiki deea588. Batchattivo: V8402343236346/10 verdi4attivi,
generale402343236966/13 verdi7attivi; Android40234323757 e iOS40234323636 verdi.
Nessunrosso alcontrollo dopo0350. NON pushare durante queste run. Prossimopush deve
includereeventuali nuove correzioni che emergono; non considerare checkpointancoraverificato.

0350 aggiunge regressione distinta con due realm/principalnonnull diversi: chiamataNative
corretta ed errore devonoeseguire nelcallee, ripristinare esattamenterealm/principaldelchiamante,
propagare eccezione7 e bilanciaresavedstack. Positivi mirati1normal/1JITless; negativoNative
pre0349 fallisce inentrambimodi. Nessuncambio produzione o fullsuiteimmutata ripetuta.
Patch0350 dryrun/apply,480/483 identiciLF con3eccezioninote; scratchora0350.
Logtarget/task-0350/. Commitlocale completo consentito, pubblicazionevietatafinchébatchattivo.

Prossimo incarico dopo fix0351: [ownership listenerDOM](handoff/0351-traced-event-listener-ownership.md),
piano soltanto. Punti giàstudiati: factoryCallbackContainer nelcodegen devepreservare incumbent
originale, storageAbortSignal possiede un’ulterioreRc; proteggeremetadata durante ricostruzione/
dispatch. Niente conteggiRcmanuali, collettoriRustpropri o rimozioneindiscriminataroot.
Scheduler22:42 riprendeGit/handoffcorrenti, nonripetere0347–0350 né analisi senza nuova domanda.

### Codex0349 — CI0348 corretta localmente, pronta alla pubblicazione

Batch9c6e1334a07 concluso: generale4022899502013/13, mobile40228995069/40228995041
verdi. V8402289950928/10: soli browserLinux normal121276470664 e JITless121276470684 rossi.
Non timeoutfont/errorecompilazione: helper0348 supera identità+valore, poi abortisce per
principalnull inprincipals.rs110 da is_platform_object_same_origin/proxyhandler.rs1019.
Cause isolata: V8 esegueNative callback nelcontesto pagina maJSContext.current_realm resta
inizialeruntime dopoPromisejob non circondato daAutoRealmServo. Nessun rollback0348 o
indebolimento checkorigini. Nativeentry ora riusa realm_of_context(scope.get_current_context())
e RealmSwitch conripristinoRAII, radicando realmattuale/salvati comegiàprevisto.

Regressione nuova: Promise.race/await nelrealmpaginaconprincipal, checkpointfuori dalrealm,
callbackcontrolla identitàesatta delrealmcalleeeprincipalnonnull; chiamatadiretta crossrealm,
callbackcheritornafalseconeccezione, ripristinorealm e savedstackvuoto. NessunpanicoltreFFI.
VecchioNativeCode fallisce normal/JITless; finali135pass2ign normal/134pass3ign JITless.
Logtarget/task-0349/negative-*.log e runtime-*.log; nessunbuildServo locale.
Patch0349 dryrun/apply scratch:480/483 identiciLF,3eccezioninote. Scratchora0349.
Controllo browservero riprenderàconlaCI delprossimopush; tuttiibatchprecedentiterminali.

Artefatto0348 pre0349:12612439502,3818217byte,
SHA2566899885815ed9b3a3b0411c35de38f4459e5f8b008dc63066241bb6ab7c5d639.
MemLab2.0.5 censusdirettoRootSet:44archi, solo1Promise duplicatasultracerpromise,
nessunadelleprecedentiradiciFontFaceSet/performance. ConfermacircoscrittarootFontReady0348,
nonrisoluzioneframe (probeancoravivo, callbackrootsrestano). Non dedurreRAMdai bytesnapshot.
Prossimo0350: ownershipCallbackObject nei listenerDOM mantenendoRcesterni e incumbentsettings;
prima valutare contenitoretracciatoesistente/codegen, nessuncollettoreRustmanuale o taglio globale
root. Scheduler22:42 riprendedal correntehandoff, nonreimplementare0347/0348/0349.

### Codex0348 completa localmente — FontFaceSet.ready tracciata dal DOM

Sviluppo diretto ancora autorizzato dall’utente; nessun tentativoClaude aggiuntivo. FontFaceSet
ora conserva Heap<JSObject> invece diRc<Promise>: riuso del pattern PromiseRejectionEvent e di
Promise::new_with_js_promise per wrapper nativi temporanei radicati. NativeRc esterne restano
protette; nessuna modifica globale aPromise o teardownDocument. IdentitàJS conservata, constructor
mantienePromise radicata fino all’installazioneHeap, getter rilegge dopo flushstili e nuovo loading
sostituisceHeap con nuovaPromise. Query stato senza allocazioni tramiteGetPromiseState. Getters
possonoallocare wrapperRust temporanei, nessun beneficioCPU/RAM dichiarato senza misure.

Nuovo helpergc-lifecycle4contratti: identità, valoreFontFaceSet, sopravvivenzaedgeDOM e risultato
attraversoTestUtils.gc. ChromeMCP155 Windowsvisibile HTTP19048: helperreale4/4 con SOLIconfini
task, SENZA GCforzata; altri checknon eseguiti. node--check e parsingRust passano; runner42totali
(34pass,8POSIXskip), REQUIRED_CHECKSinvariati. No buildServo locale o duplicatiJITless nativi
immutati. TipiServo reali, GC e rimozione delle specifiche radici allaCI. Patch0348 dryrun/apply:
480/483 identiciLF,3eccezioninote; scratchora0348. Logtarget/task-0348/.

Batch4366f3a09c8 TUTTO VERDE: V84022372888110/10, generale4022372885413/13, mobileverdi.
Artefatto0347 corrente12610891565,4873823byte,
SHA256fbb20b76fc1460847a10eec5f59888f47a94ad419319f984d35d790185c0a062:
MemLab2.0.5 censusdirettoRootSet47archi, nessun valorecella di tipo oggettoPerformance
(prima c’era). Conferma circoscritta0347 su questoscenario, non risoluzione retentionframe.
Promise/callback radici restano nell’artefattopre0348, come atteso. Non rianalizzarlo per0348!
Pubblicare0348+rifinitura553494daf82 dopoquestobatchverde; prossimo0349: cicloCallbackObject
in listenerDOM. Richiede pianoownership che preservi Rc native/incumbent settings; NON cancellare
listener solo perchéChrome non invoca callback di frameusciti. Scheduler22:42 leggehandofcorrente,
NON ripetere0347/0348. Retentioncomplessiva ancoraaperta.

### Pubblicato0347 — 4366f3a09c8; rifinitura locale durante CI

Commit4366f3a09c8 pubblicato, wiki be79aaf. Nuovo batchV840223728881,
generale40223728854, Android40223728914, iOS40223728915: controllare esiti prima di
qualsiasi ulteriorepush. Aggiunto localmente ignoretarget/ al crate di test e alla patch0347,
nessun nuovo cambio logica o test da duplicare; replay479/482 con3eccezioni note.

Analisi successiva0348: NON svuotare indiscriminatamente listener/promise solo per far
sparire lo snapshot. HTML destroy-a-document rimuove task ma consente accesso JS successivo
al Document; non prescrive qui cancellazione listener. ConfrontoChrome precedente (callback
non invocati) non prova da solo che i listener siano rimossi, perché può dipendere dal realm.
Sorgenti hanno già remove_all_listeners per document.open, non per destroy. FontFaceSet ha
Rc<Promise> autoradicata; candidato più preciso per prossimo incarico: conservarne il JS
reflector tramite edge Heap tracciato dal DOM e creare/tenere Rc radicate solo per usi nativi,
riusando Promise::new_with_js_promise. Verificare tutti i chiamanti e stabilità di fonts.ready
prima di implementare. Nessun cambiamento0348 avviato o lasciato a metà. RiferimentoHTML:
https://html.spec.whatwg.org/multipage/document-lifecycle.html#destroy-a-document

### Codex0347 completa — sviluppo diretto autorizzato dall’utente durante quotaClaude

L’utente ha chiesto esplicitamente di implementare direttamente nel frattempo: eccezione
corrente alla delega, non cambio permanente del processo. Nessun nuovo tentativoClaude.
TaskQueue scarta task inactive/throttled/ready di pipeline uscite e arrivi tardivi prima del
bypassRendering; controlmessages, workerNone e altrepipeline conservati. Distruttori fuori
dai borrow, sullo scriptthread. Unico setchiusure nella coda: sostituito closed_pipelines
inScriptThread, anche per guardiaheaders tardivi, senza raddoppiare memoria dei tombstone.
4/4 contratti Windows passano compilando sorgente reale con tipi contorno leggeri;
negativo vecchiocodice+entrypointnoop:3falliscono, controlloaltrapipeline/worker passa.
Logtarget/task-0347/queue-tests.log e negative-old-queue.log. Nessuna buildServo locale.
Patch0347 dryrun/apply scratch:479/482 identiciLF, sole3eccezioni note. Scratch ora0347.
WorkflowV8 esteso ai duefilelifecycle e harness; test su3OS normal (logicaindipendenteJIT),
servoshell normal/JITless e snapshot allaCI. test.yml/buildflags invariati, actionnonrichiedesync.
Ultimobatchpubblicatoc650f8a19ad: mobile40213022469/40213022395 verdi, sorgenti identici
ad aa028 già tuttoverde. Pubblicare0347 completa; validazione browser ancora pendente.
Non dichiarare rootPerformance eliminata finché nuovo snapshot lo conferma; restanoFontFaceSet/
callback. Prossimo0348: cleanupdiscard listener/promise, verificando contrattoHTML e riferimenti
nativi vivi prima di cambiare radici. ConfrontoChrome salvato task0346, non ripeterlosenzamotivo.
Scheduler22:42 aggiornato: leggerequestohandoff e NON reimplementare0347 dal vecchioprompt.

### Stop quota0347 — 25 ottobre17:40 Europe/Rome; ripresa unica22:42

Claude: `You've hit your session limit · resets 8:50pm (Europe/Rome)`.
Nessuna modifica sorgente0347 lasciata; repository pulito prima di questo handoff. Non
sostituire Claude con Codex né tentare prima della ripresa singola autorizzata5ore dopo.
Scheduler app confermato: ripresa-unica-claude-dopo-quota-0347, COUNT1 alle22:42 locale,
destination thread. Se stesso limite persiste registrare e fermarsi senza concatenare retry;
disattivare dopo il tentativo. Nessuna automazione periodica. Reset indicato20:50 distinto
dal tentativo22:42 richiesto dalla regola5ore dal blocco.

Batchaa028313991 completato TUTTO VERDE: V84020862053610/10, generale4020862053813/13,
Android40208620533, iOS40208620526. Diagnosi0342–0346 committate; ultimo commit prima di
questo handoff7940989387f. Prossimo incarico completo e durevole:
[0347: scarto task pipeline chiuse](handoff/0347-discarded-pipeline-tasks.md).
Sorgenti invariati dalle0341; patch0342–0346 inesistenti perché sola diagnosi.
Non rieseguire snapshot/query già acquisite senza nuova domanda. FixPerformanceTask è
separato dal successivo cleanup FontFaceSet/callback: retentionframe ancora irrisolta.

### Codex0346 revisionata — coda task di pipeline chiusa

Performance root è coerente con Trusted<Performance> di taskPerformanceTimeline parcheggiato
per pipelineabout:blank uscita. Non è AddRawValueRoot: ordine RootSet lo pone nei tracerServo.
Proprietario dedotto da sorgenti/archi, entrytype e inactive-vs-throttled non misurati. Coda
inactive rilascia solo se pipeline torna fullyactive, impossibile dopoExitPipeline; cancellare
flag non distrugge task/Trusted. Prossimo0347 cleanup code all’exit e arrivi tardivi, test altre
pipeline/worker/cancellazione/Drop su scriptthread, nessuna rimozione globale di root vive.
Retentionframe non si chiude da questo solo fix: promiseFontFaceSet e callback restano.
Batchaa028 tuttoverde; prossimo push includerà fixcoerente e diagnosi locali revisionate.

### Claude0346 — proprietario della radice Performance: task parcheggiato (da revisionare, nessun codice)

Base46f778f09a7; solo sorgenti più output0345 esistenti; nessuna build, misura browser, patch o wiki.
RootSet[46] non è AddRawValueRoot: l'ordine degli archi lo colloca dopo gli atomi, nel tracer
extra di Servo `trace_rust_roots` (42/43 documents/window_proxies, 48 = promise_table), quindi
DomRoot o `Trusted`. Unici proprietari possibili: `Trusted::new(self)` in `performance.rs`
(`queue_entry`:315, :234, :420) nel task `notify_performance_observers`; nessun DomRoot conservato.
Percorso: l'unload del documento about:blank sostituito (`update_visibility_state(Hidden)`,
document.rs:2170, chiamato da script_thread.rs:2885) accoda la voce; la sorgente performance-timeline
è throttled e, per pipeline non fully-active, il task finisce in `TaskQueue.inactive`
(task_queue.rs:91-106). Quella mappa si svuota solo quando la pipeline torna fully-active;
`handle_exit_pipeline_msg` toglie il documento e cancella i task (run no-op) ma non li rimuove, quindi
il `Trusted` non viene mai rilasciato. task_queue.rs è invariato dall'import pristino (bug upstream).
Non è un proprietario nativo legittimo: il task non può più eseguire. Piano unico (prossimo incarico):
`TaskQueue::discard_tasks_for_pipeline` (rimuove inactive+throttled della pipeline e scarta gli arrivi
successivi) chiamato in `handle_exit_pipeline_msg` dopo `clear_js_runtime`; test unitario TaskQueue con
Drop osservabile più negativo su altra pipeline; verifica CI con snapshot frame-retention e query0345.
Non chiude da solo la retention: restano RootSet[5][6][7]. Limiti: inferenza da ordine archi + ricerca
sorgente, tipo di voce non provato. Log `target/task-0346/RESULTS.txt`.

### Codex0345 revisionata — radici reali identificate; confronto discardChrome

Artefatto corrente mostra Persistent→RootSet→celle26/28/30/96→oggetti dei realm figli:
FontFaceSet promise, listenerclick injected, Performance (proprietario da identificare).
Questo corregge la precedente lettura0342: il RootSet raggiunge il cluster. Nessuna radice
stack verso cluster nello snapshot corrente. Roots Promise/Callback già presenti inServo0.5
pristino (verifica diretta archivio), non introdotte dalla migrazione. Non indebolirle globalmente.
Chrome DevTools MCP155 Windows visibile HTTP19046: dopoiframe.remove+task, listenerWindow/
Document/element non chiamati (conteggi invariati1/1/1), FontFaceSet identica, ready prima stabile
poi undefined, vecchiaPromise conserva risultato. È riferimento Chrome, non prescrizione di
replicare ogni dettaglio. Tab/server chiusi; logtarget/task-0346/CODEX_CHROME_REFERENCE.txt.
Batchaa028313991 ora TUTTO VERDE V8/mobile/generale. Prima del fix identificare proprietario
rootPerformance e verificare cleanupdiscard esistente senza rompere riferimenti nativi vivi.

### Claude0345 — radici dirette dello snapshot browser corrente: trovate (da revisionare, nessun codice)

Base8d20e438a0c; CI non controllata; nessuna modifica a sorgenti/patch/wiki. Artefatto12604704563
(run40208620536, aa028313991), hash verificato; MemLab2.0.5 (WSL Node24), helper0323/0344 più
query a un livello in `target/task-0345/` (solo parser/accessor MemLab, nessuna BFS nostra).
Il cluster del frame SOPRAVVIVE: probe 38851 sul global 54857 del realm srcdoc354/contesto14127;
sopravvive anche il realm about:blank iniziale796/16839 (stesso proxy54855, la sua Window794
tiene la cella WindowProxy100 → Window corrente). Nessun arco `C++ native stack roots` verso il
cluster. Archi diretti reali: `C++ Persistent roots` → RovesJsRootSet → celle che contengono
oggetti dei realm del frame: [7] Promise `document.fonts.ready` dello srcdoc (risolta con la
FontFaceSet stessa), [5] Promise della FontFaceSet di about:blank, [6] listener `click` dello
user script save/export di servoshell (`ports/servoshell/desktop/app.rs:116`) sul Document del
frame, [46] Performance di about:blank (proprietario non identificato). Correzione della lettura
0342: RootSet raggiunge il frame tramite queste celle. Causa: radici permanenti
`Promise::root`/`CallbackObject::root` (AddRawValueRoot, rilasciate solo nel Drop dell'Rc)
possedute da oggetti DOM del documento scartato → ciclo mai raccoglibile; `clear_js_runtime`
applica già questo schema solo a custom elements (servo#15318) e pending_image_callbacks.
Nessun fix: completo solo rilasciando tutte le radici del documento, e la radice Performance non
è mappata. Prossima azione unica: in `clear_js_runtime` rilasciare listener (verificando che
`remove_all_listeners` faccia davvero cadere i CallbackObject) e Promise della FontFaceSet,
dopo aver identificato chi radica Performance; verificare con lo snapshot CI frame-retention e
le stesse query. Limiti: un solo snapshot; attribuzione about:blank/srcdoc dedotta. Log
`target/task-0345/RESULTS.txt`. Frame retention aperta, causa candidata concreta.

### Codex0344 revisionata — snapshot cambia il repro, nuovo artefatto browser acquisito

Stessobinario0343: senza hook secondaWindow viva, con hook raccolta dalla GC dello snapshot
(normal6/6 vs3/3, JITless2/2 vs1/1). Grafo rappresenta stackcppgc verso realm/celle realmente
radicati; non calibra la radice stantia del repro che non sopravvive. Nessuna prova sulla causa
DOM, nessun fix o testflaky pubblicato. Evitare ulteriore rilettura0326 senza nuovo motivo:
acquisito artefatto corrente12604704563/run40208620536 suaa028313991,4876611byte,
SHA256ce5d4263c2b3aad9f960c18a82466b7caa055bb134829aa31c1a2e0e0371fe26,
filetarget/task-0345/frame-12604704563.heapsnapshot. Prossimo confronto diretto radici/cluster
con gli stessi helper MemLab calibrati, senza nostroparser/BFS né dichiarazioni di assenza leak.

### Claude0344 — calibrazione snapshot del repro0343 (da revisionare, nessun codice)

Base271713ce906; CI non controllata; nessuna modifica a sorgenti/patch/wiki. Stesso test0343
invariato, solo `task_0343_production_1`, stesso binario con e senza hook snapshot (GC1),
un processo per esecuzione. Senza env: seconda Window viva (normale6/6, JITless2/2). Con
snapshot: entrambe raccolte (normale3/3, JITless1/1); il grafo (letto con MemLab, helper
esistenti + `census.js` a un livello) contiene solo il contesto di default del runtime e
`other_realm`, nessuna Window del cluster. Calibrazione: lo snapshot emette archi
`C++ native stack roots` verso oggetti cppgc sullo stack al momento del proprio scan
(realm di default, cella di `other_realm`), ma la propria GC raccoglie la Window trattenuta,
quindi non mostra né esclude la parola stantia della GC precedente. Per0326: lo snapshot
descrive lo stack della propria GC, non delle GC di produzione precedenti; prova singola,
non conclusiva. Prossimo passo: stesse query sullo snapshot browser0326 verso il cluster del
frame trattenuto. Log `target/task-0344/RESULTS.txt`. Frame retention aperta.

### Codex0343 revisionata — retention nativa sensibile ai frame

Solo diagnosi parcheggiata target/task-0343, nessun test flaky pubblicato. ProduzioneJS_GC
1/8 volte lascia secondaWindow viva; low_memory_notification diretto e GCtest con entrambi
stackstate raccolgono; wrapper vivo controprova mantiene secondaWindow in ogni variante.
Normal/JITless10/10, processi separati3/3. Differenza disposizione frame, non dimostrazione
della causa DOM reale. Prossimo snapshotdebug del repro calibrerà visibilità delle radici
conservative nel grafo; nessun nuovo profiler/parser/BFS o modificaGC produzione.
Batchaa028313991 V8/generale attive, mobile verdi: commitlocali, niente push.

### Claude0343 — forma0326 con sola GC di produzione: ritenzione riprodotta (da revisionare, nessun codice)

Base aa028313991; CI non controllata; nessuna modifica a sorgenti/patch/wiki. Test scratch
(`target/task-0343/production-gc-test.rs` via `native.sh`, inserito solo in copie in %TEMP%):
cluster0326 creato in un helper `#[inline(never)]` che restituisce solo `WeakPersistent` dei
realm, poi solo `JS_GC(DOM_TESTUTILS)`. Esito deterministico (debug, normale e JITless, 10/10 e
3/3 un test per processo): con1 o8 chiamate il realm della seconda Window sopravvive senza
wrapper tenuto, la prima viene raccolta; controprova con wrapper tenuto: vivo in ogni variante.
Unica variabile isolata in runtime separati: `low_memory_notification` chiamata direttamente
(la stessa chiamata di `JS_GC`, senza il suo frame) raccoglie; la GC di testing raccoglie sia
con NoHeapPointers sia con MayContainHeapPointers. Inferenza, non prova: unico input dipendente
dal layout dei frame è la scansione conservativa dello stack (parola stantia dell'helper);
parola/frame non identificati. Non dimostra la stessa causa nella scena DOM reale, che resta
aperta, né conferma o smentisce la lettura0342 dello snapshot. Nessun test pubblicabile (esito
legato al layout della build). Prossimo passo proposto: snapshot debug esistente alla prima
`JS_GC` di questo repro per vedere se compare un arco stack/radice. Log `target/task-0343/RESULTS.txt`.

### Revisione Codex0342 — prossimo test sul percorso GC di produzione

Diagnosi sorgenti conferma il percorso detach→RemoveIFrame/reply→ExitPipeline→rimozione
Document/clear_js_runtime. È atteso prima/tra gli8taskGC, ma la sola lettura non prova il
timing effettivo nella scena. Snapshot senza edge RootSet/stack verso frame restringe le
ipotesi, NON esclude radici non rappresentate o errori nella lettura del grafo. La condivisione
dello stack marker V8 è evidenza utile, non prova assoluta dell’assenza di radici conservative.
Nessuna radice trattenente dimostrata, nessun fix. Prossima prova circoscritta: shape nativa
0326 con sola JS_GC(DOM_TESTUTILS), senza preventiva gc_for_testing; controprova wrapper vivo.
V8/generalebatchaa028313991 ancora attive, mobile verdi: non pushare. Log target/task-0342/.

### Claude0342 — ciclo di vita del frame rimosso: nessuna radice provata (da revisionare, nessun codice)

Base aa028313991; CI non controllata; nessun codice/patch/browser/build. Pipeline di rimozione
invariata rispetto a upstream (nessuna patch tocca le righe): `unbind_from_tree` →
`destroy_child_navigable` toglie il WindowProxy da ScriptWindowProxies, invia `RemoveIFrame`
e attende la risposta; il constellation chiude tutte le pipeline del contesto (about:blank e
srcdoc) e accoda `ExitPipeline` prima di rispondere, quindi lo ScriptThread toglie il Document
da `documents` e chiama `clear_js_runtime` prima o tra i task delle8GC. Tutte le radici Servo
passano solo da RootSet, che nello snapshot0326 (all'ottava GC) non raggiunge il frame: pipeline
già uscita, nessuna radice embedder. Global roves-js fuori da RootSet: nessuno per realm figlio
(contesti figli solo [weak]). Ipotesi0326 dello stack conservativo eliminata: V8152.2 usa un unico
marker per GC e grafo dello snapshot (`heap-profiler.cc:129-132`), quindi una parola di stack
sarebbe apparsa come arco. Resta non spiegato: il cluster sopravvive a una GC senza arco forte
visibile. Prossimo passo unico: test nativo della forma0326 raccolta solo con
`JS_GC(DOM_TESTUTILS)` di produzione (oggi i test usano prima `gc_for_testing`), con un negativo.
Log `target/task-0342/RESULTS.txt`. Frame retention aperta.

### Checkpoint Codex0341 — batch precedente tutto verde, pubblicazione

Batch2fc8a3f73f6 concluso: V84020463406610/10, generale4020463410213/13,
Android40204634068 e iOS40204634081 verdi. Pubblicare0337/0338/0340 e diagnosi0339/
testcontratto0341 completi e revisionati. Nessuna modifica WIP.0341 conferma contratto
SpiderMonkey IsArrayObject (non ECMAIsArray), nessun cambio produzione:134normal/133JITless,
replay470/473 con3eccezioni. Niente verifiche browser duplicate per un test di contratto.
Riprendere retention frame con un incarico diagnostico mirato, evitando altre ricerche
sovrapposte o modifiche speculative. La migrazione di produzione non è dichiarata conclusa.

### Claude0341 — audit IsArrayObject: ipotesi falsa, solo test di contratto (da revisionare, non committato)

Base 8eda6652133; CI non controllata. SpiderMonkey fissato (`mozjs_sys`140.14.0-0) implementa
`JS::IsArrayObject` con `ESClass::Array`, non con `JS::IsArray`: Proxy scriptati, anche revocati,
danno `false` senza eccezione (lo dice anche `js/public/Array.h`). `value.is_array()` di V8 coincide;
nessun wrapper cross-compartment in Roves. Chiamanti: `is_array_like` fa `assert!` sul successo
(un throw sui revocati diventerebbe panic), IndexedDB chiede un "Array exotic object". Nessuna
correzione di codice; nuovo test nativo (array, sottoclasse, proxy radice/annidato/revocato,
array-like, funzione, primitivi, `Array.isArray` avvelenato, outparam/eccezioni). Nessun negativo
sul vecchio codice: il codice è invariato e il test passa. roves-js normal134/2ign, JITless133/3ign;
patch `0341-is-array-object-builtin-class-contract-test.patch`, replay470/473 con le3eccezioni
note. README/wiki invariati (nessun cambiamento visibile). Nessuna prova browser. Log `target/task-0341/`.
Frame retention aperta.

### Codex0340 revisionata — atomi deboli, pin espliciti conservati

Revisione diff/log conclusa: riuso Interned e hash/confronto mantenuti V8, niente nuova tabella
UTF16 permanente. RootSet traccia solo pin espliciti; IdVector riceve subito ogni id e guardie
RESOLVING tracciate.133normal/132JITless e negativi vecchi normal/JITless confermati;
replay470/473 con3eccezioni. Rischio rooting verificato nei chiamanti; manca test GC dedicato
nel read-back RESOLVING (documentato, nessuna GC precisa aggiunta in callback di produzione).
Batch2fc8a3f73f6: V8/mobile verdi, generale4020463410212/13 verdi; solo Linuxportable
121203442294 attivo. NON pushare. Prossimo: verificare contratto IsArrayObject con proxy
array/revocati e chiamanti DOM/IndexedDB, prima di proporre correzioni. Frame retention aperta.

### Claude0340 — atomi non pinned raccolti senza riferimenti (da revisionare, non committato)

Base 42e4825f2b7; CI non controllata da Claude. Difetto dimostrato con test nativi nuovi sul
codice vecchio normal/JITless: 64 atomi enumerati restano dopo il rilascio degli id (0 invece
di64 finalizzati), atomo rilasciato da un owner `Heap<jsid>` mai raccolto, 8×2048 chiavi
transitorie mai recuperate. Fix: atomi nella tabella debole esistente `cell::Interned`
(hash stringa V8, confronto per contenuto, sweep ammortizzato), nessuna tabella/dipendenza
nuova; `JS_AtomizeAndPinString` resta permanente (insieme pinned tracciato), `JS_AtomizeStringN`
e `key_id` no, come SpiderMonkey. Audit chiamanti: Servo radica subito; corretti
`GetPropertyKeys` (Vec non radicato, anche simboli) e guardia `RESOLVING` ora tracciata.
roves-js normal133/2ign, JITless132/3ign, warning749 invariati; patch
`0340-unpinned-atoms-collected-when-unreferenced.patch`, replay470/473 con3eccezioni note.
Nessun test RESOLVING dedicato (GC durante il read-back non riproducibile con GC preciso),
nessun browser/build Servo. Nessun miglioramento memoria complessivo dichiarato. Log
`target/task-0340/`.

### Codex0339 — diagnosi revisionata, nessuna modifica di codice

Base 5ec9debf9e2; CI non controllata. Contratto SpiderMonkey (mozjs_sys140.14 `jsfriendapi.h`,
`JSObject.cpp`): dst senza proprietà, stessa classe, copia shape/slot, false solo per OOM.
Chiamanti reali (`wrap.rs`, `codegen.py`): reflector o expando appena creati, holder riempito
solo con membri LegacyUnforgeable. Indici omessi: irraggiungibili, identificatori WebIDL mai
numerici (`t_IDENTIFIER`). `Some(false)` ignorato: irraggiungibile, definire su oggetto vuoto
estensibile non può essere rifiutato; template senza definer/setter; eccezioni già propagate.
ConvertToString/rifiuto cambierebbero solo casi fuori contratto: nessun cambio speculativo,
nessun test/patch/browser. Log `target/task-0339/RESULTS.txt`.

### Codex0338 revisionata — altro commit locale durante CI

FinishDynamicModuleImport usa ora Promise::then2 pubblico V8 invece di compilare ogni volta
un helper che legge evaluation.then mutabile. Negativi normal/JITless dimostrano eccezione
oppure consegna anticipata con then ostile.128normal/127JITless, TLA/rejection/namespace
thenable eGC passano; replay470/473 con3eccezioni note. Nessuna nuova dipendenza.
Il percorso pagine Servo usa già AddPromiseReactions/then2: questa è una correzione della
API embedder roves-js, non un difetto delle pagine dimostrato. Nessun test browser duplicato.
Batch2fc8a3f73f6: V840204634066 conclusa verde10/10, mobile verdi; generale40204634102
ancora attiva. Commit locali consentiti, NON pushare. Proseguire prossimo incarico.

### Claude0338 — completamento import() senza `then` utente (da revisionare, non committato)

Base e73b987f350; CI non controllata da Claude. Contratto host: Servo (`module_loading.rs`
`continue_dynamic_import`) NON chiama `FinishDynamicModuleImport`, usa `AddPromiseReactions`
(già `then2`): pagine non affette; il difetto riguarda l'API esportata e i test nativi. Difetto
dimostrato sul codice vecchio normal/JITless: `then` che lancia → Finish `false`, import mai
risolto; `then` ostile che chiama subito il callback → import risolto con `'hostile'` prima della
fine del TLA (anche `resolve(chained)` assimilava via `then` utente). Verificato in V8152.2
`api.cc`: `Promise::Then` chiama `perform_promise_then`, niente lookup `then`/species. Fix: due
reazioni native `then2` con dati in un array interno [promise, namespace]; namespace risolto una
volta (export `then` ancora assimilato), rifiuto propagato; nessuna compilazione per chiamata.
Cast promise→resolver invariato (nessuna conversione controllata pubblica). Test nuovo: due patch
ostili × TLA pendente/errore/namespace thenable, GC prima del settle. roves-js normal128/2ign,
JITless127/3ign, warning749 invariati; patch `0338-dynamic-import-completion-without-user-then.patch`,
replay470/473 con3eccezioni note. Nessun check browser (percorso pagina non usa la funzione),
REQUIRED_CHECKS invariati. Log `target/task-0338/`. Retention frame/no-module resta aperta.

### Codex0337 revisionata — commit locale durante CI0335/0336

0337 elimina la radice forte dei JSON collegati mai valutati: namespace V8 come chiave
nell’ephemeron WeakMap già esistente, holder cppgc con edge al record. Nessuna ABI privata,
nessun export anticipato; contratto namespace verificato nei sorgenti del crate fissato.
Negativo vecchio codice normal/JITless trattiene5/5 contro1/1; positivi127normal/126JITless,
quattro grafi abbandonati finalizzati una sola volta e quello vivo valuta dopoGC correttamente.
Replay470/473,3eccezioni note, nessun drift nuovo. Revisione diff/log completata senza duplicaretest.
Batch2fc8a3f73f6: Android40204634068 e iOS40204634081 verdi; V8402046340667/10 verdi,
3browser attivi; generale402046341028/13 verdi,5build attive. Nessun rosso. NON pushare.
Continuare prossimo incarico circoscritto durante CI; retention frame/no-module ancora aperta.

### Claude0337 — record JSON collegati tenuti dal namespace (da revisionare, non committato)

Base 2fc8a3f73f6; CI non controllata da Claude. Difetto dimostrato con test nativo nuovo sul
codice vecchio (normal e JITless): quattro grafi collegati mai valutati (importer sorgente,
JSON radice, importer in errore, link parzialmente fallito) restano vivi dopo la GC, `(5,5)`
invece di `(1,1)`, con i private trattenuti fino allo spegnimento. Verificato nei sorgenti V8
152.2 del crate: il namespace è cachato nella cella `module_namespace` del modulo, punta al
modulo e viene azzerato solo durante PreLinking/Linking. Fix: a fine `ModuleLink` il namespace
di ogni JSON rimasto collegato diventa chiave nella WeakMap catturata dei metadata, con holder
`PrivateData` il cui campo `module` traccia il record; gli step leggono il valore e poi
sostituiscono il valore con `undefined`. Rimosso `strengthen`; nessun export anticipato, TDZ
invariata; fallimento solo per terminazione (TypeError "missing value", nessun UAF). Test
evaluation-failure aggiornato (importer in errore radicato). roves-js normal127/2ign,
JITless126/3ign; patch `0337-json-records-kept-by-their-namespace.patch`, replay470/473 con
3eccezioni note. Log `target/task-0337/`. Retention frame/no-module resta aperta e separata.

### Checkpoint Codex0336 revisionato — pubblicazione0335/0336

Batch bce416e50da concluso tutto verde: V840184060372 (10/10), generale40184060391
(13/13), Android40184060445, iOS40184060368. Via libera al push dei commit completi.
0336 revisionata: Rust e fixture invariati dopo le prove, nessuna verifica duplicata.
Chrome DevTools MCP155 Windows visibile, HTTP19036: il check reale fallisce sugli indici
HTMLCollection; isolamento successivo conferma indice/nome/dataset falliscono, expando
dati/accessor passano. È una divergenza DOM Chrome, non un motivo per cambiare le normali
regole ECMAScript dei Proxy utente; nessuna affermazione sul codice Blink non ispezionato.
Native126normal/125JITless e replay470/473 confermati dai log; browser Roves da verificare
nella prossima CI. Tab/server chiusi. Prossimo incarico: lifetime dei moduli JSON collegati
ma mai valutati, prima prove e API pubbliche mantenute, nessuna promessa sulla retention frame.

### Claude0336 — risultato trap getOwnPropertyDescriptor a prototipo null (da revisionare, non committato)

Base f7ba878ae52 (0335 inclusa); CI non controllata da Claude. Difetto dimostrato sul percorso
nativo proxy Servo-style (trap V8 → trap nativo → expando, test 0335 riusati): `descriptor_object`
usava `v8::Object::new`; con accessor `get`/`set` su `Object.prototype`, leggere il descrittore
dati (anche il nome dal trap) eseguiva i getter (`get get,get set`) e dava TypeError "Invalid
property descriptor". Fix: helper unico `jsapi_impl::descriptor_object` (null-proto,
`with_prototype_and_properties`, solo campi presenti, `SmallVec` inline ≤6) usato dal trap e dal
ramo parziale di `define_property` (rimosso il builder duplicato). Risultati SpiderMonkey sempre
completi: nessun cambiamento dopo il completamento V8. Proxy utente/descrittori utente invariati
(test). Non toccato: ramo `forward` dei wrapper (Reflect sull'oggetto avvolto; WindowProxy ha il
proprio trap). roves-js normal126/2ign, JITless125/3ign; negativi codice vecchio normal/JITless.
Check non obbligatorio `suite.html` (HTMLCollection+dataset), node --check; REQUIRED_CHECKS
invariati, runner Windows42ok/8skip, niente WSL. Patch `0336-null-prototype-proxy-descriptor-results.patch`:
dry-run+apply scratch identici, replay470/473 con3eccezioni note. Log `target/task-0336/`.
Revisione: Chrome155 visibile (evidenza esterna Codex) fallisce il check con lo stesso TypeError
e `calls get,set`; riprodotto in Node23.3/V8 12.9 come `ToPropertyDescriptor` su descrittore
ordinario (trap Proxy utente o defineProperty), non su prototipo null né proprietà propria
(`target/task-0336/chrome-analysis/`). Divergenza Chrome probabile dagli interceptor Blink
(inferenza; chiave per chiave non verificata, presumibilmente indice "0"). Fixture conforme a
WebIDL, mantenuta invariata; patch e runner non rigenerati perché nessun file sorgente cambiato.
Retention frame resta aperta.

### Codex0335 revisionata, commit locale durante CI0333/0334

0335 riproduce e corregge descrittori parziali sul percorso nativoDOMProxy→expando→JSAPI:
writable-only perso, un solo accessor azzerava l’altro; corretto anche il mirror non-configurabile.
API V8 dirette restano per descrittori rappresentabili; casi mancanti usano l’intrinseco
Reflect.defineProperty catturato prima degli script e descrittore null-prototype. Nessun
merge leggendo il target prima della definizione. Native125normal/124JITless, negativi vecchio
helper/mirror/Reflectmutabile/prototipo inquinato, replay470/473 con3eccezioni note.
Chrome DevTools MCP155 Windows visibile HTTP127.0.0.1:19035: CHECK REALE0335 di suite.html
(HTMLCollection+window) passa; non eseguite le altre prove della suite. Pagina/server chiusi.
Batchbce416e50da: V840184060372TUTTO VERDE10/10, conferma correzione POSIX0334;
generale401840603919/13 verdi,4build ancora attive al controllo0335, nessun rosso.
Non fare push. Prossimo incarico circoscritto: verificare il candidato descriptor_object
(proxy getOwnPropertyDescriptor) con Object.prototype inquinato, prima di proporre un fix.


### Claude0335 — descrittori parziali nel bridge DOM/JSAPI (da revisionare, non committato)

Base bce416e50da; CI del push (V840184060372, generale40184060391, Android40184060445,
iOS40184060368) non controllata da Claude. API: rusty_v8152.2 `PropertyDescriptor` ha solo
`new`/`(value)`/`(value,writable)`/`(get,set)`, nessun writable-only né accessor singolo.
Difetto dimostrato sul percorso di produzione (trap proxy V8 → `defineProperty` nativo →
`JS_DefinePropertyById` sull'expando, come `proxyhandler::define_property` e WindowProxy):
`{writable:false}` ignorato, `{get}`/`{set}` azzerava l'altro accessor. Secondo difetto nel
mirror del target proxy: solo con `configurable` presente e con il descrittore parziale, quindi
TypeError d'invariante V8. Fix: `jsapi_impl::define_property` lascia il percorso nativo per i
descrittori rappresentabili e usa per gli altri `ReflectDefineProperty` (intrinseco catturato
prima degli script) con descrittore a prototipo null e soli campi presenti; nessuna lettura
preventiva. Il trap rispecchia anche quando il target ha già il mirror, rileggendo il descrittore
risultante con la trap nativa `getOwnPropertyDescriptor`. Test nativo nuovo + check non
obbligatorio `suite.html` (HTMLCollection, window); REQUIRED_CHECKS invariati, niente WSL.
roves-js normal125/2ign, JITless124/3ign. Negativi deterministici: codice vecchio normal/JITless
(9controlli), solo `proxy.rs` vecchio (mirror), helper da `Reflect` globale mutabile, descrittore
con `Object.prototype`. Runner Windows42ok/8skip. Patch `0335-partial-property-descriptors.patch`:
replay470/473, stesse3eccezioni. Log `target/task-0335/`. Fuori ambito, non dimostrato:
`descriptor_object` (risultato trap `getOwnPropertyDescriptor`) usa `Object.prototype`, quindi un
`Object.prototype.get` inquinato potrebbe invalidare descrittori dati di proxy DOM; candidato
per incarico separato. Retention frame resta aperta.

### Checkpoint0333/0334 revisionato, batch precedente concluso

Batcha4e78c2e543 concluso: generale4018148512013/13, Android40181485099 e iOS40181485090
verdi. V840181485105 rosso solo nei4runtime POSIX per fixture worker mancante (8errori ciascuno).
0334 corregge la fixture, non il validatore: lista REQUIRED_CHECKS condivisa per tutte le pagine,
totali coerenti e negativi missing:<pagina>. WSLUbuntu Node24/Python3.10/bash:42/42 senza
skip, negativo vecchiafixture8/8 falliscono con lo stessoerroreCI. Replay Codex su baselineHEAD
fresco e scratch completo: file identico; logtarget/task-0334/replay-reviewed.log.
0333 commitc34be052a5f/wiki85606e6 revisionato;0334 pronta: pubblicare insieme ora che tutte
le run precedenti sono terminate. NuovaCI dovrà confermare soluzione POSIX e quiescenza task.
Nessuna modifica incompleta; task senza handler rimangono conservativi e retentionframe aperta.


### CI0332 corretta localmente —0334 completa/revisionata, pubblicazione

Batcha4e78c2e543 V840181485105:6/10 verdi,4runtime POSIX rossi (Linux/macOS normal/JITless).
Cause lette nei job121133301534 e121133301527:8fallimenti del runner simulato, tutti
`stress.html: missing required check: dedicated worker: promise jobs and queueMicrotask callbacks share one FIFO queue`.
Il validatore è corretto; è incompleta la fixture fake_browser usata solo nei test POSIX.
NON indebolire requiredcheck o saltaretest. Correzione0334 da delegare con verificaWSL reale.
I4servoshell browser e i2runtime Windows passano; generale40181485120 ancora11/13 verdi,
Linuxportable121133781156 e WindowsMSI121133781161 attivi. Non fare push durante le run.
**Claude0334 (da revisionare, non committato):** il fake browser di `RunnerTests` riceve
`REQUIRED_CHECKS` in JSON e stampa per ogni pagina elencata example+tutti i required con totale
coerente; il test di successo verifica nel notice dei totali `<pagina> n+1/n+1` per ogni pagina,
i failure path `missing:<pagina>` attendono esattamente l'errore del check rimosso. Validatore
e required invariati. WSL Ubuntu (Node24.12, Python3.10, bash5.1): suite completa 42/42 ok,
0skip, 8RunnerTests inclusi. Negativo fixture HEAD: 8/8 RunnerTests falliti con il solo errore
CI `stress.html: missing required check: dedicated worker...`. Patch
`0334-posix-fixture-required-checks.patch` dry-run+apply sullo scratch0333, file identico.
Log `target/task-0334/`. Risoluzione CI da confermare con il prossimo batch dopo il push.
0333 revisione Codex completata: quiescenza consegne già accettate, nessun registry/timerlock
in attesa, nessun auto-unregister nell’handler (non avviene in produzione).8engine testsnormal/
JITless,30ripetizioni race,124/123runtime, negativi deterministici e replay470/473. Commitlocale.
Restano conservativi task senza registrazione e delayed di registrazioni sostituite.


### Claude0333 — rimozione handler foreground (revisionata, commit locale)

Base bc49109d4ff. API rusty_v8152.2: `PlatformImpl` riceve ogni task e non lo inoltra al runner
di default avvolto (`CustomTaskRunner::wrapped_` serve solo alle capability);
`Platform::notify_isolate_shutdown` è `pub(crate)` e non intercettato; `pump_message_loop`
sarebbe polling di un runner vuoto. Nessuna API pubblica sostituisce registro/timer: fix locale.
Corsa reale: `Registration::post` controllava `active` e chiamava l'handler senza lock, quindi
una consegna da thread V8/timer poteva attraversare close/`set_handler(None)`/retire e finire nel
ramo `retired` di `Foreground::post` (leak); il timer poteva anche estrarre un task scaduto
appena prima della rimozione. Ora ogni registrazione conta le consegne (`Mutex`+`Condvar`):
`post` accetta sotto il lock del registro, il timer sotto il proprio; la rimozione disattiva
sotto entrambi e, senza lock registro/timer, attende le consegne accettate prima di rilasciare
callback e task ritardati. Ogni task accettato arriva in `pending` (chiuso) prima di `retire`, che
lo distrugge sul thread dell'isolate prima della dismissione; il ramo `retired` resta difensivo,
irraggiungibile via handler. Restano conservativi (leak, mai drop su altro thread): task senza
registrazione (post dopo la rimozione, durante dismissione o indirizzo riusato) e ritardati di
una registrazione sostituita. Test reale: task timeout `Atomics.waitAsync` ripostato da altro
thread in un handler bloccato; la rimozione attende, l'handler può rientrare nel registro, task
distrutto una volta sul thread dell'isolate prima del drop. Negativi: senza attesa → assert
"removal returned..."; attesa sotto lock registro → assert rientro (1s, nessuno stallo). La
finestra pop del timer non ha test deterministico (chiusa dall'ordine dei lock). roves-v8 8/8
normal e JITless, ripetizione foreground30/30; roves-js normal124/2ign, JITless123/3ign.
rustfmt: file già non formattato (13→18 differenze, stile del file). Patch
`0333-foreground-removal-drains-deliveries.patch`: replay470/473, stesse3 eccezioni.
README, CUSTOMIZATIONS, wiki `shell.mdx`. Log `target/task-0333/`. Non risolve retention frame/DOM.

### Pubblicazione0330–0332 completata —25ottobre

Il batch0329 è terminato TUTTO VERDE: V84017869110010/10, generale4017869087513/13,
Android40178690892 e iOS40178690851 verdi. Pubblicati i commit0330/0331/0332 e handoff
nel checkpoint root **a4e78c2e543**, wiki **0f7b9da**. Nessun sorgente WIP in checkout.
Nuovo batch identificato dopo push: V8 **40181485105**, generale **40181485120**,
Android **40181485099**, iOS **40181485090**, tutti in coda al controllo iniziale.
Il nuovo check worker FIFO ha conferma Chrome/Node, manca conferma Roves da questo batch.
Questo aggiornamento degli ID è committato solo localmente durante la CI, senza altro push.

Resta da chiudere la retention dei frame (0/2 reali), i casi WindowProxy tra thread e
 document.domain, l’audit API approssimate non banali e task worker tardivi, poi WPT selettivi,
stress lifecycle/GC, scenari PixiJS/Three.js, confronti memoria/prestazioni e release/pin consumer.
Non presentare questa lista come difetti tutti già dimostrati: distinguere problemi riprodotti
(retention) da casi ancora da verificare. Nessuna percentuale di completamento dedotta.


### Checkpoint Codex —0330/0331/0332 committate locali, ultimo job0329 pendente

0330 commit96b6c7b0862,0331 commit82cfd790a60,0332 commit2495225b863 complete e
revisionate in locale; wiki locale0f7b9da. Root remoto resta bda0f74e7f4/wiki aad5440. Chrome DevTools MCP155
Windows visibile, HTTP127.0.0.1:19032: il CHECK REALE registrato in stress.html finale passa,
senza eseguire gli altri stress (nessuna misura costosa o performance). Include pulizia
idempotente, errore costruttore worker, errore evento, postMessage che lancia; negativi rilevati.
42runner test8skip prima della revisione,4test mirati dopo;3file replay0332 identici.
V840178691100 TUTTO VERDE10/10, generale4017869087512/13 verdi: resta solo
Linuxportable121125440097 attivo al controllo finale0332, step run layout unit tests
(ICU4X line breaking). Android40178690892 e iOS40178690851 verdi. Nessun push finché
non termina. Nessuna run cancellata o sostituita; nessun altro Claude attivo.
Nessuna nuova correzione runtime dimostrata da aprire nell’attesa; evitare ricerca senza limite.
Prossimo passo: completare la conferma CI0329, pubblicare0330–0332 e verificare worker FIFO
nel browser Roves normal/JITless. Retention frame0/2 ancora irrisolta. Nessuna automazione.


### Checkpoint Codex25ottobre —0330/0331 complete in locale

0330 revisionata/committata96b6c7b0862/wiki3e344a9.0331 revisionata: solo runJobs è letto
in produzione; eliminati hook inutilizzati e Box<Vec<Rc<MicrotaskQueue>>> mai ripreso da
DeleteJobQueue. Nessun cambiamento ABI roves-js;40runner test8skip, replay470/473 con3
soleeccezioni note. Preparare commit locale, nessun push durante il batch0329.
Batch bda0f74e7f4: V8401786911009/10 verdi (Windows build121124868688 attiva),
generale401786908757/13 verdi; nessun rosso al controllo0331. Entrambi i Linux browser
normal/JITless passano la nuova FIFO obbligatoria:0329 confermata in Roves su Linux.
GC normal continua module/no-module0/2 per elementi/documenti/global; scena singola
frame/document/probe non rilasciati. Snapshot0329 riuscito4894613byte: retention irrisolta.


### Claude0332 — check FIFO microtask nel worker dedicato (da revisionare, non committato)

Base 82cfd790a60. Solo smoke/validator/test, nessun codice di produzione. `stress.html`: nuovo
check obbligatorio con worker Blob classico, casi `a` (`p1,q,p2`) e `b` (`q1,p1,p2,q2`) in due
task messaggio distinti più `ping`; nonce/fase per risposta, scadenza propria5s (dentro i10s
dell'harness), pulizia (timer, handler, terminate, revoke) in ogni esito. Busy-termination
invariati. `check_results.py`: `REQUIRED_CHECKS["stress.html"]`. `WorkerFifoCheckTests` esegue il
check e il programma worker reali in Node con un mock minimo: FIFO passa; batched (`a: p1,p2,q`),
risposte duplicate/nonce estraneo, worker muto falliscono, sempre1 worker terminato e1 URL
revocato. Runner42 OK/8skip. Negativi su copie fresche: oracolo batched, senza revoke, senza
nonce, validator senza loop obbligatori o di HEAD falliscono. Patch
`0332-worker-microtask-fifo-smoke.patch` applicata pulita sullo scratch (base = HEAD, risultato
identico al working tree). Limite: Node non è un browser; l'ordine reale nel worker Roves
normal/JITless lo verifica la CI. Log `target/task-0332/`. Ritenzione frame: aperta.
Revisione: Chrome155 (Windows, visibile, HTTP) ha passato il check registrato prima della
correzione della pulizia; sorgente finale non rieseguita in Chrome. Correzione: stato di
pulizia inizializzato prima di `new Worker`, costruttore e `postMessage` protetti con esito
informativo (nome e messaggio dell'eccezione), `done` idempotente. Nuovi casi mock: costruttore
che lancia (0 worker, 1 revoke), evento error e `postMessage` che lancia (1 terminate, 1 revoke).
`WorkerFifoCheckTests` + contratti dei check obbligatori: 4 OK; negativo senza protezione del
costruttore fallisce (`log-review-*.log`).

### Claude0331 — rimossi i trap inutilizzati e l'allocazione interrupt queue (da revisionare, non committato)

Base 96b6c7b0862 (0330 locale). Grafo: roves-js legge solo `_traps.runJobs`; `CreateJobQueue`
memorizza `interrupt_queues` senza leggerlo e `DeleteJobQueue` libera solo `RustJobQueue`
(nessun `Drop`, `dropInterruptQueues` mai chiamato): il `Box<Vec<Rc<MicrotaskQueue>>>` di ogni
runtime script non veniva mai liberato. Ora `ptr::null_mut()`; `getHostDefinedData`, `empty` e i
tre trap interrupt sono `None`; rimossi `get_host_defined_data`, `HOST_DEFINED_DATA(_CLASS)`,
`INCUMBENT_SETTING_SLOT` (usato solo lì), `empty`/`MicrotaskQueue::empty`, i trap interrupt e gli
import relativi. roves-js invariato (layout ABI, export). Nessun WebIDL: nessun codegen.
Contratto esteso: runner40 OK/8skip; HEAD96b6c7b0862 → 14 problemi; mutazioni falliscono.
rustfmt: stesse5 differenze preesistenti. Patch `0331-remove-unused-job-queue-traps.patch`:
replay 470/473, stesse3 eccezioni. README, CUSTOMIZATIONS, wiki `shell.mdx` (non committato).
Log `target/task-0331/`. Compilazione crate script: CI. Ritenzione frame: questione distinta, aperta.

### Claude0330 — rimosso il vecchio enqueue Promise (revisionato, commit96b6c7b0862)

Base bda0f74e7f4 (0329 pubblicata). Grafo verificato: roves-js in produzione chiama solo
`_traps.runJobs` (`run_embedder_microtask`); `enqueuePromiseJob` è letto solo dal test nativo
con trap proprie. Rimossi `enqueue_promise_job`, `EnqueuedPromiseCallback`, il callback WebIDL
interno `PromiseJobCallback` (usato solo lì) e gli import relativi; `enqueuePromiseJob: None`.
Invariati roves-js (nessun test nativo rieseguito), gli altri trap (host-defined data,
interrupt queue, ecc.), FIFO/terminazione0329 e passo6 0327. Codegen deterministico
(PYTHONHASHSEED=0) HEAD vs nuovo: solo79 righe `PromiseJobCallback` in meno. Nuovo
`JobQueueTrapContractTests`: runner40 OK/8skip; mutazioni e sorgenti bda0f74e7f4 falliscono.
rustfmt: nessuna nuova differenza. Patch `0330-remove-servo-promise-enqueue-path.patch`:
replay sullo scratch 470/473, stesse3 eccezioni. README/wiki invariati (nessun effetto
visibile). Log: `target/task-0330/`. Compilazione del crate script: CI.

### Ripresa25ottobre —0329 pubblicata,0330 completa localmente

0329 pubblicata rootbda0f74e7f4/wiki aad5440. Nuovo batch V840178691100, generale
40178690875, Android40178690892, iOS40178690851. Al controllo0330: V86/10 e generale6/13
verdi, restanti build attive, nessun rosso. Nessun push durante il batch.0330 revisionata:
callback enqueue Promise morto e WebIDL interno rimossi, codegen deterministico conferma
solo79righe eliminate;40runner test/8skip, replay470/473 con3eccezioni note. Commit locale.

Codex ha revisionato diff, negativi e corrispondenza dei sorgenti testati/replay. Batch
226b7c0cec7 TUTTO VERDE: V840081979370 10/10, generale40081979510 13/13,
Android40081979357 e iOS40081979379 verdi. La pagina events della0329 è byte-identica
a quella verificata da Chrome MCP155 Windows visibile (18/18 il24ottobre). Non ripetuta.
Il nuovo checkpoint deve ancora essere compilato/verificato nel browser Roves dalla CI.

Il singolo ricontrollo autorizzato del 25 ottobre ha trovato Claude disponibile: la quota
settimanale del 24 ottobre (sotto) è un evento storico concluso, non uno stop attuale.
Applicato solo `0329-incomplete-code-only.patch` (SHA verificato) su0971c273da6, poi la
revisione di Codex:
- Fallback dopo terminazione: il difetto è riprodotto. La bozza eseguiva i microtask HTML
  rimasti dopo un solo checkpoint V8, lasciando in V8 le Promise e i turni che questi
  accodavano (`a,b,c` invece di `a,b,pb,c`); i turni obsoleti avrebbero poi eseguito in
  anticipo un microtask successivo. Ora il passo 3 è un loop: checkpoint V8, poi un solo
  microtask HTML solo se ne restano, fino a code vuote. Guardia di rientro invariata.
- Secondo difetto trovato: V8 assegna al turno il contesto *corrente* e lo salta quando quel
  contesto è chiuso (global staccato da una navigazione). `JobQueueMayNotBeEmpty` ora entra nel
  realm iniziale prima di accodare. Lasciare un realm ≠ chiuderlo: entrambi i casi sono testati.
- Test nativi con mock fedele a Servo (guardia, loop, fallback, coda tracciata come radice):
  terminazione, eccezione + lavoro successivo + checkpoint annidato, runtime abbandonato e nuovo
  runtime sullo stesso thread, finestra navigata con GC. 124 normal / 123 JITless (2/3 ignorati),
  copie testate identiche. Negativi su copie fresche: loop della bozza e accodamento nel realm
  corrente falliscono ciascuno il proprio test. Runner 38 OK / 8 skip; contratto statico
  aggiornato alla forma a loop. Replay sullo scratch 0328: 469/472, le stesse 3 eccezioni.
- Log: `target/task-0329/resume-2026-10-25/`. Patch `0329-microtask-fifo-v8-queue.patch`,
  CUSTOMIZATIONS, README e wiki `shell.mdx` aggiornati (wiki non committato). I file parcheggiati
  restano come evidenza. Codice morto `enqueue_promise_job` lasciato per lavoro separato.
  Nessuna build Servo locale: la compilazione del crate script la verifica la CI.

### Storico STOP quota settimanale —24ottobre11:23 (superato il25ottobre)

**Pubblicato checkpoint completo `226b7c0cec7` / wiki `2e77a2f`:** include0327/0328,
esclude0329. Nuovo batch V8 **40081979370**, generale **40081979510**, Android
**40081979357**, iOS **40081979379** avviato/in coda al controllo dopo push. Non verificato;
nessun polling per quota settimanale e nessuna automazione di ripresa. Questo aggiornamento
con gli ID è conservato come commit documentale locale, senza nuovo push durante le run.

Claude0329/revisione ha risposto: `You've hit your weekly limit · resets 8am (Europe/Rome)`.
Il messaggio non indica il giorno: non inferire una data. Sviluppo sospeso; nessun nuovo
modello, sessione alternativa o automazione. Il retry5ore NON vale per una quota settimanale.
Il singolo ricontrollo `ripresa-claude-dopo-quota-0323` è stato eliminato dopo la ripresa riuscita.

Batch e80e43898e1 TUTTO VERDE: V840078536565 10/10, generale40078536546 13/13,
Android40078536525 e iOS40078536540 verdi. Confermate le navigazioni opache consecutive0325.
0327 (4a9335fcc41) e0328 (9f0da3885e6), wiki35f4d62, complete e revisionate e incluse
nel checkpoint226b7c0cec7/wiki2e77a2f. Gli ID del nuovo batch sono in cima, senza polling.
0327 native120normal/119JITless;0328 wrapper generati byte-identici,37runner test8skip.
La retention complessiva dei frame NON è risolta.

**0329 NON pubblicabile:** implementazione della FIFO comune V8 consegnata con native verdi,
ma revisione di fallback/terminazione/shutdown incompleta per quota. Ripristinati TUTTI i
sorgenti pubblicabili a HEAD0328, incluso il wiki. Ricostruzione0329 spostata fuori da patches/.
Salvataggi ignorati (base9f0da3885e6):
- target/task-0329/0329-incomplete-code-only.patch: solo components/ e support/, apply--check OK;
  SHA2561d1a95f29d7fe6e626e747bade7f55765484ef458df929daff8c26a170abbceb.
- 0329-incomplete-before-review.patch: diff intero (contiene documentazione vecchia: NON
  applicarlo senza filtrare perché il nuovo handoff contiene questo stop); parked-wip.json.
- 0329-incomplete-wiki.patch (basewiki35f4d62) e0329-incomplete-reconstruction.patch.
- prompt.txt, review-prompt.txt, claude-result.json eclaude-review-result.json; log test/replay.
Scratch pristine roves-0315-replay-gAXo/w/src riportato a0328 invertendo la prima0329 con
successo: park-replay-reverse.log. Non riestrarre né applicare due volte la vecchia patch.
Alla ripresa: verificare Git, applicare SOLO il code-only patch, leggere review-prompt.txt,
delegare a Claude la revisione terminazione/fallback e abbandono runtime prima di committare.
Non ricreare CLAUDE_RESUME.md, non trasformare questa bozza in una modifica completa per fretta.

0329 riproduce un difetto reale: Promise annidata con queueMicrotask produce p1,p2,q anziché
p1,q,p2. Vecchio modello negativo: negative-old.log. La bozza usa Isolate.enqueue_microtask e
una callback nel realm iniziale per una FIFO comune; non è approvata. Riferimento Codex
Chrome DevTools MCP155 Windows visibile: p1,q,p2 e q1,p1,p2,q2; events.html della BOZZA18/18
su HTTP127.0.0.1:19029, non verifica il sorgente pubblicato (17check). Browser/server chiusi.
Dubbio di revisione: fallback HTML dopo V8 termination può accodare nuove Promise/turni da
ricongiungere alla FIFO senza lasciare turni obsoleti; verificare anche eccezioni e nuovo runtime
sullo stesso thread. Nessun fix frame o garanzia globale WeakRef dichiarato.

### Ripresa24ottobre —0324 verde,0325 revisionata,0326 analizzata

Il tentativo singolo dopo quota0323 è riuscito alle09:33 circa del24ottobre (scheduler
eseguito dopo la scadenza03:03). Eliminata l'automazione `ripresa-claude-dopo-quota-0323`;
nessun polling o retry periodico rimane. Claude0324 ha completato l'incarico, Codex ha
revisionato diff, risultati normal/JITless, negativo vecchio codice e replay, senza ripeterli.
Batch b38d0a0a8b0 TUTTO VERDE: V840014801242 10/10, generale40014801233 13/13,
Android40014801183 e iOS40014801157 verdi. Il controllo browser0318 conferma0322.
La diagnostica reale resta module/no-module0/2 e frame/document/probe non rilasciati;
lo snapshot della scena è riuscito (4898314byte). Nessuna soluzione retention complessiva.
0324 pubblicata8dd9fa77dae/wiki6f1f071, batch40075199667(V8),40075199664(generale),
40075199647(Android),40075199638(iOS) TUTTO VERDE.0325 commit localeab0cadb725d/wiki6eca57a,
Chrome MCP17/17 finale; pubblicabile col checkpoint documentale0326.
0324 nomina i ClassBox nello snapshot attraverso JSClass.name e distingue wrapper:
nessuna nuova libreria, nessun cambiamento a tracing/GC/finalizer.119normal/118JITless
passati (2/3ignorati); replay468/471 identici, stesse3eccezioni note. Prossima evidenza:
artifact CI con i nomi nuovi, leggere referrer con MemLab mantenuto, non un BFS nostro.
Le proposte0323 restano in target/task-0323/proposed-realm-tests.diff, non pubblicate.

### Checkpoint attuale0318–0320

**Aggiornamento93afe3af909:** Android/iOS verdi; V840007474006 ha entrambi i Linux smoke
rossi per0318: "opaque: contentWindow changed, source is not the held window, cross-origin
allowlist unusable". Batch concluso: V8 8/10 verdi, Windows incluso; generale40007474044 13/13 verdi,
Android/iOS verdi.0322 completa e revisionata localmente: il prossimo push include la
correzione, poi la CI deve confermare il controllo browser0318. Snapshot0320 RIUSCITO:4.91MB, ottavaGC,
frame/probe/document ancora vivi; artifact12524484912 (847088byteszip), nome
v8-heap-snapshot-frame-retention. Il fallimento identità è distinto dalla cattura riuscita.
0322 pronta per pubblicazione: identità stabile del wrapper ricevente nei passaggi
locale→remoto→locale, remoto→remoto e holder distinti per il realm del Window corrente.
La cache corrente usa il vecchio global proxy come chiave e salta la cache per i proxy
remoti: il nuovo canonicalremote è restituito comeoggetto diverso dall’heldwrapper.
MemLab0321 convalidato su campione V8 reale, selector tramite API mantenuta, nienteBFSnostro.

Codex ha revisionato0318/0319 (origine WindowProxy e scheduler): Chrome MCP155 HTTP/visibile
17/17; Roves ancora da nuovaCI.0318 commit20170c737db,0319 ad91debe7d7.0320 completo:
snapshot heap nativo V8 opt-in/debug, one-shot all’ottava TestUtils.gc della scena conun
frame vuoto, JSON controllato, nessuna API JS o dipendenza nuova. Test nativi4normal+4JITless,
cargo check senza debugassertions,29runner Windows(8skipPOSIX)/29WSL tutti; replay468/471
identici con3eccezioni note. Anche roves-action README aggiornato, nessun input/pin cambiato.

Batch2e9eb7ee15f TUTTO VERDE: V839999937163, generale39999937068, Android39999936958,
iOS39999936976. Si può pubblicare0318–0320. Lo snapshot sarà prodotto dal prossimo job
Linux normal come artifact v8-heap-snapshot-frame-retention (3giorni), non dal MCP Chrome.
Analizzatore proprio di percorsi superato da0321: si usa MemLab (vedi runbook sotto), poi
lettura dell’artifact reale. Niente ipotesi di fix globale senza prove.
0317 reale normal/JITless: moduli E frame senza moduli0/2 rilasciati in tutte le categorie.
Il percorso moduli non è l’unica causa. Evitare di ripetere misure Chrome non equivalenti.

Acquisito origin/main con fast-forward fino a8e4fb2e6cf4, wiki b86bb38. Tutte le modifiche
0287–0314 sono già pubblicate: le precedenti indicazioni "solo locale" sono superate.
Codex dirige/revisiona, Claude implementa via terminale come richiesto di nuovo dall'utente.
Rimosso il file temporaneo CLAUDE_RESUME.md: questo è il punto di ripresa corrente unico.

CI sul commit8e4fb2e6cf4: generale39973655174, Android39973654912 e iOS39973655140 verdi.
V839973655294 conclusa rossa: entrambi i job Ubuntu normal/JITless compilano ma lo smoke
GC fallisce con ReferenceError: main is not defined. I restanti job passano. Correzione
mirata0315 delegata: la prima invocazione si era fermata per quota; il tentativo delle18:45
è riuscito. Claude ha consegnato, Codex ha revisionato diff ed evidenze senza ripetere test
identici. Il ricontrollo one-shot riprendi-claude-alle-18-45 è stato eliminato dopo il tentativo.
La bozza e le vecchie istruzioni sotto docs/handoff sono soltanto archivio storico, non WIP.

**0315 completo e revisionato (ripresa18:45, Claude), in questo checkpoint di pubblicazione:** main() ripristinato
identico al pre-0314, diagnostica0314 e check cleanup entry settings invariati, marker run.sh
invariati. test_runner.py: GcPageTests (marker run.sh con producer nella pagina; esecuzione
Node vm senza TestUtils/DOM: funzioni d'ingresso definite, api false, ingresso nel cleanup,
un solo result). Mock/statici, non GC browser. Esiti: unittest OK (11, 5 skip POSIX-only su
Windows); negativo pagina pubblicata (main mancante) FAIL su entrambi; negativo main non
chiamato FAIL. Replay completo pristine v0.5.0 (sha25686f45a1e…) + 312 patch (0042 saltata
come CI) + REMOVED_PATHS:0315 dry-run/apply puliti, i 2 file uguali al working tree.
Deriva preesistente non0315 (8 path diversi da HEAD: Cargo.lock, README.md,
support/MOBILE.md, MainActivity.kt, content-packer pack.rs/roundtrip.rs, ios/App.swift,
sdl3-gamepad-probe da0042): patch non aggiornate da commit precedenti, da valutare a parte.
Log in target/task-0315/. Conferma browser attesa dalla CI V8.

**0316 (Claude, revisionato da Codex, commit locale in questo checkpoint):** patch 0316-reconcile-mobile-and-content-packer-drift
registra 5 path di deriva reale (MainActivity.kt, ios/App.swift, MOBILE.md, content-packer
pack.rs/roundtrip.rs; commit c16f9b5d7ac, 16a461ef66a, c7debe9164e, a0b9268e96e). Nessun
sorgente di produzione cambiato; nessun hunk Cargo. Verifica sullo scratch0315 esistente
(senza riestrarre): dry-run/apply puliti, 468 path patchati vs HEAD: 465 identici, 3 diversi =
eccezioni documentate in CUSTOMIZATIONS (Cargo.lock radice rigenerato senza --locked;
README.md progetto, solo hunk storico0243; sonda SDL0042 saltata dalla CI). Nota: il commento
di test.yml "patches/ carries no Cargo.lock diff" è impreciso (0071…0246 hanno hunk sul lock
radice), workflow non toccato. Log: target/task-0316/verify.log. Nessuna build/test ripetuti.

**0317 (Claude, da revisionare, non committato):** CI reale dopo0315 (Linux normal/JITless):
frame rimossi del gruppo con moduli elements0/2 documents0/2 globals0/2. Per separare la
retention dalla proprietà dei moduli, gc-lifecycle.html aggiunge dopo i controlli moduli
invariati un gruppo no-module di2 frame srcdoc same-origin (vuoto + solo script classico),
con helper condivisi loadFrame (load/error once, timeout5 s esistente) e removeFrames (solo
WeakRef a elemento/documento/probe creato e tenuto solo dal global figlio). Marker obbligatori
nuovi: removed frame group module/no-module elements2 documents2 globals2, no-module frames
loaded true; diagnostica per gruppo "diagnostic removed frames released <gruppo> elements N/2
documents N/2 globals N/2" (prefisso0314 invariato): run.sh ne richiede presenza e totali /2,
qualsiasi numero rilasciato passa. Nessun marker esistente cambiato o indebolito. Verifiche:
unittest Windows (Node vm mock DOM: entrambi i gruppi, listener once, solo WeakRef; mutazioni
che tolgono record/diagnostica rilevate) + WSL Ubuntu (fixture run.sh: gruppo no-module o
diagnostica mancanti e totali errati falliscono, 0/2 passa); i nuovi test falliscono col run.sh
di HEAD. Mock, non GC browser. Patch0317 dry-run/apply puliti sullo scratch0315+0316 (ora con
0317 applicata), 465 identici + 3 eccezioni documentate. Log: target/task-0317/. Il confronto
sorgente/contesto con Chrome è affidato a Codex via MCP; i risultati browser arrivano dalla CI V8.

**0318 (Claude, da revisionare, non committato):** events.html aggiunge il check obbligatorio
"an iframe keeps its WindowProxy across origin changes and back": srcdoc same-origin, poi
sandbox="allow-scripts" + nuovo srcdoc (origine opaca), poi senza sandbox. In ogni fase:
contentWindow/event.source === held, origini (parent vs "null"), parent.document visto dal
figlio; fase opaca: held.document e global figlio SecurityError DOMException, contentDocument
null, allowlist e ping/echo via held.postMessage; ritorno: document/marker/global finali.
Token, listener unico filtrato, timer8 s, pulizia frame/listener su ogni uscita. Sotto file://
entrambe le origini sono "null": distinguono i controlli di accesso. check_results.py:
REQUIRED_CHECKS (missing required check). Ispezione: Servo deriva già l'origine opaca dal flag
sandbox (navigation.rs), nessun fix proposto. Verifiche: unittest Windows17 (5skipPOSIX), WSL17;
nuovi test falliscono col validatore di HEAD; sintassi via Node vm.Script, non browser.
Patch0318 dry-run/apply puliti sullo scratch, 465/468 identici + 3 eccezioni. Log: target/task-0318/.

### Risultati acquisiti e limiti della revisione

-0287/0299 recuperano task timer e post tra close/rimozione handler. Restano casi conservativi
  di consegna già in volo/senza handler: nessuna prova di assenza leak complessiva.
-0288–0291 implementano costruttori dinamici nativi/CSP/toString e origine import; corpi
  identici condividono il referrer cache eval come V8 nativo.0273 WIP è superato.
-0289/0294 usano incumbent V8 e fallback entry settings per callback V8. Shim simboli C++
  vincolato a rusty_v8=152.2.0: verificare ABI/simboli a ogni aggiornamento.
-0292/0300 rimuovono API morte;0302 rimuove Runtime/pilot WebIDL di fase1 e derive.
-0301/workflow accelerano bootstrap apt; batch607b2e5 tutto verde. Non dedurre tempi runtime.
-0303–0310 riusano il global proxy con detach e session_history_max_length=0 (ricarica,
  scelta utente registrata da Claude); vecchi global spostati su holder, unload prima del
  passaggio e wrapper cross-realm inoltrati. Test ripetuti/cross-origin aggiunti. Non è prova
  completa di navigazione browser o rilascio delle pagine.
-0311/0312 catturano intrinseci/builtin prima del retarget;0313 corregge discriminante unload;
  0304/0314 aggiungono diagnostica WeakRef di elementi/documenti/probe global dei frame rimossi.
  La diagnostica non è un requisito pass/fail e il batch corrente non la esegue: main manca.

Revisione iniziale: letti cambi di lifecycle/WindowProxy, shim e log CI pertinenti; non è
una revisione esaustiva di ogni riga delle158 file cambiate. Non ripetere test identici già
coperti dalla CI senza dubbio concreto. Build Servo pesanti online; verifiche locali isolate.
Prima di pubblicare patch nuove, replay completo pristinev0.5.0 come validate-servo-patches.

### Pubblicazione e prossimo controllo CI

Batch0315 root459f43a5220/wiki cf9718e TUTTO VERDE: V839991694557, generale39991694618,
Android39991694436 e iOS39991694403. Smoke GC reale normal/JITless passati; diagnostica
0314 ancora elementi/documenti/globali0/2 rilasciati in entrambi. È stato possibile
pubblicare0316 e0317 insieme dopo revisione; nuove run vanno identificate sul nuovo SHA.
0317: Windows14 test (5skipPOSIX), WSL14 (3skipNode), unione copre14; negativi omissioni,
replay465/468 identici con3eccezioni documentate. Moduli e frame senza moduli sono gruppi
separati, conteggi obbligatori ma raccolta ancora soltanto diagnostica.
Chrome DevTools MCP disponibile: riferimento preliminare Chrome155/pagina visibile;
snapshotheap negato per workspace roots del MCP. Nessuna raccolta forzata verificata,
nessun confronto retention equivalente; dettagli in ROVES_PERFORMANCE.md. Non aggirare
il blocco del MCP. Nessuna nuova automazione periodica.

### Revisione0318 e riferimento Chrome

Codex: controllo origine0318 confermato vero via Chrome DevTools MCP155 su HTTP loopback,
pagina visibile. Suite16/17: fallisce soltanto il vecchio requisito timer-prima-di-rAF;
ordine osservato sync,microtask,queueMicrotask,frame.0319 correggerà quel requisito errato,
attendendo timer e frame e verificando microtask prima di entrambi. Roves0318 ancora daCI.
0318 va conservato in locale mentre il batch2e9eb7ee15f è attivo. Nessun risultato di
performance dedotto; evidenze e condizioni in ROVES_PERFORMANCE.md.

**0319 (Claude, da revisionare, non committato):** il primo check di events.html diventa
"microtasks run before timer and frame callbacks, in either order": attende entrambi i
callback (contatore, nessun polling) e accetta solo sync,microtask,queueMicrotask seguiti da
timeout/frame in uno dei due ordini; callback mancante = timeout harness 10 s, FAIL. Nessun
REQUIRED_CHECKS/run.sh/check_results cambiato; 0318 invariato. Test Node SchedulerCheckTests
(check estratto dalla pagina + harness.js in vm, code timer/rAF finte, nessun tempo reale):
timer-first/frame-first PASS, queueMicrotask tardivo FAIL in entrambi gli ordini, callback
mancante FAIL timeout, vecchio check FAIL frame-first; mutazioni rilevate. Windows21 (5skipPOSIX),
WSL non necessario (protocollo shell non toccato). Patch0319 dry-run/apply puliti sullo scratch,
465/468 identici + 3 eccezioni. Log: target/task-0319/. Riverifica Chrome MCP a Codex; Roves daCI.

### Conferma revisione0319

0319 consegnato da Claude e revisionato:21 test Windows (5skipPOSIX), quattro negativi
rilevati, replay dei2file identico,465/468 con3eccezioni. Nessun protocollo shell modificato,
nessun motivo per ripetere WSL. Codex ha ricaricato events.html in Chrome DevTools MCP155:
17/17 verdi, origine0318 compresa. Roves0318/0319 ancora daCI. Commit locali finché il
batch2e9eb7ee15f è attivo; non confondere esito Chrome con convalida Roves.

**0320 (Claude, da revisionare, non committato):** dump nativo V8 solo debug per trovare i
retainer dei frame rimossi (0/2 rilasciati in entrambi i gruppi). Riusa
Isolate::take_heap_snapshot di v8=152.2.0, nessuna dipendenza. heap_snapshot.rs con
debug_assertions: ROVES_V8_HEAP_SNAPSHOT_PATH (assoluto, create_new) e _GC (N>0) letti una
volta per RuntimeHooks; conta solo JS_GC DOM_TESTUTILS, richiesta consumata prima del dump dopo
low_memory_notification, streaming BufWriter, errori su stderr senza panic/retry, file parziale
rimosso. Nuova frame-retention.html (un frame srcdoc vuoto, probe nel realm figlio, 8 gc
attese, rilascio solo diagnostico) eseguita da run.sh solo con V8_SMOKE_HEAP_SNAPSHOT; env
nativo solo a quel processo, file/marker/validatore obbligatori. v8.yml: solo Linux normal,
artifact v8-heap-snapshot-frame-retention 3 giorni. Test: nativi normal/JITless (4),
unittest Windows29 (8skip POSIX)/WSL29, negativi con run.sh di HEAD. Snapshot V8 Roves, non
MCP (snapshot Chrome MCP negato); nessun risparmio RAM. Log: target/task-0320/.

**0321 (Claude, revisionato da Codex, solo documentazione + strumenti temporanei ignorati):** runbook
MemLab per i retainer dello snapshot nativo. Riuso invece di un parser/BFS nostro: MemLab
(Meta, licenza MIT, npm `memlab@2.0.5`, gitHead d454ded1a817, puppeteer24.31.0 non usato)
legge il formato `.heapsnapshot` di `Isolate::take_heap_snapshot` senza conversioni. Nessuna
dipendenza di progetto/Cargo/lock, nessun browser scaricato o avviato, nessun risparmio RAM.
Installazione provata solo in WSL Ubuntu (Node24.12.0, npm11.6.2; Windows nativo non provato),
prefisso temporaneo `/tmp/roves-task0321-memlab` (≈190MB, rimuovibile con `rm -rf`):

    PUPPETEER_SKIP_DOWNLOAD=1 npm install --prefix /tmp/roves-task0321-memlab \
      --cache /tmp/roves-task0321-memlab/.npm-cache --no-audit --no-fund --ignore-scripts memlab@2.0.5

Lo shebang `#!/usr/bin/env node --expose-gc ...` (senza `-S`) non funziona su Linux e resta
bloccato al100% CPU; invocare `node --expose-gc --max-old-space-size=4096
.../node_modules/@memlab/cli/bin/memlab.js` (wrapper `target/task-0321/ml.sh`, timeout300 s).
Anche `memlab help <cmd>` resta bloccato dopo l’intestazione: usare docs ufficiali/sorgente.
Script ignorati in `target/task-0321/`: `install.sh`, `ml.sh`, `run-analyze.sh`, `run-trace.sh`,
`select-node.js` (selettore di ID one-off: solo `getFullHeapFromFile` e iterazione API MemLab,
per `--class NOME` o `--edge PROPRIETÀ`; exit1 se nessun nodo). Per l’artifact frame:

    node target/task-0321/select-node.js SNAP --edge rovesFrameRetentionProbe > sel.json
    target/task-0321/ml.sh trace --snapshot SNAP --node-id <id da sel.json> --work-dir W
    target/task-0321/ml.sh analyze object --snapshot SNAP --node-id <id> --work-dir W --output json

Il probe è un `Object` semplice: selezionarlo per proprietà, non per classe; assenza del nodo
nello snapshot = probe raccolto oppure scena diversa (verificare i marker run.sh prima).
Esito sulla cattura unitaria 0320 (`real.heapsnapshot`, 1.16MB): parse/dominatori riusciti;
eyecatcher `RovesSnapshotEyecatcher` @22409 (104 B), referrer `eyecatcher` dal JSGlobalObject
@21673 e `value` da PropertyCell; trace: GC roots → C++ native stack roots → RovesJsRealm @2 →
RovesJsCell @8 → JSGlobalProxy @21671 → global_object → eyecatcher. Percorso atteso per un
realm tenuto vivo dal test, non prova di retention browser. Limiti: `trace` con ID inesistente
esce0 senza alcuna traccia né errore (controllare che l’output contenga `@<id>`); snapshot non
valido: exit1 "Error parsing heap snapshot". `analyze global-variable` richiede un nodo
`Window` e qui restituisce `[]`. MemLab mostra solo il percorso più breve; lo snapshot V8 vede
gli oggetti C++ soltanto tramite gli archi embedder esposti (non il grafo cppgc unificato),
quindi un retainer solo nativo può comparire come radice sintetica. Una sola cattura non prova
un leak; nessun fix proposto. Log: target/task-0321/*.log, *.json, *.txt.

**0322 (Claude, revisionata da Codex, pubblicata in b38d0a0a8b0):** identità stabile del WindowProxy per il fallimento
0318 opaco di V840007474006. Causa confermata nel codice: cache WeakMap del realm ricevente
con chiave = oggetto V8 del proxy corrente e proxy remoti (DissimilarOriginWindow) restituiti
senza wrapper. Fix in roves-js: record cppgc `WindowIdentity` (proxy corrente tracciato +
Symbol non registrato come chiave WeakMap) condiviso da proxy e wrapper; il transplant con
global proxy lo ritarga e lo passa al sostituto (rimosso il campo `forwarded`); i wrapper non
tengono più il vecchio proxy in `private`; anche i proxy remoti passano dalla cache;
`GetProxyHandler` dinamico (handler del proprietario corrente). Invariati `window===globalThis`
nel realm proprietario, finalizer DOM solo nel proprietario, API JSAPI pubbliche. Test nuovo
(2 cicli locale→remoto→locale, identità, allowlist/negato, raccolta dei realm sostituiti):
fallisce prima del fix (round0 remoto), mutazione retention rilevata. roves-js isolato normal
114ok/2ignored, JITless 113ok/3ignored. Patch0322 dry-run/apply sullo scratch0315…0320,
468/471 identici + 3 eccezioni note. Log: target/task-0322/. Conferma browser: CI V8 (0318).
Revisione Codex applicata nella stessa patch: (1) solo il realm proprietario di un Window locale
riceve il proxy stesso; i proxy remoti passano sempre dalla cache, anche se creati nel realm
ricevente (test remote-first e local-first); (2) holder cross-origin con chiave = realm del Window
corrente (`realm_key`, oggetto cache del realm): holder nuovi a ogni navigazione, anche con global
proxy riusato e remoto→remoto, raccolti col realm; Location invariato; (3) ogni transplant tra
WindowProxy usa il ramo d'identità: il vecchio V8 Proxy remoto tratteneva il realm sostituito.
4 test nuovi falliscono prima; mutazione del transplant rilevata. roves-js normal 118ok/2ignored,
JITless 117ok/3ignored. Patch0322 rigenerata (4 file), reverse+apply sullo scratch, 468/471
identici + 3 eccezioni note. Log: target/task-0322/review-*.log, negative-review.log.
Limite: prova unitaria con trap finti; conferma browser in CI V8 (0318).

### Snapshot reale0320 analizzato (Codex,0321)

Artifact12524484912 del job120598951538/run40007474006: snapshot4910076byte,
SHA256 `d3eec325a044110301bbb8f536924a872fd8263eb85eaec9330be5642220aff0`.
Il validatore JSON passa. MemLab seleziona il probe@65595 tramite proprietà
`rovesFrameRetentionProbe`; referrer WeakRef.target debole, PropertyCell.value e
Window[JSGlobalObject]@61811. `trace --node-id65595` restituisce path is not found, exit0,
anche se il nodo esiste. Nessuna conclusione di assenza leak: visibilità archi nativi e filtri
MemLab limitano il risultato. Frame/documento/probe non rilasciati nella scena CI. Dettagli
in ROVES_PERFORMANCE.md; log target/task-0321/frame-{selection,trace,object}.*.

### Diagnosi0323: `TestUtils.gc` di produzione vs `gc_for_testing` (Claude, nessun difetto stabilito)

Nessuna modifica di sorgente né patch. Differenza da sorgente (v8=152.2.0): `JS_GC`
(`components/roves-js/src/runtime_impl.rs:450`) chiama `low_memory_notification` →
`Heap::CollectAllAvailableGarbage` (2–7 GC complete, `heap.cc:1295`) con stato stack
predefinito `kMayContainHeapPointers` (`heap.h:2315`): cppgc scansiona lo stack nativo in modo
conservativo. `Runtime::gc_for_testing` (`rust.rs:474`) chiama `CppHeap::CollectGarbageForTesting`
→ una GC completa `NoHeapPointers`, senza stack. In produzione `JS_GC` gira in un task Servo
(`testutils.rs:34-37`), senza frame JS. `RunJobs`/`drain_microtasks` usano
`perform_microtask_checkpoint`, che esegue `ClearKeptObjects` (`microtask-queue.cc:180`).
Regressione nativa (proposta, `target/task-0323/proposed-realm-tests.diff`, si applica a HEAD):
(1) realm rilasciato con ciclo global/oggetto/closure, osservato da WeakRef nel realm iniziale e
WeakPersistent, 8 task `JS_GC(DOM_TESTUTILS)`+`RunJobs`: realm e probe raccolti; (2) una parola
nativa viva con l'indirizzo del RealmData mantiene il realm sotto `JS_GC` e non sotto
`gc_for_testing`. Normal e JITless 2/2 ok. Questa prova conferma la differenza
per le parole di stack; non dimostra equivalenza completa dei due percorsi né la causa browser.
Snapshot CI0320 (preso dentro l'ottava `JS_GC`, opzioni predefinite = con stack), solo API
MemLab a un livello (`target/task-0323/stack-roots.js`, `one-level.js`): "C++ native stack
roots" ha 106 archi verso 11 nodi: realm@2 (pagina, contesto@7485), realm@22 (realm iniziale),
celle della pagina (proxy@37089, Promise TestUtils@115987, `gc`, `runPromiseJobs`), nessuno del
frame rimosso. Window figlio@61811 (probe@65595) ← NativeContext@14139 ← realm@1190, tenuto
solo dall'holder@102781 nel proprio EmbedderDataArray@61809: è il ciclo previsto. Il proxy@48737
è `global_proxy_object` di due contesti (@14139 e @16847/realm@1486, global@48959: riuso0303);
le sue celle@1186/@1192/@1488 sono referenziate da diversi `RovesJsClassBox` (slot o trace di
classi DOM) di classe non identificabile nello snapshot. Ignoto: chi tiene quei ClassBox;
la scansione conservativa non tiene direttamente il frame, ma via oggetti di pagina non è esclusa.
Prossimo passo proposto (da revisionare, non implementato): in `object.rs:126` `get_name`
restituisce il nome statico del `JSClass` del box (es. `HTMLIFrameElement`) invece di
`RovesJsClassBox`; effetto solo sui nomi dello snapshot. Con il prossimo artifact, interrogare
a un livello i referrer di quei ClassBox. Log: target/task-0323/ (probe-*.log, *.json, summary.txt).

### Nomi ClassBox nello snapshot0324 (Claude, diagnostica, non una correzione)

Implementato il passo sopra (patch `0324-class-box-snapshot-names.patch`). `ClassBox::get_name`
(API cppgc `GarbageCollected::get_name` di rusty_v8152.2.0, già usata) restituisce
`JSClass.name` del box; `RovesJsClassBox` resta per classi senza nome e i wrapper Window
prestati (`window_wrapper`) si chiamano `RovesJsWindowWrapper`, distinti dal Window reale.
Contratto `'static`: rusty_v8 richiede nomi vivi per sempre fuori dallo snapshot; le classi
sono statiche e tutti i `JSClass.name` del codice (Servo codegen `str_to_cstr_ptr`, script,
roves-js) sono letterali C. Nessuna dipendenza, registro o stringa allocata; trace/finalizer
invariati. Test `heap_snapshot::tests::class_boxes_carry_their_class_names` (snapshot V8 reale:
due classi distinte, fallback senza nome, wrapper prestato e nome della sua classe assente);
con il vecchio `get_name` fallisce. roves-js normal119/119 e JITless118/118 (ignorati2/3).
Replay pristine: dry-run/apply ok, 468/471 identici con le stesse3 eccezioni. Log:
`target/task-0324/`. La retention del frame rimosso resta irrisolta: serve il prossimo artifact
CI0320 per leggere a un livello i referrer dei ClassBox ora nominati.

**0325 (Claude, revisionata da Codex, completa localmente):** copertura browser di0322 remoto→remoto.
Il check obbligatorio0318 (nome invariato, tutte le prove precedenti conservate) esegue due
round same→opaco-a→opaco-b→same (7 navigazioni, timer8 s, harness10 s). Token distinti per
documento/ping/echo: messaggio di un passo precedente = FAIL "stale message". Payload figlio
con fase, marker e global: documento non cambiato = FAIL. Per ogni documento opaco: letture e
descrittori di `postMessage` uguali, chiamata con `send.call(held, …)`, invariato dopo l'echo;
l'echo deve portare il token del documento corrente (instradamento a un documento vecchio =
token errato o timeout). Nessun confronto di identità tra documenti: riferimento Codex Chrome155
MCP (Windows, visibile, HTTP loopback) dà lo stesso `postMessage` per due opachi successivi;
la bozza precedente vi passava16/17, unico fallimento quell'asserzione, rimossa. Rinnovo holder
coperto dai test nativi0322. Conferma finale Codex Chrome DevTools MCP:17/17, Chrome155 Windows, pagina visibile,
HTTP127.0.0.1:19025/events.html, reload senza cache. Nessuna misura prestazionale.
`OriginCheckTests` (Node vm, iframe finto, timer accelerati, non browser) con due strategie
valide (postMessage condiviso instradato al documento corrente come Chrome, o nuovo per
documento): entrambe passano; 8 guasti a opaque-b-0 (incl. stale-routing) falliscono con pulizia
in entrambe. 13 mutazioni della pagina (incl. events.html di HEAD, senza ping/echo) rilevate.
Windows31 test (8skip POSIX); run.sh/check_results invariati, WSL non ripetuto. Patch0325
dry-run/apply sullo scratch, 468/471 identici + 3 eccezioni. Log: target/task-0325/.

### Snapshot nominato0326 analizzato (Claude, nessun difetto stabilito, nessuna modifica)

Artifact12553680992 della run40075199667: 4911213byte, SHA256
`9a358fb91e404d4a3c9d98e86cc244f058631268bbe83f15a4e4f1ea8951805a`; validatore JSON ok. Solo API
MemLab2.0.5 a un livello (`one-level.js`, `stack-roots.js`, `select-node.js`). Probe@84963 ←
Window[JSGlobalObject]@54631. Due realm figli condividono il proxy riusato@54629 (0303):
realm@1192/contesto@14139/global@54631/`Window (cppgc)`@1190 e realm@1486/@16847/@54643/@1484.
Ogni referrer forte di probe, global, contesti, proxy, celle@930/@1188/@1488, `WindowProxy`
@1274/@1566, `HTMLIFrameElement`@1480, Document@1254/@1550, realm e loro EmbedderDataArray è
interno al cluster del frame (cicli attesi) o debole. L'unico ponte dai root: due
`RovesJsWindowWrapper`@1094/@970 ← JSProxy tenuti solo da voci efemerone di WeakMap@97915/@97341
(cache d'identità di due realm riceventi), chiave Symbol@97357 tenuto solo da
`RovesJsWindowIdentity`@928, nel cluster. Radici stack nativo (10), Persistent (RootSet@22,
MicrotaskQueue), traced handles, Handle scope/Stack roots: nessun nodo del frame.
Attenzione MemLab: `isRootNode` considera radice ogni nodo nativo che inizia per "Window"
(`@memlab/core/dist/lib/Utils.js:305`): con i nomi0324 `trace` parte da `WindowProxy`@1274,
euristica e non radice GC; i dominatori contano gli archi efemerone come essenziali.
`TakeHeapSnapshot` di rusty_v8 (opzioni predefinite) esegue `CollectAllAvailableGarbage` con
`kMayContainHeapPointers` e completa lo sweep prima del grafo (`heap-snapshot-generator.cc:3658`):
il cluster è sopravvissuto a quella GC senza un arco forte visibile.
Test diagnostico solo su copia di lavoro (`target/task-0326/abandoned-window-test.rs`, repo
intatto): Window incapsulato in due cache, proxy riusato da navigazione, tutto abbandonato,
`gc_for_testing`: raccolti, normal e JITless ok; negativo con un wrapper radicato FALLISCE come
previsto. Le cache d'identità non trattengono sotto GC precisa: nessun arco forte ingiustificato,
nessun fix. Ipotesi aperta, non provata: retainer non rappresentato come arco (parole conservative
dello stack durante la GC stessa; lo snapshot registra solo la propria scansione successiva).
Revisione Codex: questi dati descrivono soltanto gli archi visibili, non dimostrano
l’assenza di un retainer. La proposta di una GC browser senza scansione stack NON è
autorizzata né implementata: NoHeapPointers è insicuro senza una prova del punto sicuro.
Prossimo controllo distinto (svolto in0327, sotto): TODO ClearKeptObjects allo step6 del
checkpoint HTML. Log: `target/task-0326/summary.txt`.

### Step6 ClearKeptObjects del checkpoint HTML (Claude0327, revisionata da Codex)

Gap confermato: V8 svuota i kept objects solo in `PerformCheckpointInternal`
(`microtask-queue.cc:180`), che roves-js esegue solo nel drain job accodato dal promise hook;
`JobQueueIsEmpty`/`MayNotBeEmpty` sono no-op e script non usa `RunJobs`. Senza Promise (script,
timer, `queueMicrotask`) i target WeakRef restavano tenuti fino a una Promise successiva.
Fix: `ClearKeptObjects(cx)` (nome mozjs) in `runtime_impl.rs` → `Isolate::clear_kept_objects`
di rusty_v8152.2.0, riga in `wrappers2.in.rs`; chiamato una volta in `MicrotaskQueue::checkpoint`
dopo rifiuti/IDB e prima del flag false. Tutti i chiamanti (window, worker, parser) passano da qui.
Non spiega la retention della scena (usa Promise). Clear anticipato dentro il drain job invariato.
Test: nativo normal120/JITless119 ok (2/3ignorati); contratto statico in `test_runner.py`
(sorgente attuale ok, codice precedente e8mutazioni falliscono); unittest33 ok (8skip POSIX).
Patch0327 dry-run/apply sullo scratch: 469/472 identici + 3 eccezioni note. Nessuna fixture
browser (l'harness usa Promise e maschererebbe il caso). Nota: `gen_wrappers2.py` rieseguito
toglierebbe3voci esistenti (corretto in0328). Log: `target/task-0327/`.

### Generatore wrappers2: voci proprie di roves-js (Claude0328)

Causa: `JS_InitGlobalInterfaceProperties`, `Get/SetCrossOriginPropertyHolder` non esistono nelle
liste di mozjs0.21.6 (aggiunte a mano in 240b1b6/8d32714), quindi nessuna regola di visibilità
poteva conservarle. `gen_wrappers2.py`: tabella `ROVES_ENTRIES` ancorata alla voce mozjs
precedente (errore se ancora o funzione mancano), `generate()` di sola lettura e `--check`.
Rigenerazione con mozjs0.21.6 byte-identica al file versionato (incluso ClearKeptObjects);
`wrappers2.in.rs`/API invariati, nessuna compilazione Rust necessaria. `Wrappers2GeneratorTests`
in `test_runner.py`: rigenerazione da liste ricostruite (gira in CI), firme vs funzioni raw e
mutabilità cx, negativo senza tabella; `--check` reale solo se mozjs è nel registry locale
(saltato in CI). Generatore HEAD: perde le3 righe. Unittest37 ok (8skip). Patch0328 sullo
scratch: 469/472 identici + 3 eccezioni note. Log: `target/task-0328/`.

### Prossimi punti

1. Completare il batch0329 (ultimoLinuxportable in cima) prima del push;0330–0332 complete
   in locale. FIFO Window Linux normal/JITless già verde; nuovo check worker daCI dopo push.
   Se nessuna implementazione circoscritta pronta, lasciare questo handoff senza polling.
   Snapshot nominato artifact12553680992 analizzato in0326: nessun arco forte esterno visibile.
   Chrome MCP155 ha già confermato17/17; nessuna build Servo pesante locale.
2. Leggere i risultati reali della diagnostica frame dopo il fix; studiare radici residue di
   global/realm/moduli senza dichiarare risolta la retention. JSON linkati mai valutati forti.
3. Rivedere casi navigazione cross-origin/cambi thread e API approssimate non banali, shutdown
   worker/task e moduli. Poi WPT selettivi, stress GC, PixiJS/Three.js e confronto Chrome viaMCP.
4. Release e pin Action/Packmaster solo dopo criteri del piano soddisfatti e verifica artefatti.

## Storico della ripresa del 22 ottobre (0281–0286; lo stato attuale è in cima)

**0286 completa localmente:** rimozione/sostituzione handler invalida sotto mutex ma
rilascia la callback fuori dal registro; i distruttori embedder possono rientrare senza
lo stallo precedente. Nessuna dipendenza/API nuova. Motore66 test+2doctest normal/JITless; runtime100/99 (2/3ignorati);
negativo vecchio codice fallisce senza stallo. Replay2 patch precedenti+0286 identico.
Log target/task-0286/{normal,jitless,negative-lock,replay}.log.0285 commit e7f96b0cf99,
wiki05365d3. CI V839844591657:9/10 verdi, Ubuntu normal timeout bootstrap30min durante download
apt prima della compilazione. Retry del solo fallito richiesto una volta; generale ancora
attiva. Nessun push prima del batch verde; nessuna modifica workflow per questo retry.

**0285 completa localmente — recupero task consegnati prima di dismettere isolate:** il
runtime conserva Arc degli slot dei task consegnati; l'event loop riceve solo un payload
opaco Arc dello stesso slot. Esecuzione consuma Option<Task> una sola volta; close svuota
pendenti e slot consegnati mentre l'isolate è ancora valido, senza distruttori sotto lock.
Payload tardivi restano validi ma vuoti, e consegne rifiutate sono recuperate allo shutdown.
Pulizia ammortizzata degli slot completati. Arc/Mutex/Option standard, nessuna dipendenza
né API pubblica nuova. Test reali FinalizationRegistry coprono consegna tardiva e rifiuto;
un test metadati copre512 slot completati. Normale100/2ignorati e JITless99/3ignorati,
negativo senza take allo shutdown fallisce in sicurezza con sola lettura dei metadati.
Log target/task-0285/{task-lifetime,full,jitless,negative-no-reclaim,replay}.log; replay19
patch precedenti +0285 identico, scratch %TEMP%/roves-0285-replay-2558qb3w. Copia testata
ripristinata identica. Task mai accettati dopo chiusura/senza handler o ancora nel timer
mantengono le politiche conservative precedenti: non è una prova di assenza leak globale.
0284 è nel commit locale7bfad886f07/wiki0c5b380;0283 d53e6b51031;0282 7813d44dbc0.
Pubblicato8b536187562/wiki dcfd9bd; CI V839844591657/generale39844592094 attive senza
failure all'ultimo controllo, Android/iOS verdi. Nessun nuovo push mentre il batch è attivo.
Indicazione storica del22ottobre: il file temporaneo di ripresa è stato rimosso il23ottobre.

**0284 completa localmente — task già consegnati all'event loop:** il payload opaco
Dispatchable contiene ora il Task e Weak<Foreground> del runtime originale. DispatchableRun
esegue solo task del runtime destinatario corretto e ancora aperto; task fuori-lifetime o
presentati a un altro runtime non sono eseguiti né distrutti attraverso l'isolate sbagliato.
Lo shutdown chiude il Foreground e svuota la coda pendente prima della dismissione isolate;
post e close serializzano l'inserimento sotto mutex, senza chiamate embedder o distruttori
Task sotto lock. La consegna ShuttingDown prima della chiusura conserva la distruzione
normale. Callback già in volo restano soggette al contratto di lifetime dell'embedder.
Nessuna API pubblica o dipendenza nuova; wrapper opaco interno, Arc/Weak/AtomicBool esistenti.
Due test puri di ownership/lifetime, con runtime reali in sequenza, più suite completa
normal97/2ignorati e JITless96/3ignorati; negativi senza close e senza confronto owner
falliscono in sicurezza. Test reali esistenti foreground/Wasm/finalization passano.
Log target/task-0284/{full,jitless,negative-unretired,negative-wrong-owner,replay}.log;
replay18 patch precedenti +0284 identico, scratch %TEMP%/roves-0284-replay-710oojyq.
Sorgente testato ripristinato identico. Task invalidi mantengono la politica di abbandono
per evitare distruttori su isolate dismessi: nessuna garanzia di assenza leak o misuraRAM.
0283 nel commit locale d53e6b51031/wiki91f35c8; 0282 in7813d44dbc0. Pubblicato resta
8b536187562/wiki dcfd9bd. CI V839844591657/generale39844592094 attive senza failure
all'ultimo controllo; Android/iOS verdi. Continuare sviluppo/commit locali senza push.

**0283 completa localmente — task ritardati e riuso isolate:** il timer non cerca più il
handler per solo indirizzo quando scade un task. Cattura Weak<Registration> all'accodamento;
la rimozione/sostituzione disattiva la registrazione originale. Un vecchio task non viene
reinstradato al nuovo runtime che riusa l'indirizzo. Nessuna dipendenza: Arc/Weak/AtomicBool
standard; API set_handler invariata. Tre test di selezione handler (senza costruire falsi Task
V8) coprono registrazione viva, rimossa e sostituita; negativo senza invalidazione fallisce2/3.
Motore65 test +2doctest normal/JITless, runtime95/94 passati, sorgenti testati identici.
Log target/task-0283/{engine-normal,engine-jitless,runtime-normal,runtime-jitless,
negative-active-registration,replay}.log. Replay di una patch precedente (questo file) +0283
identico, scratch %TEMP%/roves-0283-replay-mjgf5r5p. Nessuna misuraCPU/RAM. Task senza
handler ancora abbandonati senza distruzione per sicurezza; task già consegnati all'embedder
non sono cancellati da questa modifica e richiedono il corretto ordine di shutdown.
0282 è nel commit locale7813d44dbc0/wiki2cc0a62; nessun nuovo push sul batch attivo.

**0282 completa localmente — drain dei job per runtime:** corretto un difetto lifecycle:
DRAIN_PENDING era thread-local e un runtime chiuso con un drain ancora accodato impediva
al runtime successivo sullo stesso thread di accodare il proprio. Ora drain_pending è un
Cell<bool> in RuntimeHooks, inizializzato per ogni runtime e letto/reset dagli stessi percorsi.
Nessuna nuova dipendenza o API pubblica. Test con due runtime consecutivi senza eseguire il
primo drain: prima falliva (secondo runtime zero job), dopo passa. Normale95/2ignorati,
JITless94/3ignorati, copia testata identica. Log target/task-0282/{full,jitless,
negative-thread-flag,replay}.log. Replay55 patch precedenti +0282 dry-run/apply identici,
scratch %TEMP%/roves-0282-replay-xqm6lu1h. Browser/worker reale resta daCI, nessuna misura.
Pubblicato8b536187562/wiki dcfd9bd (0281); regola CI rinforzata nel commit locale
e0478fb5acd. Codex continua direttamente senza Claude come richiesto. Batch pubblicato:
Android39844592038/iOS39844591645 verdi; V839844591657 attiva e generale39844592094
in coda all'ultimo controllo. Non pubblicare0282 prima della conclusione del batch.

**Pubblicazione0281 su richiesta esplicita dell'utente:** codice completo e revisionato nel
commit b2df522a76d, wiki dcfd9bd. L'utente ha chiesto di annullare il batch precedente e
pubblicare subito: richieste cancellazione202 accettate per generale39842644716 e
V839842644668 su1094b88f051. Android/iOS di quel batch già verdi. La cancellazione non è
un esito verde e non convalida0281; verificare le nuove run sul nuovo HEAD dopo il push.
Questa è un'eccezione esplicita per il checkpoint, non un cambio delle regole CI permanenti.

**0281 completata da Codex — stato corrente22ottobre:** l'utente ha corretto la quota Codex
(97% residuo) e chiesto implementazione diretta senza Claude. Nessuna nuova invocazione Claude.
HEAD pubblicato1094b88f051/wiki9fbd176,0279/0280 già pubblicate;0281 completa localmente.
Il vecchio WIP versionato sotto docs/handoff resta archivio storico, NON va riapplicato.
RealmData è cppgc: context→holder→RealmData→context è un ciclo tracciato; registro debole,
radici esplicite current/saved, RealmSwitch RAII per callback, snapshot forte in iterazione.
Principal condiviso azzerato e rilasciato esattamente una volta a finalizzazione o shutdown.
Il realm iniziale resta forte per gli intrinseci. Corretto un difetto del WIP: holder e cache
creati ora nel context di destinazione, evitando che il nuovo realm trattenga il chiamante.
Nessuna nuova dipendenza: rusty_v8/cppgc esistenti. API pubbliche invariate; puntatori Realm
validi solo se contesto/oggetto/realm attivo è raggiungibile, documentato nel codice.
Test normali94/2ignorati e JITless93/3ignorati; dieci nuovi test, negativi sicuri per registro
forte e mancanti radici saved, regressione wrapperWindow rinforzata. Log target/task-0281/
full.log e jitless.log; copia testata identica. Replay59 patch precedenti +0281 dry-run/apply
identici (12file), target/task-0281/replay.log, scratch %TEMP%/roves-0281-replay-zmy2m0rr.
Compilazione Servo/browser reale ancora daCI; nessuna raccolta pagina o misuraRAM dichiarata:
Servo mantiene global/mappe moduli; JSON linkati mai valutati restano forti. Restano aperti
cacheFunction0273, navigazione WindowProxy, incumbent, workers/termination/shutdown e WPT.
CI su1094b88f051: Android39842644914 e iOS39842644717 verdi; generale39842644716 e
V839842644668 ancora in corso. Nessun altro push prima della conclusione del batch.

**Handoff storico:** il file temporaneo di ripresa è stato rimosso il23ottobre; usare lo stato corrente in cima.
Design, istruzioni e WIP0281 sono ora versionati sotto docs/handoff/, non soltanto target/.
0279/0280 complete nei commit45d88a513e7/196ee7a5faa;0281 resta fuori dai sorgenti.
L'utente ha chiesto commit/push di salvataggio per continuare direttamente con Claude
senza questa chat, dato il quasi esaurimento della quota settimanale Codex.
Batch47a5 terminato: V8/Android/iOS verdi; generale12/13verde, Steam119978506378
scaduto nel bootstrap90min prima della compilazione. Checkpoint generale non verificato;
nuovo push esplicitamente richiesto, non una dichiarazione di tutte le CI verdi.

**Stop per quota effettiva Claude durante0281 — 22 ottobre:** HEAD locale completo
`196ee7a5faa` (0279 e0280 revisionate), wiki locale `9fbd176`; pubblicato resta
`47a5cd351e1` / wiki `c9bcc84`. 0281 incompleta e non verificata: nove sorgenti ripristinati
esattamente a HEAD, WIP salvato fuori dai sorgenti pubblicabili in
`target/0281-incomplete-realm-registry.patch`. Nessun commit o patch di produzione0281.
Log `target/claude-0281-realms.log`; piano `target/task-0281-plan.txt`, design
`target/task-0280-realm-design.md`. La CLI ha segnalato session limit con reset00:10
Europe/Rome: nessun retry, cambio modello o attesa automatica, ripresa solo su richiesta
utente. Non applicare e pubblicare il WIP come completo: mancano test e replay finale.
Il design usa cppgc RealmData/context holder, registro debole, radici current/saved,
RealmSwitch RAII e rilascio principal esattamente una volta. 0280 ha già sostituito il Global
immutabile di ClassBox con TracedReference: non rifare quel prerequisito.
Prima di riprendere0281 rileggere patch e contratto di rooting; verificare current/saved,
constructor publication, finalizer/shutdown order, principal hooks, callback reentrancy e
normal/JITless con negativi sicuri. Nessuna raccolta realm/pagina è ancora dimostrata.

**CI finale del push47a5cd351e1:** Android39822504026 e iOS39822504229 verdi; generale
39822504002 ancora attiva (all'ultimo dettaglio12/13verdi, inclusiDEB e portableLinux,
solo steam119978506378attivo). V839822504067 ancora attiva al retry del solo bootstrap
LinuxJITless, dopo timeout apt30min prima della compilazione. Non pubblicare altri commit
finché il batch non permette il push. Nessuna automazione periodica o polling creati.

**Checkpoint Codex dopo revisione0279:** pubblicato47a5cd351e1/wikic9bcc84 (0275–0278).
0279 completa e revisionata in locale: rooting test corretto, normal82/JITless81 passati,
cinque file produzione/test/replay identici; wiki locale f0cc01c. Patch0279 aggiornata.
CI pubblicata: Android39822504026 e iOS39822504229 verdi; generale39822504002 ancora
attiva; V839822504067 fallita soltanto sul bootstrapLinuxJITless119977154180:
apt stava scaricando pacchetti dal mirror Azure e lo step è scaduto a30min, prima della
compilazione. Codex ha richiesto un solo retry dei job falliti, motivato da questo log.
Nessun nuovo push finché questo batch non termina e i controlli mancanti non passano.

**0280 (completa in locale, non committata, sopra 45d88a513e7 / wiki f0cc01c) — solo
prerequisito, non il design realm:** il prototipo fissato da `JS_SetImmutablePrototype` non è più
un `v8::Global` (radice incondizionata) ma un `TracedReference<Value>` nel `ClassBox`, tracciato
in `ClassBox::trace` (stesso meccanismo cppgc di `cell.rs`, nessuna dipendenza). Il flag booleano
è sostituito dalla presenza del riferimento; i getter restituiscono un `Local` dallo scope, il
`RefCell` non resta preso durante allocazioni V8. Semantica invariata (trap Proxy
get/setPrototypeOf, `JS_SetPrototype`); RealmData/registro realm non toccati. Test
`immutable_prototypes_live_with_their_object_and_die_with_it` e `mutable_prototypes_stay_replaceable`:
roves-js normal 84/2 ignorati, JIT-less 83/3, warning 772/772 (`target/task-0280-prototype/`).
Negativo scratch con `Global` incondizionato: fallisce in modo sicuro (0 finalizzati su 2);
negativo senza trace: segfault (trace necessario). Copia testata identica ai sorgenti; replay di
52 patch precedenti identico a HEAD e 0280 applicata identica (scratch
`%TEMP%/roves-0280-replay-PUbJ`). Nessuna raccolta a livello pagina né risparmio RAM dichiarati;
il design realm completo (`target/task-0280-realm-design.md`) resta pendente per 0281. WIP 0273
conservato.

**Regola corrente — 22 ottobre:** l'utente ha eliminato i tetti di25/50 chiamate e
l'obbligo di registrare consumo e lavoro residuo per incarico. Prosecuzione autonoma,
un solo Claude attivo e incarichi circoscritti; restano revisione, vincoli CI e stop alle
quote effettive. I conteggi e i limiti descritti sotto sono storici. Nessuna nuova contabilità.
**Baseline pubblicata:** HEAD 47a5cd351e1 (wiki c9bcc84) contiene 0275-0278; le note
"non committata" di 0277/0278 sotto sono storiche. Batch nuovo, ultimo controllo Codex: V8
39822504067 in coda, generale 39822504002 attiva, Android 39822504026 e iOS 39822504229
verdi. La correzione 0278 del vecchio fallimento Linux deb (batch d434323c76c, run
39790271422 job 119865402266) non è ancora confermata. Nessun push finché il batch non termina.

**0279 (completa in locale, non committata, sopra 47a5cd351e1 / wiki c9bcc84):** record JSON
raccoglibili secondo `target/task-0279-design.md`. Voce JSON debole prima del link e dopo gli
steps (V8 li esegue una sola volta: `kEvaluated` o `kErrored`, terminazione inclusa); radici
temporanee `Modules::linking` per i record sintetici risolti, con `LinkSegment` RAII su ogni
uscita di `ModuleLink` (anche annidato o fallito) che rende forte chi V8 lascia `Instantiated`.
Caso forte residuo esplicito: un JSON lasciato `Instantiated` resta forte finché una valutazione
successiva non ne esegue gli steps; se nessuna lo fa (grafo abbandonato dopo il link, o valutazione
fallita prima di raggiungerlo e mai ripetuta) record, valore e private restano fino allo shutdown
anche se irraggiungibili. Contratto: `SetModulePrivate` può fare GC (alloca l'holder), quindi il
chiamante tiene il record radicato (Servo usa `rooted!` e non imposta private JSON). Private JSON
in `PrivateData` cppgc riusato, holder sul placeholder (`PrivateKey::JsonModulePrivate`),
voce = solo `WeakPersistent` per lo shutdown; vale finché vive il record, non la metadata di un
grafo sorgente. Test roves-js normal 82/2 ignorati, JIT-less 81/3, warning 772/772; 9 test
nuovi inclusi i negativi `#[cfg(test)]` (voci forti, niente strengthen, niente radici di link,
private nativo) e negativo scratch senza trace del holder (`target/task-0279/*.log`). Revisione
del rooting: nei test JSON ogni record usato dopo un'allocazione V8 è radicato (drop esplicito
dove serve non radicato); i record JSON arrivano all'hook di resolve solo da un registro debole
di test su `Modules`, quindi uno raccolto fallisce la risoluzione invece di restare pendente;
radicato anche `data.json` del test preesistente `module_graphs_link_evaluate_and_import_dynamically`.
Stessi esiti 82/2 e 81/3, warning 772/772 (`target/task-0279/rooting-review-{full,jitless}.log`);
copia testata identica ai sorgenti; il negativo scratch resta valido (test non toccati). Replay di
54 patch precedenti identico a HEAD, 0279 applicata identica
(`target/task-0279/rooting-review-replay.log`, scratch `%TEMP%/roves-0279-rooting-replay`). README e
wiki `shell.mdx` aggiornati (wiki non committato). Nessuna misura CPU/RAM; le mappe moduli di
Servo continuano a radicare i record. Analisi originale, ancora valida:
Fonti V8 152.2.0: namespace prima del link = ApiCheck fatale; private sul namespace non
estensibile solo per una tolleranza che V8 dichiara da rimuovere; prima del link il grafo V8 non
referenzia il modulo JSON (solo dopo `PrepareInstantiate`). L'edge pulito esiste in V8
(`host_defined_options` di `CreateSyntheticModule`) ma rusty_v8 152.2.0 non lo espone: lacuna
API registrata: con il binding la voce JSON potrebbe diventare sempre debole.

**0278 (completa in locale, non committata):** lo smoke Xvfb ha fallito la ricerca
finestra dopo circa10s pur avendo un processo ancora vivo e risultati pagina/gamepad/save.
Il ritardo della finestra è un'ipotesi coerente con il difetto riprodotto dal test, non una
causa definitivamente provata; una regressione applicativa non è esclusa dalla sola natura
del commit. Correzione: scadenza unica 60 s
(`ROVES_XVFB_READY_TIMEOUT`) per finestra e `[roves-input] ready`; fallback per nome solo
sul display privato creato dallo script; cleanup TERM→5 s→KILL; in `test.yml` il loop Linux
richiede anche il marker `ok` prima di fermare il wrapper. Nessun controllo indebolito.
Test con comandi finti `support/xvfb_window_cycle_smoke_test.sh`: 7/7 in WSL e Git Bash,
script precedente 1/7 (`target/task-0278/*.log`). Nessuna patch: file solo-repository.
Codex ha letto il log grezzo tramite connector GitHub: stdout aggregato al termine,
quindi non fornisce il tempo effettivo di comparsa finestra. Conferma reale solo dalla prossima CI Linux deb/portable.

**0277 (completa in locale, non committata, sopra ea7a37e45a6 / wiki 2629eed):** il valore
JSON di un modulo è una proprietà privata V8 (`PrivateKey::JsonModuleValue`) del placeholder
del record; rimossi `ModuleKind::Json.value` (`v8::Global` forte indipendente) e il suo clone in
`json_module_steps`. `CompileJsonModule1` registra il record forte, poi `set_private`; se fallisce
o lancia rimuove l'entry (`forget`) e restituisce null. Gli steps copiano il record fuori dal
borrow, leggono il private (`has_private` distingue l'assenza) e lanciano TypeError se manca.
Registro JSON ancora forte, `HostPrivate` invariato: nessuna raccolta JSON/realm/pagina, nessuna
misura CPU/RAM (costo: un private per record JSON, una lettura per valutazione). Test roves-js
normal 73/2 ignorati, JIT-less 72/3, warning 772/772 (`target/task-0277/{full,jitless}.log`);
nuovo `json_module_values_live_on_their_records`; negativo scratch senza la proprietà → fallisce
in valutazione senza crash (`negative-no-value.log`); copia testata identica ai sorgenti. Replay
di 52 patch precedenti (inclusa 0276) identico a HEAD e 0277 applicata identica all'albero
(`target/task-0277/replay.log`, l'etichetta "0274" è ereditata dallo script; scratch
`%TEMP%/roves-0277-replay-coUt`). CI del batch d434323c76c non ricontrollate da Claude.
Codex ha revisionato il diff e confrontato tre file produzione/copia testata/replay: identici.

**Checkpoint revisionato 0276 (22 ottobre):** completata dopo risoluzione dei permessi
con allowedTools espliciti della sessione (nessun bypassPermissions, zero nuovi dinieghi).
35 chiamate cumulative (17+18), nei50 autorizzati. Test normal72/2 ignorati, JITless71/3,
negativo e replay51 patch precedenti più0276 verificati; Codex ha confrontato i sei file
produzione/copia testata/replay, identici. Wiki locale2629eed. Nuove regole autonome in
AGENTS.md:25 iniziali, una estensione motivata fino50, stop a quota effettiva.
Ultimo controllo del batch pubblicato d434323c76c: V8, Android e iOS verdi; generale
39790271422 ancora attiva con Linux portable119865401920, Windows MSI119865402032 e
Linux deb119865402266, altre10 job verdi. Nessun push finché il batch non termina.
Nessun polling o automazione creati. I paragrafi sullo stop0276 sotto sono storici.

**Nuova autorizzazione 22 ottobre:** prosecuzione autonoma approvata, inclusa una sola
estensione motivata per incarico fino a50 chiamate cumulative. 0276 riparte da17, massimo33
aggiuntive; prima diagnosticare i permessi, senza aggirare rifiuti di revisione automatica.
Lo stop sotto è storico; restano i vincoli CI e quota, nessuna automazione periodica.

**Stop operativo 0276 — 22 ottobre:** 0275 revisionata e salvata nel commit locale
`a578da17667`, wiki locale `ef9fb35`; non pubblicata. Pubblicato resta `d434323c76c`.
0276 (funzioni native definite deboli) è incompleta: Claude ha usato 17/25 chiamate ma
la sessione non interattiva `acceptEdits` ha negato i comandi shell di scrittura, chiedendo
approvazione. Nessun sorgente modificato, nessun test eseguito. Bozza non verificata in
`target/task-0276/src.diff` e `edit_src.py`; prompt `target/task-0276-plan.txt`, log
`target/claude-0276-weak-defined-functions.log`. Il diff richiede anche due rimozioni
`defined_functions` in jsapi.rs/rust.rs; non applicarlo né pubblicarlo come completo.
Allo stop erano da chiarire permessi e budget; la nuova autorizzazione sopra consente
la ripresa con conteggio conservato. Nessuna sostituzione con implementazione Codex. Ultima lettura CI:
Android39790271798 e iOS39790271386 verdi, V839790271552 e generale39790271422 ancora
in corso, nessun nuovo push consentito. Nessuna automazione creata.

**Ripresa estesa e completamento 0274 — 22 ottobre 2026:** l'incarico 0274 si era fermato a
25 chiamate; l'utente ha poi autorizzato esplicitamente una sola estensione dello stesso incarico
(+25, massimo cumulativo 50, budget non azzerato;47 chiamate verificate). Nessun rilancio automatico, retry di quota o
automazione periodica; il ciclo autorizza nuovi incarichi circoscritti; solo un
incarico incompleto oltre soglia richiede una nuova decisione esplicita sul budget. Regole in AGENTS.md; [calibrazione](AGENT_WORKFLOW.md).

**Baseline attuale (22 ottobre):** HEAD pubblicato d434323c76c con 0274 completa; batch
precedente a538387b86e tutto verde. CI in corso su d434323c76c: V8 39790271552, generale
39790271422, Android 39790271798, iOS 39790271386; nessun push finché non sono concluse
(Codex gestisce la CI). L'intestazione 0274 sotto descrive lo stato prima del push.

**0276 (completa in locale, non pubblicata, sopra 0275 a578da17667 / wiki ef9fb35):**
funzioni native definite dalla specifica deboli come quelle dinamiche: solo `v8::Weak` più
`WeakCell` sullo stato `NativeFunction`; rimossi `NativeFunction.cell`, il parametro
`defined`, la mappa `defined_functions` (scorciatoia per indirizzo di cella, rischio di
riuso) e `trace_native_functions` con la chiamata in RootState. La funzione vive finché la
raggiunge il proprietario (oggetto, prototipo, globale); slot riservati come proprietà private
V8; stato eliminato dallo sweep ammortizzato esistente. JIT info, costruttore, slot riservati
e dispatch invariati; la lettura della JIT info ora entra in V8 (costo non misurato). Realm,
JSON e mappe Servo restano forti: nessuna raccolta a livello pagina né risparmio RAM. Test
roves-js normal 72/2 ignorati, JIT-less 71/3 (`target/task-0276/{full,jitless}.log`, righe
warning 772/772); negativo scratch con `v8::Global` forte → nuovo test fallisce
(`target/task-0276/negative-strong-root.log`); copia testata identica ai sorgenti. Replay di
51 patch precedenti (inclusa 0275) identico a HEAD e 0276 applicata identica all'albero
(`target/task-0276/replay.log`, scratch `%TEMP%/roves-0276-replay-l3cj`). CI di d434323c76c
(V8 39790271552, generale 39790271422) ancora da controllare da Codex; Android 39790271798 e
iOS 39790271386 verdi. Incarico Claude: 35 chiamate su 50 (estensione autorizzata).

**0275 (completa in locale come a578da17667, non pubblicata):** record dei moduli sorgente raccoglibili. Il
`PrivateData` cppgc dei metadati sorgente (holder nella WeakMap catturata, sotto il token host)
traccia anche la cella del record; `CompileModule1` crea sempre i metadati del modulo sorgente
(anche con private undefined) e solo dopo rende debole (`WeakCell`) la voce della tabella
`Modules`, che fino ad allora tiene il record durante le allocazioni. Il placeholder fissa
ancora il token: token→holder→metadati→record→modulo V8→token è un ciclo raccolto quando nessun
record radicato, namespace, funzione esportata, grafo statico o import dinamico pendente lo
raggiunge; il private nativo è rilasciato una volta dal finalizzatore, prima della distruzione
del runtime. `trace_modules` traccia solo le voci forti (JSON e moduli non ancora ancorati) e i
private JSON come prima; pulizia delle voci morte ammortizzata sulle inserzioni (schema delle
celle internate). JSON resta forte (nessun token V8); registri realm/globali e module map di
Servo trattengono ancora i record: nessuna raccolta a livello pagina né risparmio RAM, nulla
misurato. Costo: un holder cppgc per modulo sorgente anche senza private. Test roves-js normal
71/2 ignorati, JIT-less 70/3, warning 772 invariati
(`target/v8-incumbent-audit/task-0275-{full,jitless}.log`); negativi scratch
(`task-0275-negative.log`): radice forte → 3 test falliti, edge del record mancante → 1. Copia
testata identica ai sorgenti; replay di 48 patch precedenti identico a HEAD e 0275 applicata
identica all'albero (`target/task-0275/replay.log`, scratch `%TEMP%/roves-0275-replay-cHil`).
Nessuna modifica a fixture browser, CLI, workflow o pin. Incarico Claude: 21 chiamate su 25.

**0274 (pubblicata in d434323c76c, CI browser pendente):** WIP riapplicato senza modifiche in tests.rs e
script_module.rs; patch `patches/servo-v0.5.0/0274-owned-import-meta-resolve.patch` (sei file:
i due sorgenti e la fixture browser in support/v8-smoke). L'holder JSClass nascosto possiede un
`Rc<ModuleScript>`, ne traccia l'owner DOM, azzera lo slot prima del rilascio e non nasce con
private undefined. Simboli frontend verificati sui binding roves-js locali (GCContext,
JS_Get/SetReservedSlot, JSClass/JSClassOps, ObjectValue, wrappers2::JS_NewObject): nessuna
correzione; compilazione completa del frontend pendente in CI. Test isolati riusati, 67 normal
e 66 JIT-less: `target/v8-incumbent-audit/final-0274-{full,jitless}.log`; la copia roves-js/src
è identica ai sorgenti. Regressione browser: gc-module.js esporta `import.meta.resolve`;
gc-lifecycle.html verifica in entrambi i frame, prima e dopo la GC, l'URL assoluto relativo al
modulo (non alla base `missing-base/` del frame) e il realm del frame; run.sh richiede i marker
esatti `module resolve true` e `module resolve after collection true`. Runner in WSL 9/9:
`target/0274-continue/runner-wsl.log`; esito browser reale pendente in CI. Replay delle 57 patch
precedenti (inclusa 0272) su pristine v0.5.0 identico a HEAD, dry-run/apply 0274 identico
all'albero di lavoro: `target/0274-continue/replay.log`. Restano forti realm, module map/registro
; la cache Function (0273) resta un problema distinto e il registro debole è0275; nessuna raccolta di moduli dimostrata, nessuna misura.

**CI precedente:** batch a538387b86e tutto verde (dettagli nella tabella). Codex revisiona,
esegue commit/push e controlla una volta le run del nuovo push.
Il prossimo controllo deve essere motivato, non periodico. Nessun lavoro parallelo sullo stesso
problema delegato, nessun nuovo incarico oltre budget senza autorizzazione.

**Stato pubblicato prima di0274:** HEAD a538387b86ebe1d6f206a305da29cdb722034cbf, con codice 0271
(683686b2feb) e 0272 (6920417ad65); wiki 630c487. Le modifiche documentali di questo stop
sono locali e non avviano un nuovo push. Il codice 0273 non è pubblicato né completo:
backup locale riapplicabile `target/0273-incomplete-native-traps.patch`; sorgenti ripristinati
al codice completo di0272. La diagnosi della cache è descritta in fondo al piano.

Ultima lettura aggregata della CI sullo SHA a538387b86e:

| Workflow | Run | Esito |
| --- | --- | --- |
| V8 | [39744601812](https://github.com/DRincs-Productions/roves/actions/runs/39744601812) | Successo |
| Generale | [39744601765](https://github.com/DRincs-Productions/roves/actions/runs/39744601765) | Successo al retry (13/13) |
| Android | [39744601730](https://github.com/DRincs-Productions/roves/actions/runs/39744601730) | Successo |
| iOS | [39744601702](https://github.com/DRincs-Productions/roves/actions/runs/39744601702) | Successo |

Il primo tentativo DMG (job 119710948660) era una mancata assegnazione runner/capacità
macOS ARM64; il retry dei job falliti è terminato con successo (13/13).
Il precedente batch e709fb3f10b era tutto verde: V8 39737640415 (10/10), generale
39737640441 (13/13), Android 39737640406 e iOS 39737640414. I log Linux normal
119686997929 e JIT-less 119686998238 confermano GC e module owners di0269/0270.

Le sezioni sotto sono uno storico dell'implementazione; le indicazioni locali/CI pendente
riferite a0271/0272 vanno lette rispetto alla nuova run V8 sopra. Restano aperti cache Function,
retention realm/moduli, navigazione, incumbent, copertura WPT e validazione finale.
I commit solo documentali possono non avviare V8: verificare sempre head_sha delle run.

### Aggiornamento 22 ottobre 2026 — fatti confermati

| Verifica sul codice pubblicato | Run | Esito |
| --- | --- | --- |
| V8, incluse 0267 e 0268 | [39677755124](https://github.com/DRincs-Productions/roves/actions/runs/39677755124) | Verde 10/10 dopo il retry macOS (attempt 2) |
| Replay patch, build e bundle | [39677754946](https://github.com/DRincs-Productions/roves/actions/runs/39677754946) | Verde 13/13 dopo retry macOS (attempt 2) |
| Android | [39677755107](https://github.com/DRincs-Productions/roves/actions/runs/39677755107) | Verde |

I log Linux dei job 119491949605 e 119491949671 (normal e JIT-less) contengono entrambi
`ROVES-V8 csp constructor graph true true true` e il successo di pref-jitless: i criteri
browser Linux di 0267 e 0268 sono confermati. Il retry V8 è ora verde 10/10,
compresi tutti i runtime e la build macOS; gli smoke browser Windows/macOS restano saltati.
Il retry generale è concluso verde 13/13, inclusi SDL, tutti i bundle e smoke Steam.
Le verifiche del push precedente consentono quindi il prossimo push secondo AGENTS.md. iOS sul commit documentale 18b57a227fa è
verde dopo retry (run 39677942329). Le cancellazioni iniziali erano mancate assegnazioni
di runner; il loro superamento è verificato dai retry, non presunto.

**0269 (pubblicata, browser Linux normal/JIT-less verificato):** smoke GC browser opt-in che riusa
`TestUtils.gc()` di Servo. `TestUtils.webidl` perde `skip-unless CARGO_FEATURE_TESTBINDING`;
`dom/mod.rs` dichiara `testutils` fuori dal gate `testbinding` (rimosso da `testing/mod.rs`);
esposizione ancora dietro `dom_testutils_enabled`, default false. `run.sh` avvia un processo
separato con `--pref dom_testutils_enabled=true` su `gc-lifecycle.html` via HTTP loopback
(scadenza 20 s, marker `ROVES-V8 gc result`); `test.html` deve stampare
`ROVES-V8 testutils default undefined`. La pagina verifica nodi trattenuti con expando e
closure, raccolta effettiva via WeakRef di nodi DOM non collegati e cicli JS, `import()` da una
funzione compilata da uno script classico rimosso (42) e raccolta della funzione abbandonata.
Contratti runner WSL 9/9, test generatore V8 75/75, binding TestUtils emesso senza testbinding,
replay di 17 patch precedenti e dry-run/apply 0269 verificati. Compilazione e smoke
browser Linux normal/JIT-less confermati dalla run 39737640415. Non osserva i finalizzatori nativi né risolve retention realm/moduli o
navigazione; nessuna misura prestazionale.

**0270 (pubblicata, browser Linux normal/JIT-less verificato):** identità per sorgente dei moduli V8.
Corretti tre difetti: (1) il lookup del private del chiamante prendeva `entries.first()` del
bucket per hash d'identità, che può collidere; (2) la cache di compilazione V8 condivideva lo
stesso script (id `Some(7), Some(7)`) fra moduli con sorgente e nome identici, sovrascrivendo
`by_script_id`; (3) `JS_GetScriptedCallerPrivate` leggeva solo i metadati classic. Ora
`source_origin` (ex `classic_origin` di 0266) dà a ogni modulo un Symbol host non registrato,
`ModuleEntry` salva lo script id alla compilazione e il membro del bucket si sceglie per script
id; `script_impl::caller_private` è condiviso da `JS_GetScriptedCallerPrivate` e import
dinamico, non nascosto da `HideScriptedCaller`. Il token del modulo segue la gestione
esistente `DynamicImportSourceToken` fino al settlement. Test `tests::caller_privates` (3):
moduli identici in realm separati prima/dopo GC, collisione deterministica con helper
`#[cfg(test)]`, classic/modulo/rientro nativo; ciascun difetto reintrodotto li fa fallire.
roves-js normal 58/2 ignorati, JIT-less 57/3, warning 771 invariati. Fixture browser: due iframe
`srcdoc` same-origin importano lo stesso `gc-module.js` e `importOwner()` deve leggere il
proprio token in `gc-owner.js` prima e dopo una raccolta (marker `ROVES-V8 gc module owners`).
Contratti runner WSL 9/9; replay di 53 patch precedenti e dry-run/apply 0270 con SHA identici.
Claude non aveva accesso MCP; Codex ha verificato separatamente la sola fase module owners
con Chrome DevTools MCP: Chrome155 Windows x64, pagina loopback gc-lifecycle.html,
scheda aperta in background ma visibilityState visible, headless non determinato.
Risultato owners left/right; TestUtils undefined. Nessuna GC Chrome né misura prestazionale.
GC/browser Linux normal/JIT-less e build servoshell confermati dalla run 39737640415.
Possibile minore riuso della cache di compilazione per moduli identici, non misurato. Restano
aperte retention strutturale realm/moduli e identità di navigazione.

**0271 (completa localmente, non pubblicata, browser CI pendente):** referrer di `import()` dalle
host options V8. Riprodotto prima della correzione: con 0270 l'eval diretto in script classici e
moduli dava owner e importer `-1` (lo script eval ha un id non registrato). Fonti primarie: V8 152
passa `GetEvalOrigin()` come referrer e `GetCurrentHostDefinedOptions` usa lo stesso origin;
SpiderMonkey copia il private del chiamante nell'eval (`UpdateDebugMetadata`). Ora `dynamic_import`
usa il token ricevuto (token senza metadati = undefined, mai un altro owner) e `caller_private`
usa `get_current_host_defined_options()`; lookup token→private tramite `WeakMap.prototype.get`
catturato all'avvio e unwrap del holder cppgc. I private dei moduli sorgente riusano
`ScriptMetadata`: rimossi `by_script_id`, `module_private` e il private/retain/release per entry;
JSON separato (`ModuleKind::Json`), chiave `SourceToken`, token fissato anche sul placeholder del
record. In V8 152 il pin è difensivo: un modulo in errore conserva SFI/Script, quindi il negativo
senza pin non fallisce. Divergenza nota: `new Function` non eredita owner (probe `-1` contro 44;
SpiderMonkey eredita); l'eval usa il private corrente dell'origine, non uno snapshot. Test
roves-js normal 61/2 ignorati, JIT-less 60/3, warning 771 invariati; negativi stack-only e `get`
globale falliscono. Fixture: `evalImportOwner()` e `<base>` inesistente nei frame, marker
`ROVES-V8 gc eval module owners` prima/dopo GC; contratti runner WSL 9/9. Replay di 56 patch e
dry-run/apply 0271 identici byte per byte. Chrome MCP non accessibile a Claude; nessuna misura
CPU/RAM/GPU. Costi: holder cppgc per sorgente con private, una proprietà nascosta per record.
Codex ha verificato la sola fase evalImportOwner via Chrome DevTools MCP: Chrome155 Windows x64,
pagina loopback, scheda in background ma visibilityState visible, headless non determinato;
owners left/right, TestUtils undefined. Nessuna raccolta Chrome o verifica browser Roves.

**0272 (committata localmente, compilazione servoshell pendente in CI):** owner di
`ModuleScript` tracciato dalla VM. Prima `ModuleScript.owner` era `Option<Trusted<GlobalScope>>`,
radice esterna mantenuta finché esisteva il private `Rc`. Nuova estensione roves-js
`SetScriptPrivateTraceHook` (firma di `SetScriptPrivateReferenceHooks` invariata): `HostPrivate`
cattura valore, release e trace quando il private viene impostato, condiviso da `PrivateData` e dai
private JSON; il valore è tracciato come prima e l'hook solo se non undefined. Sostituzione: retain
del nuovo, installazione, poi release del vecchio; clear/shutdown installano `NONE` prima del
release. `trace_modules` copia i private JSON fuori dal borrow prima degli hook. Servo:
`ModuleScript` deriva `JSTraceable` con `owner: Option<Dom<GlobalScope>>`, hook
`host_trace_top_level_script` registrato in `EnsureModuleHooksInitialized`; `DomRoot` del global fino
all'aggancio del private e `Dom::as_rooted` nelle letture. Classic ancora `owner: None`;
`ModuleFetchListener.owner` resta `Trusted`. Test roves-js normal 64/2 ignorati, JIT-less 63/3,
warning 771 invariati; nuovi `tests::traced_privates` (ciclo owner→funzione→sorgente→private,
controllo senza hook, modulo sorgente e JSON con replace/undefined/re-set/shutdown, nessun trace
dopo release). Negativi: senza chiamata all'hook falliscono 2 test; JSON senza hook fallisce 1.
Replay di 48 patch su v0.5.0 pristine identico a HEAD; dry-run/apply 0272 identici. **Ambito
parziale:** la tabella `Modules` e i registri realm restano radici forti, quindi realm e moduli non
diventano raccoglibili; nessun risparmio RAM dimostrato (non misurato). Rischio futuro annotato:
`import.meta.resolve` conserva il private grezzo in uno slot nativo senza riferimento. Nessuna
prova browser, nessun MCP Chrome per Claude, nessuna misura CPU/RAM/GPU.

### Ordine operativo alla ripresa del 22 ottobre (storico)

1. Leggere AGENTS.md e questo riepilogo; verificare stato Git e SHA delle run.
2. Controllare tutte le CI del batch e709fb3f10b elencate sopra. Analizzare e correggere
   eventuali errori prima del prossimo push; attendere la conclusione di tutte le run.
3. Confermare nei log Linux normal/JIT-less i marker GC e proprietari dei moduli di 0269/0270.
   Gli smoke Windows/macOS sono saltati dalla workflow: le build non equivalgono a prove browser.
4. Pubblicare 0271 completa e revisionata solo quando il batch precedente lo consente.
   0272 è revisionata e committata; pubblicarla con 0271 quando la CI lo consente.
   Prossimi incarichi: owner di new Function; raggiungibilità della tabella `Modules` (con la
   correzione del private grezzo di `import.meta.resolve`), senza workaround di stack.
5. Continuare con le priorità strutturali: raggiungibilità realm/moduli, identità WindowProxy
   dopo navigazione, callback/incumbent, worker e shutdown. Ogni incarico deve produrre
   una modifica coerente, completa e verificabile prima di passare al controllo CI.
6. Chiudere le verifiche WPT selezionate, applicazioni reali Pixi/Three, prestazioni e
   artefatti; solo dopo valutare release e aggiornamento pin dei consumer.

**Non completato:** migrazione complessiva, binding nativi incumbent, conferma CI dello
import da eval (0271), compilazione del nuovo owner tracciato (0272), owner di `new Function`, retention realm/moduli, identità di navigazione, matrice WPT e validazione finale.
Le stringhe toString dei costruttori Proxy restano diverse da Chrome; non introdurre
una sostituzione globale di Function.prototype.toString per nascondere la differenza.

## Checkpoint di spegnimento — 21 ottobre 2026, sera

Su richiesta dell'utente il lavoro viene fermato e i commit locali vengono pubblicati
senza attendere la CI precedente. Questa è un'eccezione esplicita al normale blocco dei push
durante le run, per poter spegnere il PC; non indica una migrazione completata.

Implementazioni complete da riprendere dai risultati CI:
- 0267: grafo dei quattro costruttori dinamici corretto; runtime normal 55 passati / 2
  ignorati, JIT-less 54 / 3; contratti runner 7/7 e replay verificati.
- 0268: processo browser separato con --pref js_disable_jit=true, marker obbligatorio e
  contratti runner 8/8; replay verificato. Prova browser reale ancora pendente.
- AGENTS.md: delega implementativa a Claude e regole di lavoro/pubblicazione durante la CI.

Ultima lettura della CI precedente (codice 8442f4998b9, prima di 0267/0268):
- V8 39636431164: nove job verdi; build macOS ancora in coda nel retry.
- Generale 39636431492: undici job verdi, due bundle macOS cancellati.
- iOS 39670211302 (commit d268a91932c): build e preparazione release verdi,
  firma smoke cancellata; Android verde.
Le cancellazioni macOS consultate riportano mancata assegnazione del runner e capacità
ARM64 limitata. Non confonderle con failure di compilazione; le verifiche mancanti restano aperte.
Il push di questo checkpoint avvia nuove run e può sostituire quella V8 ancora in coda:
domani controllare prima i risultati del nuovo HEAD e correggere eventuali errori.

L'incarico successivo sul GC browser è stato interrotto durante l'analisi: nessun sorgente,
fixture o patch 0269 è stato modificato. Candidato studiato: riusare TestUtils.gc già presente
in Servo, mantenendo dom_testutils_enabled false di default e valutando la compilazione del
solo namespace fuori dal feature gate testbinding. Restano da implementare e verificare
il collegamento, la fixture e i contratti. HideScriptedCaller resta al blocco descritto
nell'audit; retention realm/moduli e identità dopo navigazione restano aperte.

## Punto della situazione — 21 ottobre 2026, commit 8442f4998b9

La CI V8 [39636431164](https://github.com/DRincs-Productions/roves/actions/runs/39636431164)
è conclusa: **8 job verdi, 2 macOS cancellati**. Runtime Linux/Windows normal e JIT-less,
runtime macOS normal e tre build servoshell passano. Gli smoke Linux normal/JIT-less
confermano proprietà WindowProxy e matrice Trusted Types true/true/true, anche con 0266.
Gli smoke Windows sono saltati; runtime macOS JIT-less e build macOS non verificati.
La CI generale [39636431492](https://github.com/DRincs-Productions/roves/actions/runs/39636431492)
conferma replay patch, contratti SDL3 e probe Linux/Windows. Probe macOS cancellato,
bundle e smoke Steam saltati: gli artefatti non sono verificati da questa run.

Restano aperti identità dopo navigazione, completamento SOP/WPT, retention realm/moduli,
stress GC browser, applicazioni reali e artefatti. Gli smoke ordinari non dimostrano
raccolta durante navigazioni ripetute né risparmio RAM. L'ultima matrice interamente verde
resta 39595584589 sul precedente commit dfa367ed8ee.
Ripresa serale: i retry confermano tutti i sei runtime V8, compreso macOS JIT-less;
la build V8 macOS è ancora in coda. Il retry SDL3 macOS è verde e ha sbloccato i bundle.
La firma iOS è stata nuovamente cancellata prima di ottenere un runner: annotazioni
GitHub «The job was not acquired by Runner of type hosted even after multiple attempts»
e capacità macOS ARM64 limitata. Un altro retry è in corso. Nessun nuovo push finché
le verifiche precedenti non sono concluse e risolte, secondo AGENTS.md.
La patch 0267 è verificata localmente e pubblicata nel push b50c6d99ce1; CI nuova pendente.
La patch 0268 (probe browser della preferenza `js_disable_jit`) è verificata solo nei
contratti del runner; il probe browser reale attende la CI.

Le diagnosi storiche seguenti vanno lette alla luce di questo aggiornamento.

Rilevazione sul commit `273c1d9db2e` (`main`) e sulla CI avviata dal rollback
`a26cb6ca39b`. Il PAT condiviso in chat ha restituito `401 Bad credentials`; non è stato
salvato né riutilizzato. Run e job sono stati controllati tramite il connettore GitHub.
**La migrazione resta incompleta.** L’utente ha autorizzato di continuare l’implementazione e
ha indicato di preferire build pesanti nella CI online dopo lo sviluppo, per ridurre il carico
sulla macchina locale.

### Risultati effettivi dell’ultimo commit

| Workflow | Esito conclusivo | Evidenza |
| --- | --- | --- |
| [V8 migration checks — 39630711476](https://github.com/DRincs-Productions/roves/actions/runs/39630711476) | 7 verdi, 3 macOS cancellati | Commit d5e8e5584c0: tutti i runtime Linux/Windows e tre build verdi. Smoke Linux normal/JIT-less confermano proprietà WindowProxy e matrice Trusted Types true/true/true. MacOS non verificato. |
| [V8 migration checks — 39625090343](https://github.com/DRincs-Productions/roves/actions/runs/39625090343) | 7 verdi, 3 macOS cancellati | Commit 438d3a30b54: runtime Linux/Windows, entrambe le build e smoke Linux, build Windows verdi. Nessuna compilazione/verifica macOS di questa run. GitHub segnala incidente Actions/webhook; non attribuire le cancellazioni al codice senza evidenze. |
| [V8 migration checks — 39595584589](https://github.com/DRincs-Productions/roves/actions/runs/39595584589) | Verde, 10/10 | Sei runtime e quattro build verdi. Smoke Linux normal/JIT-less: prototipi, trusted Function 42/guardie, nove suite, CSP Wasm e navigazione passano. Smoke browser Windows/macOS saltati. |
| [Build Servo Test Binaries — 39595584650](https://github.com/DRincs-Productions/roves/actions/runs/39595584650) | Verde, 13/13 | Replay patch, SDL3, bundle portable/installer e smoke Steam verdi. |
| [V8 migration checks — 39590810430](https://github.com/DRincs-Productions/roves/actions/runs/39590810430) | Fallita: 8 verdi, 2 smoke Linux falliti | Tutti i runtime e le build compilano. Window immutabile confermato normal/JIT-less; iframe carica ma prototipo false/true. Trusted Function ancora EvalError (0255 non inclusa). Le nove suite, CSP eval/Wasm e navigazione passano. |
| [V8 migration checks — 39587009751](https://github.com/DRincs-Productions/roves/actions/runs/39587009751) | Fallita: 8 verdi, 2 smoke Linux falliti | Tutti i 6 runtime e tutte le 4 build servoshell verdi. Le 9 suite passano nelle due modalità. Iframe append/load/contentWindow presenti; prototipo cross-origin false/false. Trusted Function fallisce; eval 42, CSP Wasm e navigazione passano. |
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
   script. La patch 0252 è ritirata. La nuova patch 0253 riusa un prototipo
   creato dal FunctionTemplate V8, consentendo il setup canonico senza sostituirlo; verifica
   browser CI pendente.
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
  richiede ancora una verifica dedicata, distinta dalla feature Cargo JIT-less. La patch 0268
  aggiunge quel probe browser in un processo separato; esito CI ancora pendente.
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
  Scansione 23 ottobre delle funzioni pubbliche con corpo vuoto o costante (15 candidati,
  script in chat, non versionato): difetti trovati e corretti `HideScriptedCaller` (0289) e,
  seguendo `RunScriptEnvironmentPreparerClosure`, l'assenza di entry per le callback di
  cleanup FinalizationRegistry (0294, lato Servo). No-op motivati: opzioni JIT/Ion,
  compartment, classi standard lazy, hint JobQueue, SetBuildId, InitializeMemoryReporter
  (about:memory senza dati V8), trace hook del global (cppgc), errori numerati SpiderMonkey.
  Seconda scansione (funzioni pubbliche che ignorano parametri, 49): corretti
  `JS_WriteStructuredClone`/CloneDataPolicy (0297) e `HasDefinedArrayBufferDetachKey` (0298);
  `NewEnvironmentChain` ignora `support_unscopables` correttamente (le estensioni di
  CompileFunction sono contesti `with` V8, che rispettano @@unscopables). `JS_SetGCParameter`
  memorizza soltanto: `js_mem_max` vale -1 (nessun limite) e V8 non cambia il limite heap dopo
  la creazione dell'isolate; gli altri sono tarature SpiderMonkey. Restano da auditare
  le API approssimate con corpo non banale che non ignorano parametri.
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

### Riferimento fixture Chrome — 21 ottobre 2026

Chrome DevTools MCP, Chrome 155.0.0.0 su Windows x64, scheda aperta in background ma
`document.visibilityState === "visible"`, `navigator.webdriver === true`; modalità headless
non determinata. La pagina `http://127.0.0.1:18765/test.html` con iframe su porta 18766 emette
append, appended, load, contentWindow true e prototipo `true true`, senza errori script
(solo favicon 404). Il nuovo timeout non scatta dopo il load. È una verifica funzionale della
fixture aggiornata e della diagnostica, non una misura prestazionale né un risultato Servo.

### Prototipo canonico Window nei template V8 — 21 ottobre 2026

Patch `0253-v8-window-canonical-template-prototype.patch`: usare FunctionTemplate con
InstanceTemplate immutabile e PrototypeTemplate dedicato consente a V8 di collegare il
prototipo prima dell’immutabilità. La prima allocazione WebIDL `WindowPrototype` inizializza
quel prototipo e la sua catena parent, invece di crearne uno da sostituire. L’assegnazione
canonica generata da Servo mantiene così la stessa identità ed è ammessa da V8.
Il campo nativo del prototipo è inizializzato subito dopo Context::new, prima di esporlo alle
conversioni JSAPI; le allocazioni successive restano oggetti nuovi.

Valutazione librerie: riusate le API pubbliche FunctionTemplate/InstanceTemplate/PrototypeTemplate
in rusty_v8 152.2.0, senza shim C++, nuova dipendenza o fork V8. Il modello di template
istanza/prototipo immutabili è documentato anche dal
[generatore Blink](https://chromium.googlesource.com/chromium/src/+/24884fb748326f3a4f736b6e5638d36fa9e6a010/third_party/blink/renderer/bindings/scripts/bind_gen/interface.py).
Nessun trap per accesso proprietà e nessun lavoro GPU; un costruttore template interno resta
associato al prototipo. Prestazioni browser non misurate.

Regressione locale: inizializzazione canonica, prototipo accettato se identico e rifiutato
se diverso tramite Object/Reflect, proprietà globali ancora modificabili. Runtime normal:
43 passati / 2 ignorati; JIT-less: 42 / 3. Replay di 33 patch precedenti, dry-run WSL e
confronto sorgenti passati. La smoke richiede di nuovo il comportamento del vero Window,
con marker distinto dall’oggetto ordinario. **Confermato con i binding Servo nella CI
39595584589, Linux normal/JIT-less.** Lifecycle e copertura WindowProxy più ampia restano aperti.

### WindowProxy remoto — 21 ottobre 2026

Diagnosi conclusiva della CI 39587009751: normal e JIT-less emettono append, appended,
load e contentWindow true; il marker prototipo è `false false`. Tutti i runtime e le build
passano. Il problema è l’accesso al prototipo remoto, non il caricamento dell’iframe.

Patch `0254-v8-remote-window-prototype-traps.patch`: mantenere il global proxy nativo V8
solo per il Window locale che esegue JS. DissimilarOriginWindow è un rappresentante remoto:
usa ora il dispatcher V8 Proxy esistente, attivando i trap cross-origin Servo. Il suo getter
prototipo restituisce null direttamente, perché il realm sintetico clona le principals del
creatore e non può provare l’origine della finestra remota. Il setter immutabile già esistente
accetta null e rifiuta un altro prototipo usando quel getter.

Nessuna nuova libreria: riusati V8 Proxy e i security handler Servo. La regressione runtime
crea un globale remoto e verifica tramite JS che i suoi trap vengano eseguiti. Normal 44
passati / 2 ignorati, JIT-less 43 / 3. Replay di 35 patch precedenti da Servo v0.5.0 pristine
per windowproxy.rs e file custom assenti, dry-run/applicazione e confronto passati.
**Verifica browser CI pendente**: non chiudere la voce sui soli test del dispatcher.
Restano aperti anche identità durante navigazioni/transplant e trap origin-aware locali.

### Argomenti trusted dei costruttori dinamici — 21 ottobre 2026

Patch `0255-v8-trusted-function-arguments.patch`: trusted Function e guardie confermati
nella CI 39595584589 Linux normal/JIT-less; matrice estesa della 0261 ancora da verificare.
Quattro adattatori V8 Proxy per realm conservano gli argomenti di Function/GeneratorFunction/
AsyncFunction/AsyncGeneratorFunction. Prima di convertire un oggetto, richiamano il callback
Servo `codeForEvalGets`: per TrustedScript si usano i dati interni e non si invoca toString,
anche se contraffatto. Gli altri valori sono convertiti una volta, nell’ordine originale.
Il controllo CSP Servo riceve i valori originali e ogni stringa UTF-16 di parametro/corpo.
Poi viene richiamato il costruttore originale V8 con quelle stringhe; restano quindi il parser
nativo dei parametri e la semantica newTarget/sottoclassi.

Una guardia Rust autorizza una sola compilazione della sorgente esatta già verificata durante
la chiamata sincrona, e ripristina lo stato precedente. Nessuna conversione argomento avviene
con questa autorizzazione attiva. Rimossi il riconoscimento Function per forma del testo e i
passaggi UTF-8 lossy. Test: input misti/contraffatti negati, TrustedScript con toString alterato
usa i dati interni senza chiamarlo, ordine/numero conversioni ordinarie, eval rientrante negato,
getter newTarget senza bypass, UTF-16, sottoclassi e tutti i costruttori. Un tentativo di uscire
dai parametri/corpo resta SyntaxError nel parser V8. Confrontare ancora identità/stringificazione
degli intrinseci proxy e import dinamico nei WPT selezionati; nessuna misura prestazionale.

Valutazione librerie: V8 Proxy e costruttori nativi, più callback CSP/Trusted Types Servo già
presenti. Nessuna nuova dipendenza né shim C++. Costo: quattro proxy per realm e composizione
sorgente aggiuntiva per costruttori dinamici espliciti; le dichiarazioni JS ordinarie non passano
dagli adattatori. La
[WPT corrente](https://raw.githubusercontent.com/web-platform-tests/wpt/master/trusted-types/eval-function-constructor.html)
richiede sia il successo all-trusted sia zero chiamate a toString contraffatto. Chrome 155
continua a rifiutare anche quel caso trusted: la sua
[nota ufficiale](https://github.com/w3c/trusted-types/wiki/Trusted-Types-for-function-constructor)
lo identifica come bug. La nuova smoke impone successo, rifiuto dei mix/oggetti finti e uso dei
dati interni se toString è alterato. La verifica del browser Roves resta obbligatoria.

CI 39590810430, sul commit 239b542fb05 senza questa patch, conferma in Linux normal il marker
Window `true true true true`. Cross-origin restituisce `false true`: sostituzione bloccata,
ma getter non nullo. Il percorso delle finestre reali di un altro realm deve ancora essere
avvolto; la sola correzione DissimilarOriginWindow non copre quel caso. Tutti i runtime passano.

### Wrapper Window tra realm — 21 ottobre 2026

Patch `0256-v8-foreign-window-realm-wrappers.patch`: la CI 395908 conferma l’immutabilità
Window nelle due modalità Linux, ma il getter cross-origin resta non nullo (`false true`).
L’iframe usa un Window reale su altro realm dello stesso script thread: gli helper identità
di wrapping saltavano il dispatcher anche dopo la correzione dei rappresentanti remoti.
Ora JS_WrapObject/JS_WrapValue e gli helper maybe_wrap creano un adattatore V8 Proxy quando un
Window nativo attraversa un realm. Una WeakMap per realm mantiene l’identità e i metodi nativi
sono catturati prima del codice utente. Tornando al realm proprietario si riottiene il proxy
nativo; le query realm dell’adattatore seguono il proprietario per i controlli d’origine.

Ownership: il wrapper traccia il cell del proprietario e delega private/slot, evitando copie
obsolete di puntatori DOM. Non esegue una seconda finalizzazione DOM. Il flag e il private-owner
sono impostati prima che l’allocazione del proxy possa raccogliere. Riutilizzati il private già
presente e un flag, senza aggiungere un owner-pointer a ogni ClassBox né Global<Context>
aggiuntivi. La WeakMap V8 mantiene il cache ephemeron; la retention globale dei realm esistente
resta da correggere. Nessuna nuova dipendenza, benchmark browser non eseguito.

Regressione locale: identità cache, realm corretto, WeakMap globale alterata, slot aggiornati
sul proprietario, GC, getter null e rifiuto sostituzione, unwrap al ritorno e un solo finalize.
Normal 46 passati / 2 ignorati, JIT-less 45 / 3. **Prototipo cross-origin confermato
nella CI 39595584589 Linux normal/JIT-less.**
Non chiudere l’audit WindowProxy: la 0259 collega gli altri trap locali agli helper origin-aware,
ma il suo smoke proprietà è ancora da verificare in browser; il
transplant di navigazione non preserva ancora tutte le identità. La chiusura anticipata delle
suite ha portato il riepilogo CI da 13 minuti di deadline a circa 39 secondi; questo è tempo
di orchestrazione del runner Linux/Xvfb, non una misura delle prestazioni del browser.

### Aggiornamenti successivi alla CI verde — 21 ottobre 2026

Patch 0266 lega i metadati privati classici al sorgente V8, usando token host Symbol,
WeakMap/cppgc e un indice debole per script id. Rimosso lo scan che li radicava sempre nel
runtime. Funzioni vive mantengono i dati; sorgenti morti e cicli JS/private/funzione li
rilasciano. Gli import pendenti tengono il token in una proprietà privata del Promise,
eliminata dall'hook nativo di risoluzione; lo shutdown conserva la pulizia esplicita residua.
Le compilazioni identiche hanno dati distinti; CompileFunction non alloca token inutilizzati.
Regressioni con conteggi Rc, GC, cicli, realm diverso, intrinseci alterati e Promise mantenuti
dopo settlement: normal 53 passati / 2 ignorati, JIT-less 52 / 3. Replay 48 patch precedenti,
dry-run e confronto dieci sorgenti riusciti. **Smoke Linux normal/JIT-less verdi nella CI 39636431164**; stress GC browser ancora aperto.
Costo da misurare: token/host-options per compilazione, holder su prima assegnazione,
possibile effetto sulla compilation cache e lookup privato alla risoluzione Promise.
Non chiudere retention di realm/moduli né dichiarare un miglioramento RAM/CPU browser.

### Grafo intrinseco dei costruttori dinamici — 21 ottobre 2026 (locale, CI da eseguire)

Patch 0267: i target originali di GeneratorFunction/AsyncFunction/AsyncGeneratorFunction
ereditavano ancora dal `Function` originale, quindi `Object.getPrototypeOf(C)` esponeva il
costruttore non avvolto. `INSTALL` crea ora i quattro proxy in una prima fase; poi, per i tre
target derivati, verifica che il genitore sia ancora l'originale e lo ricollega con
`Object.setPrototypeOf` catturato al `Function` esposto (ECMA-262 27.3.2, 27.4.2, 27.7.2).
`Function` mantiene `Function.prototype`. I proxy restano senza trap
`getPrototypeOf`/`setPrototypeOf`/`preventExtensions`: le operazioni passano ai target, così
gli invarianti Proxy valgono anche dopo `Object.preventExtensions`/`freeze` e le mutazioni
successive dell'utente sono rispettate. Nessun controllo di sicurezza rimosso.
Riferimento Chrome 155 Windows x64 (`about:blank` visibile, misurato da Codex con Chrome
DevTools MCP; headless non noto): genitore === `Function` per [F, G, A, AG] =
[false, true, true, true], `C.prototype.constructor === C` e `C instanceof Function` sempre veri.

Regressioni runtime: nomi/length/descrittori, `prototype` e back-link `constructor`, genitori,
chiamate, `Reflect.construct`, sottoclassi, mutazioni utente e un runtime separato con
costruttori non estendibili/congelati. Smoke `csp.html`: marker obbligatorio
`ROVES-V8 csp constructor graph true true true`. Locale: normal 55 passati / 2 ignorati,
JIT-less 54 / 3; contratti runner Python 7/7 via WSL. Replay delle 50 patch precedenti sui
cinque file, dry-run `patch -p1` e confronto sorgenti riusciti. Smoke browser non eseguito.
Limite residuo: `Function.prototype.toString` sui quattro proxy V8 restituisce
`function () { [native code] }` (sintassi NativeFunction valida), mentre Chrome include il nome
(`function Function() { [native code] }`). Nessuna patch a toString introdotta.
Valutazione librerie: solo V8 Proxy e `Object.setPrototypeOf` esistenti; nessuna dipendenza,
licenza o piattaforma cambiata. Costo: tre ricollegamenti alla creazione del realm; le ricerche
di proprietà ereditate sui tre costruttori attraversano il proxy `Function`. Nessuna misura.

Patch 0265 sostituisce nei callback nativi la ricostruzione Rc/clone/forget con
increment_strong_count/decrement_strong_count della libreria standard, rimuovendo l'import
mem. Stessa operazione di ownership con meno codice; nessuna dipendenza. La regressione
adapter 0264 usa già queste API ed è verde; compilazione Servo confermata su Linux normal/JIT-less e Windows nella CI 39636431164.
Replay due patch precedenti sul file pristine, dry-run e confronto sorgenti riusciti.

Patch 0264 bilancia gli hook privati di script/moduli e li rilascia prima del teardown del
runtime. Riusa il contratto SpiderMonkey 140.14 installato e Rc; i chiamanti Servo passano
Rc::as_ptr mantenendo il proprietario Rust fino all'assegnazione. Regressione con conteggi
Rc reali per script classico, modulo sorgente e JSON, sostituzioni/undefined, unico owner
engine e release con Runtime ancora vivo. Normal 49 passati / 2 ignorati, JIT-less 48 / 3;
replay 44 patch, dry-run e confronto sei sorgenti passati. Non chiude la retention durante
la vita del runtime: raccolta per script/navigazione e bfcache restano aperte.

Patch 0263 rafforza la regressione newTarget: il precedente testo con escape doppi non
coincideva con l'autorizzazione. Ora eval tenta il testo esatto ricavato dalla funzione nativa,
nel getter prototype del newTarget, per tutti e quattro i costruttori. Negato in normal e
JIT-less mentre la costruzione prevista riesce; suite complete 48/2 ignorati e 47/3.
Nessun cambio dell'implementazione o dipendenza; dry-run e confronto replay riusciti.

Patch 0262 include le due fixture già esistenti cross-origin.html e wasm-csp.html, prima
assenti dal replay. Audit: 154 patch precedenti ricostruite e 146 file tracked confrontati
nei tre runtime custom, smoke e quattro tool V8; dopo questa aggiunta tutti coincidono.
Dry-run/applicazione WSL riusciti. È una verifica di coerenza dei sorgenti, non un test
browser né un confronto esaustivo di tutte le personalizzazioni dei file upstream Servo.

Patch 0261 estende lo smoke browser CSP: 60 combinazioni miste sui quattro costruttori
dinamici, quattro esecuzioni all-trusted e quattro conversioni contraffatte. Contratto derivato
dalla [WPT corrente](https://github.com/web-platform-tests/wpt/blob/master/trusted-types/eval-function-constructor.html),
non una run WPT completa. Runner POSIX 7 test verdi; replay 12 patch, dry-run e confronto
sorgenti riusciti. Chrome 155 MCP produce true/false/false (nega già il caso all-trusted);
non abbassare il requisito WPT per imitare quel comportamento. Risultato Roves CI pendente.

Patch 0260 applica ai callback trace/finalize WindowProxy la convenzione già presente in
`private_from_object`: slot owner undefined durante la costruzione significa assenza del
proprietario, prima di chiamare `to_private`. Evita un'asserzione nel JSVal V8, mantenendo
il controllo private-null dopo transplant. Replay sorgente verificato; GC stress browser
resta da eseguire e le suite runtime isolate non esercitano questi callback Servo.

Patch 0259 implementa i trap locali delle proprietà WindowProxy riusando gli algoritmi Servo
e le annotazioni WebIDL, inclusi getter/setter consentiti, fallback e rifiuti SecurityError.
Cache WeakMap V8 per i property holder: niente rigenerazione degli accessori a ogni lettura,
intrinseci catturati prima del codice utente e raccolta di chiavi/holder non raggiungibili.
Runtime local normal 48 passati / 2 ignorati, JIT-less 47 / 3; generator 75 verdi,
generazione WebIDL completa riuscita e runner POSIX 7 verdi. La fixture estesa passa in
Chrome DevTools MCP (Chrome 155 Windows x64, scheda background visibile, HTTP due porte;
headless non determinato), ma **la verifica Roves online resta pendente**. Non chiudere SOP
completa: finestre remote, navigazione, alias e WPT più ampie richiedono ancora verifica.
Il replay ha rilevato drift preesistente nei tre file smoke: la patch include anche le
modifiche HTTP/CSP/diagnostica/uscita anticipata già presenti nei sorgenti ma non nel replay.

Patch 0258: ClassBox conserva slot/private durante la costruzione ma differisce i callback
trace nativi finché esiste la cella JSAPI. La creazione di handler/proxy può allocare prima
di quel momento. Regressione con GC prima/dopo l'inizializzazione e valori conservati verde;
runtime normal 47 passati / 2 ignorati, JIT-less 46 / 3; replay patch e dry-run verificati.
Questo corregge il passaggio di un oggetto null ai callback, non dimostra l'intero ciclo
di vita DOM né chiude l'audit retention. Verifica browser online pendente.

Patch `0257-remove-unused-classbox-context.patch`: rimosso da ClassBox il campo
RefCell<Option<Global<Context>>> sempre None e senza assegnazioni. Rimosso il ramo di lookup
mai eseguito: gli oggetti usano già `Object::get_creation_context` di V8, mentre i wrapper
Window seguono il proprietario tracciato. Meno bookkeeping per ogni oggetto, nessuna dipendenza
aggiunta. Le suite esistenti coprono risoluzione lazy tra realm e metadati dei wrapper;
nessuna misura di memoria browser. Questa pulizia non risolve la retention in `cx.realms`,
né quella di script/module privates; i relativi criteri restano aperti.

### Audit callback e incumbent — 21 ottobre 2026

Patch `0252-v8-empty-scripted-caller.patch`: `GetScriptedCallerGlobal` richiede ora un frame
JS prima di restituire il globale; con stack vuoto Servo può usare il proprio stack incumbent.
Il helper condiviso controlla il numero di frame prima di leggere il frame 0: il caso vuoto
aveva prodotto un access violation nel test locale. La regressione copre globale, private e
posizione assenti fuori da JS, chiamante presente dentro un native e stato dopo l’esecuzione.
Runtime locale isolato: normal 42 passati / 2 ignorati, JIT-less 41 passati / 3 ignorati.
Replay di 33 patch precedenti sui due file custom assenti in Servo pristine (normalizzazione LF),
dry-run/applicazione della 0252 e confronto con i sorgenti passati. Verifica CI ancora pendente.

Due problemi ulteriori sono confermati dal codice e restano aperti:
- `HideScriptedCaller` modifica un contatore globale che nessun lookup usa. Il contratto upstream
  è per attivazione: nascondere tutto il thread romperebbe il rientro JS durante callback.
  Progettare il collegamento al confine di ingresso JS e verificare callback annidati/cross-realm.
  Audit 21 ottobre: un tentativo basato sul numero di frame di `StackTrace` è stato scartato
  (microtask, Wasm, frame interni e cambi di realm alterano la profondità; costo sul percorso
  caldo). La sola contabilità Roves (trampolini nativi + guardie RAII sulle API che eseguono JS)
  non distingue un nativo chiamato direttamente da V8 da uno chiamato da un frame JS nuovo:
  `JS_CallFunctionValue` o un getter possono raggiungere il nativo con o senza frame intermedi.
  Il candidato nativo da verificare esiste in V8 (`Context::BackupIncumbentScope` +
  `Isolate::GetIncumbentContext`, che confronta lo `sp` del frame JS più interno con lo scope e
  restituisce il realm reale del chiamante), ma rusty_v8 152.2 non lo espone. Sblocco: binding
  upstream in rusty_v8 (o shim C++ minimo), poi valutare Hide/Unhide come pila di scope con un contesto
  sentinella per runtime e regressioni dedicate; `JS_GetScriptedCallerPrivate` resta non nascosto. Nessun codice cambiato.
- Nel percorso globale NewWindowProxy usa il global proxy nativo V8. La patch 0259
  aggiunge wrapper cross-origin che invocano i trap Servo, confermati negli smoke Linux;
  questa copertura supera la diagnosi iniziale dei trap non invocati, ma non chiude
  l'identità durante navigazione: il ramo globale di JS_TransplantObject restituisce
  ancora il sostituto invece di preservare tutti i riferimenti esistenti.

### Preferenza js_disable_jit — 19 ottobre 2026

L’audit ha trovato `script::init` che dichiarava di disabilitare JIT chiamando il no-op
`DisableJitBackend` in `roves-js`. V8 non espone un toggle JIT per isolate dopo l’avvio:
`--jitless` deve essere impostato prima di inizializzare il platform process-wide. Aggiunto
`JSEngine::init_with_flags` e usato dalla preferenza Servo `js_disable_jit`; l’inizializzazione
predefinita e la feature di build `roves-js/jitless` restano invariate. Rimossa la funzione
compatibilità senza effetto. Replay patch e CI runtime `39320063351` verdi. Resta da verificare
direttamente la preferenza, poiché la matrice JIT-less usa anche la feature Cargo dedicata.

**Probe browser della preferenza — 21 ottobre 2026 (patch 0268, CI da eseguire).** Lo smoke
avvia un processo servoshell aggiuntivo e breve con l'eseguibile normale e l'opzione esistente
`--pref js_disable_jit=true`: i flag V8 sono process-wide e fissati prima del primo isolate,
quindi serve un processo nuovo, non un cambio di preferenza dopo l'avvio. `pref-jitless.html`
emette `ROVES-V8 pref js_disable_jit js true wasm unavailable` solo se JS esegue (job Promise e
calcolo) e il globale `WebAssembly` manca, lo stesso osservabile di `--jitless` già usato da
suite2 nella matrice JIT-less. Marker mancante o diverso, crash e opzione `--pref` rifiutata
falliscono con annotazione; il log della pagina entra nella diagnostica, accanto ai log HTTP.
Timeout 15 secondi, chiusura anticipata sul marker tramite il pattern di completamento ora
generico di `run_page` (le suite passano `^SUITE TOTAL `). Le pagine normali continuano a
richiedere Wasm funzionante, quindi una build accidentalmente JIT-less non passa. In modalità
JIT-less il probe gira ugualmente ma non distingue la preferenza dalla feature Cargo.

Contratti runner WSL 8/8: argomenti esatti (anche con wrapper con spazi), nessuna opzione sulle
altre pagine, risposta derivata dagli argomenti; failure per flag assente, flag `false`, Wasm
ancora presente, risultato mancante, crash e `--pref` non supportato. Replay delle 14 patch
precedenti sui file smoke, dry-run `patch -p1` WSL e confronto sorgenti riusciti. **Nessun
probe browser locale** (richiede una build servoshell pesante): non chiudere la voce prima
dell'esito CI. Valutazione librerie: flag di avvio V8 `--jitless` e runner Bash/Python stdlib
esistente; nessuna dipendenza, licenza, piattaforma, flag build/bundle, default CLI o matrice
workflow cambiati. Costo: un processo CI breve in più, nessun percorso caldo di produzione;
nessuna misura prestazionale. Totali runtime invariati (normal 55/2 ignorati, JIT-less 54/3).

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

### Chiusura delle suite completate — 21 ottobre 2026

Il runner prima attendeva tutte le deadline anche dopo `SUITE TOTAL`, circa tredici minuti
per le nove suite. Ora sorveglia il log e invia TERM al processo GNU timeout dopo il totale;
accetta soltanto quel TERM richiesto, poi applica comunque il validatore completo. Crash,
errori e conteggi non validi restano failure. Le pagine browser extra conservano i timeout.
La regressione POSIX `linger` mantiene vivo il browser simulato per 120 secondi dopo il
riepilogo e richiede che l’intero runner termini entro 15 secondi. Controlli locali Windows:
parser 4 passati e 3 integrazioni POSIX saltate; sintassi Bash valida. Tutti i 7 test, inclusi i casi POSIX, passano su Ubuntu WSL in 30,491 secondi.
Nessuna nuova dipendenza.

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
  0303 (locale): identità preservata nelle navigazioni dello stesso thread script, con ricarica
  su avanti/indietro. `unload` ora prima del passaggio (0306). I wrapper tenuti da
  altri realm seguono la sostituzione, anche verso la finestra cross-origin di Servo (0308,
  0309). Restano i riferimenti tenuti dal realm della vecchia finestra stessa dopo una
  navigazione verso un altro thread (servirebbero `Context::NewRemoteContext` e `SetAccessCheckCallbackAndHandler`,
  esportati ma senza binding rusty_v8, con strutture C++ da costruire a mano: rinviato) e
  `document.domain`.
- [x] Correggere `JS_SetImmutablePrototype` sul percorso Window di produzione:
  template canonico V8 e smoke browser Linux normal/JIT-less verdi in `39595584589`.
- [x] Verificare `TrustedScript` e CSP insieme a `eval`, `Function` e compilazione Wasm: CI
  `39595584589` (eval, Function trusted/guardie, Wasm CSP Linux normal/JIT-less), grafo dei
  genitori 0267 (`csp.html` constructor graph), matrice sui quattro costruttori (0291),
  stringificazione (0290) e import dinamico (0288) verdi in CI fino a `607b2e5`.
- [ ] Esaminare proprietà e radici di realm, script, moduli e callback. La 0266 implementa
  ownership dei metadati classici tramite token/WeakMap/cppgc, con regressioni locali verdi;
  restano verifica browser e misure, realm/moduli e callback. Rilasciare gli oggetti
  non più raggiungibili dopo navigazione o chiusura, preservando quelli richiesti da
  riferimenti JS e bfcache.
- [x] Ricontrollare i costruttori callable: il collegamento alla raggiungibilità JS è già stato
  introdotto e non deve essere ripianificato come interamente mancante.
- [ ] Verificare creazione/terminazione worker, interruzione di script, shutdown e callback
  pendenti, evitando accessi a isolate o contesti distrutti. Fatto: drain/timer/task per
  runtime e recupero allo shutdown (0282–0287, 0299), terminazione V8 annullata a stack vuoto
  come SpiderMonkey (verificato 23/10), smoke worker occupati con lavoro V8 in coda (0295).
  Restano task immediati senza handler e post in volo dopo `retire` (abbandonati di proposito).

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
- [x] **Servo WebIDL — valutazione completata:** conservare parser e semantica del generatore
  (i binding di produzione girano invariati su `roves-js`). Il pilot di un backend V8 separato
  (misurato con `v8_coverage.py`, 477/477 definizioni accettate alla sola generazione) è stato
  rimosso in 0302.
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
- [x] 23 ottobre, su conferma dell'utente: rimossi Runtime di fase 1 di `roves-v8`, pilot WebIDL,
  `roves-v8-derive`, backend V8 di `codegen.py`, `run_v8.py`, `test_v8.py`, `v8_coverage.py` (0302).
  Non servono alla produzione né agli strumenti di confronto prestazioni (`support/perf/`).
- [x] Rimosso il wiring Cargo/runtime del pilot WebIDL (0246), poi anche driver, backend e test
  del pilot (0302). Continuare a eliminare codice senza effetto solo quando i chiamanti sono
  verificati, preservando rooting/tracing.

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
Al 23 ottobre tutte le run di `607b2e5` sono verdi; lo stato aggiornato è in cima al documento.
I criteri aperti sopra restano vincolanti.

I tipi V8 restano confinati al livello scripting. Non sono previste nuove API pubbliche Roves;
eventuali cambiamenti del comportamento web devono seguire gli standard e avere test dedicati.

- [ ] Trasferire l’architettura stabile nella documentazione permanente e i problemi
  prestazionali in `ROVES_PERFORMANCE.md`.
- [ ] Eliminare i documenti temporanei `V8_MIGRATION*` quando non contengono più attività aperte,
  aggiornando i riferimenti nelle istruzioni e nel README. La cronologia resta in git.

Procedere per modifiche buildabili: verifica locale pertinente, commit, push, attesa della CI e
correzione dei fallimenti prima del passo successivo. Non dichiarare conclusa la migrazione
sulla base di sole compilazioni o dell’avvio della CI.

### Preparazione del prossimo incarico — 0273 (storico: risolto in 0288)

Codex ha verificato compiler.cc V8 152: GetFunctionFromValidatedString parte da empty_function,
ma GetFunctionFromEval sostituisce eval_from_shared con il primo frame JS quando la posizione
manca. La descrizione 0271 che attribuisce il problema sempre a empty_function è incompleta:
i trap JS apply/construct di dynamic_code::INSTALL possono diventare quel frame.
Prossimo incarico: trap Proxy native V8, mantenendo conversione Trusted Types/CSP, autorizzazione
esatta e compilatore nativo originale; niente parser nostro, stack euristico o nuova dipendenza.
Verificare tutti e quattro i costruttori, subclass/newTarget, rientri, GC e sorgenti identiche
anche nello stesso realm. Riferimento Chrome DevTools MCP 155 Windows x64, loopback, tab
aperta in background ma visible, headless non determinato: due moduli /a/creator.js e
/b/creator.js con make() identici e new Function che importa ./owner.js danno a/b, anche su
otto ripetizioni (16 risultati alternati). Nessuna GC o misura prestazionale.
Prompt dettagliato locale in target/task-0273-native-traps-plan.txt; ricostruibile dai vincoli qui.

### 0273 sospesa dopo diagnosi riproducibile — 22 ottobre, ore 09:55 (storico: risolto in 0288)

Claude ha raggiunto il limite di sessione (ripristino dichiarato alle 13:10 Europe/Rome).
AGENTS.md richiede implementazione delegata: Codex ha chiesto se procedere direttamente o
riprendere con Claude; nessuna risposta ancora. La modifica NON è completa, committata o
pubblicata. Backup locale verificato riapplicabile:
`target/0273-incomplete-native-traps.patch` (6 file, include sonde sperimentali), log
`target/t0273-{repro,native,probe}.log`. I sei file sono ripristinati a 6920417ad65: nessun
prototipo viene incluso nel prossimo push. Le copie isolate testate restano quelle di0272.

La sola rimozione del frame JS non basta. La sonda con trap native dà
`first=11 unowned=11 fresh_unowned=-1 after=11`: un Function con corpo identico riusa la
sorgente della chiamata precedente; uno con corpo diverso torna correttamente undefined.
Il test completo stesso-realm fallisce. `Compiler::GetFunctionFromEval` cerca la cache con
outer_info inizialmente empty_function, context, codice e confine parametri; eval_from_shared
è corretto dal frame solo nel percorso di compilazione. Non mascherare il problema cambiando
il codice visibile, introducendo euristiche di stack o disabilitando globalmente la cache
senza una valutazione concreta. Verificare una API mantenuta che eviti il riuso scorretto,
oppure progettare una compilazione con host options preservando grammatica, realm,
newTarget, Trusted Types/CSP e toString. Il riferimento Chrome MCP a/b resta reale, ma una
sessione inspector non dimostra come lavori la cache in un browser senza inspector.

### Incarichi successivi già studiati (da precisare prima della delega)

Aggiornamento 23 ottobre: entrambi completati (import.meta.resolve in 0274, registro debole
dei moduli sorgente in 0275); il testo sotto resta solo come storico.

- **Lifetime import.meta.resolve:** oggi SLOT_MODULEPRIVATE contiene il private grezzo,
  senza trattenere Rc<ModuleScript>. Prima di indebolire la tabella Modules, la funzione
  resolve sfuggita al modulo deve possedere quei metadati e tracciarne owner. Candidato:
  un piccolo oggetto JS nascosto, costruito con JSAPI/JSClass già presenti, con reserved
  slot per Rc, trace ModuleScript e finalize/release; la funzione conserva ObjectValue(holder)
  nel proprio slot. Evita nuova mappa globale o root Trusted. Valutare se il holder cppgc
  esistente permetta meno codice. Verificare undefined, GC durante ToString rientrante,
  funzione sfuggita dopo rilascio del private sorgente, ciclo owner/funzione raccolto e
  finalizzazione singola. DefineFunctionWithReserved usa già funzioni dinamiche deboli.
- **Registro debole dei soli moduli sorgente:** riusare token host e WeakMap/PrivateData
  aggiungendo un edge tracciato alla cella record; il holder deve esistere anche con private
  undefined. Ciclo VM token→holder cppgc→record→V8Module→token, niente root esterna.
  Source ModuleEntry può tenere WeakCell; trace_modules deve smettere di radicarla;
  lookup controllato e sweep ammortizzato. Root temporaneo di record/token fino all'aggancio.
  Test: record rooted ed errored vivo dopo GC; namespace/funzioni/import statici e dinamici
  tengono private valido; rilasciati record/namespace/funzioni si libera Rc prima dello
  shutdown; cicli e import pendenti preservati. Richiede prima il fix import.meta.resolve.
  JSON resta esplicitamente forte: un synthetic module non ha host token; serve un piano
  distinto per agganciare il suo holder alla VM prima di indebolirlo. Realm restano aperti.

I dettagli locali sono anche in target/task-0274-meta-resolve-plan.txt e
 target/task-0275-weak-source-registry-plan.txt; le condizioni sostanziali sono riportate qui
per poter riprendere anche su un altro PC senza la cartella target.
