# Analisi delle prestazioni desktop di Roves

Analisi statica di `main`, commit `82f179787fdf088da81d27a8d45f4cd2dfdbf6e9`.
Branch: `perf/desktop-analysis`, creato direttamente da quel commit.
Le modifiche mobile non sono incluse. Non sono disponibili benchmark eseguiti:
le opportunità indicate sono ipotesi da misurare, non miglioramenti dimostrati.

## Risultato

Sì, ci sono opportunità concrete, ma la prima implementazione deve essere la
strumentazione integrata e il confronto controllato con Chrome. Solo quei dati
permetteranno di attribuire la scarsa fluidità a frame pacing, composizione,
renderer, GC o caricamenti. La compressione viene mantenuta nell'analisi ma
spostata all'ultimo posto; la release attuale è già ottimizzata e anche i profili
di compilazione restano esperimenti da misurare.

| Priorità | Opportunità | Evidenza nel codice | Effetto atteso da verificare |
|---|---|---|---|
| Prima | Overlay, export, buffer circolare, marcatori e benchmark Roves/Chrome | Mancano misure integrate equivalenti e ripetibili | Identificare la causa degli scatti e impedire regressioni |
| Alta, dopo la strumentazione | Refresh driver desktop legato al monitor/vsync | `components/paint/refresh_driver.rs`, `TimerRefreshDriver::observe_next_frame`: `Duration::from_millis(1000 / 120)` | Migliore regolarità dei frame e meno lavoro superfluo sui monitor lenti |
| Alta, dopo la strumentazione | Misurare un percorso di rendering senza egui quando non ci sono overlay | `desktop/gui.rs`: `Gui::update` chiama `repaint_webviews` e inserisce un callback di composizione; `Gui::paint` presenta la GUI | Ridurre lavoro CPU della shell e copie GPU, soprattutto a risoluzioni alte |
| Ultima, salvo correlazione misurata | Spostare decompressione e I/O bloccante fuori dal percorso di caricamento | `protocols/game.rs::load` chiama `ensure_available` prima di restituire il future; `protocols/packed_content.rs` usa un mutex unico | Ridurre picchi di latenza al primo caricamento di livelli/audio/texture |
| Non applicabile come semplice tuning | GC incrementale | `components/script/script_runtime.rs`: commento esplicito sulle pre-barriere non corrette | Richiede prima un intervento di correttezza nel motore |
| Media | Confrontare release, production e un profilo orientato alla velocità con ThinLTO | `Cargo.toml`: production usa `opt-level = "s"`, LTO e un codegen unit; release workflow usa `--release` | Possibile vantaggio CPU; costo di compilazione e dimensioni da misurare |
| Media per GPU limitate | Risoluzione interna configurabile per il contenuto del gioco | Viewport fisico dipendente dalla scala HiDPI in `desktop/gui.rs` e `headed_window.rs` | Ridurre il carico GPU con compromesso sulla qualità |
| Bassa, solo avvio | Ridurre/rendere opzionale il tempo minimo della splash | `desktop/app.rs`: `MIN_SPLASH_DURATION = 500 ms`, condizione in `try_finish_booting` | Avvio caldo potenzialmente più rapido, nessun beneficio sugli FPS |

I percorsi `desktop/...` e `protocols/...` della tabella sono relativi a
`ports/servoshell/`. Il content packer è in `support/content-packer/`.

## Registro delle decisioni del 2026-09-16

Questo registro conserva il ragionamento della sessione, non soltanto la roadmap
finale, così le prossime modifiche possono essere confrontate con le decisioni
effettivamente prese.

- **Segnale iniziale:** lo stesso gioco risultava poco fluido sia su desktop sia
  nel precedente percorso Android basato su Servo; passando Android alla WebView
  di sistema la fluidità percepita è migliorata notevolmente. È un indizio a
  favore di un costo nel percorso Servo/Roves, ma non isola da solo la causa e
  non costituisce ancora un benchmark.
