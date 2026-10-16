# Dipendenze: decisioni attuali e lavoro aperto

Revisione del 16 ottobre 2026. Lo storico degli aggiornamenti completati è in
[CUSTOMIZATIONS.md](../CUSTOMIZATIONS.md); il backlog operativo è in [TODO.md](../TODO.md).

Il desktop usa il motore Servo personalizzato con V8 tramite roves-js. La migrazione da
SpiderMonkey e quella finestra/input a SDL3 sono completate: non sono più candidati futuri.
WebRender resta il renderer completo. I risultati prestazionali sono in
[ROVES_PERFORMANCE.md](../ROVES_PERFORMANCE.md), senza dedurre vantaggi dalla lingua Rust.

## Direzione

- Preferire librerie mantenute e riusare Stylo/WebRender per le animazioni; completare il
  collegamento Web Animations prima di attribuire a motion accelerazione del compositore.
- Conservare Web Audio e HTML media come contratto dei giochi. AudioWorklet richiede un
  ambiente JS audio dedicato e un collegamento al grafo servo-media, non una API Roves alternativa.
- Conservare GStreamer per decoder/player/sink; cambiare il sink soltanto con evidenze di
  dropout, latenza o CPU imputabili ad esso. Le risposte canPlayType non bastano a validare playback.
- PixiJS e three.js usano WebGL 2. Misurare messaggi, sincronizzazioni e copie prima di cambiare
  il protocollo; una nuova libreria grafica non sostituisce automaticamente la semantica WebGL.
- mimalloc è già disponibile come esperimento con feature dedicata. Il guadagno CPU/RAM
  resta da misurare sul medesimo carico, senza cambiare contemporaneamente renderer o GC.
- CEF e le WebView di sistema non sono backend desktop candidati. Le piattaforme desktop
  devono distribuire un runtime controllato e offrire lo stesso contratto web.

## Candidati ancora aperti

RealFFT 3.5 è integrata nel ramo di sviluppo per sostituire la DFT quadratica di AnalyserNode.
Piano FFT, spettro e scratch vengono riusati; i risultati numerici sono verificati in CI.
Fonti: [RealFFT](https://github.com/HEnquist/realfft),
[API e licenza MIT](https://docs.rs/realfft/3.5.0/realfft/).

[web-audio-api-rs](https://github.com/orottier/web-audio-api-rs) offre un grafo Web Audio e
worklet Rust, ma richiede un bridge JS per i processori di Tone.js; il binding
[Node.js](https://github.com/ircam-ismm/node-web-audio-api) dipende da N-API e non si innesta
direttamente nei binding Servo. Nessuna delle due librerie è stata aggiunta inutilizzata.
Per WAAPI si riusano Stylo e WebRender già presenti: una libreria tween generica non copre
il contratto Animation/KeyframeEffect, promise, eventi e compositing del browser.

Cache shader persistente WebRender; avvio pigro dei sottosistemi inutilizzati; riduzione
roves-js attraverso binding V8 diretti; packaging con librerie esterne dove sostituiscano
realmente codice mantenuto da Roves. Il percorso diretto A è già predefinito dalla v0.7.4;
il percorso B per eliminare offscreen e blit rimane un esperimento da misurare, descritto in
[FAST_PATH_RENDERING.md](FAST_PATH_RENDERING.md). Non richiede di sostituire egui o WebRender.
