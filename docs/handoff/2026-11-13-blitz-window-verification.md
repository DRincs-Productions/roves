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
