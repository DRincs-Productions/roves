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

### Mappa dei contesti grafici (28 settembre 2026)

Ricognizione richiesta dal piano (fase 2) prima di qualsiasi backend alternativo. Il sorgente di
surfman 0.13 non è nel checkout: i dettagli interni dei backend (FBO del widget, share handle
D3D/IOSurface/EGLImage, swap interval, flush impliciti) restano **non confermati**. SDL3 non crea
mai un contesto GL: `.opengl()` è solo un flag della finestra e ogni contesto appartiene a surfman.
Su Windows servoshell attiva `servo/no-wgl` → `surfman/sm-angle-default`, quindi desktop Windows
gira su ANGLE (EGL su D3D11).

| Consumatore | Oggetti surfman | Thread | Formato / tipo surface | Condivisione | Lifetime / resize | Perdita contesto | Test |
|---|---|---|---|---|---|---|---|
| `SurfmanRenderingContext` (`components/shared/paint/rendering_context.rs`) | `Device`, `Context`, surface GPUOnly, `create_surface_texture` | Main (non `Send`) | ALPHA\|DEPTH\|STENCIL, GLES 3.0 / GL 3.2 | Surface → `SurfaceTexture` → texture GL; espone `connection()` | `Drop` distrugge il contesto; resize < 1x1 → `Err` | Nessuna gestione; `unwrap` su texture | `test_read_pixels`, `test_minimum_size_error`: ora in CI (step paint-api) |
| `WindowRenderingContext` | Connection da display handle, widget surface, `take_window`/`set_window` (mobile) | Main/UI | Come sopra, framebuffer del widget | Contesto padre dell'off-screen; `create_texture` per WebGL | Resize su evento SDL; su Android/OHOS pausa/ripresa ricreano la surface | Errori solo `warn!` | Smoke desktop (Xvfb con resize, macOS, Windows) |
| `OffscreenRenderingContext` (shell desktop) | Nessuna surface propria: FBO GL sul contesto padre | Main | Texture **RGBA8** + renderbuffer **DEPTH_COMPONENT24, senza stencil**; `present()` vuoto | `render_to_parent_callback` → clear + `glBlitFramebuffer` | Resize crea un nuovo FBO e copia il contenuto vecchio | Nessuna | Smoke desktop, `direct_presents>0` |
| `SoftwareRenderingContext` (headless, C API) | Adapter software, surface Generic + `SwapChain` | Main | Come sopra, GL software | Come sopra | `swap_chain.resize` | Nessuna | Unit test sopra (ora in CI); nessuno smoke headless |
| Paint/Painter (compositore) | `connection().create_adapter()` per painter, handle `SwapChains` WebGL | Main | Disegna nel framebuffer legato da `prepare_for_rendering` | Handler immagini esterne WebGL/WebGPU/media | `remove_painter` libera anche i device WebGL | Errori `make_current` solo loggati | Smoke desktop |
| WebGL → WebRender (`components/paint/webrender_external_images.rs`) | `SwapChains::take_surface` → `create_texture` → `NativeTexture`; `recycle_surface` al rilascio | Main, dentro WebRender | TEXTURE_2D o RECTANGLE | Surface fra `Device` diversi (thread WebGL → compositore) | Per frame che usa l'immagine | Nessuna | Solo indiretto (`GpuInfoPanel` crea un contesto WebGL, senza asserzioni) |
| Thread WebGL (`components/webgl/webgl_thread.rs`) | Un `Device` per painter, un `Context` per contesto WebGL, surface Generic ≤ 1024², swap chain attaccata | Thread "WebGL" | Sempre ALPHA\|DEPTH\|STENCIL, flag richiesti simulati | Front buffer via `SwapChains` | Resize della swap chain; distruzione differita se occupato | **Nessuna: `IsContextLost()` restituisce sempre `false`**; `unwrap` su swap/make-current | Nessuna asserzione |
| WebXR (`surfman_layer_manager.rs`, glwindow, OpenXR D3D11, headless mock) | Swap chain staccate per layer; glwindow ha un proprio `Connection`/`Device`/`Context` su una seconda finestra SDL; OpenXR importa texture D3D11 | Thread WebGL (layer), main (glwindow), thread XR (sessione) | DEPTH24_STENCIL8 se richiesto; OpenXR B8G8R8A8(_SRGB) | `SwapChains<LayerId>` condiviso | Per layer, nessun resize | Nessuna | Non in CI |
| Media GL (`desktop/accelerated_gl_media.rs`) | Solo lettura degli handle EGL/X11 nativi del contesto finestra | Thread "GLPlayer" (GStreamer) | Deriva dalla versione del contesto | GStreamer condivide il contesto; fence `glWaitSync` **commentato** | Init unico; attivo solo con pref `media_glvideo_enabled` (default false) e `media-gstreamer` | Nessuna | Contratti sorgente + controllo `sm-x11`; runtime non esercitato |
| egui (`desktop/gui.rs`) | Nessuno diretto: `egui_glow` sul contesto finestra | Main | Framebuffer della finestra | Ospita il callback di blit | `Gui::drop` dopo `make_current` | Nessuna | `repaint_tests` in CI |

