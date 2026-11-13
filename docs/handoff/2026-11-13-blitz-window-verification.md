# Verifica della finestra Blitz

Claude implementa. Codex prepara gli incarichi, valuta il riuso, rivede le modifiche e verifica gli eseguibili.

1. Chiudere la finestra tramite il percorso di Blitz e verificare `gpu_teardown` dopo il limite di frame e dopo una chiusura utente.
2. Verificare un timer avviato da un clic reale. La build precedente consegna il clic ma il callback da 250 ms resta in attesa. Il loop della finestra occupa il thread di Tokio. Riusare Tokio mantenendo V8 e winit sul loro thread.
3. Ripetere sulla GTX 1050 con il template originale `bb9ea779`. Controllare sfondo e personaggio sotto la UI, più avanzamenti, dialogo del nome, Salva/Carica e chiusura. Ripetere con l'artefatto CI.
4. Misurare tempi di frame e byte caricati per scena statica e scena animata. La prova a 640 × 360 registra 6,64 GB di upload in 3.600 frame. Valutare il riuso delle texture dei segmenti DOM con i comandi AnyRender già confrontabili, verificando prima la stabilità delle risorse e l'invalidazione. Il canvas Pixi resta sulla coda di Deno.

Il test a due colori conferma che il riquadro DOM copre il canvas nella finestra reale. I contatori del menu del template confermano 3.600 frame presentati con un livello canvas, zero errori e nessun readback. Dopo Start la scena rimaneva scura prima della correzione dei timer.

## Esito locale sulla GTX 1050

Il commit c5b20ddd7ab presenta il menu e la scena del template bb9ea779 in DX12. Con clic reale su Start ho visto il corridoio, James e la prima battuta. Invio ha mostrato la seconda battuta. Alt+F4 ha restituito close_request e gpu_teardown passed con QueueEmpty. Il test minimo ha confermato il timer durante la finestra. La CI 43386340554 verifica la build e il test del clic reale con timer. Restano la ripetizione con gli artefatti CI, il dialogo del nome e Salva/Carica nella finestra.

La prova interattiva ha anche raggiunto la richiesta del nome. Il tasto fisico z e il pulsante Confirm hanno portato il nome nel dialogo successivo. L'iniezione di testo non ha inserito il nome completo e il clic su Save/Load ha fatto avanzare il dialogo. Settings si apre con Esc. Il flusso Salva/Carica in finestra resta da verificare.

Sulla GTX 1050 DX12, il riuso raster ha ridotto gli upload nei primi 120 frame da 221.184.000 byte a 1.843.200 byte nella scena statica (238 hit su 240) e a 111.513.600 byte nel menu del template (119 hit su 240). La coda GPU si è svuotata a fine prova. I contatori misurano gli upload, non il tempo di frame. La prossima prova deve misurare i tempi sulle scene statiche e animate e verificare la UI sopra il canvas.

## Test automatizzato HUD Save/Load

La fase `game_hud_save_load` è stata aggiunta a `--story` dopo l'accettazione del nome, per riprodurre il punto in cui il clic manuale a (386, 313) aveva fatto avanzare due volte la narrazione. Trova il pulsante visibile, misura il centro con `getBoundingClientRect()`, invia il clic con il `EventDriver`/hit test di Blitz e registra la catena del nodo colpito. Verifica l'apertura del pannello Save/Load e l'assenza di avanzamento del dialogo anche dopo Escape. Il bridge non è stato modificato; il test usa librerie già presenti e non aggiunge dipendenze.

