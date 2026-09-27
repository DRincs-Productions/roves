# Piano operativo per le migrazioni delle librerie di Roves

Aggiornato il 27 settembre 2026. Destinatario: Codex che implementerà modifiche nel fork. Ambito: librerie e infrastruttura di Roves, non dipendenze delle applicazioni ospitate.

L'inventario dei consumatori e delle dipendenze attive è in [`docs/AUDIT_MIGRAZIONE_LIBRERIE.md`](./AUDIT_MIGRAZIONE_LIBRERIE.md); aggiornarlo con le evidenze raccolte da ogni tranche.

## Prima di eseguire

Questo documento è un piano, **non un ordine di sostituire tutte le librerie**. Leggere lo stato attuale in `Cargo.toml`, `TODO.md`, `CUSTOMIZATIONS.md`, `docs/DEPENDENCY_REVIEW.md` e `SDL3_WINDOWING_TESTING.md` prima di ogni fase. Il codice cambia: non usare versioni o numeri di riga qui come fonte definitiva. Lavorare in incrementi isolati e reversibili, confrontando il comportamento prima/dopo sullo stesso carico. Per modifiche al motore seguire la procedura del repository per `CUSTOMIZATIONS.md` e le patch riproducibili in `patches/`; verificare che la CI costruisca davvero la stessa sorgente modificata. Separare sempre compilazione, smoke test headless e prova interattiva su hardware.

Il contratto da conservare è un runtime desktop embedded basato sul fork Servo, coerente su Windows, macOS e Linux, con comportamento Web osservabile invariato. Android/iOS usano un percorso diverso: non rimuovere dipendenze condivise senza controllare i loro target. Non sostituire SpiderMonkey, Stylo, parser HTML/CSS, WebRender o GStreamer in questo piano.

| Area | Stato verificato su `main` | Decisione |
|---|---|---|
| `winit`/GilRs → SDL3 per shell desktop | Finestra, event loop, input e gamepad migrati; il gamepad macOS è disabilitato per un hang non diagnosticato | Validare e correggere gap; non rifare la migrazione |
| `surfman` | Ancora usato per contesti GL, offscreen, WebGL/WebXR/media e shell | Studiare rimozione per componenti dopo prova di parità |
| Componenti i18n/Unicode → ICU4X | Manifest include già `icu_locid`, `icu_properties`, `icu_segmenter` 1.5 | Inventariare chiamate; migrazione selettiva solo con equivalenza |
| Percorsi GL → wgpu | WebGPU usa componenti wgpu; WebRender e la shell mantengono percorsi GL | Fast path misurato, non sostituzione globale |
| jemalloc → mimalloc | `servo-allocator` usa jemalloc su alcuni target; Windows e OHOS usano System | Solo confronto A/B dopo evidenza di un collo di bottiglia |
| `log`/`env_logger` → `tracing` | Entrambi coesistono; Tracy/Perfetto sono opzionali | Consolidamento locale solo se riduce duplicazione reale |

## 1. SDL3: chiudere la migrazione già fatta

**Punto di partenza.** `ports/servoshell/desktop/headed_window.rs` usa una finestra `sdl3::video::Window`; l'event loop SDL3 è già nel codice. `SDL3_WINDOWING_TESTING.md` registra esplicitamente che la CI headless non prova interazione reale. In `TODO.md` il gamepad su macOS resta disabilitato, poiché `sdl3::init().gamepad()` può bloccarsi su runner CI. Non eliminare riferimenti a `winit` dal workspace solo per il nome: controllare `cargo tree` sul target e le dipendenze di altri componenti.

**Procedura.**

