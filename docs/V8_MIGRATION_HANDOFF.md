# V8 migration — handoff operativo

## Aggiornamento 18 ottobre (pomeriggio)

- `19a8dea` pushato: patch 0236, i costruttori callable seguono la raggiungibilit� JS (40 test locali PASS). CI in corso, guadagno RAM non misurato.
- mimalloc `PURGE_DELAY=0`: nessun effetto (privato ~626 MiB, ws ~405). Arena mimalloc ~214 MiB committed; heap JS 15 MiB usati su 58�97 totali.
- In corso: `ROVES_V8_FLAGS=--max-semi-space-size=2/8` sul binario v0.7.13 (script scratchpad run.ps1).
- Nuovi fixture `test-page/src/FullStackFixture.tsx` (modi `full-stack`, `only-*`): vedi matrice in ROVES_PERFORMANCE.md. Da buildare (npm) dopo le misure.
- Da fare: matrice per libreria Roves vs Chrome, pagina wiki prestazioni, aggiornare questi file con i risultati.

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

## Prossimi interventi

1. Esaminare costo degli aggiornamenti CSS ripetuti e invalidazioni, riusando Stylo;
   prima una riproduzione comparabile, poi eventuale modifica. Nessuna nuova build pesante gratuita.
2. Native Web Animations per motion: Servo upstream non offre ancora il playback completo;
   non attivare il pref per default. Riutilizzare Stylo/WebRender per timing/compositing,
   preservando promise/eventi, stacking e hit testing. Risparmio CPU ancora non misurato.
3. Trasferimenti ArrayBuffer worker: payload ancora copiato (4 MiB circa4,15 ms contro
   circa0,1 ms Chrome). Valutare BackingStore/serializer V8 con ownership, cleanup e
   fallback IPC; niente registro globale di puntatori o claim zero-copy non verificato.
4. Worker WebGL1/2 restituisce null e worker rAF assente: OffscreenCanvas esposto non
   certifica rendering Pixi/three nei worker. AudioWorklet mancante; Tone/howler richiedono
   prove di playback, non solo caricamento dei moduli.

## Percorsi e metodo

- Release: `target/perf-local-20261018/release-v0.7.13/roves/play.exe`.
- Debug corretto: `target/perf-local-20261018/ci-debug-a3963ea/servo-test-page/play.exe`.
- Raw CPU e worker: `target/perf-local-20261018/`; Chrome `chrome-worker-matrix.json`.
- Fixture versionate: `support/perf/`; copiarle in `test-page/dist` dopo ogni rebuild.
- Server: `python -m http.server 48761 --bind 127.0.0.1 --directory test-page/dist`.
- Percorso autorizzato: eseguibile in primo piano, pagina con `auto=1` che si chiude da sé,
  campionatore separato solo lettura. Il comando con lancio/chiusura in background era
  rifiutato dalla policy: non riproporlo tramite wrapper.
- Misure costose annunciate prima; niente agenti né build locali complete senza necessità.
  Suite mirata roves-js:39 PASS,2 ignored; patch0235 replay pristine selezionato PASS.
- Cronologia completata in CUSTOMIZATIONS.md e ROVES_PERFORMANCE.md; vecchio handoff
  disponibile nella storia git. Questo file descrive il punto corrente, non una migrazione
  completa di tutte le API web.

## Audit CSS senza modifica

CSSStyleDeclaration::set_property_inner usa già Stylo prepare_for_update e il flag changed
per evitare invalidazioni di valori invariati. Non introdurre una cache duplicata.
SetCssText ha semantica di mutazione diversa: prima confrontare lo standard e Chrome,
non eliminare notifiche MutationObserver come ottimizzazione non verificata.