La prima esecuzione sul template `main` `8a8d59bbeac53264d4f8cdb20bbd50deb34d60b9` ha mostrato che il filtro `narration()` non riconosceva la breve riga `...Zorvaq.`; il helper ora accetta testo non di controllo con almeno due lettere. Con il filtro corretto il test HUD ha riprodotto il bug al centro del pulsante Save/Load (383, 314): Blitz colpiva `div.fixed.inset-0.z-0` (`NarrationClickOverlay`) sopra il successivo `NarrationScreen`, e la storia avanzava. Chrome a 640×360 colpiva il pulsante e apriva il pannello. Causa e riproduzione corrispondono a [Blitz #1493](https://github.com/DioxusLabs/blitz/issues/1493); il commento all'issue è stato rifiutato dal plugin GitHub con 403. La CI 43428000337 (normale e JITless) era verde ma precede il filtro e non include il nuovo esito HUD; il relativo artefatto è stato eseguito localmente. La prova nome di quell'artefatto era intermittente tra run, ma il rilancio locale è passato.

La patch temporanea locale, autorizzata per il difetto Blitz che bloccava il gioco, è registrata in `vendor/blitz-dom` 0004 e `vendor/blitz-paint` 0001, con provenienza e rimozione in `ROVES_PATCHES.md`. `blitz-dom` porta i box posizionati z auto/0 nel livello step 8 del contesto di impilamento ordinandoli per z e ordine d'albero; il sollevamento si ferma ai confini di clip/overflow, `contain: paint`, `clip`, inline root, sticky e item flex/grid relativi. `blitz-paint` include il bbox dei figli sollevati nel culling. Le regressioni `src/paint_order.rs` verificano hit target e pixel: controllo A, `z-index: 0` B, `z-index: auto` C e clip overflow; coi crate originali B/C falliscono, con patch tutti e 4 passano. `cargo fmt --check`, `cargo test --locked` (16/16), build locale e applicazione delle patch ai sorgenti upstream sono passati.

Verifica end-to-end sul template aggiornato: `--story` passa Settings, le prime battute, quick save e inserimento nome `Zorvaq`; `game_hud_save_load` passa, colpisce il vero button (path DOM con Save/Load), apre `game://content/game/narration?settings=true&settings_tab=menus%2Fsave-load`, non aggiunge righe alla narrazione, poi chiude con Escape senza avanzarla (`%TEMP%/roves-story-latest-8a8d59-blitz1493-patched.log`). In finestra reale sulla GTX 1050 ho cliccato Start, poi Save/Load: il pannello si è aperto; Escape ha ripristinato la stessa battuta di James, poi Enter ha avanzato a `Me` e `He thrusts out his hand.`. La finestra è stata chiusa con Alt+F4. Un successivo avvio con limite di 180 frame ha verificato la chiusura automatica e `gpu_teardown` passed, adapter Vulkan NVIDIA GTX 1050 (`%TEMP%/roves-window-blitz1493-teardown.log`). **Resta da fare:** CI con entrambe le modalità, includendo il test aggiornato; pubblicare un commento/PR upstream quando il plugin lo consentirà; rimuovere entrambe le patch quando una release Blitz passa `paint_order.rs` senza di esse.

## Diagnostica opt-in dei tempi di frame

Per il punto 4 il probe ha una diagnostica attiva solo con `ROVES_PROBE_FRAME_TIMINGS=1` (`src/frame_timings.rs`). Durante `--window` campiona l'intervallo tra frame step JS/deno_core (`frame_step_interval`), la durata CPU del frame step (`cpu_frame_step`) e, con il renderer GPU, la durata CPU di `GpuWindowRenderer::render` (`cpu_render_present`). Conserva al massimo 3.600 campioni per serie (circa 60 s) e alla chiusura stampa un solo record `ROVES_PROBE_FRAME_TIMINGS` con count, P50/P95/P99/max, campioni oltre 16,67 ms, impostazione della cache raster e contatori di upload, hit e byte. `ROVES_PROBE_DISABLE_RASTER_CACHE=1` disattiva la cache per il confronto A/B; il default resta invariato. Scheduling e rendering non cambiano.

Comandi PowerShell sulla GTX 1050 (scena statica e template, stessa durata):

```powershell
$env:ROVES_PROBE_WINDOW_GPU = '1'
$env:ROVES_PROBE_FRAME_TIMINGS = '1'
Remove-Item Env:ROVES_PROBE_DISABLE_RASTER_CACHE -ErrorAction SilentlyContinue
cargo run --locked --manifest-path support/blitz-v8-probe/Cargo.toml -- --game <dist> --window --window-frames 3600

$env:ROVES_PROBE_DISABLE_RASTER_CACHE = '1'
cargo run --locked --manifest-path support/blitz-v8-probe/Cargo.toml -- --game <dist> --window --window-frames 3600
```

Limiti: sono tempi wall-clock del thread della shell. `cpu_render_present` non è tempo GPU e non prova che il frame sia diventato visibile; con `Fifo` la chiamata di present può attendere la swapchain. Le chiamate a `render` prima della configurazione della superficie non sono campionate. Verifica locale: `cargo fmt --check --manifest-path support/blitz-v8-probe/Cargo.toml` e `cargo test --locked --manifest-path support/blitz-v8-probe/Cargo.toml` passati, 12/12. Nessuna misura reale A/B con GPU è ancora stata raccolta.