1. Rilevare la baseline per Windows, macOS e Linux: avvio e chiusura, resize, fullscreen, DPI e cambio monitor, tastiera/scorciatoie, mouse e drag, touch/trackpad, composizione IME, cursore, clipboard, accessibility e controller. Usare la checklist dettagliata `SDL3_WINDOWING_TESTING.md` e segnare piattaforma, versione e risultato. Verificare contenuto che ridisegna continuamente e che la finestra rimanga reattiva; il `RedrawCoalescer` evita una regressione già osservata.
2. Per il gamepad macOS isolare l'inizializzazione SDL in un programma minimo con timeout e log prima/dopo ogni chiamata. Riprodurre separatamente su Mac con sessione grafica reale e su CI; distinguere blocco dell'ambiente headless, backend SDL e inizializzazione fuori dal main thread. Non riattivarlo finché connessione, disconnessione, input e shutdown sono verificati su Mac reale.
3. Correggere un gap alla volta nel bridge SDL → eventi Servo/accessibilità/IME. Conservare test unitari per conversioni pure e smoke test per startup; i test interattivi devono accompagnare le modifiche all'event loop.
4. Aggiornare checklist e `TODO.md` solo con evidenze ottenute. Non segnare “verificato” sulla base della sola CI headless.

**Accettazione:** prove interattive sui tre desktop senza regressioni Web o blocchi del loop; controller macOS riabilitato solo con prova reale. **Rollback:** ripristinare il singolo cambiamento SDL che introduce una regressione; evitare il ripristino globale di winit.

## 2. Ridurre `surfman` per responsabilità

**Perché:** SDL3 gestisce ora la finestra, ma una finestra SDL non rimpiazza automaticamente contesti GL, superfici offscreen, condivisione texture e sincronizzazione fra Servo, WebRender, WebGL, media e XR. La frase “SDL3 sostituisce surfman” è un'ipotesi da dimostrare, non una modifica a `Cargo.toml`.

**Ricognizione da produrre prima del codice.**

- Cercare `surfman`, `Surface`, `Context`, `Device`, `RenderingContext` e gli handle raw nei componenti e nei manifest. Separare usi desktop, Android/OHOS, WebGL, WebXR, media e compositore. Salvare una tabella con proprietario di ogni contesto, thread, formato, API per import/export, lifetime e test che lo esercita.
- Tracciare in `ports/servoshell/desktop/headed_window.rs` il flusso `OffscreenRenderingContext` → blit → `WindowRenderingContext`: numero di passaggi, copie, fence, present e costo misurato. Identificare quali metodi Servo accettano davvero un backend alternativo.
- Verificare per piattaforma creazione del context, surface sharing, GL/EGL/ANGLE, context loss e device reset. Non supporre che l'API GPU di SDL offra interoperabilità diretta con WebRender/ANGLE.

**Implementazione incrementale.**

1. Definire un confine interno piccolo per i soli usi della shell desktop, se l'interfaccia attuale non lo consente: creazione/distruzione context e surface, resize, make-current, offscreen, import/export e present. Il backend esistente resta la baseline. Evitare un adapter generico che nasconda semantiche differenti.
2. Costruire un prototipo limitato alla presentazione della shell o al blit finale. Misurare copia GPU, tempi di present, latenza, FPS e memoria a parità di driver e contenuto. Conservare il percorso precedente selezionabile in build developer durante la valutazione.
3. Solo se la parità funzionale è dimostrata, migrare un consumatore per volta. Testare WebGL, canvas, WebGPU, video accelerato, XR ove disponibile, screenshot, resize/HiDPI, overlay e recupero dopo context loss. Il backend deve rispettare thread affinity e ownership delle superfici.
4. Rimuovere la dipendenza `surfman` **solo dal target/crate che non la usa più**. Eseguire `cargo tree -i surfman` per target e feature rappresentativi prima di tentare rimozioni globali. Conservare gli usi non migrati, in particolare mobile.

**Accettazione:** comportamento e immagini equivalenti su tre desktop, nessuna copia o latenza aggiuntiva fuori budget, test di perdita contesto e teardown puliti. **Stop/rollback:** impossibilità di condividere superfici senza copia costosa, regressioni WebGL/media/XR o divergenze di driver; tornare al backend `surfman` per quel consumatore. Non forzare la rimozione per ottenere un grafo dipendenze più corto.

## 3. i18n/Unicode: valutare ICU4X in modo selettivo

