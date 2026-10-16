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
| `ROVES_V8_FLAGS="--max-semi-space-size=32"` | Opzioni aggiuntive per V8, per provare la taratura del GC senza ricompilare |
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
segna `hidden` quando la finestra è ridotta a icona, nascosta o coperta. Misura sulla v0.7.9 (patch 0212):

| | Visibilità | fps | CPU |
| --- | --- | --- | --- |
| Pagina di prova WebGL, visibile | `visible` | 60 | 34% |
| Ridotta a icona | `hidden` (evento `visibilitychange`) | 0 | 0% |
| Ripristinata | `visible` | 60 | 27% |
| Visual novel, visibile | | 60 | 54% |
| Visual novel, ridotta a icona | `hidden`; pixi-vn chiama `muteAll()` | 0 | 1,9% (solo grafo audio) |

Fino alla v0.7.8 ogni pagina risultava `hidden` anche a finestra visibile, un difetto di Servo
corretto nella stessa patch. Il confronto a finestre entrambe visibili resta da rifare.

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
  Dalla v0.7.8 c'è una cache su disco dei binari degli shader, come quella di Chrome: sulla
  scena typewriter + pixi il paint più lento dei primi secondi scende da 176 ms (v0.7.7) a
  10-12 ms già dal secondo avvio.
- **Pause del GC di V8:** una GC completa di 19 ms durante la scena. Il GC è atomico; renderlo
  incrementale richiede barriere di scrittura in roves-js.
- A regime il paint della pagina resta sotto i 7 ms per frame.

## Costo dei binding DOM (14 ottobre 2026)

A regime (dopo 5 s) motion e GSAP su PixiJS non hanno quasi frame lunghi: motion 0 frame oltre
i 33 ms, GSAP 1. Il typewriter invece ne ha 8-11, fino a circa 200 ms, contro i 2 frame da 33 ms
di Chrome. Il GC non li spiega: in 30 s ci sono 13 scavenge (quasi tutti da 1-2 ms) e una sola
Mark-Compact da 21 ms. Il tempo va nella ricreazione delle 314 `<span>` del dialogo.

`test-page/public/dombench2.html`, ms per 1000 chiamate:

| Operazione | Chrome | Roves v0.7.9 |
| --- | --- | --- |
| `document.createElement` | 0,2 | 7,4 |
| `document.createTextNode` | 0,2 | 6,6 |
| `textContent`, `style.opacity` (scrittura) | 0,2 | 3,0-3,3 |
| `setAttribute`, `className`, `appendChild` | 0,1-0,2 | 1,4-1,8 |
| getter (`tagName`, `nodeType`) | ~0 | 0,2-0,45 |
| JavaScript puro (ciclo, oggetti) | 3,2 / 8,2 | 3,8 / 11,1 |

Dalla v0.7.10 Windows usa mimalloc al posto dell'allocatore di sistema: nelle build A/B,
ripetute 3 volte, le operazioni DOM costano dal 13% al 27% in meno (`createElement` 6,6-6,95 →
5,7-5,9 ms, `setAttribute` 1,6 → 1,1-1,3 ms per 1000 chiamate). Anche roves-js crea i wrapper in
meno tempo (patch 0213), ma il grosso del costo resta nel codice DOM di Servo: serve un profilo
con i simboli.

Il JavaScript puro è in linea con Chrome. Il microbenchmark evidenziava il costo dei wrapper
DOM (circa 7 µs a oggetto) e dei setter; il profilo successivo del typewriter non li identifica
come costo dominante. Non generalizzare il risultato del microbenchmark alla scena completa.

## Profilo del thread di script (15 ottobre 2026)

Registrato con samply (WPT installato, prompt UAC) sulla scena `?perf=typewriter-pixi`, build
`perf-ab` con mimalloc e simboli. Il thread `Script#1` è occupato circa il 37% del tempo:

| Voce (sul tempo occupato) | Inclusivo |
| --- | --- |
| Callback JavaScript (rAF, eventi, scheduler di React via MessagePort) | 58% |
| Layout (`reflow`) | 27% |
| ResizeObserver: un layout per ogni elemento osservato a ogni frame | 15,8% |
| Costruzione della display list | 10% |

