# Roves — Prestazioni

## Stato verificato, 18 ottobre 2026

Engine **v0.7.13**, sorgente `a3963eae5c6`: desktop CI `39122648517` 13/13,
V8 `39122648553` 10/10, Android/iOS e release `39122949519` verdi. Sei archivi
plain/Steam presenti. Windows SHA256
`20ea28aba13b10508593db1d295bd939a20be23cfb14958ccba1b89bc9b7d3f2`, commit incorporato verificato.
Nove controlli DOM, FIFO128/timer, intrinsics e scena Pixi/typewriter visibile PASS.
Patch 0235 verificata anche sul debug CI: uscita anticipata senza il precedente panic
di overflow; l'errore null di chiusura si osserva anche nella v0.7.12.

Distribuzione verificata: roves-action **v0.1.9**, alias `v0` a `cc7aba6`, CI consumer
`39128141935` 9/9; Packmaster **v0.1.9** `ceb9c1b`, release `39127760654` verde,
tre archivi presenti. Corretto un pin importante: la UI Packmaster indicava v0.7.12,
ma il backend di produzione scaricava ancora v0.4.18. Ora TS e Rust leggono lo stesso
`src/lib/shell-version.txt` (v0.7.13). Le build di test che usano latest mascheravano
il problema. Archivio Windows verificato tramite SHA256; il pin condiviso è verificato nei sorgenti
della release. La ricerca di stringhe nel binario ottimizzato non è conclusiva, né
questa verifica equivale a una prova completa della GUI.

### Misure e limiti

Scena typewriter/Pixi, Windows 11/i7-8700/GTX1050, 1024×740, DPR1, piano Risparmio
di energia invariato. Campioni CPU/RAM a 60–80 s; GPU non campionata.
v0.7.13: **87,44% di un core**, 626,3 MiB privati, 59,45 rAF/s, p99 19,86 ms,
10 frame >33 ms. v0.7.12 vicino a60 rAF/s: 89,92%,636,0 MiB,p99 19,84 ms,8 frame lunghi.
Chrome155.0.8059.84 via MCP:82,70%,335,4 MiB,55,3 rAF/s,p99 18,90 ms,7 frame lunghi.
Singole corse e carichi host diversi: il circa2,8% CPU fra versioni non dimostra un
guadagno stabile. Nessuna parità globale; memoria e jitter restano da migliorare.
Le vecchie corse Roves a30 rAF/s non sono risparmio CPU a pari lavoro.
Solo nella corsa v0.7.13 il PID in primo piano è verificato per tutti i campioni.
Chrome MCP disabilita il throttling in background; visibilityState da sola non prova il fuoco OS.

Worker:1024 job ×8388608 iterazioni, risultati controllati, pool riusato durante ogni corsa.

| Worker | Roves calcolo ms | CPU intera vita s | Picco privato MiB | Chrome calcolo ms |
| --- | --- | --- | --- | --- |
| 1 | 8376,32 | 11,00 | 173,7 | 8275,6 |
| 2 | 4153,22 | 10,97 | 187,6 | 4190,8 |
| 4 | 2077,71 | 11,08 | 217,3 | 2035,5 |
| 8 | 1092,38 | 10,47 | 249,8 | 1032,3 |

Roves ottimizzato headless, Chrome MCP visibile: throughput aritmetico simile in questa
prova, non parità CPU/UI. CPU include startup/warmup/chiusura; memoria è picco campionato.
Il primo campione CPU a1 worker aveva timestamp finale invalido: conservato e marcato
invalido, sostituito dalla corsa `1-checked`. Più worker aumentano CPU istantanea e RAM;
non riducono il lavoro totale. Valutare piccoli pool riusati (2/4), code e batch limitati
nel gioco reale prima di aggiungere una libreria di gestione pool.

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
| PixiJS, three.js | WebGL 2 | Funzionano sul thread DOM; WebGL nei worker non ancora disponibile. Ogni chiamata WebGL è un messaggio verso il thread WebGL |
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
v0.7.12 dal 17 ottobre; i risultati seguenti conservano versioni e condizioni delle singole prove.

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

### Trasporto audio: verifica con build ottimizzate

Ripetizione con mimalloc ottimizzato `20f899d` prima e `1e9c7dc` dopo: stessa fixture,
headless 1024×740, 44,1 kHz, renderer Microsoft Basic Render Driver, tre processi nuovi per
combinazione e ordine alternato (12 PASS). CPU nella finestra centrale di circa 9,8 s;
esclusi warm-up e busy loop finale. Non è una misura della GPU o della CPU del gioco.

| Analizzatori non letti | CPU prima / dopo, mediana (% di un core) | Picco privato prima / dopo, mediana |
| --- | --- | --- |
| 0 | 10,35 / 9,65 | 190,06 / 189,70 MiB |
| 8 | 15,07 / 10,68 | 194,78 / 195,63 MiB |

