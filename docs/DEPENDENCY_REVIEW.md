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

## Inventario e valutazione

Le versioni correnti sotto sono estratte da `Cargo.lock`, non dedotte dai soli
range di `Cargo.toml`. Le versioni native GStreamer provengono dal bootstrap.

| Gruppo | Versione Roves | Candidato/verifica | Valutazione |
|---|---|---|---|
| SpiderMonkey bindings | mozjs 0.21.0; mozjs_sys 140.13.0-0 | mozjs 0.21.6, che richiede mozjs_sys 140.14.0-0 | Prima candidata: restare nella serie 0.21 e leggere il diff prima di aggiornare |
| SpiderMonkey salto di serie | Pin workspace `=0.21` | mozjs 0.26.0 richiede mozjs_sys 153.0.0-1 | Migrazione ampia, separata dal patch update e da eventuali lavori sulle barriere GC |
| Media nativo | macOS bootstrap 1.22.3; Windows bootstrap/CI 1.22.8 | GStreamer stabile 1.28.7 | Priorità alta per pipeline, manutenzione e compatibilità codec; testare il pacchetto intero |
| Media Rust | gstreamer 0.25.3; glib 0.22.8 | Non selezionato un nuovo target dei binding | Non confondere versioni dei binding con runtime nativo |
| GUI | egui/egui-winit/egui_glow 0.34.3 | egui 0.36.2 | Candidata media: aggiornamento coordinato e verifica dialoghi/accessibilità |
| Dialoghi | egui-file-dialog 0.13.0 | La pagina consultata riporta 0.13.0 | Compatibilità con nuova egui da confermare; non presumere che il dialogo accetti 0.36 |
| Windowing | winit 0.30.13 | Release ufficiale verificata 0.30.13 | Tenere: non c'è un aggiornamento stabile dimostrato dalle fonti consultate |
| Surface management | surfman 0.13.0 | Docs consultate 0.13.0 | Tenere; sostituirlo richiede preservare interop e condivisione superfici |
| Renderer web | webrender/webrender_api 0.70.0 | Registry consultato riporta 0.70.0 | Nessun aggiornamento più recente verificato; alcune pagine latest risultano datate |
| ANGLE wrapper | mozangle 0.6.0 | mozangle 0.7.0 | Candidata media per WebGL, soprattutto Windows; verificare il codice effettivamente compilato |
| Async runtime | tokio 1.53.1 | Docs consultate 1.53.1 | Tenere: correggere prima I/O bloccante e copie nell'integrazione |
| Raster images | image 0.25.10 | Docs consultate 0.25.10 | Tenere il contenitore; profilare decoder e feature prima di sostituzioni |
| Compressione pack | zstd 0.13.3; zstd-sys con zstd 1.5.7 | zstd 0.14.0, dipendenza zstd-safe 8 | Candidata circoscritta: non equivale a un guadagno automatico di decompressione |
| Allocatore Rust | tikv-jemallocator/tikv-jemalloc-sys 0.6.1 dove selezionati | tikv-jemallocator 0.7.0 | Candidata media con sys coordinato e controlli delle API statistiche |
| Unicode/layout | ICU4X locid/segmenter 1.5.0, properties 1.5.1 | icu_segmenter 2.3.0 consultato | Migrazione major, bassa priorità prestazionale per giochi canvas; test testuale esteso |
| GPU APIs | wgpu-core/types 30.0.0; anche serie 29 tramite altre dipendenze | Non scelto un nuovo target | Valutare il gruppo completo, non aggiornare una sola versione guardando il nome wgpu |

