# Aggiornare o sostituire librerie in Roves

Analisi del 13 settembre 2026, basata su `main` al commit
`82f179787fdf088da81d27a8d45f4cd2dfdbf6e9`. Branch indipendente:
`analysis/dependency-review`. Nessuna modifica a manifest, lockfile o runtime.
L'analisi riguarda soprattutto il motore e il confezionamento desktop; le
modifiche mobile e il branch dell'analisi prestazionale non sono inclusi.

## Decisione proposta

Il desktop deve restare un runtime **embedded basato su una versione custom di
Servo**, distribuita e controllata da Roves. Possiamo modificare Servo anche
pesantemente e sostituire sue librerie interne, ma Windows, macOS e Linux devono
esporre lo stesso contratto Web/Roves e comportarsi nello stesso modo. Le WebView
di sistema restano una scelta mobile e un riferimento esterno di benchmark, non
un backend desktop candidato; anche CEF non è la direzione del prodotto.

La strategia è quindi un **Servo Game Profile**: specializzare scheduling,
compositore, input, audio, caricamento e feature del fork per un singolo gioco,
valutando aggiornamenti o sostituzioni interne con implementazioni incorporabili
e versionate da Roves. Le prime nuove dipendenze da valutare sono strumenti di
profiling orientati ai frame. SpiderMonkey e WebRender rimangono la baseline,
ma non sono intoccabili: una sostituzione richiede un piano per binding DOM,
semantica Web, determinismo multipiattaforma e migrazione completa.

Priorità significa ordine di indagine, non approvazione di una versione pronta
alla distribuzione. Le versioni online sono quelle osservate nelle fonti indicate;
ricontrollare release, eventuali yanked e compatibilità quando si implementa.

**Stato (2026-09-16): la maggior parte dei candidati di questo documento è stata
implementata e verificata via CI** — mozjs 0.21.6, GStreamer 1.28.7, mozangle 0.7.0,
zstd 0.14.0, tikv-jemallocator 0.7.0/0.7.1, egui/egui-winit/egui_glow 0.36.2,
egui-file-dialog 0.15.0, SDL3 per il gamepad (al posto di gilrs), e Tracy/Perfetto
(Perfetto era già presente upstream; aggiunta la feature `tracing-tracy`). Le sezioni
di inventario/analisi-per-candidato che descrivevano questi aggiornamenti come
proposte sono state rimosse da questo file: la loro implementazione reale, con root
cause dei problemi trovati e come sono stati risolti, è in
[`CUSTOMIZATIONS.md`](../CUSTOMIZATIONS.md). Il lavoro ancora aperto (sostituzione
finestra/event-loop SDL3, mozjs 0.26 major, ICU4X, wgpu fast path, mimalloc, telemetria
frame Roves) è tracciato con portata reale misurata in [`TODO.md`](../TODO.md), non qui.
Questo documento resta come registro delle decisioni architetturali e del
ragionamento che le ha motivate — non più come lista di aggiornamenti da fare.

## Decisioni confermate: SDL3 e audio Web trasparente

- **SDL3 è da implementare.** L'obiettivo è sostituire progressivamente il
  maggior numero di responsabilità duplicate possibile, iniziando da
  `gilrs`/gamepad e arrivando a finestra/event loop al posto di `winit`.
  Ogni fase deve conservare API Web, input, IME, accessibilità, fullscreen,
  superfici e comportamento sui tre desktop. `surfman` si rimuove soltanto
  quando SDL e il compositore coprono realmente context/offscreen/interoperabilità.
- **Nessuna API audio Roves per le funzioni di base.** I game engine e Tone.js
  devono continuare a usare Web Audio e HTML media. Un cambio audio è accettato
  solo se trasparente, conforme e dimostra latenza/jitter/CPU migliori.
- **Kira non è nella roadmap base.** Può essere rivalutata solo come dettaglio
  interno se implementa senza compromessi le trait e la semantica esistenti;
  non aggiungere un secondo sistema o API pubblica.