Con otto nodi, singole CPU prima 14,25/15,07/15,68%; dopo 10,68/12,06/6,96%.
L'ultima corsa dopo ha CPU host circa 27%, contro circa 9–12% nelle altre: conservarla,
senza attribuire causalmente al trasporto tutto il calo mediano del 29,1%. Escludendo l'intera
terza coppia, il calo delle mediane è circa 22,5%. Alle 18:20/18:23 era attivo il profilo
Windows Risparmio di energia, non modificato: la frequenza CPU è una possibile variabile,
non una causa dimostrata. Nessuna misura di parità con Chrome o di assenza di glitch audio.
Log e campioni: `target/perf-local-20261017/transport-comparison-release/`.

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
ha ricostruito e verificato il runtime `1e9c7dcfea2` con entrambi gli allocatori. Le misure devono
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
Verifica post-modifica sulla portable debug CI `56957e15df8`: tre cicli nello stesso parent,
conservando tutti gli oggetti Worker, creano quattro thread e ne rilasciano quattro ogni volta.
Memoria privata baseline/ready/after (MiB): 142,59/204,02/166,63;
166,63/203,83/166,01; 166,18/203,79/166,26. Nessuna crescita progressiva osservata nei tre
cicli; il residuo rispetto alla prima baseline non è stato attribuito a uno specifico componente.
La fixture automatica nasconde il DOM e scrive solo in console per evitare nuovi thread di
rendering del testo diagnostico. Non confrontare questi valori debug con la RAM ottimizzata
precedente per stimare un risparmio percentuale. Nessuna nuova dipendenza o scheduler.

La revisione distingue gli errori di ricezione da una coda temporaneamente vuota dopo WakeUp:
quest'ultima deve mantenere attivo il worker. Il probe `support/perf/worker-queue-probe.html`
controlla 128 risposte FIFO, messaggi dopo la raffica, timer cancellati e un ping finale.
Il riferimento passa in Chrome MCP, nel runtime precedente e nell'artefatto finale `56957e15df8`;
le sole prove sequenziali di messaggistica non coprono la gestione dei risvegli senza task.

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

### Accessi ai globali nei worker: costo evitabile dei binding

Nuova fixture `support/perf/worker-compute-bench.html?workers=1`: 64 job da 1048576 iterazioni
di un calcolo intero deterministico, controllo del risultato di ogni job e warm-up separato.
`&cached=1` conserva Math.imul in una variabile locale al worker; il controllo separa il
calcolo V8 dall'accesso ripetuto al globale. Finestra visibile 1024×740, Windows, stesso HTTP.

| Prova diagnostica singola | Tempo del lavoro fisso |
| --- | --- |
| Roves mimalloc ottimizzato `8db6eab`, Math.imul nel ciclo | 8551,32 ms |
| Stesso Roves, funzione conservata in variabile locale | 74,13 ms |
| Chrome 155 via DevTools MCP, Math.imul nel ciclo | 69 ms; ripetizione 104,5 ms |

Il tentativo più grande (1024 job da 8388608 iterazioni) supera il timeout Roves di 60 s;
non è un risultato completo. Chrome passa quel carico con 1/2/4/8 worker in
8176,5/4089,5/2061,2/1085,2 ms. Aumentare i worker riduce la latenza occupando più core:
questi tempi non dimostrano meno CPU totale. Una pausa rAF Chrome da 213,8 ms nella corsa
a due worker resta registrata; nessuna attribuzione causale a questo carico da una sola corsa.

Il generatore assegnava anche ai globali non-Window tre hook delle classi standard SpiderMonkey.
In roves-js sono già no-op perché V8 crea quelle classi; la loro presenza installa comunque un
interceptor sul globale. La patch 0232 rimuove le tre assegnazioni e riusa gli accessi nativi
V8. Restano gli hook reali di Window per le interfacce DOM lazy. Nessuna cache scritta a mano
o dipendenza aggiunta. I 74 test del generatore e la riproduzione di 72 patch sui due file
pristine passano. La verifica runtime post-modifica è riportata sotto; non attribuire alla
patch il guadagno del solo controllo JavaScript con funzione locale.
Questo intervento è successivo al tag v0.7.12, all'epoca in compilazione.
Log: `worker-compute-small-{global,local}.stdout.log`, `worker-compute-8db6eab-1.stdout.log`,
`chrome-worker-compute-1024.json`, sotto `target/perf-local-20261017/`.

Verifica worker post-modifica: tutti i 13 job desktop e i 10 V8 passano su `1edef828eea`.
Portable debug scaricato con SHA256 `2f6227d2fd13fbffabf80a62297e505fc1387f3014fc78a59cdd26ea367840be`
e commit incorporato `1edef82` verificati. Stesso calcolo da 64 job, accesso Math.imul diretto:
93,05 ms e controllo FIFO/timer PASS. È una build debug contro la precedente ottimizzata:
il risultato conferma la rimozione del costo diagnostico, senza una percentuale comparabile
fra profili o una misura di CPU del gioco. Log `worker-compute-native-after-1edef82.stdout.log`.

### Accessi globali sul thread principale