**Correzione rispetto al riepilogo precedente:** Roves **usa già crate ICU4X** (`icu_locid`, `icu_properties`, `icu_segmenter` nel manifest). “Migrare a ICU4X” significa cercare eventuali implementazioni parallele e versioni incompatibili, non introdurre ICU4X da zero né sostituire automaticamente tutte le crate Unicode.

1. Inventariare chiamate e feature per `icu_*`, `unicode-*`, `unicode_categories`, `sys-locale` e `encoding_rs`. Classificare per algoritmo: locale, segmentazione, proprietà, bidi, normalizzazione, conversione encoding e API Web osservabile. Individuare quali crate sono transitive e quali sono usate direttamente; registrare dimensione binario e dati incorporati per target.
2. Scegliere **un** servizio duplicato e creare casi di confronto con lingue e scritture diverse: combinazioni, emoji/ZWJ, RTL, CJK, locale e boundary Unicode. Per le API Web confrontare output con il comportamento richiesto dalle specifiche e i test Web Platform applicabili; mantenere `encoding_rs` o altre librerie dove i mapping Web richiedono quella semantica.
3. Prototipare lo stesso servizio con una versione ICU4X compatibile nel grafo corrente. Valutare provider dati, bundle/offline, dimensione, startup, allocazioni, throughput e supporto delle piattaforme. Aggiornare insieme le crate ICU correlate se richiesto dalla loro compatibilità, senza trascinare una major non necessaria.
4. Sostituire il servizio solo se elimina davvero una duplicazione e passa i test di equivalenza. Rimuovere la vecchia dipendenza diretta solo dopo verifica del grafo per target; una dipendenza transitiva può restare legittimamente.

**Accettazione:** parità di comportamento, dati i18n versionati e disponibili offline, costo di startup/memoria accettabile. **Stop:** differenze nelle API Web, dati mancanti o aumento ingiustificato del bundle. HarfBuzz/FreeType e il text shaping non rientrano in questa sostituzione.

## 4. Percorsi OpenGL: sperimentare wgpu dove una misura lo giustifica

`wgpu-core`/`wgpu-types` supportano già WebGPU. **wgpu non è un compositor Web** e non sostituisce WebRender, né implica che `egui_glow`, `glow`, `gleam`, WebGL e ANGLE possano sparire insieme. Il `TODO.md` classifica il fast path per una WebView/canvas fullscreen come backlog.

1. Prima misurare su un gioco campione ripetibile: CPU JS, paint/compositing, upload e copie GPU, tempo di present, frame oltre budget, p50/p95/p99, memoria e consumo. Tracce Tracy/Perfetto sono già disponibili in build diagnostiche; registrare dispositivo, driver, refresh rate e configurazione.
2. Se il costo è nel percorso offscreen/blit, confrontare un prototipo di composizione diretta della singola WebView; se è nel disegno Web, ottimizzare il passaggio responsabile. Scegliere wgpu solo se l'interoperabilità GPU e i benchmark sostengono la scelta. Non duplicare l'intero renderer per eliminare `glow`.
3. Definire condizioni di attivazione verificabili e fallback automatico al percorso WebRender: overlay/dialoghi, UI DOM sovrapposta, trasparenza, effetti CSS, capture e altri casi che richiedono composizione completa. Il cambio percorso non deve alterare stacking, focus, accessibilità o semantica delle API.
4. Testare ingressi/uscite dal fast path, resize, DPI, alpha/color space, screenshot, context/device loss e cambi di finestra su tre desktop. Confrontare prestazioni e qualità visiva con la baseline, poi scegliere se mantenere la feature. Migrare `egui_glow` separatamente solo se il backend alternativo risolve un problema misurato della shell.

**Accettazione:** miglioramento ripetibile della metrica che ha motivato il lavoro, senza regressioni sul percorso generale. **Rollback:** disabilitare la feature e conservare WebRender/GL. Nessuna rimozione globale di OpenGL finché WebGL e gli altri consumatori la richiedono.

## 5. Allocatore: confronto condizionale jemalloc/mimalloc

