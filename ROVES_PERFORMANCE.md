# Roves — Performance tracking

## Obiettivo

Ridurre CPU e memoria dei giochi eseguiti con Roves, con particolare attenzione allo stato inattivo, fino a ottenere un comportamento almeno comparabile a Chrome a parità di pagina, risoluzione, backend grafico e condizioni di misura.

## Stato

- Fase corrente: fast path A implementato opt-in; verifica CI e misure A/B
- Modifiche al codice runtime: polling gamepad adattivo e scheduling repaint egui pubblicati su `main`
- Ottimizzazioni confermate: riduzione percepibile del consumo nel test reale dell'utente; CI desktop/mobile verde
- Analisi fast path completata in `docs/FAST_PATH_RENDERING.md`: egui resta il piano overlay;
  percorso A salta painter/tessellazione mantenendo l'off-screen, percorso B introduce una
  presentazione differita per rendere direttamente sul framebuffer della finestra.
- Ultimo commit pronto e verificato: `89fc060` (`perf: prepare direct rendering fast path`),
  con workflow GitHub Actions `35987823977` completamente verde.

## Criteri di misura da definire

- CPU media e picco a pagina inattiva, dopo warm-up
- CPU media con scena PixiJS controllata
- GPU utilization e frequenza di presentazione
- FPS e frame time p50/p95/p99
- RSS/private working set e memoria GPU
- Numero di thread e processi
- Allocazioni/heap JS dopo GC e memoria WebRender/cache
- Consumo con finestra visibile, non in focus e minimizzata

Il confronto con Chrome dovrà misurare l'incremento causato dalla pagina, non soltanto il totale di un browser che potrebbe avere processi condivisi già avviati.

## Fatto

- Esaminato il loop eventi SDL3: quando non esistono deadline periodiche usa una wait bloccante, quindi non emerge un busy-loop incondizionato del main thread.
- Individuato il polling gamepad desktop: la feature è abilitata di default e mantiene un risveglio ogni 100 ms quando il delegate esiste, anche se non è collegato alcun controller.
- Individuate feature desktop abilitate di default da valutare separatamente: gamepad, WebGPU, WebXR, clipboard, JIT e logging massimo a livello info.
- Individuati i pool configurati di default: layout, worker generici, runtime async e WebRender; possono aumentare thread e memoria di base.
- Verificato che il repaint della WebView esegue paint e present; le richieste di redraw sono coalesciate per evitare accodamento illimitato.
- Separata l'animazione dello splash, limitata alla fase di caricamento, dal consumo persistente a gioco avviato.
- Implementato il primo intervento isolato: polling gamepad a 1 secondo quando non esistono controller aperti o effetti aptici pendenti; la cadenza torna a 100 ms durante l'uso attivo.
- Aggiunti test unitari per i tre stati della cadenza (idle, controller aperto, aptica pendente) ed estesa la descrizione del relativo step CI.
- Corretto il bridge SDL3/egui: la richiesta del frame successivo usa ora il `repaint_delay` dell'output corrente invece dello stato globale del contesto.
- Aggiunti test per repaint immediato, repaint differito e assenza di repaint pianificato.
- Aggiunte alla pagina diagnostica tre fixture deterministiche selezionabili via query string: pagina vuota, PixiJS statico con un solo render e ticker fermo, PixiJS animato.
- Le fixture prestazionali escludono i pannelli diagnostici normali, il loop `requestAnimationFrame` del gamepad, i probe FPS e i timer Steam, evitando che il benchmark misuri la pagina di test invece del runtime.
- Aggiunti contatori shell opt-in (`ROVES_PERF_LOG_INTERVAL_MS`) per distinguere eventi reali, timeout del loop, redraw accodati/coalesciati, paint WebView e presentazioni finali.
- Estesi i contatori con run/tessellazione/paint egui, blit del framebuffer e present diretti o
  composti; aggiunta una decisione fast-path pura che richiede tutte le condizioni di sicurezza.

## In corso

- Preparare una matrice di benchmark riproducibile Roves/Chrome per misurare separatamente CPU,
  RAM e costo del painter egui dopo l'implementazione.
- Valutare l'integrazione degli eventi gamepad nell'event pump SDL principale per eliminare anche il polling hot-plug inattivo a 1 Hz.
- Completare lo scheduling delle richieste egui differite tramite una deadline del loop eventi.
- Rendere adattivo il polling delle callback Steam nelle sole build `steam`.
- Analizzare il costo di memoria di thread pool, WebRender, SpiderMonkey, WebGPU/WebXR e cache senza ridurre le API disponibili al gioco.

