# Roves — Performance tracking

## Obiettivo

Ridurre CPU e memoria dei giochi eseguiti con Roves, con particolare attenzione allo stato inattivo, fino a ottenere un comportamento almeno comparabile a Chrome a parità di pagina, risoluzione, backend grafico e condizioni di misura.

## Stato

- Fase corrente: analisi iniziale
- Modifiche al codice runtime: nessuna
- Ottimizzazioni confermate: nessuna

## Criteri di misura da definire

- CPU media e picco a pagina inattiva, dopo warm-up
- CPU media con scena PixiJS controllata
- GPU utilization e frequenza di presentazione
- FPS e frame time p50/p95/p99
- RSS/private working set e memoria GPU
- Numero di thread e processi
- Allocazioni/heap JS dopo GC e memoria WebRender/cache
- Consumo con finestra visibile, non in focus e minimizzata

Il confronto con Chrome dovrà misurare l'incremento causato dalla pagina, non soltanto il totale di un browser che potrebbe avere processi condivisi già avviati.

## Fatto

- Esaminato il loop eventi SDL3: quando non esistono deadline periodiche usa una wait bloccante, quindi non emerge un busy-loop incondizionato del main thread.
- Individuato il polling gamepad desktop: la feature è abilitata di default e mantiene un risveglio ogni 100 ms quando il delegate esiste, anche se non è collegato alcun controller.
- Individuate feature desktop abilitate di default da valutare separatamente: gamepad, WebGPU, WebXR, clipboard, JIT e logging massimo a livello info.
- Individuati i pool configurati di default: layout, worker generici, runtime async e WebRender; possono aumentare thread e memoria di base.
- Verificato che il repaint della WebView esegue paint e present; le richieste di redraw sono coalesciate per evitare accodamento illimitato.
- Separata l'animazione dello splash, limitata alla fase di caricamento, dal consumo persistente a gioco avviato.

## In corso

- Stabilire se il consumo CPU inattivo proviene dal polling gamepad, da richieste di frame ancora attive, da WebRender/compositor, da egui o da thread secondari.
- Determinare quali feature inizializzano risorse anche quando il gioco non le usa.
- Analizzare il costo di memoria di thread pool, WebRender, SpiderMonkey, WebGPU/WebXR e cache.
- Preparare una matrice di benchmark riproducibile Roves/Chrome.

## Ipotesi da verificare

| Priorità | Ipotesi | Segnale atteso | Stato |
| --- | --- | --- | --- |
| Alta | Polling gamepad ogni 100 ms impedisce l'idle profondo | Wake-up regolari del main thread; CPU ridotta senza feature gamepad | Da misurare |
| Alta | La pagina mantiene callback `requestAnimationFrame` o repaint indiretti anche senza ticker Pixi espliciti | Redraw/present continui; contatore RAF o repaint non nullo | Da misurare |
| Alta | Feature inutilizzate (`webgpu`, `webxr`, gamepad) aumentano il footprint iniziale | RSS/thread inferiori in build minimali A/B | Da misurare |
| Media | WebRender o il compositor presenta frame invariati | Present regolari senza variazioni della display list | Da misurare |
| Media | Egui richiede repaint persistenti | `has_requested_repaint()` resta vero dopo il caricamento | Da misurare |
| Media | Pool di thread sovradimensionati per un runtime single-game | Molti thread inattivi con stack/allocator residenti | Da misurare |
| Media | Cache e heap non vengono ridotti dopo il caricamento | RSS elevato dopo GC e idle prolungato | Da misurare |
| Bassa | Lo splash continua oltre la fase prevista | Wake-up a circa 30 FPS oltre gli 8 secondi | Da verificare |

## Prossimi passi di analisi

1. Costruire una pagina minima vuota e una scena PixiJS deterministica senza ticker.
2. Raccogliere baseline con stessa macchina, dimensione finestra, VSync, build release e warm-up.
3. Aggiungere temporaneamente contatori diagnostici per wake-up, redraw, paint, present e callback RAF.
4. Profilare per thread e acquisire un flame graph/ETW equivalente durante 30–60 secondi di idle.
5. Eseguire build A/B disabilitando una feature alla volta, senza ancora adottare modifiche definitive.
6. Classificare ogni intervento per guadagno CPU, RAM, compatibilità e rischio di regressione.

## Decisioni

- Non verranno applicate ottimizzazioni prima di avere baseline e profiling per thread.
- CPU e RAM saranno trattate separatamente: una riduzione dei wake-up non implica necessariamente una riduzione del footprint.
- Le funzioni da gioco (gamepad, Steam, WebGPU) non saranno rimosse globalmente senza una strategia opt-in/opt-out compatibile.

## Registro

### 2026-09-22

- Avviata l'analisi prestazionale.
- Creato il registro.
- Identificati i primi candidati nel loop SDL3, nelle feature predefinite e nei pool di thread.
- Nessuna modifica funzionale effettuata.
