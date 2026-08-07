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

## 2. Verificare che la GPU venga usata correttamente (no fallback software)

Da verificare che questa build di Servo usi effettivamente l'accelerazione hardware per il
rendering (WebGL/WebGPU) e non finisca su un fallback software (es. llvmpipe/SwiftShader),
che per un gioco significherebbe prestazioni inaccettabili. `../test-page/` (vedi
`CUSTOMIZATIONS.md`, voce sul bridge `steam:`) ha due pulsanti — "Test PixiJS render" e
"Test Three.js render" — pensati apposta per questa verifica, ed entrambi ora riportano anche
gli fps a schermo, non solo "ok/failed". Da fine 2026-08-07 la pagina include anche
`GpuInfoPanel`, che legge `WEBGL_debug_renderer_info` e stampa il renderer/vendor GPU
*effettivo* (mascherato e non mascherato) e un'euristica "software renderer" — questo è
esattamente il "quale renderer/GPU viene effettivamente riportato" che mancava. Restano da
fare: controllare i log di Servo/ANGLE al lancio, e soprattutto **verificarlo su una build
reale** su ciascuna piattaforma della matrice CI (Windows/macOS/Linux) — non ancora fatto.

## 3. Schermata bianca su contenuto `file://` — causa risolta in questo fork, ma non ancora nella build "embedded" reale

**Stato:** causa individuata e patchata in questo fork (vedi `CUSTOMIZATIONS.md`, patch
`0007-stable-file-origin-for-module-script-loading`). **Verificato end-to-end il
2026-08-07**: `../test-page/` (bundle Vite multi-chunk, script esterno) carica ed esegue
correttamente su una build reale — niente più schermata bianca, i pulsanti diagnostici
rispondono. Quel test reale aveva scoperto una conseguenza dell'origine opaca su
`localStorage`/`sessionStorage`/`indexedDB` (più clipboard, causa indipendente) — risolto,
vedi la voce del 2026-08-07 in `CUSTOMIZATIONS.md`.

Causa: `ImmutableOrigin::new_opaque_for_file()` in `components/url/origin.rs` generava un
UUID casuale nuovo ad ogni chiamata per gli URL `file://`, quindi due chiamate a `.origin()`
sullo stesso URL `file://` non erano mai uguali tra loro. Questo faceva fallire
silenziosamente il fetch di qualunque `<script type="module" src="...">` esterno aperto via
`file://` (la fetch va in modalità "cors", che richiede same-origin, che per `file://` non
era mai vero) — lo script non veniva mai eseguito e la pagina restava bianca. Qualunque
bundle Vite "normale" (multi-chunk, script esterno) — sia quello di `../test-page/` sia
quello del progetto principale (root `vite.config.ts`, chunk multipli via `manualChunks`) —
ne era affetto. Il fix rende l'origine `file://` stabile (stesso id fisso per tutte le
origini `file://`) invece che casuale ad ogni chiamata — vedi `CUSTOMIZATIONS.md` per i
dettagli e i trade-off accettati (storage condiviso tra documenti `file://`, inerte per
questo fork perché apre sempre un solo documento `file://` e non espone navigazione ad
altri).

**Importante — questo fix da solo non basta per la build "embedded" reale:** il punto 1 di
questo file spiega che `../.github/workflows/embedded.yml` scarica oggi il binario
`servoshell` **stock, non patchato**, non il fork con le patch di questa cartella. Questo fix
vive solo nelle patch di questo fork, quindi finché il punto 1 non viene risolto (far
costruire/consegnare a `embedded.yml` il binario patchato), la build "embedded" realmente
distribuita continuerà ad avere questo identico bug, non toccata da questa patch.

Prossimi passi: una volta risolto anche il punto 1, ripetere la verifica con il `dist/`
reale del gioco (non solo `../test-page/`).

## Note

- Punto risolto nella sessione del 2026-08-06: stato di navigazione browser morto
  (location/back-forward/load-status/favicon) rimosso da `gui.rs` — vedi
  `CUSTOMIZATIONS.md`, patch `0003-strip-dead-browser-navigation-state-and-favicon-pipeline`.
  Lasciato intenzionalmente intatto `browser_tab`/`toolbar_button`: sono dead code senza
  alcun chiamante, già eliminati dal compilatore nelle build di release, quindi rimuoverli
  non avrebbe alcun effetto sul pacchetto di gioco finale.
- Punto risolto nella sessione del 2026-08-07: `localStorage`/`sessionStorage`/`indexedDB`/
  `navigator.storage` bloccati dall'origine opaca di `file://` — vedi `CUSTOMIZATIONS.md`,
  patch `0008-allow-storage-for-file-origin`. Insieme a questo, tutti i 18 pref del bundle
  `EXPERIMENTAL_PREFS` di upstream (clipboard, IndexedDB, WebGL2, WebGPU, OffscreenCanvas,
  Notifications, CSS Grid/Container Queries, ecc.) sono ora accesi di default — vedi patch
  `0009-default-on-experimental-web-platform-prefs`. Non ancora verificato end-to-end su una
  build reale (solo `cargo check` sui crate coinvolti, e `servo-script` non è stato
  verificabile in questa sandbox — vedi caveat nella voce 0008 di `CUSTOMIZATIONS.md`) —
  stesso caveat delle altre voci di questo file in attesa del punto 1 (build patchata reale).
- Punto risolto nella sessione del 2026-08-07: menu contestuale (tasto destro) disabilitato
  del tutto, no-op, nessun menu alternativo — vedi `CUSTOMIZATIONS.md`, patch
  `0010-disable-context-menu-popup`. `Dialog::ContextMenu` (rendering, costruttore, variante)
  rimosso interamente da `dialog.rs` in quanto diventato dead code.
- Punto risolto nella sessione del 2026-08-07: scorciatoie di reload (`Ctrl+R`/`F5`) rimosse
  da `headed_window.rs` — vedi `CUSTOMIZATIONS.md`, patch
  `0011-remove-page-reload-shortcuts`. Lasciata intenzionalmente intatta l'API di embedding
  nativo (`egl::App::reload()`, Android/OpenHarmony), fuori scope.
- Punto risolto nella sessione del 2026-08-07: navigazione "indietro"/"avanti" rimossa da
  tutti gli input path lato giocatore — scorciatoie da tastiera e pulsanti laterali del mouse
  in `headed_window.rs` — vedi `CUSTOMIZATIONS.md`, patch
  `0012-remove-back-forward-navigation`. Le voci equivalenti nel menu contestuale sono coperte
  dalla rimozione del menu stesso, sopra. Stessa nota di scope: API di embedding nativo
  (`egl::App::go_back`/`go_forward`) lasciata intatta.
  Nessuna di queste tre voci è stata verificata contro una build reale in questa sessione:
  `cargo check -p servoshell` in questa sandbox non completa per una lacuna di toolchain
  preesistente e indipendente da queste modifiche (manca `libclang` per la build script di
  `mozangle`, necessaria per qualunque binario `servoshell` su qualsiasi piattaforma — vedi
  `CUSTOMIZATIONS.md`). Verificate invece applicando le tre patch in sequenza contro
  un'estrazione pristine del tag `v0.4.0` e confrontando byte-per-byte il risultato con la
  copia di lavoro di questo fork — le patch riproducono fedelmente la modifica, ma il codice
  non è stato controllato dal compilatore in questa sessione. Stesso caveat delle altre voci
  di questo file in attesa del punto 1 (build patchata reale) e di un ambiente con `libclang`
  disponibile.
