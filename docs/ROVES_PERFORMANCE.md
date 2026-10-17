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

## Frame lunghi del typewriter: il GC ne spiega solo una parte (16 ottobre 2026)

La scena `?perf=typewriter-pixi` ora scrive `RAFLONG t=… dt=…` per ogni frame sopra i 33 ms, con
l'istante dal caricamento. Con `ROVES_V8_FLAGS=--trace-gc` su v0.7.11 (30 s):

- 7 frame lunghi (da 41 a 129 ms). Uno solo coincide con una GC: una Mark-Compact da 110 ms a
  t=26,6 s. Le altre 14 GC sono scavenge da 0,6 a 6,3 ms.
- Gli altri 6 non coincidono con nessuna GC né con un picco di disegno del thread principale (a
  regime `webview_paint` massimo 23 ms). I picchi da 128-215 ms di `webview_paint` cadono nei primi
  5 s, fuori dalla finestra di misura.
- Heap iniziale più grande (`--initial-old-space-size=256 --max-semi-space-size=32`): 6 e 5 frame
  sopra 33 ms, come senza (8, 6, 5, 8). Non migliora e non diventa predefinito.

Il GC spiega una pausa di questa corsa, ma non gli altri sei frame lunghi. Il lavoro del
thread di script, il layout durante la costruzione del dialogo e la schedulazione restano
candidati: per distinguerli serve un profilo mirato ai frame lunghi (samply, con prompt UAC).

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

Il blocco iniziale della policy è stato superato con l'esecuzione autorizzata del 17 ottobre:
risultati prima/dopo e limiti sono nella sezione dedicata sotto.

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
Il primo tentativo locale con `-x -o URL` era invalido: `-o` richiede un filename e `-x`
può chiudere la pagina prima dei controlli audio. Un log riportava soltanto `Panic hook called`;
la patch 0229 conserva il dettaglio prima dell'uscita `hard_fail`. Il panic non si è riprodotto
nelle successive dodici prove autorizzate con argomenti corretti: non attribuirlo alla coda.