## Ipotesi da verificare

| Priorità | Ipotesi | Segnale atteso | Stato |
| --- | --- | --- | --- |
| Alta | Polling gamepad ogni 100 ms impedisce l'idle profondo | Wake-up regolari del main thread; CPU ridotta con polling adattivo | Correzione in verifica |
| Alta | La pagina mantiene callback `requestAnimationFrame` o repaint indiretti anche senza ticker Pixi espliciti | Redraw/present continui; contatore RAF o repaint non nullo | Da misurare |
| Alta | Feature inutilizzate (`webgpu`, `webxr`, gamepad) aumentano il footprint iniziale | RSS/thread inferiori in build minimali A/B | Da misurare |
| Media | WebRender o il compositor presenta frame invariati | Present regolari senza variazioni della display list | Da misurare |
| Media | Egui richiede repaint persistenti | `has_requested_repaint()` resta vero dopo il caricamento | Correzione in verifica |
| Media | Pool di thread sovradimensionati per un runtime single-game | Molti thread inattivi con stack/allocator residenti | Da misurare |
| Media | Cache e heap non vengono ridotti dopo il caricamento | RSS elevato dopo GC e idle prolungato | Da misurare |
| Bassa | Lo splash continua oltre la fase prevista | Wake-up a circa 30 FPS oltre gli 8 secondi | Da verificare |

## Interventi candidati ordinati

1. **Fast path di rendering del gioco.** Oggi Servo disegna in un contesto off-screen, il risultato viene composto nella scena egui e infine presentato nella finestra. Verificare se il caso senza dialoghi o overlay può evitare lavoro e buffer intermedi, conservando un percorso compatibile per le UI native.
2. **Eventi gamepad senza polling idle.** Convogliare hot-plug e input attraverso l'event pump SDL principale, se i vincoli SDL e la gestione multi-window lo consentono, eliminando la deadline periodica quando non esiste attività.
3. **Deadline egui differite.** Memorizzare il `repaint_delay` finito e includerlo nel `ControlFlow::WaitUntil`, così animazioni e cursori restano corretti senza trasformare un repaint futuro in un loop immediato.
4. **Callback Steam adattive.** Ridurre i wake-up del thread `run_callbacks()` quando l'integrazione Steam non ha lavoro sensibile alla latenza, limitatamente alle build compilate con la feature `steam`.
5. **Modularità senza perdita di compatibilità Chrome.** Un gioco Roves può potenzialmente usare qualunque API disponibile in Chrome. Le capacità predefinite non vanno quindi rimosse o ridotte globalmente. La modularità già disponibile può essere migliorata con profili o opzioni esplicite di build, mantenendo come default il runtime completo e misurando separatamente il costo reale di ogni feature.
6. **Dimensionamento dei pool di thread.** Valutare preset o euristiche per layout, worker generici, runtime async e WebRender solo dopo misure di RSS e frame time; privilegiare il runtime completo e non sacrificare fluidità o compatibilità per ridurre la memoria nominale.

## Strategia di esecuzione

1. Aggiungere una baseline diagnostica ripetibile con pagina vuota, PixiJS statico e PixiJS animato.
2. Registrare wake-up, redraw, paint, present, RSS, thread e frame time dopo un warm-up definito.
3. Usare la CI per test funzionali, contratti del loop eventi e applicabilità delle patch; non imporre soglie prestazionali rigide sui runner GitHub condivisi.
4. Analizzare e prototipare il fast path di rendering come intervento a maggiore impatto potenziale.
5. Affrontare in seguito gamepad event-driven, deadline egui, Steam e pool di thread in commit isolati e misurabili.

## Prossimi passi di analisi

1. Costruire una pagina minima vuota e una scena PixiJS deterministica senza ticker.
2. Raccogliere baseline con stessa macchina, dimensione finestra, VSync, build release e warm-up.
3. Aggiungere temporaneamente contatori diagnostici per wake-up, redraw, paint, present e callback RAF.
4. Profilare per thread e acquisire un flame graph/ETW equivalente durante 30–60 secondi di idle.
5. Eseguire build A/B disabilitando una feature alla volta, senza ancora adottare modifiche definitive.
6. Classificare ogni intervento per guadagno CPU, RAM, compatibilità e rischio di regressione.

