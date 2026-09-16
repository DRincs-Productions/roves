# Aggiornare o sostituire librerie in Roves

Analisi del 13 settembre 2026, basata su `main` al commit
`82f179787fdf088da81d27a8d45f4cd2dfdbf6e9`. Branch indipendente:
`analysis/dependency-review`. Nessuna modifica a manifest, lockfile o runtime.
L'analisi riguarda soprattutto il motore e il confezionamento desktop; le
modifiche mobile e il branch dell'analisi prestazionale non sono inclusi.

## Decisione proposta

La preferenza progettuale aggiornata è accettare cambiamenti architetturali
quando rendono Roves più adatto ai videogiochi, mantenendoli però come prototipi
separati e confrontabili prima di sostituire il percorso stabile. Aggiornamenti
mirati di mozjs, GStreamer, egui, mozangle e allocatore restano utili, ma non sono
la sola strada. Le prime nuove dipendenze da valutare sono strumenti di profiling
orientati ai frame; le architetture candidate sono un backend WebView desktop,
un profilo Servo specializzato per giochi e, solo come progetto distinto, un
runtime Canvas/WebGL/WebGPU a compatibilità Web ridotta.

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

## Valutazione architetturale orientata ai videogiochi — 2026-09-16

### Criterio fondamentale: quale compatibilità conservare

Servo non è soltanto un renderer: fornisce DOM, CSS, layout, eventi, fetch,
storage, Canvas, WebGL/WebGPU, Web Audio, workers e integrazione JavaScript.
Una libreria grafica da videogiochi sostituisce solo una parte di questo insieme.
Prima di scegliere un'architettura, ogni gioco Roves va classificato:

- **Web completo:** usa DOM/CSS e librerie browser oltre al canvas.
- **Canvas-first:** usa il DOM quasi solo per creare canvas e bootstrap.
- **Runtime controllato:** il gioco può dipendere da una API Roves ridotta e
  accetta di non essere più una normale applicazione Web.

Questa distinzione decide se una sostituzione è un'ottimizzazione o la creazione
di un nuovo runtime incompatibile.

### Candidato A — backend WebView desktop con Wry

Wry incorpora i motori di sistema: WebView2 su Windows, WKWebView su macOS e
WebKitGTK su Linux. È il candidato con il minor tempo per ottenere un confronto
architetturale reale, e replica sul desktop la decisione già presa per Android.

**Vantaggi:** compatibilità Web elevata, motori maturi, meno codice browser da
mantenere in Roves, shell Rust e bridge nativi conservabili, ottimo backend di
riferimento per verificare se il problema percepito appartiene a Servo.

**Limiti:** motore e comportamento cambiano per piattaforma; WebGPU, codec e
feature possono non essere uniformi. Su Windows il motore è Edge/Chromium:
Roves può eliminare UI e servizi superflui della shell, ma non possiede il
renderer/GC e non ha una base credibile per promettere di battere Chrome nel
cuore del workload. Aggiornamenti del runtime di sistema sono meno controllabili.

**Decisione proposta:** prototipo opzionale desktop, non sostituzione immediata.
Eseguire lo stesso harness su Servo, Wry e Chrome. Se Wry elimina gli scatti,
diventa sia fallback distribuibile sia oracle per isolare il percorso Servo.

