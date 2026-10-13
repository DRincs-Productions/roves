# Roves — Prestazioni

## Obiettivo

Ridurre CPU e memoria dei giochi eseguiti con Roves fino a un comportamento comparabile a
Chrome, a parità di pagina, risoluzione e condizioni di misura.

## Regole

- Si interviene solo dopo una misura. Ogni correzione si conferma con la stessa misura, prima e
  dopo, possibilmente sul gioco vero.
- CPU e RAM si trattano separatamente.
- Nessuna API web viene rimossa dal runtime predefinito per migliorare i numeri: un gioco deve
  poter usare tutto ciò che userebbe in Chrome. Eventuali profili ridotti saranno espliciti e
  alternativi.
- La CI non ha soglie prestazionali: i runner condivisi sono troppo rumorosi. Le misure valide
  sono su hardware reale con build release, mai di debug.

## Come misurare

Tutto è spento per default e si attiva con variabili d'ambiente prima di avviare `play.exe`.

| Strumento | Cosa dà |
| --- | --- |
| `ROVES_PERF_LOG_INTERVAL_MS=5000` | Nel `roves.log`, tre tipi di riga: `[roves-perf]` (frame, redraw, presentazioni), `[roves-perf-time]` (µs per fase: `webview_paint`, `egui_run`, `shell_paint`, `present`) e `[roves-perf-constellation]` (messaggi del Constellation per tipo, con tempo di gestione) |
| `ROVES_EXTRA_ARGS="-p 5"` | Avvia un gioco già impacchettato con argomenti extra; per esempio `-p 5` attiva il profiler temporale di Servo (script, layout, paint) |
| `[roves-gc]` nel log | Sempre attivo: segnala ogni GC completa che dura 8 ms o più, con la dimensione dell'heap JS |

CPU per thread su Windows: PowerShell su `Get-Process play`, con i nomi dei thread letti tramite
`GetThreadDescription` (accesso `0x0800`).

Microbenchmark del binding JS → motore: `cargo test --release --lib bench_native_calls --
--ignored --nocapture` in `components/roves-js`.

## Valori attuali (12 ottobre 2026)

Visual novel (`visual-novel-template`, build Windows con shell v0.7.3), al menu, senza
interazione, GTX 1050, 60 Hz:

| Misura | Valore |
| --- | --- |
| CPU totale | 110% di un core |
| Thread `main` (paint WebRender, egui, present) | 38% |
| Thread del driver NVIDIA (`nvwgf2umx.dll`, senza nome) | 15,5% |
| `WebGL` / `Script#1` / `WRRenderBackend` | 6% / 5% / 4% |
| fps | 60 |
| `webview_paint` per frame | ~8,7 ms |
| `egui_run` per frame (include `webview_paint`) | ~8,9 ms |
| `present` per frame | ~7,4 ms |
| RAM | 846 MB working set, 659 MB memoria privata |
| Thread del processo | 150 |
| Messaggi al Constellation | ~120 al secondo |
| Pause del GC ≥ 8 ms | nessuna |

Riferimento: shell v0.7.3 su pagina vuota 228 MB; scena PixiJS piccola 275 MB.

## Confronto con Chrome (13 ottobre 2026)

Stesso gioco aperto in Chrome e in Roves v0.7.7, misurati insieme per 10 s:

| | CPU | GPU 3D | Memoria privata |
| --- | --- | --- | --- |
| Chrome (13 processi, compresi eventuali altri tab) | 11% | 0% | 999 MB |
| Roves | 102% | 6,5% | 973 MB |

La memoria è alla pari. Chrome però, in quel momento, aveva quasi certamente la finestra coperta:
Chrome sospende il rendering delle finestre non visibili, e lo dice lo 0% di GPU. Roves invece
continuava a disegnare 60 frame al secondo. Dalla v0.7.8 anche Roves rallenta la pagina e la
segna `hidden` quando la finestra è ridotta a icona, nascosta o coperta. Il confronto a finestre
entrambe visibili resta da rifare.

## Fluidità (frame pacing), 13 ottobre 2026

Misura: intervalli tra un `requestAnimationFrame` e l'altro per 20 s. Nella pagina di test le
scene `?perf=typewriter-pixi` e `?perf=typewriter-three` (motion + react-markdown-typewriter
sopra PixiJS o three.js) scrivono una riga `RAFSTATS` nel log.

| Scena | Browser | p95 | p99 | Peggiore | > 33 ms |
| --- | --- | --- | --- | --- | --- |
| Menu della visual novel (dopo 8 s) | Chrome | 17,0 ms | 17,1 ms | 17,3 ms | 0 |
| Menu della visual novel (dopo 8 s) | Roves v0.7.7 | 18,3 ms | 23,9 ms | 70 ms | 4 |
| typewriter + pixi (dal caricamento) | Chrome | 17,0 ms | 17,1 ms | 133 ms | 9 |
| typewriter + pixi (dal caricamento) | Roves v0.7.7 | 18,5 ms | 48,8 ms | 326 ms | 13 |
| typewriter + three (dal caricamento) | Roves v0.7.7 | 18,3 ms | 26,8 ms | 273 ms | 10 |