`components/allocator/lib.rs` usa jemalloc dove la configurazione lo prevede, `System` su Windows/OHOS e con `use-system-allocator`. Espone `heap_reports`, `usable_size` e `libc_compat`: sostituire la sola dichiarazione `#[global_allocator]` romperebbe statistiche e interoperabilità FFI.

1. Aprire l'esperimento soltanto se profili indicano contesa, frammentazione, RSS o pause p99 attribuibili **all'allocatore Rust**. Registrare workload riproducibili: avvio, caricamento asset, sessione prolungata, teardown e gioco continuo. Separare heap JS, memoria GPU e librerie native dal consumo gestito dall'allocatore Rust.
2. Aggiungere mimalloc come selezione di build **mutuamente esclusiva** nei target interessati. Conservare la configurazione esistente come default. Adeguare `heap_reports`, `usable_size`, `libc_compat` e `allocation-tracking` con API corrette per il nuovo allocatore; verificare che malloc/realloc/free attraversino FFI con lo stesso proprietario.
3. Confrontare N run sugli stessi OS/hardware, build release, contenuto e impostazioni. Misurare RSS, picco, heap attivo, frammentazione, CPU, frame p95/p99, startup e stabilità. Riportare intervalli/variabilità, non solo una media favorevole.
4. Promuovere mimalloc a default **solo** se un miglioramento materiale è riproducibile senza regressioni di stabilità, memoria o distribuzione. Se non emerge, rimuovere l'esperimento e lasciare jemalloc/System come prima.

**Accettazione:** miglioramento documentato e test di allocazione/FFI superati su ogni target coinvolto. **Rollback:** ripristinare la selezione precedente; mai liberare memoria con un allocatore diverso da quello che l'ha creata.

## 6. Logging: eventuale convergenza su tracing

La shell usa `log`/`env_logger`, mentre `tracing`, Tracy e Perfetto sono già presenti come feature diagnostiche. Prima di cambiare cercare dove si inizializzano logger/subscriber e quali librerie esterne emettono eventi `log`; il bridge `tracing-log` può essere necessario, e i filtri/configurazioni esistenti vanno preservati.

1. Mappare punti di inizializzazione, filtri, output e feature per desktop/mobile, release e profiling. Identificare doppie emissioni reali o lacune nelle trace.
2. Se c'è un vantaggio concreto, migrare **un solo punto di inizializzazione** e usare un bridge per le dipendenze `log`, evitando doppia registrazione e duplicazione dei messaggi. Mantenere l'output di default e la configurazione utente equivalenti; non emettere trace ad alta frequenza in release.
3. Verificare errori startup, panic, filtri, output senza feature `tracing`, e build con Tracy e Perfetto. Rimuovere `env_logger` solo quando nessun target lo usa.

**Accettazione:** nessun log perso o duplicato e costo release invariato. **Stop:** migrazione invasiva dell'ecosistema Servo senza beneficio misurabile.

## Ordine di lavoro consegnabile a Codex

1. **Baseline e verifica SDL3**: checklist hardware, diagnosi macOS gamepad in harness minimo e correzioni mirate. È il lavoro più concreto; una CI verde non lo completa.
2. **Profiling del rendering e mappa `surfman`**: produrre misure e inventario prima di scrivere un nuovo backend. Un eventuale prototipo riguarda un consumatore alla volta.
3. **Audit i18n**: scegliere una duplicazione verificata e fare un confronto ICU4X circoscritto.
4. **Fast path wgpu**: iniziare solo se il profiling del punto 2 indica un costo GPU/compositing risolvibile.
5. **mimalloc e logging**: lavori opzionali, attivati rispettivamente da evidenza dell'allocatore e da duplicazioni diagnostiche concrete.

Per ogni tranche consegnare: stato iniziale e misura, cambi di codice/manifest/lockfile, `CUSTOMIZATIONS.md` e patch ove richieste, test eseguiti e piattaforme realmente provate, confronto con baseline, limiti e istruzioni di rollback. Non dichiarare conclusa una migrazione perché compila; non ampliare la portata a un'altra libreria per “ripulire” il workspace senza prova che sia nel grafo del binario interessato.