- **GStreamer va separato per responsabilità.** Il grafo Web Audio è già in
  `servo-media-audio`; GStreamer fornisce oggi il sink reale tramite appsrc,
  conversione/resampling e autoaudiosink, oltre a player, decoder, video e
  WebRTC. Il primo esperimento non deve sostituire il grafo: deve sostituire
  soltanto `AudioSink` con cubeb, CPAL o miniaudio.
- **web-audio-api-rs è un confronto del grafo**, non la prima sostituzione:
  valutarlo solo se conformità, scheduling o prestazioni del grafo
  `servo-media-audio` risultano insufficienti.

## Decisioni definitive della revisione del 2026-09-16

- **Hermes escluso:** la compatibilità Web completa e il throughput di gameplay
  hanno precedenza sui vantaggi mirati a React Native, startup e bytecode.
- **GStreamer mantenuto:** è attivamente mantenuto; la serie stabile 1.28 ha
  ricevuto la bug-fix 1.28.7 il 7 settembre 2026. Aggiornare runtime e plugin
  distribuiti, poi ottimizzare la pipeline corrente. Cubeb/CPAL/miniaudio si
  rivalutano soltanto se misure mostrano latenza, jitter, dropout o CPU imputabili
  al sink GStreamer. Nessuna API audio Roves di base e nessuna Kira.
- **Tracy e Perfetto durante l'implementazione:** Tracy profondo solo in build
  developer/profiling; nelle release lasciare una diagnostica Roves leggera,
  opt-in e senza logging per frame, esportabile in JSON/CSV/Perfetto.
- **mimalloc rinviato:** esperimento soltanto davanti a evidenza di contesa,
  frammentazione, RSS o pause di allocazione.
- **wgpu fast path rinviato:** ottimizzazione futura nel TODO, non nel ciclo di
  implementazione attuale.
- **SDL3 confermato:** migrazione progressiva con obiettivo di consolidare più
  sottosistemi possibile senza perdere funzionalità.

