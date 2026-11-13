# Verifica della finestra Blitz

Claude implementa. Codex prepara gli incarichi, valuta il riuso, rivede le modifiche e verifica gli eseguibili.

1. Chiudere la finestra tramite il percorso di Blitz e verificare `gpu_teardown` dopo il limite di frame e dopo una chiusura utente.
2. Verificare un timer avviato da un clic reale. La build precedente consegna il clic ma il callback da 250 ms resta in attesa. Il loop della finestra occupa il thread di Tokio. Riusare Tokio mantenendo V8 e winit sul loro thread.
3. Ripetere sulla GTX 1050 con il template originale `bb9ea779`. Controllare sfondo e personaggio sotto la UI, più avanzamenti, dialogo del nome, Salva/Carica e chiusura. Ripetere con l'artefatto CI.
4. Misurare tempi di frame e byte caricati per scena statica e scena animata. La prova a 640 × 360 registra 6,64 GB di upload in 3.600 frame. Valutare il riuso delle texture dei segmenti DOM con i comandi AnyRender già confrontabili, verificando prima la stabilità delle risorse e l'invalidazione. Il canvas Pixi resta sulla coda di Deno.

Il test a due colori conferma che il riquadro DOM copre il canvas nella finestra reale. I contatori del menu del template confermano 3.600 frame presentati con un livello canvas, zero errori e nessun readback. Dopo Start la scena rimaneva scura prima della correzione dei timer.