La correzione diagnostica `1e9c7dcfea2` passa tutti i 13 job
[desktop/patch/test](https://github.com/DRincs-Productions/roves/actions/runs/39022147447),
oltre ad Android e iOS. Scaricato il portable Windows aggiornato, verificato contro il digest
SHA-256 pubblicato da GitHub (`79cc181d…68df4cb`). Dopo il blocco iniziale della policy,
le prove autorizzate sotto forniscono PASS runtime e dati CPU sul trasporto audio.
Evidenze locali: `target/perf-local-20261017/verification.json`. La release versionata rimane
v0.7.11; questi controlli riguardano il ramo di sviluppo.

Le fixture typewriter/GSAP/motion ora disattivano anche il contatore FPS dei pannelli di
sfondo, che prima aggiornava React ogni 500 ms. Il pannello three.js rilascia geometria,
materiale e canvas allo smontaggio. Sono correzioni della pagina di misura: ripetere entrambe
le baseline sulla nuova pagina, senza attribuire al motore il risparmio dei timer rimossi.

Fixture ripetibile: `support/perf/analyser-transport-bench.html?nodes=8`, senza letture;
`&read=1` aggiunge letture a circa 60 Hz. Dopo warm-up di 2 s, finestra di 12 s e un secondo
di script occupato, controlla la storia audio completa. Il secondo occupato va escluso dalla
misura CPU. Chrome 155.0.8059.84, visibile con viewport 929×861, passa via Chrome DevTools
MCP sia senza letture (`reads: 0`, 13.008 ms) sia con letture (`reads: 6824`, 13.022 ms), a
48 kHz. Sono prove funzionali, non un confronto di CPU con Chrome. Il precedente pass Roves
20f899dcfbb (44,1 kHz, headless) è pre-modifica; la verifica post-modifica è ora riportata
nella prova autorizzata del 17 ottobre qui sotto.

### Prova autorizzata del trasporto audio, 17 ottobre 2026

Passando alla modalità con approvazione, la richiesta `require_escalated` consente finalmente
`Start-Process` e la cattura di CPU/RAM/log. La build `1e9c7dcfea2` supera il benchmark con
argomenti corretti. Il precedente panic non si ripresenta in questa prova; la sua causa
originaria non è stata accertata. I rifiuti di policy riportati sopra descrivono i tentativi
precedenti, non un blocco attuale di questa via di esecuzione.

Confronto preliminare: artefatto debug precedente conservato come `ci-debug-20f899d` e nuovo
portable CI `1e9c7dcfea2`, stessa pagina HTTP locale, modalità headless, 44,1 kHz, viewport
1024×740. Tre processi nuovi per variante e numero di nodi (12 prove), ordine alternato.
Campioni del processo ogni circa 200 ms: finestra centrale di circa 9,8 s dopo `TRANSPORT
READY`, escludendo avvio e busy loop finale. Tutte le prove riportano `TRANSPORT PASS`.

| Analizzatori non letti | CPU prima / dopo, mediana (% di un core) | Picco memoria privata prima / dopo, mediana |
| --- | --- | --- |
| 0 | 16,1 / 14,9 | 194,6 / 194,5 MiB |
| 8 | 42,6 / 24,4 | 200,1 / 201,3 MiB |

Con 8 nodi la CPU mediana scende del 42,8%, con circa 1,2 MiB privati in più. Singole corse
CPU prima: 52,4 / 42,6 / 38,4%; dopo: 24,4 / 23,2 / 27,0%. Il controllo a 0 nodi varia da
13,2 a 18,9% fra le due build: non sottrarre le baseline per dedurre un costo preciso per nodo.
100% corrisponde a un core logico, non all'intera CPU. I picchi di working set con 8 nodi
sono 167,3 / 168,6 MiB (mediane). Non è una verifica di crescita memoria su lunghe sessioni.

Sono artefatti debug conservati, senza una ricostruzione A/B ottimizzata in questo esperimento.
Il renderer headless è Microsoft Basic Render Driver via ANGLE: questi dati non misurano GPU,
rendering visibile, analizzatori letti a ogni frame o CPU totale del gioco e non confrontano
Chrome. Non attribuire il risultato al typewriter, che non usa questi analizzatori.
Log e campioni: `target/perf-local-20261017/transport-comparison/`, incluse statistiche JSON.

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

### Ispezione WAAPI in Chrome, 17 ottobre 2026

Chrome DevTools MCP, Chrome 155.0.8059.84 su Windows, scena production
`?perf=typewriter-pixi`, visibile, viewport 1024×740, DPR 1, canvas Pixi 300×200.
Campionando `document.getAnimations()` ogni 200 ms per 10 s (48 osservazioni), la pagina
espone da 1 a 120 animazioni native contemporanee: proprietà `opacity` e `transform`.
Questa scena usa quindi il percorso nativo in Chrome; Roves conserva il fallback JavaScript
perché WAAPI è incompleta. È un motivo concreto per dare priorità a playback/timing WAAPI
riusando Stylo/WebRender, ma non quantifica CPU risparmiabile o frame lunghi evitabili.

Il percorso compositor va completato insieme al playback: `Painter::perform_updates` usa
già `WebRender DynamicProperties` per il colore del cursore, ma invia liste `transforms` e
`floats` vuote. Le trasformazioni e l'opacità della display list restano valori statici.
Riutilizzare i binding dinamici di WebRender è quindi un candidato per evitare ricostruzioni
della scena a ogni frame. Prima dell'integrazione verificare stacking context, invalidazione,
hit testing e memoria GPU; rendere disponibile `element.animate` da solo non realizza questo
percorso e non dimostra una riduzione di CPU.

Una registrazione DevTools sulla stessa pagina termina con il riepilogo corretto, ma senza
dettagli CPU utilizzabili. L'esportazione del file è negata dal MCP perché il percorso del
repository non rientra nelle sue radici configurate. Non è disponibile un profilo grezzo da
analizzare e non si ricavano percentuali JavaScript/layout da quel riepilogo.

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

## 17 ottobre: isolamento della pagina di confronto

Il ramo `?perf=` veniva scelto dentro `App`, dopo gli import statici dei pannelli diagnostici.
Questi caricavano anche Tone.js: Chrome emetteva il banner Tone e avvisi di AudioContext
senza interazione. La selezione avviene ora nell'entry point; React.lazy/Suspense e lo splitting
ESM di Vite caricano `App` soltanto nella pagina diagnostica normale. Nessuna dipendenza nuova.

`npm run build` passa. Chrome DevTools MCP verifica che typewriter/Pixi non scarichi il chunk
App né emetta il banner Tone; la pagina normale carica App e conserva i pulsanti Tone.
L'utente ha confermato testo e scena in movimento in Roves. Le precedenti corse visibili
con import eager sono esplorative: rifare le baseline con la stessa pagina corretta in entrambi
i browser. Questa modifica alla fixture non è un guadagno del motore.

La CI ottimizzata [39034634597](https://github.com/DRincs-Productions/roves/actions/runs/39034634597)
ricostruisce il runtime `1e9c7dcfea2` con entrambi gli allocatori. Le misure nuove devono
registrare commit, versione, viewport 1024×740, DPR 1, visibilità, GPU, riscaldamento e intervallo;
separare i contatori GPU dalle corse CPU per limitare l'interferenza del campionamento.

Il campionatore usa `Date.now() - performance.now()` per allineare il riscaldamento: Roves
restituisce attualmente `performance.timeOrigin` nell'epoca monotona interna, anziché Unix.
È un problema di compatibilità da correggere separatamente, non una causa dimostrata dei
frame lunghi. Non mescolare tempi assoluti dei due browser senza questa correzione di misura.

### Corse esplorative sulla fixture corretta

Windows 11, i7-8700, GTX 1050/ANGLE D3D11, viewport 1024×740, DPR 1, canvas Pixi
300×200, finestra visibile. Campionamento da 60 a 80 s dal caricamento. Chrome 155.0.8059.84
è pilotato via DevTools MCP; CPU e memoria provengono dai contatori di processo Windows
letti da terminale, non dal profiler MCP. Si sommano soltanto il browser MCP e i suoi processi
figli, escludendo il Chrome personale. Ogni corsa Roves usa un processo nuovo.

| Corsa CPU senza contatori GPU | CPU (% di un core logico) | Picco memoria privata |
| --- | --- | --- |
| Chrome, contesto usato nelle prove precedenti | 61,8% | 535,0 MiB |
| Chrome, nuovo contesto isolato | 67,0% | 365,6 MiB |
| Roves ottimizzato `20f899dcfbb` | 46,4% | 639,5 MiB |
| Roves `20f899dcfbb`, diagnosi aggiuntiva per thread | 29,4% | 664,2 MiB |
| Roves `20f899dcfbb`, statistiche mimalloc e chiusura regolare | 37,6% | 656,1 MiB |

Queste corse non dimostrano parità o superiorità CPU: la variabilità è elevata, i contesti
Chrome differiscono e il runtime Roves è quello ottimizzato precedente alla coda audio.
La diagnosi per thread non ha ottenuto i nomi: non attribuire le percentuali a script o layout.
Non confrontare la somma dei working set Chrome con il singolo working set Roves come RAM
fisica equivalente, perché le pagine condivise possono essere contate più volte.

Una corsa GPU Chrome separata, con nuovo contesto isolato, registra cinque osservazioni:
motore 3D 2/3/2/2/3%, memoria dedicata 30,6 MiB e condivisa 4,9–5,2 MiB. L'interferenza del
campionamento CIM esclude quella corsa dal confronto CPU. Il probe rAF da 60 a 80 s registra
1198 intervalli, p99 17,0 ms e un intervallo sopra 33 ms; non è una misura GPU dettagliata.

`MIMALLOC_SHOW_STATS=1`, riusando la diagnostica della libreria e chiudendo regolarmente
Roves, riporta un picco di 251 MiB impegnati nelle arene mimalloc e 656 MiB privati nel
processo. Il GiB riservato dalle arene è spazio virtuale, non RAM residente. I picchi non
sono necessariamente simultanei: non sottrarli per ricavare una ripartizione precisa.
Il solo `mi_process_info` descrive il processo, non tutte e sole le allocazioni Rust.

Dati grezzi locali: `target/perf-local-20261017/visible/`, nomi `*-clean-*`;
le corse precedenti senza `clean` usano gli import eager e non sono la nuova baseline.

## Web Worker: gestione e verifica del ciclo di vita (17 ottobre 2026)

Prima priorità: rilasciare thread e isolate quando il gioco termina un worker. Ogni dedicated
worker usa già un thread e un runtime V8 propri; l'attesa usa `crossbeam_channel::select!`,
senza polling continuo. Un pool Rayon/Tokio non sostituisce automaticamente questi event loop
JavaScript con stato e affinità di thread. Si riusano Crossbeam e le API V8 già integrate.

Problema riprodotto sulla build ottimizzata mimalloc `1e9c7dcfea2`: quattro worker inattivi
allocano e toccano ciascuno 8 MiB, poi il parent chiama `terminate()` conservando gli oggetti
Worker. Cinque secondi dopo, tutti e quattro gli ID dei thread aggiunti sono ancora presenti.
I thread totali passano da 61 a 65 e poi 64: il thread scomparso non è uno dei quattro nuovi.
Memoria privata: 125,6 MiB prima, 195,7 MiB con worker pronti, 195,5 MiB dopo terminate.
È una singola prova diagnostica headless, non un confronto prestazionale con Chrome.

`terminate()` impostava il flag e richiedeva un interrupt V8, che non risveglia il `select!`
quando il worker è inattivo. La patch 0230 invia anche il messaggio WakeUp già esistente sul
canale Crossbeam. Il loop usa inoltre `close()` quando un canale è disconnesso o l'handler
richiede lo shutdown, così il chiamante non ripete l'attesa su un canale già chiuso.
La verifica post-modifica è ancora necessaria: non dichiarare recuperati thread o memoria
sulla sola base dell'ispezione del codice. Nessuna nuova dipendenza o scheduler personalizzato.

Fixture: `support/perf/worker-lifecycle-bench.html?auto=1`. Registra fasi baseline, ready,
terminated e after, lasciando aperto il parent per il campionamento esterno. Gli oggetti Worker
rimangono referenziati; la liberazione degli isolate non deve dipendere dal GC del parent.
La memoria dell'allocatore può restare in cache: gli ID dei thread sono un controllo separato.

Seconda priorità: misurare avvio, ping-pong di messaggi piccoli, trasferimenti di ArrayBuffer
e calcolo CPU, con 1/2/4/8 worker e scene uguali in Chrome/Roves. Il backend usa già
ValueSerializer/ValueDeserializer di V8, ma per gli ArrayBuffer trasferiti scrive i byte nella
tabella serializzata e li copia in un nuovo buffer in lettura. Il distacco del mittente non
dimostra quindi un trasferimento senza copie. Un futuro percorso nello stesso processo deve
riusare BackingStore V8 e preservare ownership, errori, cleanup e il percorso multiprocesso;
non aggiungere una tabella globale di buffer senza gestione completa del ciclo di vita.