Fonti primarie per JS: [mozjs 0.21.6](https://docs.rs/crate/mozjs/0.21.6)
e [mozjs 0.26.0](https://docs.rs/crate/mozjs/0.26.0).
Per media: [release notes GStreamer 1.28](https://gstreamer.freedesktop.org/releases/1.28/).
Per GUI/windowing: [egui 0.36.2](https://docs.rs/crate/egui/0.36.2),
[release egui](https://github.com/emilk/egui/releases/tag/0.36.2),
[winit 0.30.13](https://github.com/rust-windowing/winit/releases/tag/v0.30.13),
[surfman 0.13.0](https://docs.rs/crate/surfman/0.13.0),
[egui-file-dialog](https://docs.rs/crate/egui-file-dialog/0.13.0).
Altre verifiche: [WebRender registry](https://crates.io/crates/webrender/versions),
[tokio 1.53.1](https://docs.rs/crate/tokio/1.53.1),
[image 0.25.10](https://docs.rs/crate/image/0.25.10),
[zstd release consultata](https://docs.rs/crate/zstd/latest),
[jemallocator release consultata](https://docs.rs/crate/tikv-jemallocator/latest),
[mozangle release consultata](https://docs.rs/crate/mozangle/latest),
[ICU segmenter release consultata](https://docs.rs/crate/icu_segmenter/latest).

## Aggiornamenti che meritano una prova

### mozjs: patch prima, nuovo motore dopo

Il pin `js = { package = "mozjs", version = "=0.21", ... }` richiede esattamente
0.21.0, non tutta la serie 0.21.x. Quindi un semplice update del lockfile non
seleziona 0.21.6: serve una modifica deliberata al requisito e un lockfile coerente.
Confrontare il codice e le modifiche native tra i due pacchetti; il catalogo delle
versioni non dimostra da solo quali difetti vengano corretti.

0.26 cambia anche la versione nativa dell'engine. Verificare binding generati,
rooting/tracing, API JS, compilazione JIT/Wasm e test Web Platform pertinenti.
Non presumere che aggiornare SpiderMonkey risolva le pre-barriere Servo: il
commento in `components/script/script_runtime.rs` segnala un problema di
integrazione. La roadmap GC richiede una verifica indipendente.

### GStreamer: intervenire sulla versione che viene distribuita

`python/servo/platform/macos.py` fissa gli installer 1.22.3; Windows fissa 1.22.8
in `windows.py` e nel workflow release. Una macchina può avere un runtime diverso:
registrare la versione realmente installata e quella incorporata nel bundle.
Aggiornare soltanto il crate Rust non cambia automaticamente questi installer.

La serie 1.28 documenta miglioramenti della decodifica hardware su Apple e
integrazione D3D12 su Windows, oltre a correzioni; il beneficio dipende da codec,
hardware e pipeline selezionata. Non attribuire questi vantaggi ai giochi che
usano solo audio o ai plugin che non vengono distribuiti. Fonte:
[release notes ufficiali](https://gstreamer.freedesktop.org/releases/1.28/).

La migrazione deve aggiornare URL, asset disponibili, runtime e devel insieme,
cache/marker, librerie e plugin selezionati, packaging macOS/Windows e CI.
`python/servo/gstreamer.py` e le liste plugin sono parte della verifica.
Testare audio Web Audio, video, seek, fine stream, pausa/ripresa, codec distribuiti
e WebRTC se incluso. Verificare ABI, requisiti GLib/OS e redistribuzione del
pacchetto. Nessuna specifica vulnerabilità della build Roves è stata accertata
in questa analisi; le release notes non sostituiscono un audit della build effettiva.

### egui: famiglia coordinata, beneficio della shell

Aggiornare insieme egui, egui-winit, egui_glow ed eventuali emath/epaint risolti,
con una versione compatibile del file dialog e di AccessKit. I tipi delle diverse
serie 0.x possono essere incompatibili: evitare due GUI separate per adattare
un solo widget. Ispezionare le API usate nelle patch della shell e verificare
splash, dialoghi, clipboard, IME, input, fullscreen e accessibilità.

Le release 0.36.2 documentano correzioni GUI, non una garanzia di aumento degli
FPS del gioco. Confrontare costo egui con contenuto statico e overlay animato.
Se la shell è marginale, tenere la versione esistente finché c'è una ragione
funzionale o di manutenzione per migrare.
[Changelog ufficiale](https://github.com/emilk/egui/releases/tag/0.36.2).

### zstd e allocatore: esperimenti circoscritti

Il packer usa tar+zstd con lettura/estrazione streaming e conserva già alcuni
formati compressi in tar non compresso. Un aggiornamento del wrapper zstd va
confrontato con il codice nativo risolto, i flag e la compatibilità tra pack
vecchi e nuovi. Il primo esperimento deve misurare pack/estrazione, CPU, RAM e
avvio freddo/caldo, non FPS su una scena già caricata.

Per jemallocator aggiornare wrapper e sys in modo compatibile. Roves usa API
specifiche per `mallctl`, statistiche, `usable_size` e funzioni malloc/free:
non basta sostituire `#[global_allocator]`. Su Windows il percorso corrente è
specifico della piattaforma; la modifica del ramo jemalloc non copre Windows.
Registrare throughput, frammentazione/RSS e p99 dei frame. Le allocazioni di
SpiderMonkey, GStreamer e altre librerie native non seguono necessariamente
l'allocatore globale Rust.

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

### Audio: GStreamer e Kira

**GStreamer** conserva compatibilità con media Web, codec, streaming e WebRTC.
È pesante e la distribuzione di plugin/runtime deve essere resa uniforme.

**Kira** è progettata per giochi e offre mixer, clock, tween, streaming e audio
spaziale. È interessante per un'API Roves nativa e per audio di gioco a latenza
controllata. Svantaggio: non implementa Web Audio, HTML media o WebRTC; sostituire
GStreamer richiederebbe ricostruire la semantica browser. Due stack audio possono
competere per device, focus e mixing. Fonte: [Kira](https://docs.rs/kira/latest/kira/).

Decisione: aggiornare GStreamer per le API Web; valutare Kira come backend o API
gaming solo dopo una mappa precisa delle funzioni Web Audio richieste, con un
unico coordinatore dei dispositivi.

### Tracy e Perfetto

[Tracy](https://github.com/wolfpld/tracy) è un candidato prioritario per build
diagnostiche: frame, zone CPU/GPU, allocazioni e lock. Svantaggi: integrazione
nativa, overhead da quantificare e formato/tool dedicato; non deve essere attivo
nelle release normali.

[Perfetto](https://perfetto.dev/docs/) consente timeline, eventi per thread e
analisi programmabile, oltre al confronto con Chromium. Svantaggi: SDK/export
più complessi di CSV e costo dei dati ad alta frequenza.

Decisione: buffer e aggregati Roves restano fonte stabile; exporter Perfetto per
tracce confrontabili e Tracy opzionale nelle build developer.

### Allocatore

mimalloc resta un buon esperimento A/B incorporabile e uniforme. Può migliorare
latenza o frammentazione delle allocazioni Rust, ma non controlla automaticamente
heap SpiderMonkey, risorse GPU e tutte le librerie native. Va misurato con RSS,
p99 e ownership corretta delle allocazioni cross-FFI.

## Piano corretto

1. Telemetria dei frame ed exporter Perfetto; Tracy opzionale.
2. Baseline del medesimo fork Servo sui tre desktop con suite di conformità Roves.
3. Servo Game Profile: pacing, composizione diretta e repaint separati.
4. Esperimenti isolati su mozjs, mozangle e allocatore.
5. Spike SDL3 soltanto come possibile piattaforma interna della shell embedded.
6. Prototipo wgpu/canvas fast path dentro Servo, preservando fallback WebRender.
7. Valutazione Kira per API gaming senza rompere Web Audio.
8. Solo dopo evidenze insufficienti da SpiderMonkey, studio V8/JSC come port
   completo; nessun motore JS viene scambiato senza binding e test equivalenti.

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
| Hermes | Bytecode compatto, attenzione a startup e memoria | Progettato per React Native, non DOM; throughput dei giochi da dimostrare | Studio RAM/avvio, non prima scelta fluidità |
| QuickJS | Piccolo, rapido all'avvio | Interprete, rischio throughput insufficiente | Mod e script secondari |
| Boa | Rust e facile embedding | Dichiarato sperimentale e interprete | Non candidato production oggi |

Fonti: [V8 embedding](https://v8.dev/docs/embed),
[JavaScriptCore](https://docs.webkit.org/Deep%20Dive/JSC/JavaScriptCore.html),
[Hermes](https://github.com/facebook/hermes) e [Boa](https://boajs.dev/docs/intro).

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

1. Costruire baseline ricostruibile da Servo 0.5.0 + patch Roves, registrando toolchain, target, feature, lockfile e librerie native effettive.
2. Provare mozjs 0.21.6 in una patch separata: leggere diff, aggiornare pin/lock, test JS/DOM/workers/Wasm e GC pertinente. Non abilitare GC incrementale.
3. Provare la nuova distribuzione GStreamer in una patch separata per installer/plugin/packaging e testare le app confezionate, non soltanto il link in CI.
4. Valutare egui con dipendenze compatibili; se bloccata dal dialogo, documentare costo di adattamento invece di forzare il bump.
5. Provare zstd e allocator separatamente; valutare mozangle su workload e driver WebGL pertinenti.
6. Investigare deduplicazione/feature solo per il grafo desktop effettivo; rinviare le sostituzioni architetturali salvo nuove prove.

Per ogni candidato: compilazione sui target interessati, test pertinenti, avvio
del bundle, input/audio/grafica/salvataggi, p95/p99 dei frame, caricamenti, RSS,
dimensione e tempi di build. Alternare baseline/candidato con contenuto e cache
controllati. Un bug fix utile può giustificare un update senza guadagno FPS;
un update prestazionale richiede misure che superino la variabilità tra run.

Eseguire audit delle advisory applicabili al lockfile e ai componenti nativi
quando si prepara una release; nessun audit completo è stato eseguito qui.
Gli aggiornamenti di runtime richiedono patch coerenti con la ricostruzione
upstream usata dai workflow, oltre ai file della checkout corrente.

## Limiti della verifica attuale

Sono stati letti manifest, lockfile, call site pertinenti, allocator, packer,
bootstrap nativo e workflow release. Fonti online primarie sono state consultate
per i candidati indicati. Alcune pagine latest di WebRender/file-dialog sono
datate: non sono usate per affermare l'assenza assoluta di altre release.
Non sono stati eseguiti Cargo resolution, compilazione, audit o benchmark:
Cargo e un ambiente desktop/GPU non sono disponibili qui. Le compatibilità
proposte restano da dimostrare; questo branch salva una decisione motivata e un
piano di prova, non aggiornamenti già validati.