- **Obiettivo:** raggiungere Chrome e, dove la specializzazione di Roves lo
  consente, superarlo sullo stesso gioco e hardware. Il confronto comprende
  fluidità, RAM, CPU/GPU e avvio; non viene dichiarato successo sulla sola base
  del linguaggio Rust o degli FPS medi.
- **Prima azione:** osservabilità integrata prima delle ottimizzazioni. Sono
  prioritari overlay opzionale, JSON/CSV, buffer circolare pre-scatti, marcatori
  asset/GC/navigazione/shader e benchmark identici Roves/Chrome.
- **Niente log per frame:** il percorso caldo accumula dati in memoria e produce
  riepiloghi o dump al trigger, perché logging e serializzazione sincroni
  falserebbero i frame time.
- **Frame time scomposto:** quando osservabile, misurare intervallo tra frame,
  CPU della shell, egui/composizione, Servo/WebRender, attesa present/GPU,
  richieste e frame prodotti/saltati. Registrare anche refresh, GPU/renderer,
  visibilità/minimizzazione e conteggi oltre 25/50/100 ms.
- **Metriche decisionali:** p50/p95/p99, massimo, frame oltre il budget del
  display, sequenze di scatti e presentazione effettiva hanno precedenza sugli
  FPS medi. rAF da solo non prova quando il frame è apparso sullo schermo.
- **Ordine delle cause:** dopo gli strumenti, indagare frame pacing/vsync,
  renderer e presentazione, lavoro della shell/repaint, quindi pause JS/GC.
  I/O e compressione restano più in basso e vengono promossi solo se i marcatori
  li correlano agli scatti.
- **GC incrementale:** resta un progetto importante ma non un flag prestazionale:
  prima correggere e verificare le pre-barriere Servo–SpiderMonkey, poi misurare
  pause, throughput e memoria.
- **Dipendenze:** valutare aggiornamenti isolati e misurabili; non sostituire
  SpiderMonkey, WebRender o librerie native sulla sola aspettativa che una
  versione diversa sia più veloce.
- **Interpretazione di Rust:** la shell specializzata può eliminare funzioni e
  overhead di un browser generalista, ma DOM, heap JS, texture, cache, driver e
  librerie native determinano gran parte di memoria e latenza. Il vantaggio deve
  risultare dai numeri.
- **Compressione:** ultimo intervento previsto. Documentarne comunque l'impatto
  possibile su avvio, cambio scena, picchi CPU, copie e RAM; non attribuirle
  la scarsa fluidità stabile senza evidenza.

## Decisioni operative aggiunte il 2026-09-16

L'obiettivo dichiarato è raggiungere o superare Chrome nella fluidità dei giochi
rappresentativi, sullo stesso hardware e con impostazioni equivalenti. Non basta
confrontare gli FPS medi: il criterio principale è la distribuzione dei frame time
e, in particolare, p95, p99, frame oltre il budget del display e scatti consecutivi.

Prima priorità: costruire una modalità diagnostica disabilitata per default che
fornisca insieme questi cinque strumenti:

1. **Overlay di sviluppo opzionale**, con FPS, frame time corrente, p50/p95/p99,
   massimo recente, frame oltre budget, refresh rilevato e renderer/GPU.
2. **Esportazione JSON e CSV**, con schema/versione, clock monotono, metadati di
   build, sistema, GPU, monitor e configurazione, per confronti ripetibili.
3. **Buffer circolare in memoria**, dimensionato per conservare gli ultimi secondi
   senza fare log per frame. Uno scatto, una soglia configurabile o un comando
   manuale congela e scarica il contesto precedente e successivo all'evento.
4. **Marcatori temporali correlabili**, almeno per caricamento/estrazione asset,
   navigazione, GC e relative pause/slice, compilazione shader e primo utilizzo
   delle pipeline. Ogni marcatore deve avere inizio/fine, categoria e identificatore
   senza includere percorsi o dati sensibili per default.
5. **Harness comparativo Roves/Chrome**, che esegua gli stessi workload, input,
   risoluzione, scala, warm-up e durata. Il gioco deve includere un percorso
   deterministico o replay; Chrome va avviato con profilo pulito e configurazione
   documentata. I risultati devono indicare quando le metriche non sono equivalenti
   (per esempio rAF contro presentazione effettiva).