## Decisioni

- Non verranno applicate ottimizzazioni prima di avere baseline e profiling per thread.
- CPU e RAM saranno trattate separatamente: una riduzione dei wake-up non implica necessariamente una riduzione del footprint.
- Il runtime predefinito deve continuare a permettere al gioco di usare potenzialmente tutte le capacità web disponibili, come in Chrome.
- Le funzioni da gioco e web (gamepad, Steam quando richiesto, WebGPU, WebXR, clipboard e altre API) non saranno rimosse globalmente. Eventuali profili minimali saranno espliciti e alternativi al runtime completo.
- La modularità esistente verrà sfruttata per esperimenti A/B e distribuzioni consapevoli, non come scorciatoia per dichiarare risolto il consumo del runtime completo.

## Registro

### 2026-09-22

- Avviata l'analisi prestazionale.
- Creato il registro.
- Identificati i primi candidati nel loop SDL3, nelle feature predefinite e nei pool di thread.
- Nessuna modifica funzionale effettuata.

### 2026-09-22 — Polling gamepad adattivo

- Ridotta da 10 Hz a 1 Hz la sola scansione hot-plug quando non è presente un controller attivo.
- Conservata la cadenza di 10 Hz quando un controller è aperto o esiste un effetto aptico pendente.
- Aggiunti test unitari eseguiti dallo step `mach test-unit -p servoshell` della CI.
- Da misurare il guadagno CPU effettivo e verificare la latenza hot-plug massima di circa un secondo.

### 2026-09-22 — Decisione repaint egui per frame

- Eliminata la decisione basata su `Context::has_requested_repaint()` dopo la chiusura del frame.
- Il bridge legge ora `ViewportOutput::repaint_delay` prodotto dal frame corrente.
- Solo un ritardo zero accoda immediatamente un altro redraw; richieste differite non vengono trasformate in un loop immediato.
- Aggiunti tre test unitari inclusi nello step CI di `servoshell`.
- Resta da misurare la frequenza reale di `paint/present` con una scena PixiJS inattiva.

### 2026-09-23 — Riscontro reale e piano successivo

- L'utente ha confermato che il runtime funziona sensibilmente meglio dopo polling gamepad adattivo e correzione del repaint egui.
- Le workflow desktop Servo, Android e iOS relative al commit pubblicato sono terminate con successo.
- Confermato il requisito di compatibilità: il gioco deve poter usare potenzialmente tutto ciò che userebbe in Chrome; nessuna feature web verrà rimossa dal profilo predefinito per ottenere artificialmente numeri migliori.
- Ordinati i prossimi interventi: benchmark diagnostico, fast path di rendering, gamepad event-driven, deadline egui differite, callback Steam adattive e studio dei pool di thread.

### 2026-09-23 — Fixture di benchmark riproducibili

- `?perf=blank`: baseline del runtime senza contenuto animato.
- `?perf=pixi-static`: inizializza PixiJS, disegna una scena una volta, ferma esplicitamente il ticker e non pianifica altri frame.
- `?perf=pixi-animated`: stessa scena e stessa dimensione, con ticker PixiJS attivo, per misurare frame pacing e costo per frame.
- La normale pagina diagnostica resta invariata per i test manuali funzionali; le fixture usano un ramo minimale che non monta i suoi timer e probe.

### 2026-09-23 — Contatori diagnostici del runtime

- `ROVES_PERF_LOG_INTERVAL_MS=<millisecondi>` abilita righe aggregate `[roves-perf]` in `roves.log`.
- I contatori separano wake-up da evento e da timeout, richieste redraw realmente accodate e coalesciate, dispatch redraw, paint WebView e presentazioni della finestra.
- La diagnostica è spenta per default, non crea thread o timer e non modifica lo scheduling del gioco.
- La CI verifica parsing della configurazione e reset atomico degli snapshot; le soglie CPU/RAM restano escluse dai runner condivisi.

### 2026-09-24 — Verifica CI e pausa per regressione salvataggi

- La CI completa dei contatori diagnostici è verde su Linux, macOS e Windows, incluse le varianti installer e portabili.
- Il miglioramento percepito delle prestazioni è stato confermato nuovamente dall'utente.
- Il lavoro prestazionale successivo resta il fast path di rendering; la correzione indipendente dei salvataggi viene mantenuta in un commit separato per non confondere misure e regressioni.

