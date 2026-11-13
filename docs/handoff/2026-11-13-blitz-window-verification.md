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

`cargo fmt --check` è passato; `cargo test --locked --manifest-path support/blitz-v8-probe/Cargo.toml` è passato (8 test, compilando il probe aggiornato). La run 79 sul commit `01d8d940466` non include ancora questa modifica; il test template sarà eseguito nella prossima CI dopo il push autorizzato. Il controllo è diagnostico e il suo esito sarà riportato come warning, senza alterare il gate principale a due dialoghi. Dopo la CI, ripetere il clic fisico in finestra sul template per distinguere un problema di coordinate DPI o composizione da quello riprodotto nel percorso headless.

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