L'overlay legge aggregati già raccolti e non deve cambiare il percorso di rendering
quando è nascosto. Il percorso caldo registra timestamp e contatori in strutture
preallocate; niente riga di log, allocazione o serializzazione per ogni frame.
JSON/CSV vengono prodotti a fine sessione o al trigger. Il profiler misura il
proprio overhead con una prova A/B e dichiara campioni persi o incompleti.

### Metriche minime e significato

Registrare separatamente intervallo rAF, inizio/fine lavoro Servo, rendering
WebRender, composizione egui, richiesta redraw e presentazione/swap quando il
backend la rende osservabile. Un singolo valore chiamato “frame time” sarebbe
ambiguo. Assegnare un frame ID propagabile tra le fasi e usare un clock monotono.
Derivare il budget dal refresh attuale, gestendo cambio monitor e VRR senza
presumere sempre 16,67 ms. Memoria RSS, CPU e GPU sono metriche parallele.

Il confronto deve includere almeno fluidità stabile, una scena con allocazioni
JS, primo ingresso in una scena con shader nuovi e caricamento di asset. Conservare
baseline e candidati nello stesso formato e introdurre soglie di regressione in CI
solo dopo avere quantificato rumore e stabilità dei runner.

### Decisione su SpiderMonkey, WebRender e librerie native

Non sostituire ora questi componenti. La versione corrente è una baseline
funzionante, ma non è considerata automaticamente ottimale. Il ramo separato
`analysis/dependency-review` ha rilevato `mozjs 0.21.0`/SpiderMonkey 140:
la prima prova sensata è un aggiornamento compatibile nella serie 0.21, isolato e
coperto da test. Il salto alle serie mozjs più nuove cambia SpiderMonkey e binding,
ha superficie di migrazione e rischio di correttezza molto maggiori e non dimostra
da solo un miglioramento dei frame time o la soluzione delle pre-barriere.

WebRender 0.70 è profondamente integrato con Servo: non emerge una sostituzione
drop-in da fare prima delle misure. Prima confrontare il fork con gli aggiornamenti
upstream Servo e profilare scene building, render e present; recepire correzioni
mirate o aggiornare in modo coordinato è preferibile a cambiare renderer.
Analogamente ANGLE/surfman e driver grafici vanno aggiornati o variati soltanto
quando tracce e matrice GPU mostrano un problema specifico. GStreamer riguarda
principalmente media e non è candidato generale per correggere il frame pacing.

Rust può ridurre overhead e rendere più controllabile la shell, ma RAM e fluidità
dipendono soprattutto da DOM, heap SpiderMonkey, texture, cache, process model e
driver. “Scritto in Rust” non costituisce un risultato prestazionale: Roves deve
dimostrarlo contro Chrome con RSS, p95/p99 e frame oltre budget.

### Compressione: priorità finale

Compressione, estrazione e copie restano strumentate e spiegate, ma vengono
spostate all'ultimo posto della roadmap. Incidono soprattutto su avvio, primo
accesso e cambio scena. Possono causare scatti durante il gameplay solo se lettura
o decompressione avvengono nel percorso temporale della scena. I marcatori e il
buffer circolare stabiliranno se esiste questa correlazione prima di modificarne
formato, concorrenza o caching.

## 1. Ritmo dei frame

`1000 / 120` è una divisione intera: il timer richiede 8 ms, nominalmente 125
scadenze al secondo, non 120. Questo non dimostra che il gioco produca 125 FPS:
la coda eventi, il rendering e la presentazione possono limitare la frequenza.
Il timer però non segue il refresh reale di un monitor a 60, 144 o 165 Hz.

Servo supporta già un `RefreshDriver` personalizzato (`BaseRefreshDriver::new`).
Non è stata trovata un'implementazione desktop equivalente a quella OHOS nella
shell esaminata. Prima di intervenire, registrare tempi dei callback rAF,
rendering e presentazione e verificare come il backend limita lo swap.

