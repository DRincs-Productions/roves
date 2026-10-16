# Fast path di rendering desktop

Analisi del 24 settembre 2026 sul percorso SDL3 del fork Servo `v0.5.0`.

## Decisione

Il fast path va sviluppato senza sostituire egui. Il repository usa già egui `0.36.2` ed
`egui-file-dialog 0.15.0`, aggiornati rispettivamente da `0.34.3` e `0.13.0`. Non risulta
installato un secondo toolkit desktop come Iced, Slint, Vizia o Floem. Il backend winit di egui
è già stato sostituito dal bridge locale `SdlEguiGlow`, mentre egui resta necessario per splash,
errori di caricamento, dialoghi, permessi, autenticazione, selezione file e accessibilità.

Cambiare toolkit riscriverebbe queste funzioni e il relativo bridge SDL3 senza rimuovere il
costo dominante del frame Web. La scelta resta quindi: conservare egui come piano overlay e
saltarlo quando il gioco non sta mostrando UI native.

## Percorso composto (fallback)

Quando overlay o altre condizioni richiedono il percorso composto:

1. Servo/WebRender dipinge nel `OffscreenRenderingContext` posseduto da `HeadedWindow`.
2. `Gui::update` esegue comunque un frame egui e registra un `PaintCallback` sul layer di sfondo.
3. Il callback chiama `OffscreenRenderingContext::render_to_parent_callback`, che pulisce la
   destinazione e copia il framebuffer con `glBlitFramebuffer`.
4. `Gui::paint` tessella le shape egui, esegue `egui_glow::Painter::paint_primitives` e libera le
   texture scadute.
5. La shell chiama `WindowRenderingContext::present`.

La toolbar e le tab sono già disabilitate, ma questo non elimina il frame egui né il framebuffer
off-screen. Le recenti correzioni di scheduling evitano che il percorso venga eseguito
continuamente quando la pagina è davvero inattiva; il costo resta però presente per ogni frame
richiesto dal gioco.

## Fast path A: presentazione diretta dell'off-screen

Implementato e attivo per default dalla shell v0.7.4 (prima solo con `ROVES_DIRECT_PRESENT=1`).
Misura sulla visual novel al menu, 60 fps: CPU da 98,6% a 91,1% di un core, `shell_paint` da
~128 a ~77 µs per frame, nessuna tassellazione egui.

Comportamento già implementato:

- Servo continua a usare l'attuale `OffscreenRenderingContext`.
- Il normale `Gui::update` continua inizialmente a drenare input e AccessKit.
- Se non esistono overlay, dialoghi, tooltip, errori o splash, la shell invoca direttamente il
  callback di blit e presenta la finestra senza tessellare o disegnare shape egui.
- Qualunque overlay riattiva nello stesso frame il percorso composto esistente.

Questo elimina il lavoro del painter egui dai frame normali ma non elimina l'allocazione
off-screen né `glBlitFramebuffer`. È una fase utile per misurare separatamente il costo egui dal
costo della copia GPU.

### Condizioni conservative

Il percorso A è ammesso solo quando:

- esiste esattamente una WebView attiva;
- la pagina occupa l'intera area fisica della finestra;
- non esiste alcun `Dialog` attivo;
- splash e schermata di errore sono disattivati;
- non esistono status tooltip o focus egui;
- AccessKit è inattivo e non ci sono aggiornamenti pendenti;
- non ci sono texture egui da caricare o liberare.

Se una condizione cambia, il fallback composto deve essere immediato e non richiedere la
ricreazione del contesto GL.

## Fast path B: WebView sul framebuffer della finestra

Secondo esperimento, quello con il guadagno potenziale maggiore:

- la singola WebView dipinge direttamente nel `WindowRenderingContext`;
- il `present()` richiesto internamente da `ServoShellWindow::repaint_webviews` viene differito;
- la shell può disegnare eventuali overlay egui sullo stesso back buffer;
- un solo `present()` conclude il frame.