`createElement` e i binding DOM non sono tra le voci principali. Il costo del typewriter viene dai
layout forzati dal ResizeObserver, che PixiJS, motion e molte librerie React usano. La patch 0221
fa un solo layout prima di controllare tutti gli elementi osservati, come Servo già fa per
IntersectionObserver.

Per ripetere il profilo: `samply record --save-only --unstable-presymbolicate -o profile.json.gz --
play.exe <url>` da un PowerShell amministratore (richiede il Windows Performance Toolkit), poi i
nomi dal `.pdb` con `llvm-symbolizer`.

## Stato alla v0.7.11 (16 ottobre 2026)

Typewriter + PixiJS (`markdown-typewriter/react`), 30 s a regime, due corse per versione:

| Versione | Frame sopra 33 ms | p99 |
| --- | --- | --- |
| v0.7.10 | 8 e 6 | 20,8 e 19,2 ms |
| v0.7.11 | 8 e 5 | 20,3 e 21,8 ms |
| Chrome | circa 2 | 17,1 ms |

La correzione del ResizeObserver (patch 0221) non cambia la scena in modo misurabile: sta nel
rumore. Restano circa 8 frame lunghi ogni 30 s; non è ancora chiaro se siano pause del GC di V8 o
picchi di layout. GSAP e motion su PixiJS hanno 0-1 frame sopra 33 ms. Prossimo passo, solo se
serve: un nuovo profilo (vedi sopra) a scena ferma sui frame lunghi.

## Frame lunghi del typewriter: non è il GC (16 ottobre 2026)

La scena `?perf=typewriter-pixi` ora scrive `RAFLONG t=… dt=…` per ogni frame sopra i 33 ms, con
l'istante dal caricamento. Con `ROVES_V8_FLAGS=--trace-gc` su v0.7.11 (30 s):

- 7 frame lunghi (da 41 a 129 ms). Uno solo coincide con una GC: una Mark-Compact da 110 ms a
  t=26,6 s. Le altre 14 GC sono scavenge da 0,6 a 6,3 ms.
- Gli altri 6 non coincidono con nessuna GC né con un picco di disegno del thread principale (a
  regime `webview_paint` massimo 23 ms). I picchi da 128-215 ms di `webview_paint` cadono nei primi
  5 s, fuori dalla finestra di misura.
- Heap iniziale più grande (`--initial-old-space-size=256 --max-semi-space-size=32`): 6 e 5 frame
  sopra 33 ms, come senza (8, 6, 5, 8). Non migliora e non diventa predefinito.

Quindi la causa dei frame lunghi sta nel lavoro del thread di script o nel layout durante la
costruzione del dialogo, non nel GC. Per trovarla serve un profilo mirato ai frame lunghi
(samply, con prompt UAC).

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
| howler.js | Web Audio, `<audio>`, `canPlayType` | Dalla v0.7.8 sono riconosciuti WAV, MP3, OGG Vorbis e Opus, WebM, AAC, MP4/M4A (`audio/x-m4a`) e FLAC. Prima Opus, AAC e MP4 risultavano "" per un errore di rilevamento dei codec |

## Ulteriore audit CPU: trasporto dei campioni audio (16 ottobre 2026)

Audit della build precedente (20f899dcfbb), non una misura di CPU: `components/media/audio/analyser_node.rs`
clona e miscela il blocco di ingresso, poi invia un callback per ogni quantum di 128 campioni.
`components/script/dom/audio/analysernode.rs` accoda per ogni callback un task DOM
`append_analysis_block`, che aggiorna il ring buffer dell'analizzatore. Il percorso resta
attivo anche senza chiamate JavaScript ai getter di analisi.

A 44,1 kHz questo implica circa 345 callback/task al secondo per analizzatore; a 48 kHz,
375. Sono frequenze ricavate dal codice, non campioni di profiling. RealFFT riduce il costo
dello spettro ma non questo traffico. Con più analizzatori vanno misurati task, CPU del thread
script e memoria delle code, separando analizzatori letti e non letti.