Esperimento: confrontare il driver attuale con un driver sincronizzato al display.
Un intervallo calcolato con precisione superiore corregge l'arrotondamento ma
non sostituisce il vsync. Evitare un limite universale a 60 FPS: penalizzerebbe
i monitor con refresh elevato. Gestire cambio monitor, minimizzazione e pausa.

## 2. Composizione della shell

Ogni redraw normale entra in `Gui::update` e `Gui::paint` tramite
`HeadedWindow::handle_window_event`. La toolbar è nascosta, ma resta il percorso
egui: aggiornamento, rendering WebView, callback e presentazione del parent.
`components/shared/paint/rendering_context.rs::render_to_parent_callback`
usa `blit_framebuffer`, che esegue anche un clear del rettangolo destinazione.

Non presumere un doppio swap del monitor: `window.rs::repaint_webviews` chiama
`present()` sul contesto WebView, ma il contesto offscreen ha `present()` vuoto.
La presentazione finale avviene nel parent. Questo va distinto dal costo del blit.

Esperimento: misurare separatamente aggiornamento GUI, paint WebView, blit e
present. Solo se il costo è rilevante, aggiungere un percorso diretto per una
singola WebView senza dialoghi/overlay, ripristinando il percorso GUI quando serve.
Preservare accessibilità, input, ridimensionamento, splash e schermate di errore.
Non eliminare il clear senza verificare copertura, alpha e stato OpenGL.

## 3. Contenuti compressi e scatti

`GameProtocolHandler::load` fa controlli filesystem, eventuale estrazione e
apertura del file prima di restituire il future. `PackedContent::ensure_available`
serializza l'estrazione con un solo mutex. Il primo file richiesto può richiedere
l'estrazione dell'intero pack, attraverso `ensure_file_available` del content packer.
Questo può occupare il thread che invoca il loader; non è dimostrato che sia
sempre il thread UI. Distinguere questo caso dagli scatti del motore JS/GPU.

Esperimento: confrontare una build non compressa con cache fredda e calda della
build compressa, sullo stesso contenuto. Misurare richieste e decompressione.
Se confermato: prefetch del livello successivo, pack più piccoli e separati per
livello, estrazione tramite worker con deduplicazione per pack e cancellazione.
Un semplice mutex per pack permette più concorrenza ma non rende l'I/O asincrono.
Non aggiungere una cache RAM indiscriminata: il filesystem è già cacheato dal SO
ed una seconda copia può aumentare la memoria dei giochi con texture grandi.

## GC JavaScript: correzione dopo l'approfondimento

Il GC incrementale non è un candidato da abilitare tramite preferenza.
`components/script/script_runtime.rs`, subito prima di impostare
`JSGC_INCREMENTAL_GC_ENABLED`, dichiara: “Pre-barriers aren't implemented correctly
at the moment, so this preference defaults to false.” La disabilitazione ha quindi
una ragione di correttezza, non una semplice scelta di prestazioni.

L'indicazione iniziale di provarne l'abilitazione viene ritirata. Prima servono
pre-barriere corrette, revisione del tracing e verifiche dedicate nel motore.
Per ridurre pause GC nell'immediato, profilare le allocazioni del gioco e valutare
il riuso di buffer/oggetti nei percorsi caldi. Il GC per zona è un parametro
separato e non risolve il problema delle pre-barriere.

## 4. Profili di compilazione

`.github/workflows/release.yml` usa `./mach build --release`, non production.
`production` eredita release ma sceglie `opt-level = "s"`: è una scelta orientata
alla dimensione. Il profilo `profiling` mantiene simboli e abilita ThinLTO.
Non cambiare globalmente production da `s` a `3` senza confrontare giochi reali.

Matrice suggerita: release attuale; production attuale; profilo sperimentale
con `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`. Confrontare anche
compilazione e dimensioni. PGO è un secondo passo, solo con workload rappresentativi.
Non usare `target-cpu=native` per gli eseguibili pubblici: limiterebbe la compatibilità.
Le opzioni Rust non assicurano gli stessi cambiamenti per SpiderMonkey o altre
librerie C/C++: verificare separatamente la loro compilazione.

Fonte primaria: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
e [rustc codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html).