Riferimento manutenzione: [GStreamer 1.28 release notes](https://gstreamer.freedesktop.org/releases/1.28/).

## Architettura vincolante: Servo embedded e uniforme — 2026-09-16

### Vincoli non negoziabili

- Il motore desktop è il fork custom di Servo, non la WebView installata nel SO.
- La build incorpora versioni note dei componenti e non dipende dal motore Web
  scelto o aggiornato dall'utente.
- Windows, macOS e Linux espongono lo stesso insieme di API e la stessa semantica.
  Le differenze inevitabili di driver, codec e sistema devono essere normalizzate,
  rilevate nei test e mai trasformate silenziosamente in tre prodotti diversi.
- Roves resta compatibile con giochi Web secondo una matrice dichiarata. Un
  profilo canvas-first può essere un fast path interno di Servo, non un runtime
  desktop alternativo incompatibile.
- Android e iOS sono l'eccezione esplicita: usano WebView native.

Wry/WebView2/WKWebView/WebKitGTK e CEF restano utili soltanto come baseline di
confronto per benchmark. Non devono comparire nella roadmap come sostituti del
runtime desktop.

### Servo Game Profile — architettura raccomandata

Il fork può divergere in modo importante dall'impostazione da browser, purché
mantenga un contratto desktop unico:

1. refresh driver collegato al display, con scheduling e frame pacing pensati
   per fullscreen e refresh elevato;
2. percorso di composizione diretto per una sola WebView quando overlay e
   dialoghi non sono visibili;
3. repaint indipendenti per contenuto e shell;
4. fast path canvas/WebGL/WebGPU che eviti lavoro DOM/layout/compositing quando
   la pagina del gioco non lo richiede, senza cambiare la semantica osservabile;
5. budget e priorità coordinati per JS, WebRender, decoder, I/O e compilazione
   shader;
6. warm-up dichiarativo di shader, pipeline e asset attraverso API Roves;
7. input/gamepad a bassa latenza e politiche coerenti per focus, occlusione,
   fullscreen e background;
8. profilo di feature embedded generato dal packaging, validato contro la matrice
   di API richiesta dal gioco.

Il vantaggio rispetto a Chrome può nascere dalla specializzazione e dal controllo
end-to-end, non semplicemente dalla scelta di una crate diversa.

## Candidati interni al fork Servo

### SpiderMonkey e possibili motori JavaScript

**SpiderMonkey aggiornato** è la prima scelta: conserva binding, modello GC e
integrazione Servo. Prima aggiornamento mozjs compatibile, poi nuova major in un
branch dedicato. Svantaggi: rimane un motore generalista, il GC incrementale
richiede comunque correggere le pre-barriere Servo e una nuova major può cambiare
API, JIT, rooting e memoria.

**V8** è incorporabile e molto ottimizzato, ma sostituire SpiderMonkey significa
rifare binding DOM, conversioni dei valori, rooting/handle, eccezioni, promise,
worker, moduli, GC, debugging e build C++ multipiattaforma. È grande, complesso
da compilare e non è orientato specificamente ai giochi. La stessa versione
incorporata darebbe uniformità, ma il costo è paragonabile a un port del motore.

**JavaScriptCore** è un'altra opzione JIT, ma presenta lo stesso problema dei
binding e una pipeline di build/distribuzione da rendere identica sui tre desktop.
Usarlo non significa ottenere WKWebView e non offre automaticamente le API Web.

**QuickJS** è piccolo, incorporabile e rapido all'avvio, con reference counting
e raccolta dei cicli. Lo svantaggio decisivo per il runtime principale è essere
un interprete: giochi JavaScript CPU-heavy potrebbero perdere molto rispetto a
un JIT. È più adatto a mod, configurazione o script isolati. Fonte:
[QuickJS](https://bellard.org/quickjs/quickjs.html).

Decisione: SpiderMonkey resta baseline. V8/JSC possono essere studiati soltanto
come migrazione completa con prototipo DOM minimo; QuickJS non è candidato per
il gameplay principale senza risultati sorprendenti su benchmark reali.

### WebRender, wgpu e Vello

**WebRender** resta il renderer di riferimento perché implementa la scena Web,
clip, stacking, compositing e integrazione Servo. Lo svantaggio è che il percorso
generale può fare più lavoro di quanto serva a un singolo canvas fullscreen.

**wgpu** è un ottimo candidato interno per API GPU moderne, backend e fast path,
ma non sostituisce DOM, CSS, Canvas2D, WebGL o il compositore Web. Usarlo come
sostituto totale richiederebbe reimplementare queste parti; usarlo sotto percorsi
mirati di Servo è molto più realistico. Fonte: [wgpu](https://wgpu.rs/).

**Vello** può essere utile per rendering vettoriale/2D, ma non è un sostituto
completo di WebRender. Aggiungerlo crea inoltre un secondo stack GPU e possibili
duplicazioni di wgpu, cache, shader e memoria.

Decisione: ottimizzare WebRender e progettare un canvas fast path; valutare wgpu
come componente coordinato del fork, non come libreria magica che sostituisce il
browser renderer.

### winit/surfman oppure SDL3

**SDL3** è realmente orientato ai videogiochi e offre display, eventi, gamepad,
aptica, audio e API GPU con un contratto multipiattaforma. Potrebbe sostituire
parti della shell o fornire controller/aptica più completi.
Fonte: [API SDL3](https://wiki.libsdl.org/SDL3/APIByCategory).

Svantaggi: Servo usa già winit e surfman; una migrazione richiede ridisegnare
event loop, IME, accessibilità, finestre, context/surface sharing e integrazione
del compositore. SDL normalizza molte API ma non elimina differenze di driver e
sistema. Usarne soltanto gamepad/aptica può duplicare gli event loop.

Decisione: candidato architetturale serio, ma iniziare con uno spike della shell
che dimostri presentazione, input, resize, fullscreen e superfici condivise sui
tre desktop prima di sostituire winit/surfman.

**Stato (2026-09-16):** gamepad (gilrs→SDL3) fatto, senza passare dallo spike qui
descritto — sostituzione diretta, con l'accortezza reale trovata solo durante
l'implementazione che `sdl3::init()` richiede di girare sul thread `main()`, non
un thread dedicato come faceva gilrs (vedi CUSTOMIZATIONS.md). La sostituzione di
finestra/event-loop/IME/accessibilità descritta sopra **non è stata tentata**:
portata reale misurata (non stimata) in TODO.md prima di decidere di rimandarla.

### Audio: GStreamer e Kira

**GStreamer** conserva compatibilità con media Web, codec, streaming e WebRTC.
È pesante e la distribuzione di plugin/runtime deve essere resa uniforme.

**Kira** è progettata per giochi e offre mixer, clock, tween, streaming e audio
spaziale. È interessante per un'API Roves nativa e per audio di gioco a latenza
controllata. Svantaggio: non implementa Web Audio, HTML media o WebRTC; sostituire
GStreamer richiederebbe ricostruire la semantica browser. Due stack audio possono
competere per device, focus e mixing. Fonte: [Kira](https://docs.rs/kira/latest/kira/).

Decisione aggiornata: mantenere GStreamer come backend multimediale unico e
aggiornarlo alla serie stabile corrente. Kira è esclusa dalla roadmap: non
aggiungere API Roves audio né un secondo motore, salvo nuove evidenze future.

### Tracy e Perfetto

[Tracy](https://github.com/wolfpld/tracy) è un candidato prioritario per build
diagnostiche: frame, zone CPU/GPU, allocazioni e lock. Svantaggi: integrazione
nativa, overhead da quantificare e formato/tool dedicato; non deve essere attivo
nelle release normali.

[Perfetto](https://perfetto.dev/docs/) consente timeline, eventi per thread e
analisi programmabile, oltre al confronto con Chromium. Svantaggi: SDK/export
più complessi di CSV e costo dei dati ad alta frequenza.

Decisione: buffer e aggregati Roves restano la fonte stabile; exporter Perfetto
per tracce confrontabili e Tracy opzionale nelle build developer. Nelle release
pubbliche mantenere soltanto metriche aggregate e buffer circolare a basso costo,
disabilitati per default e attivabili esplicitamente per diagnosticare problemi
degli utenti. Zone profonde Tracy, call stack, allocazioni e tracing GPU restano
compilate esclusivamente nelle build developer/profiling.

### Allocatore

mimalloc non è lavoro attuale. Resta nel backlog condizionale soltanto se la
telemetria mostra contesa dell'allocatore Rust, frammentazione/RSS o pause p99
attribuibili alle allocazioni. Non controlla automaticamente heap SpiderMonkey,
risorse GPU e tutte le librerie native; un eventuale test deve preservare
ownership corretta delle allocazioni cross-FFI.

## Piano corretto

Stato per punto (2026-09-16) — dettaglio implementazione in CUSTOMIZATIONS.md,
lavoro ancora aperto in TODO.md:

1. Telemetria dei frame ed exporter Perfetto; Tracy opzionale. **Perfetto e Tracy
   fatti** (Perfetto era già upstream; aggiunta la feature `tracing-tracy`). La
   telemetria Roves con buffer circolare **non è stata iniziata** — è una feature
   nuova, non implicita in "Tracy e Perfetto" da soli; vedi TODO.md.
2. Baseline del medesimo fork Servo sui tre desktop con suite di conformità Roves.
   Verifica continua via `test.yml` a ogni push su `patches/**`.
3. Servo Game Profile: pacing, composizione diretta e repaint separati. **Non
   iniziato.**
4. Esperimenti isolati su mozjs, mozangle e allocatore. **Fatti** (mozjs 0.21.6
   patch-level, mozangle 0.7.0, tikv-jemallocator 0.7.0/0.7.1) — tutti verificati
   verdi via CI.
5. Spike SDL3 soltanto come possibile piattaforma interna della shell embedded.
   **Gamepad (gilrs→SDL3) fatto** con integrazione diretta sul main thread, non lo
   spike separato qui descritto — vedi CUSTOMIZATIONS.md per il perché (vincolo
   reale di `sdl3::init()` sul thread `main()`). **Finestra/event-loop non
   tentati** — portata reale misurata in TODO.md.
6. Prototipo wgpu/canvas fast path dentro Servo, preservando fallback WebRender.
   **Non iniziato**, rinviato per esplicita decisione.
7. Conservare GStreamer per Web Audio/media e misurarne latenza e jitter.
   **Aggiornamento a 1.28.7 fatto** (con due bug reali trovati e risolti sul lato
   Windows, vedi CUSTOMIZATIONS.md); misure di latenza/jitter **non fatte**.
8. Solo dopo evidenze insufficienti da SpiderMonkey, studio V8/JSC come port
   completo; nessun motore JS viene scambiato senza binding e test equivalenti.
   **Non applicabile ancora** — mozjs resta sulla serie 0.21 (major 0.26 rimandato,
   portata reale: ~300 file nel motore vendorizzato).

Ogni cambiamento deve produrre lo stesso comportamento osservabile sui tre
desktop, essere incorporato/versionato nel bundle e avere rollback. I benchmark
contro Chrome e WebView restano riferimenti esterni, non proposte di backend.

## Approfondimento: motori JS, SDL3 e audio trasparente

### Alternative a SpiderMonkey entro i vincoli embedded

Il problema non è soltanto incorporare un motore JS: bisogna rifare l'integrazione
Servo con WebIDL/DOM, GC, promise, worker, moduli, eccezioni e debugger. Tutti i
candidati devono essere versionati e compilati da Roves sui tre desktop.

| Motore | Vantaggi | Svantaggi per Roves | Giudizio |
|---|---|---|---|
| SpiderMonkey | Binding Servo esistenti, JIT e Wasm maturi | Generalista; pre-barriere da correggere; major update complessi | Baseline e prima scelta |
| V8 | JIT maturo e API ufficiale di embedding | Port completo dei binding, build C++ pesante, memoria; non gaming-specific | Principale sfidante se SpiderMonkey limita |
| JavaScriptCore | VM ottimizzante multi-tier | Stesso costo dei binding; pipeline Windows/Linux da controllare | Secondo sfidante |
| QuickJS | Piccolo, rapido all'avvio | Interprete, rischio throughput insufficiente | Mod e script secondari |
| Boa | Rust e facile embedding | Dichiarato sperimentale e interprete | Non candidato production oggi |

Fonti: [V8 embedding](https://v8.dev/docs/embed),
[JavaScriptCore](https://docs.webkit.org/Deep%20Dive/JSC/JavaScriptCore.html),
[Boa](https://boajs.dev/docs/intro).

V8 è quindi l'unica alternativa che oggi giustificherebbe un vero prototipo
prestazionale, ma soltanto dopo avere aggiornato e misurato SpiderMonkey. Un
benchmark JS isolato non basta: serve almeno un adapter WebIDL con Pixi/Three,
Tone, worker e Wasm, misurando warm-up, p99, pause GC e RSS.

### Cosa sostituirebbe SDL3 nel codice attuale

Oggi Roves usa:

- `winit` per finestra, display, event loop, input, resize, focus e wake-up;
- `egui-winit` per shell e accessibilità;
- `gilrs` in un thread dedicato per gamepad e force feedback;
- `surfman` in paint, WebGL, WebXR e media per context, superfici offscreen e
  condivisione col compositore;
- WebRender/ANGLE per rendering Web e WebGL.

SDL3 può sostituire in modo credibile **winit + gilrs**, unificando finestra,
eventi, controller, hotplug, rumble, trigger rumble, sensori e aptica. È utile
anche in ottica console, ma i port console continuano a richiedere SDK, accesso
e integrazioni specifiche.

Non sostituisce automaticamente surfman, WebRender, WebGL, DOM o egui. SDL_GPU
è un'API grafica, non un compositore Web. All'inizio il codice aumenterebbe:
servono adapter SDL→Servo, SDL→egui/AccessKit e una soluzione per le superfici.
Dopo una migrazione completa potrebbe diminuire eliminando thread gilrs,
conversioni winit e parte delle diramazioni piattaforma.

Spike consigliato:

1. SDL3 solo per gamepad/aptica dietro il delegate Servo;
2. finestra/event loop in un binario sperimentale;
3. superfici/compositore soltanto dopo aver validato input, IME, accessibilità,
   fullscreen, resize e presentazione sui tre desktop.

### Kira, GStreamer e Tone.js

Tone.js usa la Web Audio API. Due integrazioni diverse producono effetti diversi:

- Se Kira viene esposta come **API Roves**, aggiunge possibilità gaming ma Tone.js
  non cambia e il gioco deve usare chiamate specifiche.
- Se un nuovo motore implementa le trait `Backend`/`AudioBackend` di
  `servo-media` e la semantica Web Audio, Tone.js continua a funzionare senza
  modifiche e beneficia automaticamente di minore latenza/jitter.

Kira non implementa già AudioContext, nodi, AudioParam, automazioni, offline
context e worklet con la semantica Web. Adattarla completamente è un progetto,
non un semplice cambio di backend.

Candidati più diretti per migliorare l'API standard:

- [web-audio-api-rs](https://github.com/orottier/web-audio-api-rs), perché
  implementa Web Audio in Rust; copertura e conformità vanno verificate;
- [cubeb](https://github.com/mozilla/cubeb), I/O cross-platform a bassa latenza
  proveniente dall'ecosistema Mozilla; non implementa il grafo;
- [CPAL](https://github.com/RustAudio/cpal), I/O Rust di basso livello;
- [miniaudio](https://miniaud.io/), I/O/mixing/decodifica facilmente embedded;
- Kira, migliore per un'API gaming aggiuntiva che come rimpiazzo Web Audio.

GStreamer può restare per player HTML audio/video, codec, streaming e WebRTC,
mentre un backend dedicato gestisce il grafo Web Audio. Questa separazione
potrebbe rendere Tone.js più fluido senza API Roves, ma aumenta i backend e
richiede un unico coordinamento di clock, device, focus, volume e mixing.

Esperimento corretto: misurare Tone.js corrente (latenza output, jitter, dropout,
CPU), provare web-audio-api-rs con CPAL o cubeb dietro `servo-media`, eseguire
le stesse pagine senza modificarle e lasciare video/WebRTC a GStreamer.

### Altri candidati pertinenti

SDL3 è il candidato principale per piattaforma/input; web-audio-api-rs più cubeb
è il nuovo candidato audio trasparente; Tracy/Perfetto restano prioritari per
misurare; mimalloc per RSS/p99; wgpu per un fast path canvas interno. GLFW,
glutin, rodio o un altro runtime async coprono meno responsabilità e non sono
sostituzioni architetturali più adatte al problema.

## Sostituzioni: quali hanno senso?

| Sostituzione | Giudizio | Motivo e costo |
|---|---|---|
| jemalloc → mimalloc, come backend opzionale | Esperimento possibile | Intervento relativamente delimitabile, ma introspezione e allocator ownership richiedono adattamenti; vantaggi non misurati |
| zstd → LZ4 opzionale per pack selezionati | Solo se decompressione è il collo di bottiglia | Nuovo formato/codec nel manifest e compatibilità dei launcher; confrontare dimensioni, I/O e latenza end-to-end |
| egui → altro toolkit | Non giustificato al momento | Prima separare repaint GUI/contenuto; riscrivere overlay, input/accessibilità non elimina il costo del motore web |
| surfman → glutin/SDL | Non raccomandato come scorciatoia | Roves richiede interop offscreen e superfici condivise, non soltanto creare un contesto OpenGL |
| WebRender → wgpu/Vello | Progetto architetturale, non sostituzione locale | Un'API GPU o renderer vettoriale non rimpiazza da sola scene, clip, compositing e integrazione del motore web |
| SpiderMonkey → V8/QuickJS | Non raccomandato per questo obiettivo | Rooting, binding DOM, GC, scheduling e API andrebbero reimplementati; prestazioni migliori non dimostrate |
| GStreamer → rodio/cpal | Non equivalente | Playback audio non sostituisce pipeline video, WebRTC e integrazione Web Audio del motore |
| Tokio → altro runtime | Non raccomandato | Il problema individuato è l'I/O sincrono nell'integrazione: un altro scheduler non lo rende non bloccante |
| image → decoder specifico per formato | Solo dopo profiling | Preservare formati, animazioni, colore/alpha e limiti; il pacchetto usa già decoder specializzati come dipendenze |

Le alternative sono valutazioni d'integrazione, non risultati di benchmark.
Fonti per le capacità dichiarate: [mimalloc Rust](https://docs.rs/crate/mimalloc/latest),
[LZ4 streaming](https://docs.rs/lz4_flex/latest/lz4_flex/),
[surfman e condivisione superfici](https://docs.rs/crate/surfman/0.13.0),
[glutin](https://docs.rs/glutin/latest/glutin/),
[rodio playback audio](https://docs.rs/rodio/latest/rodio/).

## Ridurre duplicazioni e feature prima di riscrivere

Il lockfile contiene 1121 record di pacchetti e 70 nomi con più versioni.
Questo è l'inventario del workspace, non il numero di librerie incluse in un
eseguibile desktop. Non prova da solo un consumo runtime eccessivo.

Caso concreto: `servo-webgpu` dipende da wgpu-core/types 30, mentre Vello 0.9
porta wgpu 29. Si vedono anche più versioni skrifa, vello_cpu e objc2.
Non forzare una deduplicazione con override tra major incompatibili. Su un host
con Cargo usare il grafo delle feature della build effettiva per determinare
quali rami siano attivi prima di riallineare famiglie.

Il requisito workspace image disabilita i default, ma il lock mostra anche
ravif, exr e tiff. Questo non dimostra che tali decoder siano attivi nel target:
feature unification, dipendenze di altri membri e build opzionali vanno separate.
Esaminare `cargo tree -e features` per il target/profilo scelto e il binario prodotto.

Analogamente webgpu/webxr sono feature della shell e Vello è opzionale.
Una build specializzata senza funzionalità inutilizzate può ridurre dimensioni
e tempi di build, ma richiede una matrice esplicita di API supportate per i giochi.
Non promettere più FPS dalla sola rimozione di crate non usati nel frame.

## Altri elementi del progetto

ICU4X 1→2 interessa layout/fonts/script e le API di misurazione della memoria:
valutare la migrazione con shaping/segmentazione e lingue reali, non come upgrade
isolato del segmenter. Versioni crypto prerelease sono fissate nel workspace:
prima verificare i vincoli upstream e il lock, senza applicare aggiornamenti
indiscriminati o dedurre insicurezza dalla sola etichetta prerelease.

La toolchain Rust è fissata a 1.95.0 e Crown usa componenti interni del compilatore:
qualsiasi aggiornamento richiede coordinare i file indicati in rust-toolchain.toml
e verificare Crown. Nessun nuovo target Rust è stato selezionato qui.

`test-page/package.json` è una pagina diagnostica separata, non il renderer di
Roves. Aggiornare React/Pixi/Three/Tone lì cambia il workload dei benchmark:
fissare un lockfile e una build del workload prima dei confronti prestazionali.
Non attribuire un miglioramento della test-page al motore senza confronti identici.
Le versioni npm più recenti non sono state verificate: nessun bump viene proposto.

## Piano di esecuzione e verifica

**Punti 1-5: eseguiti (2026-09-16), ciascuno in patch isolata e verificato verde via
`test.yml` prima del passo successivo** — dettaglio completo, incluse due
scoperte reali durante l'implementazione (rinomina DLL GStreamer, dipendenza
`swscale` mancante, entrambe non previste dall'analisi originale), in
CUSTOMIZATIONS.md. Punto 6 non ancora affrontato.

1. ~~Costruire baseline ricostruibile da Servo 0.5.0 + patch Roves~~ — baseline
   già esistente (questo repo), verificata a ogni run CI.
2. ~~Provare mozjs 0.21.6 in una patch separata~~ — fatto, verde al primo tentativo.
   GC incrementale non abilitato, come da piano.
3. ~~Provare la nuova distribuzione GStreamer in una patch separata~~ — fatto, ma
   non solo un "provare": l'installer Windows di GStreamer 1.28.x non pubblica più
   coppie MSI, ha richiesto ripensare il meccanismo di installazione non
   interattiva da zero (vedi CUSTOMIZATIONS.md).
4. ~~Valutare egui con dipendenze compatibili~~ — fatto (egui-file-dialog 0.15.0 è
   la prima versione compatibile con egui 0.36); trovato e risolto un vero
   breaking change (`EguiGlow::run` ora passa `&mut Ui` invece di `&Context`).
5. ~~Provare zstd e allocator separatamente; valutare mozangle~~ — fatto per tutti
   e tre, patch isolate, verdi via CI.
6. Investigare deduplicazione/feature solo per il grafo desktop effettivo;
   rinviare le sostituzioni architetturali salvo nuove prove. **Non ancora
   affrontato.**

Per ogni candidato: compilazione sui target interessati, test pertinenti, avvio
del bundle, input/audio/grafica/salvataggi, p95/p99 dei frame, caricamenti, RSS,
dimensione e tempi di build. Alternare baseline/candidato con contenuto e cache
controllati. Un bug fix utile può giustificare un update senza guadagno FPS;
un update prestazionale richiede misure che superino la variabilità tra run.

Eseguire audit delle advisory applicabili al lockfile e ai componenti nativi
quando si prepara una release; nessun audit completo è stato eseguito qui.
Gli aggiornamenti di runtime richiedono patch coerenti con la ricostruzione
upstream usata dai workflow, oltre ai file della checkout corrente.

## Limiti della verifica originale (analisi del 13 settembre 2026)

Al momento di questa analisi erano stati letti manifest, lockfile, call site
pertinenti, allocator, packer, bootstrap nativo e workflow release, con fonti
online primarie consultate per i candidati indicati — ma senza Cargo resolution,
compilazione, audit o benchmark reali (Cargo e un ambiente desktop/GPU non erano
disponibili in quella sessione).

**Aggiornamento (2026-09-16): la maggior parte dei candidati è stata da allora
implementata e verificata per davvero**, via `cargo update`/`cargo metadata` per
la risoluzione delle dipendenze e via `test.yml` (build reale su Windows/macOS/
Linux + smoke test di lancio) per la compilazione — non più solo una decisione
motivata sulla carta. Il dettaglio di ogni verifica, incluse le scoperte fatte
solo durante l'implementazione reale (non prevedibili dalla sola lettura di
changelog/documentazione), è in CUSTOMIZATIONS.md. Restano senza verifica reale:
audit di sicurezza del lockfile/componenti nativi, benchmark prestazionali
(p95/p99 dei frame, latenza/jitter audio, CPU/RAM di pack/estrazione — tutti
esplicitamente richiesti da questo documento ma non eseguibili senza hardware
reale), e ovviamente tutto il lavoro ancora aperto elencato in TODO.md.
