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

## Percorso corrente

Per ogni frame visibile della pagina desktop:

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

Primo esperimento, a rischio contenuto:

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
- AccessKit non richiede un aggiornamento dell'albero;
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

I contatori `[roves-perf]` vanno estesi con:

- `direct_presents` e `composited_presents`;
- `egui_runs`, `egui_tessellations` e `egui_paints`;
- `framebuffer_blits`;
- tempo CPU aggregato di WebView paint, egui update/paint e present;
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

## Sequenza di implementazione

1. Estendere i contatori per distinguere frame diretti e composti. **Completato.**
2. Estrarre una decisione pura per il fast path con test unitari. **Completato.**
3. Implementare il fast path A dietro `ROVES_DIRECT_PRESENT=1` per misure A/B.
4. Misurare se il painter egui incide materialmente su CPU/GPU frame time.
5. Solo con un risultato positivo, prototipare il contesto a presentazione differita del fast
   path B.
6. Rendere predefinito un percorso soltanto dopo smoke test completi e misure su hardware reale.

I contatori aggiunti sono inattivi insieme al resto della diagnostica quando
`ROVES_PERF_LOG_INTERVAL_MS` non è impostata; non introducono quindi timer o logging nelle build
normali. `direct_presents` rimarrà zero finché il prototipo opt-in del punto 3 non viene attivato.