## 5. Cosa è già corretto

- Il loop desktop termina in `ControlFlow::Wait` o `WaitUntil` (`desktop/app.rs::set_running_control_flow`): non emerge un busy loop universale da correggere.
- JIT baseline, Ion e compilazione off-thread sono già abilitati nelle preferenze predefinite (`components/config/prefs.rs`); abilitare nuovamente il JIT non è un'ottimizzazione.
- Il manifest dei pack è caricato da `PackedContent::resolve`, non riletto ad ogni richiesta.
- Il repaint della WebView attiva è già separato dalle altre WebView (`window.rs`).

## Piano di misurazione riproducibile

Usare Windows, macOS e Linux con GPU/driver documentati. Almeno un monitor a
60 Hz e uno a refresh elevato, risoluzione e scala HiDPI fisse. Usare la stessa
build del gioco e gli stessi percorsi d'input. Test-page offre Pixi/Three/audio
come smoke test; aggiungere un gioco reale prima di trarre conclusioni.

Workload: pagina statica; scena 2D animata con sprite; scena 3D WebGL; DOM animato;
caricamento di un livello con molti asset; avvio caldo e freddo; minimizzazione.
Separare scenari CPU e GPU limitati. Disattivare log diagnostici e profiler nelle
misure finali; usarli solo per attribuire il costo e ripetere poi senza strumenti.

Dopo un warm-up di 30 secondi, registrare 60 secondi per almeno cinque run
alternando baseline e candidato. Riportare intervalli rAF p50/p95/p99, frame
oltre il budget del monitor, CPU, GPU, RSS e tempi di caricamento/avvio. I tempi
rAF non misurano direttamente la presentazione sullo schermo: affiancare strumenti
GPU/presentazione nativi. Considerare rumore, temperatura e cache OS.

La shell espone `--profiler-trace-path` in `ports/servoshell/prefs.rs` e un time
profiler: verificare la sintassi con l'help dell'eseguibile costruito. Per il codice
nativo usare il profilo profiling e strumenti di campionamento del sistema.
Attribuire JS, layout, rendering e I/O prima di scegliere la patch.

Accettare un candidato solo se il miglioramento supera la variabilità tra run,
non peggiora p99/memoria in modo significativo e supera le verifiche funzionali
(input, audio, fullscreen, accessibilità, resize, salvataggi). Registrare i numeri
nel branch. L'analisi attuale non modifica runtime o configurazione di release.

## Approfondimento: percorso effettivo del frame

Il percorso osservato è:

1. `TimerRefreshDriver` pianifica una callback; `BaseRefreshDriver` risveglia il loop.
2. `RunningAppState::spin_event_loop` esegue il lavoro Servo e aggiorna le richieste delle finestre.
3. `Painter::needs_repaint` verifica sia le ragioni di repaint sia `wait_to_paint` del refresh driver.
4. La shell richiede un redraw; il gestore headed esegue `Gui::update`.
5. `ServoShellWindow::repaint_webviews` invoca `WebView::paint`, poi `Paint::render` e `Painter::render`.
6. WebRender aggiorna e renderizza la scena; il callback offscreen compone il risultato nel parent egui.
7. `Gui::paint` presenta il parent.

Non tutte queste operazioni si eseguono immediatamente alla scadenza del timer:
coda eventi, messaggi, disponibilità del frame e attesa dello swap si interpongono.
Questo spiega perché il timer da 8 ms non basta a stimare FPS o latenza input.
`Paint::handle_messages` deduplica già `NewWebRenderFrameReady` per painter:
aggiungere una seconda deduplicazione senza misure probabilmente non aiuta.

### Repaint della GUI contro repaint del contenuto

`WebView::paint` chiama il rendering senza un controllo locale del dirty state.
`Paint::render` inoltra direttamente a `Painter::render`. Quindi un redraw
richiesto da un overlay può arrivare anche al renderer della pagina. Non dimostra
che WebRender ricostruisca ogni volta la scena: `renderer.update()` e
`renderer.render()` sono distinti dal lavoro di scene building.