Prima di cambiare il trasporto, valutare i canali bounded di `crossbeam-channel`, già presente,
e un buffer audio mantenuto esternamente. Non aggiungere una coda scritta a mano. Una soluzione
deve preservare gli ultimi 32768 campioni, ordine, downmix, silenzio, fftSize e smoothing,
senza bloccare il thread audio. Non basta scartare blocchi o conservare solo l'ultimo:
altererebbe lo spettro. Verificare anche backlog durante script occupato e distruzione del nodo.

Priorità: misurare il costo con 0/1/più analizzatori prima di implementare il batching.
Il tentativo di avviare la nuova prova locale è stato respinto dal controllo automatico con
`blocked by policy`; non sono disponibili nuovi risultati CPU o confronti Chrome da questa
sessione e il runtime non è stato modificato sulla base di questa sola ipotesi.

### Migrazione del trasporto in sviluppo

Il successivo intervento sostituisce i task DOM con `crossbeam-queue::ArrayQueue` 0.3.14:
256 blocchi mono conservano gli ultimi 32768 campioni, indipendentemente da fftSize.
`force_push` sostituisce atomicamente il blocco più vecchio quando la coda è piena; i getter
importano i campioni nel motore di analisi. Non introduce una coda o primitive unsafe nostre.
Rimuove anche la radice DOM `Trusted` dal callback. Il grafo audio, il downmix e il callback
per blocco restano: la CPU audio non scompare. Il buffer pendente aggiunge fino a circa
128 KiB di payload mono per nodo, oltre al buffer storico esistente e ai metadati della coda;
la memoria è limitata anche senza letture. Va confrontata con il precedente backlog di task.