| gsap + pixi, 48 elementi (dal caricamento) | Chrome | 17,0 ms | 17,1 ms | 17,3 ms | 0 |
| gsap + pixi, 48 elementi (dal caricamento) | Roves v0.7.7 | 18,5 ms | 20,0 ms | 212 ms | 3 |
| motion + pixi, 48 elementi (dal caricamento) | Roves v0.7.7 | 18,5 ms | 19,7 ms | 55 ms | 4 |

La media è la stessa (60 fps); Roves però salta dei frame. Le cause misurate:
- **Compilazione degli shader di WebRender al primo uso:** paint da 180 a 410 ms nei primi
  secondi. `--pref gfx_precache_shaders=true` toglie i picchi ma blocca l'avvio per circa 4 s.
  La soluzione in corso è una cache su disco dei binari degli shader, come quella di Chrome.
- **Pause del GC di V8:** una GC completa di 19 ms durante la scena. Il GC è atomico; renderlo
  incrementale richiede barriere di scrittura in roves-js.
- A regime il paint della pagina resta sotto i 7 ms per frame.

## Librerie dei giochi

Ogni gioco usa motion o GSAP per l'interfaccia, PixiJS o three.js per la scena, Tone.js o
howler.js per l'audio. Tutte devono funzionare bene in Roves. Le scene `?perf=` della pagina di
test le misurano.

| Libreria | API del motore | Stato |
| --- | --- | --- |
| motion | Web Animations (`element.animate`), altrimenti stili scritti da JavaScript a ogni frame | Funziona via JavaScript. Le Web Animations di Servo sono incomplete: accese, motion si blocca (il typewriter si ferma alla prima lettera), quindi restano spente |
| GSAP | Stili scritti da JavaScript a ogni frame | Funziona; paint della pagina sotto gli 8 ms con 48 elementi |
| PixiJS, three.js | WebGL 2 | Funzionano; ogni chiamata WebGL è un messaggio verso il thread WebGL |
| Tone.js | Web Audio, AudioWorklet | `AudioWorkletNode` assente |
| howler.js | Web Audio, `<audio>`, `canPlayType` | WAV, MP3, OGG Vorbis e FLAC sì; Opus, AAC, M4A e MP4 rispondono "" pur con i plugin GStreamer nel pacchetto |

## Aperto

- **RAM:** circa 850 MB già al menu. Serve una ripartizione tra heap JS
  (righe `[roves-gc]`), immagini e texture, e il resto del processo.
- **Pause del GC:** il GC di V8 è atomico e su un solo thread. Marcatura parallela o
  concorrente non sono sicure, perché gli hook di trace di Servo non sono thread-safe; quella
  incrementale richiede barriere di scrittura in roves-js. Da affrontare se le righe
  `[roves-gc]` mostrano pause lunghe.
- **Paint della WebView sul thread `main`:** è quasi tutto il render di WebRender (mediana
  "Painting" del profiler di Servo ~4,3 ms) e avviene a ogni frame perché il menu anima un
  canvas PixiJS. Il thread "senza nome" al 15% è il driver NVIDIA che esegue quel lavoro GL.
  Si riduce solo con meno lavoro GL per frame (WebRender o il percorso WebGL → composizione).
- **Confronto a finestre visibili con Chrome:** da rifare con le due finestre affiancate. A
  finestra visibile, il costo per frame di Roves (render di WebRender più driver) resta il punto
  debole.
- **Latenza audio:** buffer di uscita portato da 200 a 40 ms (v0.7.8). Da verificare a orecchio
  aprendo le impostazioni del gioco.
- **Formati audio:** `canPlayType` risponde vuoto per AAC, M4A e WebM. Probabilmente mancano
  plugin GStreamer nel pacchetto.
- **Gamepad:** sostituire il polling hot-plug residuo (1 Hz) con gli eventi SDL.
- **Steam:** rendere adattivo il polling delle callback nelle build `steam`.
- **Allocatore:** mimalloc diventa default solo con un miglioramento ripetibile su hardware
  reale.

Fatto il 12 ottobre: presentazione diretta predefinita (CPU da 98,6% a 91,1%; egui ora costa
~0,1 ms per frame e non viene più tassellato).

Fatto il 13 ottobre (v0.7.7):
- icone SVG del menu della visual novel: compaiono 230 ms dopo il menu, prima 1,4-2 s. Le
  immagini `data:` non passano più dalla rete;
- cache HTTP su disco: al secondo avvio nessuna richiesta di rete per l'audio remoto.

Interventi già fatti e relative misure: `CUSTOMIZATIONS.md`.