Esperimento più circoscritto del bypass totale egui: tenere separati il dirty
state dell'overlay e quello del contenuto, ricomponendo il framebuffer esistente
quando cambia solo l'overlay. Misurare con pagina statica e dialogo animato.
Questo richiede preservare resize, context loss, screenshot, metriche di paint
e tick delle animazioni: saltare semplicemente `webview.paint()` può impedire
la corretta progressione del refresh driver, notificato dentro `Painter::render`.

## Approfondimento: il caricamento a blocchi non è interamente asincrono

`components/net/filemanager_thread.rs::fetch_file_in_chunks` usa `spawn_task`
che, in `components/net/async_runtime.rs`, chiama `Handle::spawn` su Tokio.
Dentro il task il reader è però `std::io::BufReader<std::fs::File>` e
`reader.fill_buf()` è una lettura sincrona. `yield_now().await` avviene dopo
il blocco, non rende asincrona la lettura che lo precede.

Oltre all'estrazione dei pack, l'I/O a cache fredda può quindi occupare i worker
Tokio usati da altri task. Distinguere almeno tre misure: attesa mutex ed
estrazione; apertura/metadata; trasferimento dei blocchi. Valutare I/O async
appropriato o un pool bloccante dedicato e limitato. Non dedurre dal nome
`spawn_blocking_task` del wrapper che usi `tokio::spawn_blocking`: quel wrapper
chiama `block_on` e non è una soluzione pronta da riutilizzare.

### Allocazioni e copie per blocco

Il blocco nominale è 32 KiB (`FILE_CHUNK_SIZE = 32768`). Il loader:

- copia il buffer di `fill_buf()` tramite `.to_vec()`;
- copia il chunk nel `ResponseBody::Receiving` tramite `extend_from_slice`;
- crea un'altra copia per `Data::Payload(chunk.to_vec())`;
- invia il payload attraverso un canale non limitato.

Evidenza: queste copie sono presenti nel codice. Ipotesi: possono incidere sui
caricamenti grandi; il costo reale rispetto a decodifica immagini/audio e upload
GPU non è ancora noto. Il body conserva la risposta mentre il canale può avere
payload in coda: la memoria di picco va misurata, non stimata come un solo blocco.

Esperimenti separati: eliminare la copia temporanea di `fill_buf()` mantenendo
gli ownership corretti; preallocare il body con limite quando la dimensione è
nota; verificare se il protocollo consente payload condivisi invece di copie;
misurare la necessità di backpressure con un consumatore lento. Un canale
limitato richiede adeguare producer/consumer e cancellazione, non solo cambiare
il costruttore. Preservare range HTTP, EOF, file modificati durante il caricamento
e propagazione degli errori: l'attuale `fill_buf().unwrap()` merita anche una
revisione di robustezza indipendente dalle prestazioni.

## Approfondimento: contenuti, cache e concorrenza

`ensure_pack_extracted` usa un marker per saltare pack già estratti. L'estrazione
legge uno stream zstd/tar: non carica deliberatamente tutto l'archivio in RAM.
La ricerca file→pack usa `manifest.files.get`, mentre il pack viene cercato
linearmente nella lista. Indicizzare anche i pack è possibile ma ha priorità
bassa: normalmente decompressione e scritture dominano quella ricerca.

Prima di parallelizzare, controllare che due pack non scrivano percorsi comuni
e coordinare la cancellazione con `clear_content_cache`. Marker, invalidazione
tramite hash del contenuto e aggiornamenti del gioco devono restare coerenti.
Un file già presente può essere osservato mentre si estrae: un futuro design
parallelo deve definire quando diventa leggibile, usando staging/commit dove
necessario. Limitare la concorrenza per evitare saturazione disco e picchi RAM.

## Approfondimento: finestra nascosta e consumi

`WebView::set_throttled` è disponibile e l'integrazione EGL lo utilizza.
Nella shell desktop esaminata non è stato trovato un uso di `set_throttled`, né
un gestore funzionale di `WindowEvent::Occluded` collegato a quell'API; il nome
dell'evento compare nel tracing. Questo è un gap d'integrazione da verificare
con una finestra minimizzata, non la prova che ogni piattaforma continui a
renderizzare a pieno ritmo: il window manager e Servo possono introdurre altre
limitazioni.