Fonte: [documentazione Wry](https://docs.rs/wry/latest/wry/) e
[matrice WebView Tauri](https://v2.tauri.app/reference/webview-versions/).

### Candidato B — CEF, Chromium confezionato e controllato

CEF offre Chromium embedded consistente sui tre desktop e permette di fissare
la versione distribuita.

**Vantaggi:** compatibilità molto vicina a Chrome, comportamento più uniforme di
tre WebView di sistema, ecosistema e diagnostica Chromium.

**Limiti:** dimensione elevata, aggiornamenti di sicurezza continui, architettura
multiprocesso e consumo RAM difficilmente coerenti con l'obiettivo di una shell
leggera. Permette di eguagliare più rapidamente Chrome, non crea facilmente un
vantaggio strutturale su Blink/V8.

**Decisione proposta:** usarlo eventualmente come esperimento o baseline
controllata, non come prima scelta strategica.
Fonte: [documentazione CEF](https://chromiumembedded.github.io/cef/).

### Candidato C — “Servo Game Profile”, raccomandato per differenziarsi

Conservare SpiderMonkey, DOM e WebRender, ma creare un percorso esplicitamente
specializzato per una singola WebView fullscreen:

- refresh driver guidato dal display e frame pacing misurabile;
- composizione diretta quando overlay e dialoghi non sono attivi;
- separazione tra repaint del gioco e repaint della shell;
- profilo di feature per rimuovere funzioni browser non richieste dalla matrice
  Roves, senza rimuoverle alla cieca;
- priorità e pool coordinati tra JS, scene building, decoder e I/O;
- warm-up controllato di shader/pipeline e risorse previste dal gioco;
- policy esplicite per fullscreen, focus, occlusione, controller e latenza input;
- API Roves e marcatori prestazionali integrati nel ciclo del frame.

Questo è il percorso con la migliore possibilità teorica di superare Chrome in
un workload ristretto, perché conserva un motore Web ma può eliminare lavoro da
browser generalista. È anche quello che richiede più lavoro sul fork e test di
correttezza. Non esiste una singola “game library” che implementi questo profilo.

### Candidato D — runtime Canvas-first: JS + wgpu + API Roves

Architettura radicale: mantenere un motore JavaScript e implementare soltanto le
API richieste dai giochi, usando wgpu per GPU e una shell/input/audio dedicata.
Potrebbe ridurre memoria, superficie e scheduling non necessari.

Il costo è assimilabile a creare un piccolo browser/game engine: occorre fornire
Canvas/WebGL o migrare i giochi a WebGPU, fetch, timing, workers, input, audio,
storage, immagini/font e abbastanza DOM per le librerie usate. wgpu implementa
l'accesso GPU, non HTML, CSS, Canvas2D o WebGL. Three.js/PixiJS non diventano
automaticamente compatibili collegando wgpu.

**Decisione proposta:** trattarlo come possibile Roves 2 o runtime alternativo,
solo dopo un inventario delle API realmente usate. Ha senso se si accetta una
piattaforma Roves più stretta del Web; non come sostituzione trasparente di Servo.
Fonte: [wgpu](https://wgpu.rs/) e [specifica WebGPU](https://www.w3.org/TR/webgpu/).

## Librerie candidate con una reale ottica gaming

### Tracy e Perfetto — priorità alta per lo sviluppo

[Tracy](https://github.com/wolfpld/tracy) è un profiler di frame in tempo reale
con zone CPU/GPU, memoria, lock e telemetria, progettato anche per videogiochi.
È un candidato forte per build diagnostiche e per correlare frame, scene building,
GC e presentazione. Non deve però diventare l'unico formato dei benchmark o una
dipendenza attiva nelle release normali.

[Perfetto](https://perfetto.dev/docs/) offre tracce temporali, track per thread e
analisi programmabile. È particolarmente utile per confrontare eventi Roves con
tracce di sistema e Chromium. La scelta consigliata è mantenere il buffer e lo
schema aggregato Roves come fonte stabile, poi aggiungere un exporter compatibile
con Perfetto/Chrome Trace; Tracy può fornire la vista interattiva profonda nelle
build da sviluppatore.

### SDL3 — interessante, ma non sostituisce Servo

SDL3 è veramente orientato ai giochi e copre finestra, display, input, gamepad,
aptica, audio e API GPU. Potrebbe diventare la piattaforma della shell di un
runtime Canvas-first, oppure migliorare controller e dispositivi.

Nel percorso Servo attuale sostituire winit/surfman con SDL3 non risolve DOM,
JavaScript, WebRender o frame pacing e rischia di complicare condivisione delle
superfici. Valutarlo prima per il sottosistema gamepad/aptica o nel prototipo D,
non come modifica globale iniziale.
Fonte: [API SDL3](https://wiki.libsdl.org/SDL3/APIByCategory).

### Kira — ottima libreria game-audio, ma API diversa dal Web Audio

[Kira](https://docs.rs/kira/latest/kira/) offre mixer, clock, tween, streaming e
audio spaziale orientati ai giochi. È un buon candidato per una futura API audio
nativa Roves o per il runtime controllato.

Non è una sostituzione trasparente di GStreamer/Web Audio: i giochi Web si
aspettano AudioContext, nodi, scheduling e media browser. Potrebbe convivere come
API opzionale, ma due motori audio richiedono policy su device, focus e mixing.

### QuickJS — piccolo, deterministico, non il candidato per massimi FPS JS

QuickJS è piccolo, incorporabile, con avvio rapido e garbage collection basata
su reference counting con rimozione dei cicli. Questo lo rende interessante per
script di configurazione, mod o un runtime molto controllato.
La documentazione lo descrive come interprete: prima di usarlo per gameplay
JavaScript intensivo servono benchmark contro SpiderMonkey JIT. Sostituirlo in
Servo richiederebbe comunque rifare binding DOM, rooting e integrazione, quindi
non è la scorciatoia per correggere il GC corrente.
Fonte: [QuickJS](https://bellard.org/quickjs/quickjs.html).

### SpiderMonkey, WebRender, ANGLE e surfman

- **SpiderMonkey:** tenere come motore principale nel profilo Servo. Provare prima
  mozjs 0.21.6, poi un prototipo separato con nuova major. È già un motore JIT
  browser di fascia alta; nessun motore JS è specificamente “gaming” mantenendo
  automaticamente i binding Web di Servo.
- **WebRender:** tenere nel profilo Servo. È già un renderer GPU adatto a scene
  Web; ottimizzare percorso e composizione prima di sostituirlo.
- **ANGLE/mozangle:** candidato concreto per aggiornamenti WebGL e compatibilità
  driver, soprattutto Windows. Testare per matrice GPU, shader e stutter.
- **surfman:** mantenere finché serve l'interoperabilità delle superfici Servo.
  SDL/glutin non sono sostituzioni equivalenti senza ridisegnare il compositore.

### Allocatore: mimalloc come esperimento, non architettura

mimalloc resta un candidato ragionevole per una build A/B perché l'intervento è
più delimitato. Può influenzare latenza e frammentazione delle allocazioni Rust,
ma non governa automaticamente heap SpiderMonkey, texture GPU o tutte le librerie
native. Misurare RSS e p99, non soltanto throughput sintetico.

## Percorso raccomandato

1. Implementare telemetria frame con schema Roves ed exporter Perfetto; integrare
   Tracy solo nelle build di sviluppo.
2. Costruire un backend desktop Wry opzionale e confrontarlo con Servo e Chrome
   sullo stesso gioco. Non rimuovere Servo.
3. Implementare il Servo Game Profile: display pacing, percorso diretto,
   repaint separati e feature profile misurato.
4. Eseguire in parallelo esperimenti isolati mozjs 0.21.6, mozangle e mimalloc,
   uno per volta sulle tre architetture pertinenti.
5. Decidere con dati se Wry è un backend di produzione, Servo resta principale
   o entrambi diventano selezionabili per gioco/piattaforma.
6. Avviare il runtime Canvas-first soltanto se l'inventario dimostra che i giochi
   possono rinunciare a DOM/CSS e accettare una API Roves specifica.

Questa sequenza accetta cambiamenti architetturali come richiesto, ma conserva
baseline e rollback: “cambiare tutto e poi misurare” non consente di capire quale
scelta abbia aiutato. Ogni prototipo viene ritestato completamente, mentre le
modifiche interne restano attribuibili.

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
