# TODO — cose da fare su questo fork di Servo

Backlog di lavoro noto ma non ancora fatto sulla copia vendorizzata/patchata di Servo in
questa cartella. Vedi [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) per le modifiche già
applicate e [`CLAUDE.md`](./CLAUDE.md) per il protocollo da seguire quando si chiude uno di
questi punti (aggiornare `CUSTOMIZATIONS.md` + rigenerare la patch nella stessa sessione).

---

## 1. Collegare `embedded.yml` al build patchato invece del binario Servo stock

**Stato:** noto, in sviluppo — non ancora prioritario.

`../.github/workflows/embedded.yml` scarica oggi il binario `servoshell` ufficiale
precompilato da `servo/servo` (vedi `SERVO_TAG`), non il fork patchato in questa cartella.
Finché resta così, ogni release "embedded" mostra ancora la UI browser stock (toolbar/tab
strip) che le patch in `CUSTOMIZATIONS.md` rimuovono solo nel codice, non nel binario
distribuito.

Quando si affronta questo punto, `servo/.github/workflows/test.yml` è già il punto di
partenza corretto: builda da sorgente (tag pristine + `patches/`) — oggi è dormiente perché
GitHub Actions scopre i workflow solo alla radice di un repo reale (vedi commento in testa a
quel file). Le opzioni sono, in ordine di probabile complessità:
- pubblicare `servo/` come repo standalone (es. `BlackRam-oss/servo`, già menzionato in
  `CLAUDE.md`) e far scaricare a `embedded.yml` gli artifact di build di quel repo;
- oppure integrare uno step di build-from-source direttamente dentro `embedded.yml` stesso.

## 2. Rimuovere il popup del menu contestuale (tasto destro) in `dialog.rs`

`Dialog::ContextMenu` in `ports/servoshell/desktop/dialog.rs` espone un menu tasto-destro in
stile browser (Back/Forward/Reload/View Source/Inspect...). Per un videogioco questo è
indesiderato: rompe l'immersione e espone voci come "View Source" che non hanno senso fuori
da un browser.

**Deciso:** disabilitarlo del tutto (no-op sul tasto destro), non sostituirlo con un menu
alternativo. Non ancora toccato — nessuna patch esiste per questo punto.

## 3. Rimuovere la possibilità di effettuare il reload della pagina

Per un videogioco il reload della pagina (scorciatoia da tastiera e/o voce di menu) è
indesiderato: può resettare stato di gioco in modo inatteso. Da individuare dove il reload è
attualmente esposto (es. `Dialog::ContextMenu` in `ports/servoshell/desktop/dialog.rs`,
eventuali keybinding come Ctrl+R/F5) e disabilitarlo. Non ancora toccato — nessuna patch
esiste per questo punto.

## 4. Rimuovere del tutto la possibilità di navigazione "indietro"

L'utente non deve poter tornare a una pagina precedente in nessun modo (scorciatoie da
tastiera, gesture del trackpad/mouse, ecc.) — per un videogioco tornare "indietro"
può rompere completamente lo stato di gioco. Da `CUSTOMIZATIONS.md` (voce del 2026-08-06
sulla rimozione dello stato di navigazione morto da `gui.rs`): la navigazione back/forward
reale (Alt+freccia sinistra/destra, ecc.) chiama `WebView::go_back`/`go_forward` direttamente
in `headed_window.rs`, bypassando `Gui` — è lì che va cercato e disabilitato il keybinding,
oltre a qualunque voce "Back" rimasta nel menu contestuale (vedi punto 2 sopra). Non ancora
toccato — nessuna patch esiste per questo punto.

## 5. Verificare che la GPU venga usata correttamente (no fallback software)

Da verificare che questa build di Servo usi effettivamente l'accelerazione hardware per il
rendering (WebGL/WebGPU) e non finisca su un fallback software (es. llvmpipe/SwiftShader),
che per un gioco significherebbe prestazioni inaccettabili. `../test-page/` (vedi
`CUSTOMIZATIONS.md`, voce sul bridge `steam:`) ha già due pulsanti — "Test PixiJS render" e
"Test Three.js render" — pensati apposta per questa verifica: controllare non solo che il
rendering funzioni, ma anche quale renderer/GPU viene effettivamente riportato (e i log di
Servo/ANGLE al lancio) su ciascuna piattaforma della matrice CI (Windows/macOS/Linux). Non
ancora verificato su una build reale.

## Note

- Punto risolto nella sessione del 2026-08-06: stato di navigazione browser morto
  (location/back-forward/load-status/favicon) rimosso da `gui.rs` — vedi
  `CUSTOMIZATIONS.md`, patch `0003-strip-dead-browser-navigation-state-and-favicon-pipeline`.
  Lasciato intenzionalmente intatto `browser_tab`/`toolbar_button`: sono dead code senza
  alcun chiamante, già eliminati dal compilatore nelle build di release, quindi rimuoverli
  non avrebbe alcun effetto sul pacchetto di gioco finale.