Esperimento: misurare CPU/GPU e tick rAF con app visibile, minimizzata, coperta
e semplicemente senza focus. Se necessario, collegare minimizzazione/occlusione
al throttling, ripristinandolo al ritorno. Non usare automaticamente perdita di
focus come pausa: un gioco visibile su un altro monitor può dover continuare.
Definire comportamento per audio, multiplayer, input e avanzamento simulazione.
Questo intervento punta a consumo e disponibilità CPU, non ad aumentare gli FPS
mentre il gioco è visibile.

## Approfondimento: WebGL, parallelismo e log

### Query WebGL sincrone

In `components/script/dom/webgl/webglrenderingcontext.rs`, `Finish`, diverse
query `GetParameter` e `DrawingBufferWidth/Height` inviano un comando e aspettano
`receiver.recv()`. Altre proprietà, come alcuni limiti hardware, sono già
restituite da campi locali. Non tutte le query attraversano il canale.

Per un gioco che interroga frequentemente stato/risultati, questi round trip
possono diventare un costo CPU/di sincronizzazione. Tracciare numero e durata
prima di intervenire. Nel gioco, memorizzare i valori stabili e aggiornare le
dimensioni in risposta ai resize; evitare `finish()` nei frame normali. Nel
motore, un'eventuale cache deve rispettare stato, resize e context loss.
Non trasformare query sincrone in risultati obsoleti per ottenere FPS maggiori.

### Pool WebRender

`Painter::new` limita i worker WebRender al minimo tra parallelismo disponibile
e `thread_pool_webrender_workers_max`, che per default vale 4. Il metodo di upload
texture distingue già ANGLE (`Immediate`) dagli altri renderer (`PixelBuffer`).
Non aumentare i worker al numero di core indiscriminatamente: scene building
può migliorare mentre JS, decoder e Tokio competono per gli stessi core.
Provare 2/4/8 worker solo se i profili mostrano scene building CPU limitato;
valutare anche macchine con pochi core. Lasciare invariato il metodo di upload
senza tracce GPU/driver che giustifichino una variante.

### Logging

`desktop/logging.rs` configura un file e il livello predefinito `info`.
Il controllo speciale degli errori di caricamento prende il mutex solo su
specifici record `Error`, non per ogni messaggio: non è un lock globale del frame.
I log ad alta frequenza del gioco o del motore possono invece falsare le misure.
Confrontare logging normale e `RUST_LOG=warn` in un test controllato, conservando
una modalità diagnostica; non eliminare gli errori per inseguire le prestazioni.

## Ordine operativo rivisto

| Ordine | Lavoro | Criterio di uscita |
|---|---|---|
| 1 | Telemetria frame, overlay, JSON/CSV, buffer circolare e marcatori | Overhead misurato; dati correlabili e schema documentato |
| 2 | Benchmark deterministico Roves/Chrome | Baseline ripetibile su 60 Hz e refresh elevato, CPU/GPU/RSS inclusi |
| 3 | Refresh monitor, vsync e frame pacing | Riduzione dimostrata di p95/p99 e frame oltre budget |
| 4 | Attribuzione renderer, GPU, egui e presentazione | Collo di bottiglia identificato prima di bypass o aggiornamenti |
| 5 | Repaint overlay separato e lavoro superfluo della shell | Vantaggio misurato senza regressioni funzionali |
| 6 | GC: prima correttezza, poi pause e tuning | Pre-barriere verificate e confronto pause/memoria |
| 7 | Aggiornamenti mirati mozjs/ANGLE/surfman e profili build | Un componente per volta, benchmark e rollback |
| 8 | I/O bloccante e caricamenti | Intervenire se i marcatori coincidono con gli scatti |
| 9 | Compressione, pack e prefetch | Ultimo passo; intervenire solo con correlazione dimostrata |

Le migliori prime patch non sono necessariamente quelle con il maggior numero
di flag. La scelta deve seguire il collo di bottiglia del gioco rappresentativo.
Questa revisione estende e corregge l'analisi statica; non contiene benchmark
runtime né stime percentuali di miglioramento. Non cambia codice di produzione.