### 2026-09-24 — Analisi del fast path e verifica toolkit UI

- Confermato che la libreria UI modernizzata era egui stesso: `0.34.3 → 0.36.2`, con
  `egui-file-dialog 0.13 → 0.15`; non è presente un toolkit sostitutivo installato nel runtime.
- Confermata la decisione di non riscrivere splash, dialoghi, input e AccessKit in un altro
  toolkit: ciò non eliminerebbe il costo del percorso WebRender/off-screen.
- Mappato il frame corrente: WebRender off-screen → callback/blit egui → painter egui → present.
- Definite due fasi: bypass del painter egui conservando l'off-screen, poi WebView diretta con
  `present()` differito per eliminare anche framebuffer e blit intermedi.
- Completati contatori direct/composited e decisione fast-path pura con fallback testati.
- Prossimo commit prestazionale: esperimento opt-in `ROVES_DIRECT_PRESENT=1` per il percorso A.

### 2026-09-26 — Handoff delle prestazioni a Codex

- Il lavoro prosegue dal commit `89fc060b05913b3f8bf7210c374b644001aeae93` su `main`.
- Implementare soltanto il fast path A in questa fase; il fast path B con rendering diretto nel
  framebuffer della finestra rimane un esperimento successivo, subordinato alle misure A/B.
- Il percorso deve essere disattivato per default e attivabile con
  `ROVES_DIRECT_PRESENT=1`. Il fallback composto esistente deve avvenire nello stesso frame se
  manca anche una sola condizione di sicurezza: WebView unica e full-window, nessun dialogo o
  status overlay, nessun focus egui, AccessKit inattivo e nessuna texture egui pendente.
- `Gui::update` deve continuare a eseguire egui per drenare input, dialoghi e AccessKit. Solo il
  pass di tessellazione/paint viene saltato nel fast path A; l'off-screen e il blit restano.
- La presentazione diretta deve incrementare `direct_presents`, `framebuffer_blits` e
  `window_presents`; il fallback deve continuare a incrementare `composited_presents`.
- Aggiungere unit test e uno smoke test CI reale che fallisca se l'opzione è attiva ma nessuna
  presentazione diretta viene osservata nei log diagnostici.
- Prima di modificare leggere integralmente `CLAUDE.md`. Ogni modifica ai file Servo deve avere
  una voce in `CUSTOMIZATIONS.md` e la nuova patch sequenziale
  `patches/servo-v0.5.0/0041-*.patch`, applicabile alla sorgente upstream pulita. La patch non
  deve contenere workflow o documenti specifici di Roves.
- Aggiornare `docs/FAST_PATH_RENDERING.md`, questo registro, README e pagina wiki pertinente.
  Il cambiamento interno opt-in non aggiunge un flag `mach bundle`, quindi non richiede una
  nuova opzione Packmaster né modifiche a `roves-action`, salvo variazioni effettive della
  superficie build/bundle.
- Eseguire commit e push diretti su `main` (già autorizzati), osservare la CI fino al termine e
  correggere autonomamente eventuali errori prima di considerare concluso il lavoro.

### 2026-09-26 — Fast path A opt-in

- Implementato `ROVES_DIRECT_PRESENT=1`, con parsing rigoroso e default disabilitato.
- `Gui::update` continua a eseguire egui e drenare input/dialoghi/AccessKit; il paint diretto
  mantiene l'off-screen, esegue il blit e un solo present, senza tessellazione né painter egui.
- La decisione per frame richiede WebView unica a piena finestra, assenza di dialoghi/status/focus,
  AccessKit inattivo e nessuna texture pendente; splash, errori e ogni condizione mancante
  conservano il percorso composto nello stesso frame.
- Contatori diretti: `direct_presents`, `framebuffer_blits`, `window_presents`; fallback:
  `composited_presents`. Test unitari del parsing e delle condizioni, smoke CI desktop con
  intervallo diagnostico di 1000 ms e obbligo di osservare `direct_presents > 0`.
- Patch sequenziale `0041-direct-present-fast-path.patch`, limitata ai file Servo.
- Nessuna modifica alla superficie build/bundle, Packmaster o roves-action. Fast path B e
  rimozione dell'off-screen rimandati; nessun guadagno CPU/RAM dichiarato senza misure reali.