Correttezza: i blocchi silenziosi ora azzerano la porzione del buffer; l'indice iniziale
include l'ultimo campione disponibile. Test aggiunti per overflow e ordine dei campioni,
silenzio dopo suono e invalidazione/cache dello spettro: 4/4 test audio locali in release
passati. Il commit `605d486b079` supera la validazione della serie patch e tutti i job CI
desktop, Windows, macOS, Linux, Android e iOS:
[workflow desktop](https://github.com/DRincs-Productions/roves/actions/runs/38952244417).
L'archivio portable Windows del job è stato scaricato; l'avvio runtime locale non ha prodotto
log o codice d'uscita, quindi non lo considero una verifica funzionale post-modifica.

Le fixture typewriter/GSAP/motion ora disattivano anche il contatore FPS dei pannelli di
sfondo, che prima aggiornava React ogni 500 ms. Il pannello three.js rilascia geometria,
materiale e canvas allo smontaggio. Sono correzioni della pagina di misura: ripetere entrambe
le baseline sulla nuova pagina, senza attribuire al motore il risparmio dei timer rimossi.

Fixture ripetibile: `support/perf/analyser-transport-bench.html?nodes=8`, senza letture;
`&read=1` aggiunge letture a circa 60 Hz. Dopo warm-up di 2 s, finestra di 12 s e un secondo
di script occupato, controlla la storia audio completa. Il secondo occupato va escluso dalla
misura CPU. Chrome 155.0.8059.84, visibile con viewport 929×861, passa via Chrome DevTools
MCP sia senza letture (`reads: 0`, 13.008 ms) sia con letture (`reads: 6824`, 13.022 ms), a
48 kHz. Sono prove funzionali, non un confronto di CPU. La build Roves post-modifica non ha
ancora una verifica runtime valida; il precedente pass Roves 20f899dcfbb (44,1 kHz, headless)
è pre-modifica e non va usato come prova della nuova coda.

Typewriter: una corsa Chrome MCP visibile (viewport 929×861, canvas 300×200, GTX 1050 via
ANGLE/D3D11) dà p99 17,4 ms e 6 frame sopra 33 ms in 20 s dopo 5 s di warm-up. Roves
20f899dcfbb visibile dà p99 22,7 ms e 7 frame sopra 33 ms. Le finestre non erano affiancate e
le risoluzioni non erano normalizzate: dati esplorativi, non una nuova baseline convalidata.
Il tentativo headless typewriter termina invece con `SurfaceImportFailed(BadAttribute)`:
non usarlo per misurare rendering o dedurre una regressione del percorso visibile.

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
- **Playback audio:** il rilevamento dei codec è stato corretto in v0.7.8. Resta da verificare
  il playback dei file reali con i plugin inclusi nel bundle.
- **Gamepad:** sostituire il polling hot-plug residuo (1 Hz) con gli eventi SDL.
- **Steam:** rendere adattivo il polling delle callback nelle build `steam`.

Fatto il 12 ottobre: presentazione diretta predefinita (CPU da 98,6% a 91,1%; egui ora costa
~0,1 ms per frame e non viene più tassellato).

Fatto il 13 ottobre (v0.7.7):
- icone SVG del menu della visual novel: compaiono 230 ms dopo il menu, prima 1,4-2 s. Le
  immagini `data:` non passano più dalla rete;
- cache HTTP su disco: al secondo avvio nessuna richiesta di rete per l'audio remoto.

Interventi già fatti e relative misure: `CUSTOMIZATIONS.md`.

## Audit CPU e compatibilità del 16 ottobre 2026

I campioni preliminari CPU (Roves 115% di un core, Chrome 97%) e memoria privata
(651/352 MB) non costituiscono una baseline convalidata: mancavano conferma visiva e GPU
per processo. Non usarli come prova di regressione né come obiettivo già raggiunto.

Il codice WebGL sincronizzava ogni lettura di drawingBufferWidth/Height con il thread
WebGL; anche la validazione del framebuffer predefinito usa questi getter. La nuova cache
legge le dimensioni effettive del backend una volta per allocazione e si invalida al resize.
Non usa le dimensioni richieste dal canvas, che possono differire da quelle allocate.
La modifica è verificata nelle build CI e localmente: risultati ripetibili riportati sotto.

La fixture Pixi ora attende init prima di distruggere l'applicazione, evitando cleanup
prematuro in StrictMode, forza WebGL e omette il timer FPS nelle fixture prestazionali.
Questo corregge due fonti di errore nel confronto; non dimostra la causa di ogni pagina bianca.

Web Animations non è un semplice flag: Element.animate non avvia l'animazione, Animation
non implementa playback e KeyframeEffect ignora le opzioni del costruttore. Riutilizzare
Stylo per interpolazione e WebRender per compositing è la direzione; servono anche timing,
promise/eventi, cancellazione e gestione delle proprietà non compositabili. Rimane disattivato.

AudioWorkletNode e AudioWorkletGlobalScope non sono implementati. Tone.js va verificato per
funzione, distinguendo i nodi Web Audio presenti dai processori worklet assenti. Il lavoro
richiede integrazione col grafo servo-media e V8 audio dedicato; nessuna equivalenza con Chrome
è dimostrata dalla sola riuscita del caricamento della libreria.

Ordine operativo: confronto production valido; verifica cache WebGL; correlazione GC/layout;
WAAPI motion; traffico Pixi/three e presentazione B solo secondo i profili; AudioWorklet.

### Libreria integrata: RealFFT

L'analisi di frequenza Web Audio usava una DFT manuale O(N²), con trigonometria per ogni
coppia campione/bin. Il ramo di sviluppo usa RealFFT 3.5 / RustFFT, O(N log N), conservando
finestra Blackman, scala 1/N e smoothing. Piano e buffer sono riusati tra letture.
Questo beneficia AnalyserNode e le funzioni delle librerie audio che lo usano; non costituisce
un miglioramento misurato della scena typewriter o di tutto Tone.js. I test numerici CI e
le verifiche runtime locali sono passati; i tempi misurati sono riportati sotto.

### Riferimenti ripetibili prima di RealFFT

Windows 11 10.0.26200, i7-8700 (6 core/12 thread), Chrome 155.0.8059.84,
12 letture per fftSize su sinusoide con uscita silenziosa.
Benchmark in `support/perf/audio-analysis-bench.html`. Chrome misurato dall'utente nella
stessa pagina; v0.7.11 verificata visivamente prima della prova.

| fftSize | v0.7.11 p50 / p95 | Chrome p50 / p95 |
| --- | --- | --- |
| 2048 | 27,35 / 67,55 ms | 0,20 / 3,10 ms |
| 8192 | 425,49 / 428,63 ms | 0,90 / 1,30 ms |

Entrambi trovano il bin 32 a circa −13,5556 dB e terminano con PASS. Con soli 12 campioni
il p95 è il massimo della corsa, non una stima robusta delle code di latenza.
Una prova Roves separata headless dà 31,82 / 64,56 ms e 448,55 / 588,78 ms: non mescolarla
con quella visibile.

Il benchmark dei getter WebGL (`support/perf/webgl-getters-bench.html`) sulla v0.7.11 visibile
dà 4,67 ms (WebGL 1) e 4,37 ms (WebGL 2) per 2000 coppie width/height. Il test funzionale
di resize e readback (`test-page/public/webgl-dimensions.html`) passa anche headless.

Durante una corsa typewriter visibile v0.7.11 è comparsa una GC completa di 62 ms
(heap JS 16 MB usati su 56 MB); RAFSTATS riporta 9 frame oltre 33 ms in 20 s dopo 5 s
di warm-up. Questo conferma una pausa GC reale, ma non attribuisce al GC tutti i frame lunghi.

### Build CI ottimizzate, commit 20f899dcfbb

CI verdi: [desktop e patch, 13 job](https://github.com/DRincs-Productions/roves/actions/runs/38911122483),
[V8, 10 job](https://github.com/DRincs-Productions/roves/actions/runs/38911122489),
[Windows ottimizzato e test numerici](https://github.com/DRincs-Productions/roves/actions/runs/38911122549).
Anche Android/iOS e la CI desktop sul ramo main sono verdi. Nessuna build locale del motore.

Scaricati entrambi gli ZIP `perf-ab`, verificati SHA-256 pubblicati da GitHub e commit sorgente.
Prove locali sulla stessa pagina in modalità headless:

| API / carico | v0.7.11 | System + RealFFT | mimalloc + RealFFT |
| --- | --- | --- | --- |
| Spettro, fftSize 2048: p50 | 26,82 ms | circa 0,03 ms | circa 0,03 ms |
| Spettro, fftSize 8192: p50 | 432,38 ms | circa 0,09 ms | circa 0,09 ms |
| 2000 coppie getter dimensioni WebGL 1: p50 | 4,47 ms | 0,67 ms | 0,74 ms |
| 2000 coppie getter dimensioni WebGL 2: p50 | 4,34 ms | 0,73 ms | 0,75 ms |

Tre processi nuovi per variante/carico; tabella con la mediana dei tre p50. Per l'audio v0.7.11
la corsa immediatamente precedente è una sola (12 letture per fftSize), coerente con quelle
precedenti. Per i getter anche la baseline ha tre corse. Variabilità iniziale System:
WebGL 1 1,55/0,67/0,67 ms; WebGL 2 1,39/0,73/0,70 ms. Non scartare la prima corsa dal report.

Tutte le prove audio trovano il bin 32 a circa −13,5556 dB e riportano PASS. Entrambe le
build passano resize ripetuti e readback pixel di WebGL 1/2, HTMLCanvas e OffscreenCanvas.
Anche il pacchetto debug scaricato passa i controlli funzionali; i suoi tempi non sono
usati in tabella. Log e screenshot locali: `target/perf-local-20261016/`.

Il beneficio riguarda spettro audio e getter WebGL. Non misura CPU totale del gioco, GPU o
RAM, né risolve WAAPI o AudioWorklet. Chrome è stato misurato visibile dall'utente e la nuova
build headless: non dedurne un vantaggio generale su Chrome. Queste prove non giustificano
un cambio dell'allocatore predefinito. Le build `perf-ab` sono sperimentali, non una release
versionata consumata da roves-action o Packmaster.