## Lavoro pianificato: rendere sicuro il GC incrementale

Questo intervento viene incluso nel piano anche se richiede un lavoro lungo.
L'obiettivo è correggere l'integrazione Servo–SpiderMonkey, verificare che il
GC incrementale sia sicuro e solo allora valutarne l'abilitazione per i giochi.
Non è escluso dalla roadmap: resta escluso soltanto come semplice tuning tramite
flag. Non viene stimata una durata prima di conoscere l'estensione del problema.

### Fase 1: delimitare il problema

- Verificare se il commento sulle pre-barriere descrive ancora un difetto presente nella versione di SpiderMonkey usata da Roves; cercare test, issue e interventi upstream pertinenti.
- Mappare rooting, tracing e mutazioni dei riferimenti gestiti da Servo, inclusi i collegamenti tra oggetti DOM Rust e oggetti JavaScript, i binding generati e gli eventuali wrapper della libreria mozjs.
- Individuare quali operazioni richiedono pre-barriere e quali le applicano già. Distinguere i requisiti del GC incrementale da quelli del GC generazionale e dalle post-barriere.
- Preparare una riproduzione minima o un test di stress che evidenzi il difetto. Un commento da solo non stabilisce né la causa completa né l'ampiezza della correzione.

Risultato richiesto: inventario dei percorsi interessati, evidenza riproducibile,
requisiti di correttezza e scelta tra recepire una correzione upstream o intervenire
nell'integrazione locale. Se il problema non è riproducibile, documentare i limiti
delle prove invece di considerarlo automaticamente risolto.

### Fase 2: implementare le correzioni

Correggere i percorsi individuati usando le API e le astrazioni di barriera
compatibili con la versione effettiva di SpiderMonkey. Verificare anche generatori
e wrapper condivisi: una modifica centralizzata può essere preferibile a molte
correzioni manuali nei singoli DOM. Non prescrivere ora una soluzione senza aver
completato l'inventario. Mantenere il default incrementale disabilitato durante
questa fase e rendere le patch ricostruibili nel modello upstream+patch di Roves.

### Fase 3: verificare la correttezza

Aggiungere test di regressione basati sui difetti trovati e stress test con
raccolta incrementale, mutazioni tra slice, creazione/distruzione di DOM e
riferimenti Rust–JS, navigazione e teardown. Includere worker e contesti multipli
se l'inventario mostra percorsi condivisi interessati. Usare verificatori/barrier
checking e GC zeal dove disponibili nella versione e nella configurazione di
SpiderMonkey in uso; controllare che siano realmente attivi.

Eseguire le verifiche funzionali pertinenti e strumenti di rilevamento degli
errori di memoria nelle configurazioni supportate. Provare Windows, macOS e Linux.
Una sessione senza crash e un benchmark FPS non costituiscono una verifica
sufficiente delle barriere. Documentare anche test e strumenti non disponibili.

### Fase 4: misurare e decidere l'abilitazione

Dopo la verifica della correttezza, confrontare GC non incrementale e incrementale
sugli stessi giochi e profili di build. Misurare durata e distribuzione delle
pause GC, frame p95/p99, throughput e memoria; separare warm-up e caricamenti.
Valutare il budget delle slice rispetto al refresh del monitor senza promettere
che ogni slice rispetti rigidamente quel budget. Non modificare contemporaneamente
GC per zona, dimensioni heap o JIT: servono confronti attribuibili.

Solo dopo questi passaggi proporre un'abilitazione sperimentale controllata e,
se giustificato dai risultati, il nuovo default. Conservare una possibilità di
rollback. Se il GC incrementale non migliora i giochi rappresentativi, la
correzione di correttezza resta utile ma l'abilitazione non è obbligatoria.

Questo filone può essere pianificato insieme agli altri interventi prestazionali,
ma deve avere patch e revisioni dedicate. Il presente aggiornamento aggiunge il
lavoro alla roadmap; non implementa ancora le barriere né abilita il GC.
