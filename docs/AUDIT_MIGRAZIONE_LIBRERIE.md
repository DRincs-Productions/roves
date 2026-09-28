# Audit delle migrazioni di librerie

Aggiornato il 27 settembre 2026. Questo audit registra usi trovati nei manifest e nel codice
del checkout corrente. Un nome assente dal grafo della shell non implica che sia eliminabile dal
workspace o dagli altri target Servo.

## SDL3 e `winit`

- `servoshell` usa SDL3 per finestra, event loop e input desktop. La dipendenza `egui-winit` e il
  backend `winit` di `egui_glow` non sono nel grafo normale Windows di `servoshell` (`cargo tree
  --locked -p servoshell --target x86_64-pc-windows-msvc --edges normal`).
- `winit` resta una dipendenza di sviluppo di Servo e può restare nei percorsi mobile o nei tool;
  la verifica va ripetuta per il target interessato prima di modificare il workspace.
- Il gamepad è gestito dal delegate SDL sul thread principale. Il probe CI collega un dispositivo
  virtuale SDL e verifica apertura, stato di assi/pulsanti e rimozione su Linux, Windows e macOS.
  I test unitari verificano la conversione SDL→Servo. Nessuno dei due prova la consegna a una
  WebView live; il runtime gamepad macOS resta disabilitato in attesa delle prove interattive
  elencate in `SDL3_WINDOWING_TESTING.md`.

## `surfman` e grafica

La ricerca `cargo tree --locked -i surfman -p servoshell --target x86_64-pc-windows-msvc` e i
manifest mostrano un uso condiviso, non limitato alla finestra desktop.

| Area | Consumatori trovati | Responsabilità visibile nel codice | Passo necessario prima di rimuoverlo |
|---|---|---|---|
| Shell desktop | `servoshell`, `headed_window.rs`, `headless_window.rs`, `accelerated_gl_media.rs` | Contesti GL, rendering offscreen, present/blit e display nativo per media accelerati | Tracciare thread, surface e costo del passaggio finale; prototipo sostituibile e test su driver reali |
| Compositore e paint | `servo-paint`, `servo-paint-api`, `components/paint` | Swap chain, surface texture, condivisione con WebRender ed immagini esterne | Parità di composizione, screenshot, resize, sincronizzazione e teardown |
| WebGL e canvas | `servo-webgl`, `components/webgl`, `webgl_thread.rs` | Creazione dei context, FBO, swap chain e rendering WebGL | Test WebGL/canvas, perdita del context e backend ANGLE/GL per piattaforma |
| WebXR | `servo-webxr`, `surfman_layer_manager.rs`, backend OpenXR | Layer/surface XR e adapter grafico OpenXR | Dispositivo/runtime XR disponibile e verifica di ogni backend |
| Mobile e headless | paint API condivisa e contesto software Servo | Context software/offscreen oltre ai backend desktop | Build e smoke test per Android/OHOS e modalità headless |

Il grafo Cargo Windows (`cargo tree --locked -i surfman -p servoshell --target
x86_64-pc-windows-msvc`) conferma i consumer `servo-paint`, `servo-paint-api`, `servo-webgl`,
`servo-webxr` e `servoshell`. La dipendenza diretta desktop di `servoshell` non è ridondante:
`desktop/accelerated_gl_media.rs` legge il context EGL e il display nativo di surfman per inizializzare
i media accelerati su Windows/Linux; su macOS il bridge è un no-op. La CI ora protegge esplicitamente
questa interop e la feature raw-window-handle del manifest. La dipendenza Android/OHOS è in una sezione target separata.
La patch 0045 restringe `surfman/sm-x11` a Linux, inclusi paint e WebXR; il controllo `cargo tree` della matrice desktop verifica che Linux lo mantenga e che Windows/macOS non lo abilitino. `surfman` resta nel grafo dei target che usano i context grafici e non viene rimosso.

L'audit non identifica un consumatore isolato che possa perdere la dipendenza senza cambiare
prima l'interfaccia condivisa di rendering. La shell registra ora i tempi per fase WebView paint, egui, shell paint e present
(`[roves-perf-time]`, patch 0046), ma non è ancora stato raccolto un benchmark GPU o una misura
di latenza su hardware reale; quindi non c'è evidenza per iniziare un nuovo backend SDL/wgpu né per dichiarare una
parità. Il lavoro corretto resta misurare il percorso offscreen→present e prototipare un solo
consumatore, mantenendo `surfman` come baseline.

## Unicode e ICU4X

- ICU4X 1.5 è già una dipendenza diretta e usata: `icu_segmenter` fornisce word/line breaking in
  `components/layout/flow/inline`, `icu_properties` fornisce proprietà Unicode per layout/font,
  `icu_locid` è usato da layout e font.
- `unicode-bidi` resta usato per livelli e riordino bidirezionale nel layout e per classi bidi
  negli input. Sostituirlo non è parte di questo audit.
- `unicode-segmentation` è usato da `components/shared/base/rope.rs` per confini grapheme;
  resta distinto dal line/word breaking ICU4X. `unicode_categories` è usato solo dal classificatore
  `::first-letter`. Un confronto esaustivo di sette gruppi General_Category, con ICU4X 1.5.1 e
  `unicode_categories` 0.1.1, ha trovato 22.523 disaccordi carattere/gruppo: ICU4X riconosce
  molte assegnazioni Unicode successive che la tabella della crate più vecchia tratta
  come non assegnate. Siccome ciò cambia il range Web esposto da `::first-letter`, la migrazione è
  stata annullata e la dipendenza runtime resta. Ripetere il confronto insieme a un futuro upgrade
  della versione dati prima di riaprire la sostituzione.
- `encoding_rs` è usato dal percorso encoding Web e da `tendril`; conservarlo.

La prossima migrazione sensata richiede un servizio duplicato dimostrato e casi di equivalenza
che includano grapheme combining, emoji ZWJ, RTL, CJK e boundary. Non è stata cambiata alcuna
implementazione Unicode perché una divergenza qui può alterare testo Web osservabile.

## Allocatore e logging

- `servo-allocator` usa jemalloc per statistiche heap (`mallctl`, `usable_size`) e per l'ABI
  `malloc/realloc/free` su alcuni target. Windows/OHOS hanno già percorsi `System`; non esiste
  in questo checkout una misura che attribuisca un problema di memoria o frame time a jemalloc.
  Non aggiungere mimalloc senza workload misurato e test dell'ownership FFI.
- `servoshell` usa `env_logger` per output tradizionale, configurazione e file di log speciali;
  tracing è una feature separata per subscriber e layer Tracy/Perfetto. Non è stata trovata una
  doppia inizializzazione dimostrata che giustifichi la rimozione di `env_logger`.

## Stato e test

La CI SDL verifica applicazione delle patch Servo pulito, contratti sorgente, probe SDL virtuale
su tre sistemi desktop, suite unit test `servoshell`, suite `servo-layout` con casi ICU4X e
bundle/package smoke test. La matrice di packaging può essere più lenta dei test preliminari;
vedi il run collegato al commit più recente.
La CI headless non sostituisce le prove di finestra, Web Gamepad API, DPI, IME, accessibilità,
XR o GPU su hardware reale. Nessun obiettivo di rimozione condizionale si considera completato
finché mancano benchmark e confronto di parità previsti nel piano.
