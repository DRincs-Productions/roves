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
| `ROVES_DIRECT_PRESENT=1` | Presentazione diretta: salta tassellazione e paint di egui |

CPU per thread su Windows: PowerShell su `Get-Process play`, con i nomi dei thread letti tramite
`GetThreadDescription` (accesso `0x0800`). È il modo in cui sono stati trovati i colli di
bottiglia qui sotto.

Microbenchmark del binding JS → motore: `cargo test --release --lib bench_native_calls --
--ignored --nocapture` in `components/roves-js`.

## Risolto

| Versione | Problema | Misura |
| --- | --- | --- |
| set. 2026 | Polling gamepad a 10 Hz anche senza controller; repaint egui in loop | Miglioramento confermato dall'utente |
| v0.7.1 | Timer di Windows a 15,6 ms: `setTimeout(0)` durava ~15 ms e i frame stavano a 32–44 fps | `recv_timeout(1 ms)` da 15,7 a 1,6 ms; visual novel da 44 a 60 fps |
| v0.7.1 | Chiamate JS → nativo lente in roves-js | Getter DOM da ~500 a ~100 ns; callback da 722 a 479 ns |
| v0.7.1 | WebGL 2: errore finto a ogni frame con PixiJS, scritto nel log; `vertexAttribIPointer` rotto | Nessun errore, warning limitati a 20 per contesto |
| v0.7.1 | Salvataggi grandi passati nell'URL: blocco con salvataggi a raffica | 20 salvataggi da 1,5 MB in meno di 20 s in CI |
| v0.7.2 | Log su file non bufferizzato (una scrittura su disco per riga) | Buffer da 64 KB |
| v0.7.3 | Livelock delle `MessagePort`: messaggi verso porte chiuse rimbalzavano per sempre tra pagina e Constellation | Visual novel: Script#1 dal 95% al 3,4%, Constellation dal 79% a meno dell'1% |

Presentazione diretta (`ROVES_DIRECT_PRESENT=1`), misura A/B sulla visual novel (GTX 1050):
CPU totale da 212% a 204% di un core. Guadagno piccolo: per ora resta opzionale.

## Aperto

- **RAM:** la visual novel occupa circa 830 MB già al menu, contro i 228 MB dello shell su una
  pagina vuota e i 275 MB di una scena PixiJS piccola. Serve una ripartizione tra heap JS
  (righe `[roves-gc]`), immagini e texture, e il resto del processo.
- **Pause del GC:** il GC di V8 è atomico e su un solo thread. Marcatura parallela o
  concorrente non sono sicure, perché gli hook di trace di Servo non sono thread-safe; quella
  incrementale richiede barriere di scrittura in roves-js. Da affrontare se le righe
  `[roves-gc]` mostrano pause lunghe.
- **egui:** costa circa 2 ms per frame anche quando non mostra nulla.
- **Formati audio:** `canPlayType` risponde vuoto per AAC, M4A e WebM. Probabilmente mancano
  plugin GStreamer nel pacchetto.
- **Gamepad:** sostituire il polling hot-plug residuo (1 Hz) con gli eventi SDL.
- **Steam:** rendere adattivo il polling delle callback nelle build `steam`.
- **Allocatore:** mimalloc diventa default solo con un miglioramento ripetibile su hardware
  reale.

Dettagli tecnici di ogni intervento: `CUSTOMIZATIONS.md`. Analisi della presentazione diretta:
`docs/FAST_PATH_RENDERING.md`.