Questo può eliminare framebuffer off-screen, texture colore/depth e blit a piena finestra. Non
può essere ottenuto semplicemente passando oggi il contesto della finestra alla WebView:
`repaint_webviews()` presenta immediatamente dopo `WebView::paint()`, quindi un overlay verrebbe
disegnato nel frame successivo e potrebbe produrre flicker o due swap per frame. Serve un
contratto di presentazione differita, limitato alla shell desktop.

Una possibile implementazione è un wrapper `RenderingContext` desktop che inoltra rendering,
resize e letture al `WindowRenderingContext`, ma rende `present()` una richiesta differita. La
shell possiede l'unico present reale dopo l'eventuale pass egui. Il percorso off-screen corrente
rimane disponibile come fallback finché il prototipo non supera i test.

## Metriche A/B

I contatori `[roves-perf]` già disponibili misurano:

- `direct_presents` e `composited_presents`;
- `egui_runs`, `egui_tessellations` e `egui_paints`;
- `framebuffer_blits`;
- tempo CPU aggregato di WebView paint, egui update/paint e present; **Completato** come riga
  `[roves-perf-time]` (totale e massimo in µs per `webview_paint`, `egui_run`, `shell_paint`,
  `present`; tempo wall-clock lato CPU, il lavoro GPU asincrono emerge soprattutto in `present`);
- dimensione e numero dei framebuffer permanenti.

Le fixture restano `?perf=blank`, `?perf=pixi-static` e `?perf=pixi-animated`. Il confronto deve
usare la stessa build release, GPU, risoluzione, VSync e warm-up. La CI verifica contratti e
fallback, non impone soglie temporali sui runner condivisi.

## Test necessari

- unit test della decisione fast-path/fallback per ogni condizione precedente;
- smoke test che apre e chiude Save File, file picker, prompt e permessi durante il fast path;
- resize, fullscreen, DPI/display change e minimizzazione;
- screenshot/readback e schermata di errore;
- AccessKit attivo e disattivato;
- PixiJS statico senza ticker e PixiJS animato;
- Windows portable/MSI, macOS portable/DMG e Linux portable/DEB.

## Lavoro restante

1. Misurare il costo residuo di offscreen/blit separandolo dal render WebRender.
   Le attese in present sono tempi wall-clock, non consumo CPU puro.
2. Solo con evidenze favorevoli, prototipare il contesto a presentazione differita del fast
   path B.
3. Rendere predefinito il percorso B soltanto dopo smoke test completi e misure su hardware reale.

I contatori aggiunti sono inattivi insieme al resto della diagnostica quando
`ROVES_PERF_LOG_INTERVAL_MS` non è impostata; non introducono quindi timer o logging nelle build
normali. Un frame diretto incrementa `direct_presents`, `framebuffer_blits` e
`window_presents`; il fallback incrementa `composited_presents`. `egui_runs` continua a crescere
anche nel percorso diretto; `egui_tessellations` e `egui_paints` contano solo i frame composti.

### Verifica automatica del percorso A

Gli unit test coprono ogni condizione di fallback.
Gli smoke test desktop avviano i bundle reali con
`ROVES_PERF_LOG_INTERVAL_MS=1000` e falliscono senza almeno un intervallo con
`direct_presents > 0`. Restano i controlli di avvio, salvataggio e ciclo finestra SDL3.
Non sono imposte soglie CPU, RAM o frame-time sui runner condivisi.

La decisione usa l'output egui corrente, include le texture da caricare e liberare e viene
consumata dal paint. Splash ed errori la azzerano; anche un dialogo appena chiuso mantiene il
frame composto. Un resize tra update e paint o un callback di blit indisponibile conserva il
fallback nello stesso frame. Il percorso B resta da implementare. I test manuali della matrice
sopra e le misure A/B su hardware reale restano necessari prima di valutare un cambio del default.