**Frame desktop in regime (senza resize):** 2 framebuffer permanenti (FBO off-screen RGBA8 + D24
e framebuffer della finestra, esclusi i target interni di WebRender e le texture egui), 2 clear
(sfondo off-screen, clear a forbice della destinazione), **1 `glBlitFramebuffer`** a piena finestra
in entrambi i percorsi A e composto, 1 present (quello off-screen è vuoto), nessun fence o flush
esplicito nel codice del repository. Un resize aggiunge una copia. Android/OHOS non hanno lo stadio
off-screen: WebRender disegna direttamente nel `WindowRenderingContext`, a conferma che il contratto
Servo supporta già il rendering diretto che il fast path B vuole portare su desktop.

**Conseguenze per il piano.** (1) Il costo che il fast path B può eliminare è esattamente un FBO
RGBA8+D24, un clear e un blit a piena finestra per frame; va misurato con `[roves-perf-time]`
(`shell_paint`) prima di decidere. (2) Nessun consumatore gestisce oggi la perdita del contesto o il
reset del device: il criterio di accettazione "test di perdita contesto" del piano non ha una
baseline da preservare e un backend alternativo non deve dichiararla come parità. (3) WebXR, media
GL e `SoftwareRenderingContext` non sono esercitati da nessuno smoke test: non rimuovere o
modificare quei percorsi basandosi sulla sola CI verde.

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
  doppia inizializzazione dimostrata che giustifichi la rimozione di `env_logger`. La mappa
  completa è nella sezione seguente.

### Mappa di logging e tracing (28 settembre 2026)

| Punto | Dove | Piattaforma / feature | Filtro | Output |
|---|---|---|---|---|
| `RovesLogger` (env_logger avvolto) | `ports/servoshell/desktop/logging.rs` `init`, chiamato da `desktop/cli.rs` | Desktop, sempre compilato; **solo se argv è vuoto** (avvio da bundle/doppio clic) | `RUST_LOG`, default `info` | `<game_data_dir>/roves.log`, troncato a ogni avvio |
| `Servo::setup_logging` (`BothLogger` env_logger + `FromEmbedderLogger`) | `components/servo/servo.rs`, chiamato da `desktop/app.rs` e `ffi/capi` | Processo principale; su desktop è no-op se `RovesLogger` è già installato (fallimento di `set_boxed_logger` ignorato, patch del fork) | `RUST_LOG` senza default: solo `error` se non impostato | stderr + inoltro Warn+ al constellation |
| `set_logger` (`BothLogger` + `FromScriptLogger`) | `components/servo/servo.rs`, `run_content_process` | Solo processi di contenuto multiprocesso (`-M`) | `RUST_LOG` passato dal constellation | stderr + IPC al constellation |
| `android_logger` | `ports/servoshell/egl/android/mod.rs` | Android, solo se il flag JNI `log` è vero | Moduli Debug fissi + `logStr` da Java | logcat, tag `servoshell` |
| `hilog::Logger` | `ports/servoshell/egl/ohos/mod.rs` | OHOS | Moduli fissi, `--log-filter`/pref `log_filter` | hilog, opzionale `servo.log` |
| `init_tracing` (`tracing_subscriber` registry) | `ports/servoshell/lib.rs` | Feature `tracing` (+ `tracing-perfetto`, `tracing-hitrace`, `tracing-tracy`) | `--tracing-filter` o `SERVO_TRACING`, default OFF; **non legge `RUST_LOG`** | Solo layer Perfetto (`servo.pftrace`), HiTrace, Tracy; nessun layer fmt |