`support/perf/main-intrinsics-bench.html`: 8388608 iterazioni, warm-up, tre alternanze di
accesso Math.imul diretto e funzione locale nello stesso processo. Tutti i risultati coincidono.
Roves ottimizzato `8db6eab`: diretto 1017,11/1033,59/1020,40 ms; locale 7,87/7,84/7,84 ms.
Chrome 155 via MCP: diretto 7,8/7,8/7,9 ms; locale circa 7,8–8,0 ms. Visibile 1024×740,
GTX1050/ANGLE. Questi microbenchmark non quantificano il tempo speso nei globali dal gioco.

La patch 0233 usa [SetNativeDataProperty di V8](https://v8.github.io/api/head/classv8_1_1Object.html)
per i nomi delle interfacce DOM enumerati e abilitati. Registrazione dopo window.init_document:
alcuni predicati leggono l'URL del Document e l'hook globale iniziale è troppo presto.
Costruzione ancora lazy; nessun interceptor globale per gli intrinseci.
I resolver esistenti definiscono il descrittore definitivo. Il flag JSPROP_RESOLVING evita
che le definizioni di alias avviino altri getter; nessun registro o cache aggiunto.
L'alternativa SetLazyDataProperty non è usata: fallisce quando la callback Servo modifica
la proprietà che V8 vuole sostituire automaticamente. Il prototipo è stato corretto localmente.
Passano 38 test V8 locali (due microbenchmark ignorati), inclusi override, cancellazione,
alias, errori/ritentativi, interfacce disabilitate, precedenza sui prototipi e GC.
La riproduzione ordinata di 31 patch sui sette file passa. Verifica browser post-modifica
ancora necessaria; non dichiarare misurato il guadagno del nuovo percorso Window.
Il controllo fra globali ha trovato che il callback usava il realm chiamante: la patch 0234
entra nel realm proprietario e ripristina il chiamante anche dopo un'eccezione. Corregge inoltre
l'import DomObject richiesto da ScriptThread, mancante nella prima compilazione CI (E0599).
Riproduzione selezionata di 30 patch sui tre file PASS. Alla pausa richiesta dall'utente
il 17 ottobre, desktop `39069276284` è a 10/13 job verdi e V8 `39069276329` a 7/10;
Android e iOS passano. Windows ha completato la compilazione e assembla il bundle.
Attendere gli esiti finali, scaricare il portable e verificare hash/commit `1e0f476`
prima dei test browser. Piano di ripresa in `V8_MIGRATION_HANDOFF.md`.

### Verifica browser del 18 ottobre — cronologia prima del completamento

Le CI sopra sono concluse: desktop13/13 e V8 10/10 verdi. Le suite Linux normal/jitless
riportano tutte le asserzioni eseguite PASS (skip espliciti). Portable Windows con hash
`8b31e710adc5a6af3758848af24275153b2eba5fc646d8206824a52c03ed2a48` e prefisso
incorporato `1e0f476` verificati. Nove controlli DOM PASS, FIFO128/timer PASS,
calcolo worker64job PASS in89,58ms. Intrinseci thread principale, diretto
7,96/7,88/7,92ms, catturato7,89/7,91/7,88ms: il grande costo diagnostico non compare.
Build debug contro precedenti ottimizzate: nessuna percentuale di CPU del gioco dedotta.
Chrome MCP attuale ha viewport929×861 e tempi10,6–13,6ms diretti: non è una nuova
coppia equivalente al riferimento precedente.

Lo smoke App in background è respinto dalla policy. Il diverso percorso in primo piano
con chiusura propria della pagina passa sulla build1e0f476: Pixi/WebGL, sette trasformazioni
della scena e sette stati degli stili delle lettere. innerText rimane costante perché il
typewriter anima un testo già interamente presente nel DOM. App diagnostica normale PASS:
14 pulsanti e modulo Tone caricato; non è una prova di playback. Nessun panic nelle due corse.
Log `app-foreground-styles.stdout.log` e `normal-app-1e0f476.stdout.log` nella cartella18ottobre.

Il controllo headless con uscita anticipata/screenshot trova un overflow nel riportare
la colonna sconosciuta di un'eccezione V8. Anche v0.7.12 riporta un errore null senza
posizione in quel percorso; non è una prova di regressione Window. Patch0235 usa
conversione/addizione controllate Rust e zero per sconosciuto: test rosso prima,
suite39verdi/2ignorati dopo. Commit a3963eae5c6, nuova CI in corso. Tagv0.7.13 creato
per le build ottimizzate; release39122949519 e verifica/pin ancora da completare.

### Riferimenti CPU del 18 ottobre e condizioni della finestra

Stessa scena typewriter/Pixi, viewport1024×740, DPR1, misura60–80s, piano Risparmio
di energia invariato. CPU/RAM da contatori Windows, GPU non campionata in queste corse.

| Corsa | CPU di un core | Privati MiB | rAF/s circa | p99 ms | CPU host |
| --- | --- | --- | --- | --- | --- |
| v0.7.12, prima corsa | 27,45% | 621,2 | 30,0 | 49,92 | 18,73% |
| v0.7.12, Chrome ridotto | 46,56% | 615,1 | 32,6 | 54,82 | 12,68% |
| v0.7.12, richiesta di attivazione | 89,92% | 636,0 | 59,3 | 19,84 | 17,97% |
| Chrome155 MCP | 82,70% | 335,4 | 55,3 | 18,90 | 18,68% |

Le corse lente rimangono nel report e non dimostrano risparmio CPU. Il PID del fuoco
resta Chrome anche quando AppActivate restituisce true: non assumere il fuoco verificato.
visibilityState=visible non dimostra che la finestra sia davanti alle altre.
Chrome MCP è headed e avviato con disable-background-timer-throttling,
disable-backgrounding-occluded-windows e disable-renderer-backgrounding.
Windows11 può ignorare la richiesta di timer fini delle finestre coperte:
[documentazione Microsoft](https://learn.microsoft.com/windows/win32/api/timeapi/nf-timeapi-timebeginperiod).
È una possibile interferenza, non una causa provata per tutte le corse.
Chrome espone ANGLE/NVIDIA GTX1050/D3D11; i log del compositor Roves confermano
GTX1050/D3D11 (driver32.0.15.8163). Le stringhe WebGL Roves restano generiche Mozilla/Servo.
I log sono in `target/perf-local-20261018/`; nessuna misura post-fix ottimizzata ancora disponibile.

### Riferimento visibile aggiornato del 17 ottobre, v0.7.12

Una nuova coppia sulla stessa `perf-measured.html?perf=typewriter-pixi`, visibile,
1024×740, DPR 1, canvas 300×200. Piano Windows Risparmio di energia invariato;
campionamento CPU/RAM da 60 a 80 s, contatori GPU disattivati.

| Browser | CPU di un core | Picco privato | CPU totale macchina | Frame >33 ms / p99 |
| --- | --- | --- | --- | --- |
| Roves v0.7.12, release `0d89099` | 88,03% | 661,4 MiB | 22,93% | 9 / 21,04 ms |
| Chrome 155.0.8059.84 | 89,49% | 399,6 MiB | 19,27% | 6 / 17,10 ms |

Chrome: pagina/console via DevTools MCP, CPU/RAM dai contatori Windows dell'albero
del browser MCP PID 10020; escluso il browser personale. Entrambi 1187 intervalli rAF.
Una sola coppia, carico esterno e stati dei processi differenti: nessuna prova di parità
o di risparmio dovuto alle patch successive al tag. Log in
`target/perf-local-20261017/visible/*current-baseline-1*`.
`support/perf/native-globals-probe.html`, controllo di nove comportamenti DOM, alias Image/Audio/Option e realm degli
iframe passa in Chrome e nella release v0.7.12; poi ripetuto con successo sul binario v0.7.13.

La CI consumer [39067409866](https://github.com/DRincs-Productions/roves-action/actions/runs/39067409866)
passa tutti i nove job al terzo tentativo e pubblica i nove asset attesi. Il blocco era
il lock npm del template: commit `7df7c23` e `f8f459c` ripristinano i pacchetti nativi
opzionali multipiattaforma. Per il secondo intervento npm genera 150 record aggiunti,
conservando identici i 1047 record precedenti; dry-run Linux x64/macOS arm64 e build CI
reali passano. Nessuna dipendenza runtime aggiunta al motore.

### Rendering nei worker: limite funzionale verificato

`support/perf/worker-canvas-probe.html`, build ottimizzata `1e9c7dcfea2` e Chrome
155.0.8059.84 via DevTools MCP, entrambi visibili su Windows/GTX 1050:

| API nel dedicated worker | Roves | Chrome |
| --- | --- | --- |
| Costruttore OffscreenCanvas | presente | presente |
| requestAnimationFrame | assente | presente |
| OffscreenCanvas WebGL 1 | null | contesto e pixel rosso corretti |
| OffscreenCanvas WebGL 2 | null | contesto e pixel rosso corretti |

Il codice di creazione dei contesti fa downcast a Window: il solo costruttore OffscreenCanvas
non dimostra supporto al rendering WebGL nei worker. Le precedenti prove di dimensioni/readback
OffscreenCanvas sul thread DOM restano valide, ma non provano questo percorso. Il probe non
verifica la presentazione su un canvas DOM trasferito; anche quel collegamento ha TODO nel
ricevitore del trasferimento. Non suggerire di spostare PixiJS/three.js nei worker come soluzione
già disponibile. Prima usare worker per calcoli/preparazione dati, con concorrenza e code limitate;
spostare lavoro tra thread non ne riduce automaticamente la CPU complessiva.

### Messaggi Worker: riferimento e rimozione delle copie intermedie

`support/perf/worker-messaging-bench.html?auto=1`, Windows/i7-8700, finestra visibile
1024×740, build ottimizzata mimalloc `1e9c7dcfea2`, Chrome 155.0.8059.84 via DevTools MCP.
Tre processi nuovi Roves e tre navigazioni nello stesso browser/contesto MCP Chrome;
quattro iterazioni iniziali e 16 misurate per dimensione/modalità. Tutte le sei corse passano
distacco e controllo degli estremi del payload. Il p95 di 16 campioni è il massimo.

| Buffer 4 MiB, mediana dei tre p50 | Roves prima | Chrome |
| --- | --- | --- |
| Clona: round-trip | 5,44 ms | 3,60 ms |
| Trasferisci: round-trip | 5,77 ms | circa 0,10 ms |
| Clona: tempo sincrono di postMessage | 1,03 ms | 0,80 ms |
| Trasferisci: tempo sincrono di postMessage | 1,34 ms | sotto la risoluzione di circa 0,1 ms |

Round-trip trasferimento Roves: 4,70/6,47/5,77 ms; Chrome circa 0,10 ms in tutte e tre.
Non dedurre CPU nulla da un tempo Chrome arrotondato a zero. Le prime corse esplorative
con la fixture priva del campo constructor_ms sono conservate separatamente.
Il costruttore blocca il parent per 2,03/2,62/2,12 ms in Roves, circa 0,1–0,2 ms in Chrome;
il tempo totale fino al messaggio ready è 26,43/30,72/30,38 contro 14,9/19,6/25,7 ms.
L'attesa sincrona del contesto nel costruttore è un candidato successivo: renderla asincrona
richiede preservare terminate() immediato e chiusura del parent durante l'avvio.

La patch 0231 elimina due ulteriori copie del vettore serializzato al confine roves-js/DOM:
le conversioni standard From e mem::take trasferiscono la proprietà del Vec prodotto da V8.
Rimuove tre helper di copia raw e l'inizializzazione manuale di Vec con set_len: circa 19 righe
di implementazione in meno. Restano il serializer V8, il formato bytes-only e le copie dei
payload trasferiti descritte sopra: non è ancora un trasferimento ArrayBuffer senza copie.

Test locali: 33 passati e due microbenchmark ignorati; i due test structured clone passano,
incluso il nuovo controllo che indirizzo/capacità dell'allocazione non cambino. La riproduzione
delle patch sui file pristine passa. Le dieci verifiche V8 multipiattaforma su `8db6eab`
passano; le build ottimizzate System/mimalloc della CI `39046530793` passano entrambe.
Scaricato il mimalloc `8db6eab`, verificati SHA256 pubblicato
`f3dc275c0a75857d1b25a0f16cf6fb2d1af6dc8ccd62f766db1764921e5eb517`
e prefisso commit incorporato nel binario. Questa build precede la correzione finale dei
risvegli `56957e1`: usarla per misurare le copie, non per certificare quella correzione.

Tre prove iniziali dopo passano (4 MiB clone p50 4,71/4,81/3,72 ms; transfer
5,41/4,39/3,82 ms). Segue confronto alternato prima/dopo, tre processi nuovi per versione,
stessa fixture e finestra; ordine prima/dopo, dopo/prima, prima/dopo, tutti sei PASS:

| 4 MiB, mediana dei tre p50 | Prima `1e9c7dc` | Dopo `8db6eab` |
| --- | --- | --- |
| Clona: round-trip | 5,72 ms | 3,88 ms |
| Trasferisci: round-trip | 6,37 ms | 4,15 ms |
| Clona: postMessage sincrono | 1,06 ms | 0,60 ms |
| Trasferisci: postMessage sincrono | 1,26 ms | 0,84 ms |

Round-trip clone prima 5,87/5,72/5,43, dopo 3,88/4,03/3,69 ms; transfer prima
6,92/6,21/6,37, dopo 3,78/4,15/4,16 ms. Riduzioni delle mediane circa 32%/35%, limitate
a questa API e dimensione: non sono percentuali di CPU risparmiata nel gioco. Chrome resta
circa 0,10 ms per il trasferimento nel riferimento MCP precedente; serve ancora rimuovere
le copie del payload. La prima apertura di una build mostra avvio più lento (fino a 183 ms):
non dedurre miglioramenti di startup da queste corse. Il profilo energetico non è stato cambiato.
Log confronto: `target/perf-local-20261017/worker-messaging-paired-*.stdout.log`.
Log: `target/perf-local-20261017/worker-messaging-baseline-*.stdout.log` e
`chrome-worker-messaging-baseline-*.json`.

Per il codice dei giochi, valutare un pool piccolo e riusato, coda limitata e batch di messaggi.
[workerpool](https://github.com/josdejong/workerpool) offre un pool per browser/Node con licenza
Apache-2.0; [Comlink](https://github.com/GoogleChromeLabs/comlink) semplifica RPC, non sostituisce
la gestione delle copie nel motore. Non aggiunti a Roves: qui non esiste un pool applicativo
scritto a mano da rimpiazzare, e le fixture devono misurare le API Worker direttamente.

### Build ottimizzata aggiornata e confronto allocatori

La CI [39034634597](https://github.com/DRincs-Productions/roves/actions/runs/39034634597)
è verde (entrambe le build, test numerici, avvio bundle e confronto). ZIP scaricati, commit
`1e9c7dcfea2` confermato nelle note e SHA-256 verificati: System `295199c4…0203e9`, mimalloc
`8ff1680b…1d60d9`. Queste build precedono le patch worker 0230/0231. La pagina diagnostica
normale con App lazy passa anche in Roves: i pulsanti Tone sono presenti.

Typewriter/Pixi sulla fixture corretta, CPU e memoria da 60 a 80 s, contatori GPU spenti:

| Allocatore/corsa | CPU di un core | CPU totale macchina | Picco privato | Frame >33 ms / p99 |
| --- | --- | --- | --- | --- |
| System 1 | 50,85% | 65,36% | 518,7 MiB | 9 / 29,62 ms |
| System 2 | 94,72% | 25,3% circa | 514,3 MiB | 10 / 26,02 ms |
| System 3 | 34,27% | 17,83% | 519,8 MiB | 1 / 19,22 ms |
| mimalloc 1 | 34,40% | 17,04% | 635,9 MiB | 1 / 19,77 ms |
| mimalloc 2 | 37,87% | 20,05% | 625,5 MiB | 1 / 18,43 ms |
| mimalloc 3 | 34,13% | 16,36% | 625,0 MiB | 3 / 19,38 ms |

La prima System è contaminata dal forte carico esterno e rimane visibile nel report.
La variabilità della CPU System non consente di attribuire una percentuale di risparmio
all'allocatore. Il suo consumo privato è inferiore nelle tre corse; mantenere il default
mimalloc mentre si approfondisce il compromesso CPU/memoria. I riferimenti Chrome sopra
provengono da momenti e stati di processo diversi: nessuna parità globale con Chrome è
dimostrata. I frame da 60 a 80 s non sono confrontabili direttamente con i vecchi test da
5 a 25 s o da 30 s; non attribuire la differenza alla rimozione dei task audio.

### mimalloc: ritardo di purge (18 ottobre 2026) — nessun effetto

Release v0.7.13 ottimizzata, typewriter/Pixi visibile 1024×740, CPU e memoria 60–80 s,
due corse alternate per configurazione, variabile d'ambiente `MIMALLOC_PURGE_DELAY=0`
contro default. Picco privato 628,5/621,2 MiB (default) contro 629,7/630,8 MiB (purge 0);
working set 402–410 MiB in tutte; CPU 89–98% di un core, rumore maggiore della differenza.
Conclusione: la memoria privata di ~626 MiB non è ritenzione dell'allocatore restituibile
al sistema; nessun codice o opzione da aggiungere. Il divario privato/working set (~220 MiB)
indica memoria impegnata ma non toccata (riserve V8/driver GPU): prossimo passo è attribuirla
con VMMap/heap V8, non cambiare allocatore. Script: scratchpad run.ps1 (non tracciato).

### Matrice di test per libreria (in preparazione, 18 ottobre 2026)

Richiesta dell'utente: un test completo con tutte le librerie insieme e test isolati per
libreria, per attribuire CPU/RAM. Fixture in `test-page/src/FullStackFixture.tsx`
(`?perf=<modo>`), stesso carico in Roves e Chrome, stessa finestra 1024×740 DPR1:

| Modo | Contenuto | Libreria |
| --- | --- | --- |
| `full-stack` | tutto insieme | three.js, PixiJS, motion, markdown-typewriter, GSAP, Tone.js, Worker |
| `only-three` | 400 cubi istanziati con 3 luci + 4000 particelle | three.js |
| `only-tone` | sequenza in loop: PolySynth, MembraneSynth, NoiseSynth, FeedbackDelay, Reverb | Tone.js |
| `only-motion` | box motion + dialogo typewriter (animazioni DOM per lettera) | motion, markdown-typewriter |
| `only-gsap` | tween GSAP su un elemento DOM | GSAP |
| `only-worker` | worker che calcola a slice da 6 ms ogni ~10 ms | Worker |
| `pixi-animated` (esistente) | scena 2D | PixiJS |

Il carico worker è sintetico (aritmetica intera), non un gioco reale. L'audio parte al
caricamento e al primo click/tasto: se il contesto resta sospeso il log scrive `TONE state=`.
Non ancora coperti: animazioni/transizioni CSS native, `<video>` (GStreamer), canvas 2D,
WebAssembly, salvataggi (IndexedDB/localStorage), gamepad. Nessun risultato ancora: le
tabelle verranno aggiunte solo dopo corse alternate Roves/Chrome con la build CI corrente.

### Matrice per libreria, prima corsa (18 ottobre 2026)

Roves release v0.7.13 (ottimizzata, mimalloc) contro Chrome 155 lanciato dallo script (profilo
temporaneo, `--app`, flag anti-throttling e autoplay, NON via MCP). Finestra 1024×740 DPR1,
primo piano, un'unica corsa per cella, GTX1050, piano Risparmio di energia invariato.
CPU = % di un core in 60–80 s sull'intero albero di processi; privato = picco sull'albero
(Chrome: 7–8 processi, il working set somma pagine condivise e non è confrontabile).
Frame da 20 s di rAF a partire da 60 s.

| Modo | Roves CPU | Chrome CPU | Roves privato | Chrome privato | Roves p99 / >33 ms | Chrome p99 / >33 ms |
| --- | --- | --- | --- | --- | --- | --- |
| blank | 12,1% | 9,9% | 453 MiB | 209 MiB | 32,4 ms / 4 (651 frame) | 17,1 / 0 (1201) |
| pixi-animated | 61,1% | 37,7% | 500 MiB | 210 MiB | 20,1 / 1 | 17,1 / 0 |
| only-three | 79,4% | 46,4% | 521 MiB | 237 MiB | 19,6 / 0 | 17,1 / 0 |
| only-motion (+typewriter) | 49,6% | 28,3% | 546 MiB | 240 MiB | 30,4 / 2 | 17,1 / 0 |
| only-gsap | 27,7% | 35,5% | 454 MiB | 213 MiB | 31,8 / 0 | 17,1 / 0 |
| only-worker | 48,4% | 46,6% | 500 MiB | 234 MiB | 32,1 / 0 (650 frame) | 17,1 / 0 (1201) |

Letture prudenti: tre/Pixi/motion costano più CPU in Roves (circa +30–75% relativo), GSAP e
il worker sono alla pari o meglio; la RAM privata è circa 2× in ogni modo, anche a pagina
vuota (≈450 MiB contro ≈210): la base di Roves, non una libreria, domina. Nelle pagine
senza repaint continuo (blank, worker) Roves consegna rAF a ~32/s invece di 60 (p99 ≈32 ms):
da indagare, può essere una scelta di risparmio o un limite di cadenza. Singola corsa per
cella: differenze piccole non sono dimostrate.

**Lacuna di compatibilità trovata (non prestazionale):** in Roves mancano
`createWaveShaper`, `createDelay`, `createConvolver`, `createDynamicsCompressor`,
`createPeriodicWave`, `createScriptProcessor` e AudioWorklet (probe
`support/perf/webaudio-nodes-probe.html`; Chrome li ha tutti). Tone.js ne usa molti
(Reverb → Convolver, FeedbackDelay → Delay, Compressor, Distortion/WaveShaper, matematica sui
segnali): un'eccezione non gestita nella creazione dei nodi fa smontare la pagina React
(`only-tone`/`full-stack` restavano fermi a ~10–12% CPU). Il fixture ora crea ogni pezzo in
try/catch e registra `TONEINFO ok/failed`. Il motore `servo-media` ha il DSP del WaveShaper
ma non il binding DOM; Delay, Convolver e Compressor sono solo segnaposto senza DSP.

### Audio (Tone.js): crescita lineare di memoria e frame persi (18 ottobre 2026)

Con il fixture tollerante (ogni pezzo Tone creato in try/catch), release v0.7.13, stesso
protocollo della matrice, click sintetico per sbloccare l'audio:

| Modo | Roves CPU | Roves privato | Roves p99 / >33 ms | Chrome CPU | Chrome privato | Chrome p99 / >33 ms |
| --- | --- | --- | --- | --- | --- | --- |
| only-tone | 57% | 1225 MiB | 48,3 ms / 71 | 56,4% | 310 MiB | 17,1 / 0 |
| full-stack | 184% | 1394 MiB | 44,2 ms / 46 | 94,1% | 376 MiB | 17,1 / 0 |

In Roves Reverb e FeedbackDelay non si creano (`createWaveShaper` assente) e il resto gira,
cioè Chrome fa più lavoro. La memoria cresce linearmente (~9 MiB/s, heap Rust: arena mimalloc
~1 GiB al picco). Bisect con `?tone=`: tone=none 440 MiB piatto, membranesynth +0,3 MiB/s,
polysynth +1,2 MiB/s, **noisesynth +8 MiB/s**. Cause trovate nel motore audio: (1)
`AudioBufferSourceNode.start()` clonava in profondità l'intero `AudioBuffer` a ogni avvio
(Tone avvia un nuovo sorgente di rumore per colpo); (2) nessun nodo veniva mai rimosso dal
grafo audio, quindi ogni sorgente disconnessa teneva la sua copia fino alla chiusura del
contesto. Correzione `e190b93` (patch 0237): campioni condivisi con `Arc`, rilascio del nodo
quando l'oggetto script viene raccolto e il nodo è finito o non raggiunge più la
destinazione. Test unitario del grafo passa; verifica sul gioco/fixture dopo la CI.
Non ancora misurato: vecchio valore da battere 1225 MiB / 71 frame >33 ms.

**Verifica della correzione (debug CI, stesso fixture `only-tone&tone=noisesynth`, 18 ottobre 2026).**
Build `a3963ea` (prima): privato 533, 607, 694, 771 MiB a 20/30/40/50 s, circa 8 MiB/s.
Build `e190b93` (dopo; ZIP SHA-256 `500118b51e834b35e4ad5bd8d0ae3c279592596a136340c0371a8462d216bd53`,
commit incorporato verificato): 444, 448, 456, 458 MiB, circa 0,5 MiB/s. Stessa macchina e stessa
pagina; sono build di debug, quindi conta la pendenza, non il livello assoluto. Resta da misurare
il confronto ottimizzato Roves/Chrome di `only-tone` e `full-stack`.

### Dove va la CPU nella scena 3D (campionamento a thread, 18 ottobre 2026)

Binario ottimizzato `e20cffa` (mimalloc) con il suo PDB, scena `only-three`, finestra visibile 1024×740.
Strumento: piccolo campionatore in user mode (sospende i thread più occupati ogni 2 ms, legge RIP e
una scansione dello stack, simbolizza con `llvm-symbolizer`; pesi = cicli CPU del thread tra due
campioni via `QueryThreadCycleTime`). Non richiede privilegi di amministratore, ma sospendere i
thread aggiunge costo in `ntdll`: i valori sono indicativi, non misure di profilatore ETW.

| Thread | Quota dei cicli campionati | Contenuto |
| --- | --- | --- |
| principale (event loop SDL + WebRender) | ~38% | `Renderer::draw_frame`, `bind_texture`, `use_program`, `create/destroy_surface_texture` (surfman/ANGLE); ~76% dei cicli in `ntdll`/kernel, ~9% libGLESv2, ~3% driver |
| thread senza frame Rust (driver NVIDIA) | ~21% | `nvwgf2umx.dll` + `ntdll` |
| script (V8) | ~13% | quasi solo attesa; calcolo in `sin/cos` e ricerche di stringhe |
| WebGL | ~8% | `WebGLThread::process`, swap chain surfman |

Lettura: la CPU della scena 3D è dominata dal pilotare la GPU (ANGLE → D3D11 → driver, con la
creazione/distruzione di una texture di superficie condivisa per frame), non dal JavaScript.
Chrome fa lo stesso lavoro nel processo GPU con un unico dispositivo D3D e senza questo passaggio.
Candidati: riusare la `SurfaceTexture` tra i frame invece di crearla e distruggerla (surfman),
ridurre i cambi di stato GL del compositor, verificare la sincronizzazione tra contesto WebGL e
contesto WebRender. Nessuna modifica fatta: serve prima un profilo ETW (richiede amministratore).

### Cadenza di requestAnimationFrame per tipo di lavoro (18 ottobre 2026)

Fixture `support/perf/raf-cadence.html`: quattro cicli rAF da 4 s ciascuno, finestra visibile,
frame al secondo consegnati alla callback. Roves release v0.7.13 contro Chrome 155 (script,
flag anti-throttling), una corsa ciascuno.

| Lavoro nella callback | Roves | Chrome |
| --- | --- | --- |
| nessuno | 33 | 60 |
| riempimento canvas 2D | 60 | 60 |
| `gl.clear` WebGL | 60 | 60 |
| trasformazione di un elemento DOM | 60 | 60 |

La causa è una scelta di Servo in `script_thread.rs`
(`maybe_schedule_rendering_opportunity_after_ipc_message`): se i rAF girano ma non producono
display list, il tick successivo viene rimandato a 30 ms perché "il rendering non cambia". Un gioco
che disegna (canvas 2D, WebGL, DOM) riceve 60 al secondo, quindi i valori di 32 rAF/s misurati nelle
scene `blank`, `only-worker` e `only-tone` sono un effetto del fixture senza disegno, non un difetto
per i giochi. Un ciclo di sola logica (nessun disegno) vedrebbe 33 al secondo invece di 60: se
servisse, il ritardo è una costante da una riga. Nessuna modifica fatta.

### Costo di un AudioContext inattivo (18 ottobre 2026)

Fixture `support/perf/audio-idle.html`: pagina senza audio (`none`), un `AudioContext` senza nodi
(`empty`, quello che un gioco ha tra un suono e l'altro) e un oscillatore a guadagno zero
(`silent`). Click sintetico per sbloccare l'audio, CPU su 20 s a partire da 30 s dopo l'avvio.
Roves `e20cffa` ottimizzato contro Chrome 155, una corsa per cella.

| Variante | Roves CPU | Chrome CPU | Roves privato | Chrome privato |
| --- | --- | --- | --- | --- |
| nessun audio | 0,5% | ~0% | 363 MiB | 247 MiB |
| contesto vuoto, `running` | 8,3% | 4,3% | 367 MiB | 267 MiB |
| oscillatore a gain 0 | 9,3% | 10,9% | 367 MiB | 244 MiB |

Un contesto audio in esecuzione costa un po' di CPU anche in silenzio in entrambi i browser; la
differenza con Chrome è piccola (4 punti percentuali con il contesto vuoto). Nessun intervento.
Il consumo fisso del motore a `about:blank` è 322 MiB privati contro 247 MiB di Chrome a pagina vuota.