`env_logger` è usato solo da `servoshell` e da `components/servo` (dipendenza non opzionale).
`servo_tracing::instrument` e le macro di `profile_traits` si espandono in `tracing::instrument`
/span solo con la feature `tracing` del crate chiamante, altrimenti a costo zero. Le macro in
`constellation/tracing.rs`, `paint/tracing.rs` e `servoshell/desktop/tracing.rs` sono
`log::trace!` con target personalizzati, non chiamate a `tracing`.

**Esito.** Nessun record raggiunge oggi due backend: `init_tracing` evita di proposito
`LogTracer`/`SubscriberInitExt::init` (commento in `lib.rs`), quindi `log` non entra in tracing.
Lacune reali, nessuna delle quali riguarda l'avvio da bundle di un gioco:

1. Avvio desktop da CLI con argomenti: nessun logger finché `Servo::setup_logging` non gira in
   `App`; i `log::info!` di `cli.rs` precedenti vanno persi, poi il filtro predefinito è solo
   `error` invece di `info`.
2. `FromEmbedderLogger` non gira mai negli avvii da bundle (vince `RovesLogger`): gli avvisi del
   processo principale non raggiungono `handled_warnings` del constellation tramite `log`.
   Compromesso già documentato in `logging.rs`.
3. Con la feature `tracing` attiva, gli eventi `tracing` di crate terze che normalmente cadono su
   `log` (tramite la feature `tracing/log`, presente nel lockfile; quale crate la attivi non è
   confermato senza `cargo tree`) spariscono da `roves.log` dopo `init_tracing`, perché il
   subscriber non ha layer testuali.
4. Android con flag `log` falso: nessun logger installato.
5. Stampe doppie non legate a log/tracing: messaggi console (`println!` + `log::log!`) e panic hook
   (stderr + `error!`) in esecuzione da terminale.

Una migrazione a `tracing` non risolverebbe nessuna duplicazione, perché non ce ne sono;
toccherebbe cinque inizializzazioni in tre piattaforme e il forwarding al constellation. Secondo il
piano (fase 6) resta quindi **non avviata**. Se servisse, le correzioni mirate sono locali: 1
(inizializzare `RovesLogger` o un filtro `info` anche negli avvii CLI) e 3 (un layer fmt verso
`roves.log` quando `tracing` è attiva), senza rimuovere `env_logger`.

## Stato e test

La CI SDL verifica applicazione delle patch Servo pulito, contratti sorgente, probe SDL virtuale
su tre sistemi desktop, suite unit test `servoshell`, suite `servo-layout` con casi ICU4X e
bundle/package smoke test. La matrice di packaging può essere più lenta dei test preliminari;
vedi il run collegato al commit più recente.
La CI headless non sostituisce le prove di finestra, Web Gamepad API, DPI, IME, accessibilità,
XR o GPU su hardware reale. Nessun obiettivo di rimozione condizionale si considera completato
finché mancano benchmark e confronto di parità previsti nel piano.
